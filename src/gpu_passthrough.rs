use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// IOMMU Group information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IommuGroup {
    pub id: u32,
    pub devices: Vec<PciDevice>,
    pub isolated: bool,
    pub viable_for_passthrough: bool,
}

/// PCI Device representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PciDevice {
    pub address: String,     // e.g., "0000:01:00.0"
    pub vendor_id: String,   // e.g., "10de" (NVIDIA)
    pub device_id: String,   // e.g., "2684" (RTX 4090)
    pub vendor_name: String, // e.g., "NVIDIA Corporation"
    pub device_name: String, // e.g., "GeForce RTX 4090"
    pub iommu_group: Option<u32>,
    pub driver: Option<String>, // vfio-pci, nvidia, nouveau, etc.
    pub in_use: bool,
}

/// Snapshot of a GPU's current binding state
#[derive(Debug, Clone)]
pub struct DeviceBindingInfo {
    pub driver: Option<String>,
    pub in_use: bool,
    pub reserved_for: Option<String>,
}

/// GPU capabilities and features
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuCapabilities {
    pub compute_capability: Option<String>, // CUDA compute capability
    pub vram_mb: Option<u64>,
    pub pcie_generation: Option<u8>,
    pub pcie_lanes: Option<u8>,
    pub sriov_capable: bool,
    pub vgpu_capable: bool,
    pub reset_bug: bool, // Known NVIDIA reset bug
    pub generation: Option<GpuGeneration>,
    pub minimum_driver: Option<String>,
    pub recommended_kernel: Option<String>,
    pub tcc_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GpuGeneration {
    Pascal,
    Turing,
    Ampere,
    AdaLovelace,
    Blackwell,
    Unknown,
}

impl fmt::Display for GpuGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            GpuGeneration::Pascal => "Pascal (RTX 10)",
            GpuGeneration::Turing => "Turing (RTX 20)",
            GpuGeneration::Ampere => "Ampere (RTX 30)",
            GpuGeneration::AdaLovelace => "Ada Lovelace (RTX 40)",
            GpuGeneration::Blackwell => "Blackwell (RTX 50)",
            GpuGeneration::Unknown => "Unknown",
        };

        write!(f, "{}", label)
    }
}

/// GPU Passthrough configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPassthroughConfig {
    pub device_address: String,
    pub mode: PassthroughMode,
    pub romfile: Option<PathBuf>,
    pub multifunction: bool,
    pub audio_device: Option<String>,   // GPU audio device address
    pub usb_controller: Option<String>, // USB controller for looking glass
    pub x_vga: bool,                    // Primary VGA device
    pub display: DisplayMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PassthroughMode {
    Full,        // Full device passthrough
    SrIov,       // SR-IOV virtual function
    Vgpu,        // NVIDIA vGPU
    ManagedVfio, // Managed by Nova
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisplayMode {
    None,         // Headless
    Spice,        // SPICE display
    LookingGlass, // Looking Glass for near-native
    VirtioGpu,    // Virtio GPU (software)
}

/// GPU Manager - handles all GPU passthrough operations
pub struct GpuManager {
    /// Available GPUs detected on the system
    gpus: Vec<PciDevice>,

    /// IOMMU groups
    iommu_groups: Vec<IommuGroup>,

    /// GPU reservations (device_address -> vm_name)
    reservations: HashMap<String, String>,

    /// Discovered GPU capabilities by PCI address
    gpu_capabilities: HashMap<String, GpuCapabilities>,

    /// nvbind integration enabled
    pub nvbind_available: bool,

    /// System configuration
    pub config: GpuSystemConfig,

    discovered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSystemConfig {
    pub vfio_enabled: bool,
    pub iommu_enabled: bool,
    pub iommu_mode: Option<String>, // intel_iommu or amd_iommu
    pub kernel_modules: Vec<String>,
    pub blacklisted_drivers: Vec<String>,
}

impl GpuManager {
    pub fn new() -> Self {
        Self {
            gpus: Vec::new(),
            iommu_groups: Vec::new(),
            reservations: HashMap::new(),
            gpu_capabilities: HashMap::new(),
            nvbind_available: Self::check_nvbind(),
            config: GpuSystemConfig::detect(),
            discovered: false,
        }
    }

    /// Check if nvbind is available on the system
    fn check_nvbind() -> bool {
        Command::new("nvbind")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Discover all GPUs and IOMMU groups
    pub fn discover(&mut self) -> Result<()> {
        log_info!("Discovering GPUs and IOMMU groups...");

        self.gpus.clear();
        self.iommu_groups.clear();
        self.gpu_capabilities.clear();

        // Discover PCI devices
        self.discover_pci_devices()?;

        // Discover IOMMU groups
        self.discover_iommu_groups()?;

        // Check GPU capabilities
        self.discover_gpu_capabilities()?;

        log_info!(
            "Discovered {} GPUs in {} IOMMU groups",
            self.gpus.len(),
            self.iommu_groups.len()
        );

        self.discovered = true;

        Ok(())
    }

    /// Ensure discovery has been performed (idempotent)
    pub fn ensure_discovered(&mut self) -> Result<()> {
        if !self.discovered {
            self.discover()?;
        }
        Ok(())
    }

    /// Discover PCI devices (GPUs)
    fn discover_pci_devices(&mut self) -> Result<()> {
        // Use lspci to discover GPUs
        let output = Command::new("lspci")
            .args(&["-nn", "-D"])
            .output()
            .map_err(|e| {
                log_error!("Failed to run lspci: {}", e);
                NovaError::SystemCommandFailed
            })?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            // Look for VGA/3D/Display controllers
            if line.contains("VGA compatible controller")
                || line.contains("3D controller")
                || line.contains("Display controller")
            {
                if let Some(device) = self.parse_lspci_line(line) {
                    log_debug!("Found GPU: {} ({})", device.device_name, device.address);
                    self.gpus.push(device);
                }
            }
        }

        Ok(())
    }

    /// Parse lspci output line
    fn parse_lspci_line(&self, line: &str) -> Option<PciDevice> {
        // Example: 0000:01:00.0 VGA compatible controller [0300]: NVIDIA Corporation [10de:2684] (rev a1)
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let address = parts[0].to_string();

        // Extract vendor:device IDs from brackets [10de:2684]
        let ids = line
            .split('[')
            .nth(1)?
            .split(']')
            .next()?
            .split(':')
            .collect::<Vec<&str>>();

        if ids.len() != 2 {
            return None;
        }

        let vendor_id = ids[0].to_string();
        let device_id = ids[1].to_string();

        // Extract device description
        let desc_start = line.find(": ")? + 2;
        let desc_end = line.find(" [").unwrap_or(line.len());
        let description = line[desc_start..desc_end].to_string();

        // Split vendor and device name
        let (vendor_name, device_name) = if let Some(pos) = description.rfind('[') {
            (description[..pos].trim().to_string(), description.clone())
        } else {
            ("Unknown".to_string(), description)
        };

        // Check current driver
        let driver = Self::get_device_driver(&address);

        Some(PciDevice {
            address,
            vendor_id,
            device_id,
            vendor_name,
            device_name,
            iommu_group: None,
            driver,
            in_use: false,
        })
    }

    /// Get the current driver for a PCI device
    fn get_device_driver(address: &str) -> Option<String> {
        let driver_path = format!("/sys/bus/pci/devices/{}/driver", address);

        if let Ok(link) = fs::read_link(&driver_path) {
            if let Some(driver) = link.file_name() {
                return Some(driver.to_string_lossy().to_string());
            }
        }

        None
    }

    /// Discover IOMMU groups
    fn discover_iommu_groups(&mut self) -> Result<()> {
        let iommu_path = Path::new("/sys/kernel/iommu_groups");

        if !iommu_path.exists() {
            log_warn!("IOMMU not enabled or not available");
            return Ok(());
        }

        // Update GPU IOMMU group information
        for gpu in &mut self.gpus {
            let group_path = format!("/sys/bus/pci/devices/{}/iommu_group", gpu.address);

            if let Ok(link) = fs::read_link(&group_path) {
                if let Some(group_name) = link.file_name() {
                    if let Ok(group_id) = group_name.to_string_lossy().parse::<u32>() {
                        gpu.iommu_group = Some(group_id);
                    }
                }
            }
        }

        // Build IOMMU group list
        if let Ok(entries) = fs::read_dir(iommu_path) {
            for entry in entries.flatten() {
                if let Some(group_id_str) = entry.file_name().to_str() {
                    if let Ok(group_id) = group_id_str.parse::<u32>() {
                        let devices = self.get_iommu_group_devices(group_id);

                        // Skip groups that do not contain any discoverable GPUs
                        if devices.is_empty() {
                            continue;
                        }

                        let isolated = devices.len() == 1;
                        let viable = isolated || self.is_group_viable(&devices);

                        self.iommu_groups.push(IommuGroup {
                            id: group_id,
                            devices: devices.clone(),
                            isolated,
                            viable_for_passthrough: viable,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Get all devices in an IOMMU group
    fn get_iommu_group_devices(&self, group_id: u32) -> Vec<PciDevice> {
        self.gpus
            .iter()
            .filter(|gpu| gpu.iommu_group == Some(group_id))
            .cloned()
            .collect()
    }

    /// Check if an IOMMU group is viable for passthrough
    fn is_group_viable(&self, devices: &[PciDevice]) -> bool {
        // Group is viable if all devices are GPUs or GPU-related
        // (e.g., GPU + GPU audio device)
        devices.iter().all(|d| {
            d.device_name.contains("NVIDIA")
                || d.device_name.contains("AMD")
                || d.device_name.contains("Intel")
                || d.device_name.contains("Audio")
        })
    }

    /// Discover GPU capabilities (CUDA, VRAM, etc.)
    fn discover_gpu_capabilities(&mut self) -> Result<()> {
        // Use nvidia-smi for NVIDIA GPUs
        if self.nvbind_available {
            self.discover_capabilities_via_nvbind()?;
        } else {
            self.discover_capabilities_via_nvidia_smi()?;
        }

        Ok(())
    }

    /// Discover GPU capabilities via nvbind
    fn discover_capabilities_via_nvbind(&mut self) -> Result<()> {
        log_info!("Using nvbind for GPU capability discovery");

        let output = Command::new("nvbind")
            .arg("info")
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if output.status.success() {
            let info = String::from_utf8_lossy(&output.stdout);
            log_debug!("nvbind info: {}", info);

            // Parse nvbind text output
            // Format: "GPU 0: GeForce RTX 4090"
            //         "  PCI Address: 0000:01:00.0"
            //         "  Memory: 24576 MB"
            let mut current_name: Option<String> = None;
            let mut current_addr: Option<String> = None;
            let mut current_memory: Option<u64> = None;

            let flush_entry = |manager: &mut GpuManager,
                               name: Option<String>,
                               addr: Option<String>,
                               memory: Option<u64>| {
                if let (Some(name), Some(addr)) = (name, addr) {
                    let normalized = GpuManager::normalize_pci_address(&addr);
                    let caps_entry = manager
                        .gpu_capabilities
                        .entry(normalized.clone())
                        .or_default();

                    if let Some(mem) = memory {
                        caps_entry.vram_mb = Some(mem);
                    }

                    if let Some(detected_gen) = GpuManager::detect_gpu_generation(&name) {
                        caps_entry.generation = Some(detected_gen);
                    }

                    if matches!(caps_entry.generation, Some(GpuGeneration::Blackwell)) {
                        GpuManager::apply_blackwell_requirements(caps_entry);
                    }
                }
            };

            for line in info.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    flush_entry(
                        self,
                        current_name.take(),
                        current_addr.take(),
                        current_memory.take(),
                    );
                    continue;
                }

                if trimmed.starts_with("GPU ") {
                    if let Some(colon_pos) = trimmed.find(':') {
                        let gpu_name = trimmed[colon_pos + 1..].trim().to_string();
                        current_name = Some(gpu_name);
                    }
                } else if trimmed.starts_with("PCI Address:") {
                    let addr = trimmed
                        .split(':')
                        .nth(1)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    current_addr = Some(addr);
                } else if trimmed.starts_with("Memory:") && trimmed.contains("MB") {
                    if let Some(memory_str) = trimmed
                        .split_whitespace()
                        .find(|s| s.chars().all(|c| c.is_ascii_digit()))
                    {
                        if let Ok(memory_mb) = memory_str.parse::<u64>() {
                            current_memory = Some(memory_mb);
                        }
                    }
                }
            }

            flush_entry(
                self,
                current_name.take(),
                current_addr.take(),
                current_memory.take(),
            );
        }

        Ok(())
    }

    /// Discover GPU capabilities via nvidia-smi
    fn discover_capabilities_via_nvidia_smi(&mut self) -> Result<()> {
        log_info!("Using nvidia-smi for GPU capability discovery");

        let output = Command::new("nvidia-smi")
            .args(&[
                "--query-gpu=pci.bus_id,name,memory.total,pcie.link.gen.current,pcie.link.width.current,compute_cap",
                "--format=csv,noheader",
            ])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout);
                log_debug!("nvidia-smi output: {}", info);

                // Parse CSV output
                // Format: "00000000:01:00.0, GeForce RTX 5090, 32768 MiB, 5, 16, 9.0"
                for line in info.lines() {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 6 {
                        let bus_id = parts[0];
                        let name = parts[1];
                        let memory_str = parts[2].replace(" MiB", "").replace(" MB", "");
                        let pcie_gen = parts[3];
                        let pcie_lanes = parts[4];
                        let compute_cap = parts[5];

                        let normalized = GpuManager::normalize_pci_address(bus_id);
                        log_debug!(
                            "GPU {}: {} (Memory: {} MiB, PCIe Gen {}, x{} lanes, CC {})",
                            normalized,
                            name,
                            memory_str,
                            pcie_gen,
                            pcie_lanes,
                            compute_cap
                        );

                        let caps_entry =
                            self.gpu_capabilities.entry(normalized.clone()).or_default();

                        caps_entry.compute_capability = Self::parse_non_empty(compute_cap);
                        if let Ok(memory_mb) = memory_str.parse::<u64>() {
                            caps_entry.vram_mb = Some(memory_mb);
                        }
                        caps_entry.pcie_generation = pcie_gen.parse::<u8>().ok();
                        caps_entry.pcie_lanes = pcie_lanes.parse::<u8>().ok();

                        if let Some(detected_gen) = Self::detect_gpu_generation(name) {
                            caps_entry.generation = Some(detected_gen.clone());
                        }

                        caps_entry.tcc_supported = matches!(
                            caps_entry.generation,
                            Some(GpuGeneration::AdaLovelace) | Some(GpuGeneration::Blackwell)
                        );

                        if matches!(caps_entry.generation, Some(GpuGeneration::Blackwell)) {
                            GpuManager::apply_blackwell_requirements(caps_entry);
                        } else if matches!(caps_entry.generation, Some(GpuGeneration::AdaLovelace))
                        {
                            caps_entry
                                .minimum_driver
                                .get_or_insert_with(|| "545.0".to_string());
                        }
                    }
                }
            } else {
                log_warn!("nvidia-smi command failed or no NVIDIA GPUs detected");
            }
        } else {
            log_warn!("nvidia-smi not available on system");
        }

        Ok(())
    }

    fn apply_blackwell_requirements(caps: &mut GpuCapabilities) {
        caps.minimum_driver = Some("560.0".to_string());
        caps.recommended_kernel = Some("6.9".to_string());
        caps.vgpu_capable = true;
        caps.sriov_capable = true;
        caps.tcc_supported = true;
    }

    fn parse_non_empty(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("n/a") {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn normalize_pci_address(bus_id: &str) -> String {
        let trimmed = bus_id.trim();
        if trimmed.len() > 12 {
            trimmed[trimmed.len() - 12..].to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn detect_gpu_generation(name: &str) -> Option<GpuGeneration> {
        let lower = name.to_lowercase();

        if lower.contains("rtx 50")
            || lower.contains(" 50") && lower.contains("rtx")
            || lower.contains("blackwell")
            || lower.contains("gb202")
        {
            Some(GpuGeneration::Blackwell)
        } else if lower.contains("rtx 40")
            || lower.contains("4090")
            || lower.contains("4080")
            || lower.contains("ada")
        {
            Some(GpuGeneration::AdaLovelace)
        } else if lower.contains("rtx 30")
            || lower.contains("3090")
            || lower.contains("3080")
            || lower.contains("ampere")
        {
            Some(GpuGeneration::Ampere)
        } else if lower.contains("rtx 20")
            || lower.contains("2080")
            || lower.contains("2070")
            || lower.contains("turing")
        {
            Some(GpuGeneration::Turing)
        } else if lower.contains("gtx 10") || lower.contains("1080") || lower.contains("pascal") {
            Some(GpuGeneration::Pascal)
        } else {
            None
        }
    }

    /// Configure a GPU for passthrough
    pub fn configure_passthrough(&mut self, device_address: &str, vm_name: &str) -> Result<()> {
        log_info!(
            "Configuring GPU {} for passthrough to VM '{}'",
            device_address,
            vm_name
        );

        // Find the GPU
        let _gpu = self
            .gpus
            .iter()
            .find(|g| g.address == device_address)
            .ok_or_else(|| NovaError::ConfigError(format!("GPU {} not found", device_address)))?;

        // Check if GPU is already reserved
        if self.reservations.contains_key(device_address) {
            return Err(NovaError::ConfigError(format!(
                "GPU {} is already reserved by VM '{}'",
                device_address, self.reservations[device_address]
            )));
        }

        // Unbind from current driver
        self.unbind_driver(device_address)?;

        // Bind to vfio-pci
        self.bind_vfio_pci(device_address)?;

        // Reserve the GPU
        self.reservations
            .insert(device_address.to_string(), vm_name.to_string());

        log_info!(
            "GPU {} successfully configured for passthrough",
            device_address
        );
        Ok(())
    }

    /// Unbind a device from its current driver
    fn unbind_driver(&self, device_address: &str) -> Result<()> {
        let driver_path = format!("/sys/bus/pci/devices/{}/driver/unbind", device_address);

        if Path::new(&driver_path).exists() {
            fs::write(&driver_path, device_address).map_err(|e| {
                log_error!("Failed to unbind driver: {}", e);
                NovaError::SystemCommandFailed
            })?;
            log_debug!("Unbound driver from {}", device_address);
        }

        Ok(())
    }

    /// Bind a device to vfio-pci driver
    fn bind_vfio_pci(&self, device_address: &str) -> Result<()> {
        // Load vfio-pci module
        let _ = Command::new("modprobe").arg("vfio-pci").output();

        // Write device IDs to vfio-pci new_id
        let gpu = self
            .gpus
            .iter()
            .find(|g| g.address == device_address)
            .ok_or_else(|| NovaError::ConfigError(format!("GPU {} not found", device_address)))?;

        let new_id_path = "/sys/bus/pci/drivers/vfio-pci/new_id";
        let device_ids = format!("{} {}", gpu.vendor_id, gpu.device_id);

        fs::write(new_id_path, device_ids).map_err(|e| {
            log_error!("Failed to bind to vfio-pci: {}", e);
            NovaError::SystemCommandFailed
        })?;

        log_debug!("Bound {} to vfio-pci", device_address);
        Ok(())
    }

    /// Public helper to bind a device to vfio without reserving it to a VM
    pub fn bind_device_to_vfio(&mut self, device_address: &str) -> Result<()> {
        log_info!("Binding GPU {} to vfio-pci", device_address);
        self.unbind_driver(device_address)?;
        self.bind_vfio_pci(device_address)?;
        self.refresh_device_status();
        Ok(())
    }

    /// Refresh runtime state for discovered devices (driver bindings, in-use status)
    pub fn refresh_device_status(&mut self) {
        for gpu in &mut self.gpus {
            gpu.driver = Self::get_device_driver(&gpu.address);
            gpu.in_use = matches!(gpu.driver.as_deref(), Some(driver) if driver != "vfio-pci");
        }
    }

    /// Forcefully unbind a device from its current driver without binding to vfio
    pub fn force_unbind_device(&self, device_address: &str) -> Result<()> {
        log_info!("Force unbinding host driver from {}", device_address);
        self.unbind_driver(device_address)
    }

    /// Obtain the current binding snapshot for a device
    pub fn binding_info(&self, address: &str) -> Option<DeviceBindingInfo> {
        self.gpus
            .iter()
            .find(|gpu| gpu.address == address)
            .map(|gpu| DeviceBindingInfo {
                driver: gpu.driver.clone(),
                in_use: gpu.in_use,
                reserved_for: self.reservations.get(address).cloned(),
            })
    }

    /// Return a list of VFIO-related kernel modules that are not currently loaded
    pub fn missing_vfio_modules(&self) -> Vec<&'static str> {
        ["vfio", "vfio_pci", "vfio_iommu_type1"]
            .iter()
            .copied()
            .filter(|module| !Path::new(&format!("/sys/module/{}", module)).exists())
            .collect()
    }

    /// Check whether all VFIO kernel modules are active
    pub fn vfio_modules_ready(&self) -> bool {
        self.missing_vfio_modules().is_empty()
    }

    /// Attempt to reattach a device to the host driver stack
    pub fn reattach_device_driver(&mut self, device_address: &str) -> Result<()> {
        log_info!("Reattaching GPU {} to host drivers", device_address);
        self.unbind_driver(device_address)?;

        let probe_path = Path::new("/sys/bus/pci/drivers_probe");
        if probe_path.exists() {
            if let Err(err) = fs::write(probe_path, format!("{}", device_address)) {
                log_error!(
                    "Failed to trigger drivers_probe for {}: {}",
                    device_address,
                    err
                );
                return Err(NovaError::SystemCommandFailed);
            }
        } else {
            log_warn!("drivers_probe interface not available on this kernel");
        }

        self.reservations.remove(device_address);
        self.refresh_device_status();
        Ok(())
    }

    /// Load required VFIO kernel modules
    pub fn load_vfio_stack(&self) -> Result<()> {
        for module in ["vfio", "vfio_pci", "vfio_iommu_type1"] {
            match Command::new("modprobe").arg(module).output() {
                Ok(output) if output.status.success() => {
                    log_debug!("Loaded module {}", module);
                }
                Ok(output) => {
                    log_error!(
                        "Failed to load module {}: {}",
                        module,
                        String::from_utf8_lossy(&output.stderr)
                    );
                    return Err(NovaError::SystemCommandFailed);
                }
                Err(err) => {
                    log_error!("Failed to execute modprobe {}: {}", module, err);
                    return Err(NovaError::SystemCommandFailed);
                }
            }
        }

        Ok(())
    }

    /// Generate libvirt XML for GPU passthrough
    pub fn generate_libvirt_xml(&self, config: &GpuPassthroughConfig) -> Result<String> {
        let mut xml = String::new();

        xml.push_str("    <hostdev mode='subsystem' type='pci' managed='yes'>\n");
        xml.push_str("      <source>\n");

        // Parse PCI address (0000:01:00.0)
        let parts: Vec<&str> = config.device_address.split(':').collect();
        if parts.len() != 3 {
            return Err(NovaError::ConfigError(
                "Invalid PCI address format".to_string(),
            ));
        }

        let domain = &parts[0][0..4];
        let bus = &parts[1];
        let slot_func: Vec<&str> = parts[2].split('.').collect();
        let slot = slot_func[0];
        let function = slot_func.get(1).unwrap_or(&"0");

        xml.push_str(&format!(
            "        <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>\n",
            domain, bus, slot, function
        ));

        xml.push_str("      </source>\n");

        if let Some(romfile) = &config.romfile {
            xml.push_str(&format!("      <rom file='{}'/>\n", romfile.display()));
        }

        xml.push_str("    </hostdev>\n");

        // Add audio device if specified
        if let Some(_audio_address) = &config.audio_device {
            xml.push_str("    <hostdev mode='subsystem' type='pci' managed='yes'>\n");
            xml.push_str("      <source>\n");
            xml.push_str(&format!(
                "        <address domain='0x0000' bus='0x{}' slot='0x{}' function='0x{}'/>\n",
                "01", "00", "1"
            )); // Simplified
            xml.push_str("      </source>\n");
            xml.push_str("    </hostdev>\n");
        }

        Ok(xml)
    }

    /// Release a GPU from passthrough
    pub fn release_gpu(&mut self, device_address: &str) -> Result<()> {
        log_info!("Releasing GPU {} from passthrough", device_address);

        // Unbind from vfio-pci
        self.unbind_driver(device_address)?;

        // Rebind to original driver (nvidia, nouveau, etc.)
        // This is handled automatically by the kernel in most cases

        // Remove reservation
        self.reservations.remove(device_address);

        log_info!("GPU {} released", device_address);
        self.refresh_device_status();
        Ok(())
    }

    /// List all available GPUs
    pub fn list_gpus(&self) -> &[PciDevice] {
        &self.gpus
    }

    /// Retrieve discovered capabilities for a specific GPU (if available)
    pub fn capabilities_for(&self, device_address: &str) -> Option<&GpuCapabilities> {
        self.gpu_capabilities.get(device_address)
    }

    /// Determine whether any detected GPUs are part of the RTX 50-series family
    pub fn any_blackwell_gpus(&self) -> bool {
        self.gpu_capabilities
            .values()
            .any(|caps| matches!(caps.generation, Some(GpuGeneration::Blackwell)))
    }

    /// List all IOMMU groups
    pub fn list_iommu_groups(&self) -> &[IommuGroup] {
        &self.iommu_groups
    }

    /// Get reservations
    pub fn get_reservations(&self) -> &HashMap<String, String> {
        &self.reservations
    }

    /// Check system configuration for GPU passthrough readiness
    pub fn check_system_requirements(&self) -> GpuSystemStatus {
        GpuSystemStatus {
            iommu_enabled: self.config.iommu_enabled,
            vfio_available: self.config.vfio_enabled,
            gpus_detected: !self.gpus.is_empty(),
            nvbind_available: self.nvbind_available,
            kernel_modules_loaded: self.check_kernel_modules(),
            issues: self.identify_issues(),
        }
    }

    /// Check if required kernel modules are loaded
    fn check_kernel_modules(&self) -> bool {
        self.vfio_modules_ready()
    }

    /// Identify system configuration issues
    fn identify_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if !self.config.iommu_enabled {
            issues.push("IOMMU not enabled in kernel parameters".to_string());
        }

        if !self.config.vfio_enabled {
            issues.push("VFIO module not loaded".to_string());
        }

        if self.gpus.is_empty() {
            issues.push("No GPUs detected on the system".to_string());
        }

        if self
            .iommu_groups
            .iter()
            .filter(|g| g.viable_for_passthrough)
            .count()
            == 0
        {
            issues.push("No viable IOMMU groups for GPU passthrough".to_string());
        }

        if self
            .gpu_capabilities
            .values()
            .any(|caps| matches!(caps.generation, Some(GpuGeneration::Blackwell)))
        {
            issues.push(
                "RTX 50-series detected — ensure NVIDIA driver 560+, Linux kernel 6.9+, and TCC mode for low-latency consoles (Looking Glass)."
                    .to_string(),
            );
        }

        issues
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GpuSystemStatus {
    pub iommu_enabled: bool,
    pub vfio_available: bool,
    pub gpus_detected: bool,
    pub nvbind_available: bool,
    pub kernel_modules_loaded: bool,
    pub issues: Vec<String>,
}

impl GpuSystemConfig {
    pub fn detect() -> Self {
        let iommu_enabled = Path::new("/sys/kernel/iommu_groups").exists();
        let vfio_enabled = Path::new("/sys/module/vfio").exists();

        // Detect IOMMU mode
        let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
        let iommu_mode = if cmdline.contains("intel_iommu=on") {
            Some("intel".to_string())
        } else if cmdline.contains("amd_iommu=on") {
            Some("amd".to_string())
        } else {
            None
        };

        Self {
            vfio_enabled,
            iommu_enabled,
            iommu_mode,
            kernel_modules: vec!["vfio".to_string(), "vfio_pci".to_string()],
            blacklisted_drivers: vec!["nouveau".to_string()],
        }
    }
}

impl Default for GpuManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for GpuPassthroughConfig {
    fn default() -> Self {
        Self {
            device_address: String::new(),
            mode: PassthroughMode::Full,
            romfile: None,
            multifunction: true,
            audio_device: None,
            usb_controller: None,
            x_vga: true,
            display: DisplayMode::None,
        }
    }
}

impl GpuPassthroughConfig {
    /// Generate vfio device arguments for QEMU
    pub fn qemu_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if !self.device_address.is_empty() {
            let mut device = format!("vfio-pci,host={}", self.device_address);

            if self.multifunction {
                device.push_str(",multifunction=on");
            }

            if self.x_vga {
                device.push_str(",x-vga=on");
            }

            match self.mode {
                PassthroughMode::SrIov => device.push_str(",disable-err=on"),
                PassthroughMode::Vgpu => device.push_str(",enable-migration=on"),
                PassthroughMode::ManagedVfio | PassthroughMode::Full => {}
            }

            args.push("-device".to_string());
            args.push(device);
        }

        if let Some(audio) = &self.audio_device {
            if !audio.is_empty() {
                args.push("-device".to_string());
                args.push(format!("vfio-pci,host={}", audio));
            }
        }

        if let Some(controller) = &self.usb_controller {
            if !controller.is_empty() {
                args.push("-device".to_string());
                args.push(format!("vfio-pci,host={}", controller));
            }
        }

        args
    }
}
