use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub address: String,        // e.g., "0000:01:00.0"
    pub vendor_id: String,       // e.g., "10de" (NVIDIA)
    pub device_id: String,       // e.g., "2684" (RTX 4090)
    pub vendor_name: String,     // e.g., "NVIDIA Corporation"
    pub device_name: String,     // e.g., "GeForce RTX 4090"
    pub iommu_group: Option<u32>,
    pub driver: Option<String>,  // vfio-pci, nvidia, nouveau, etc.
    pub in_use: bool,
}

/// GPU capabilities and features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuCapabilities {
    pub compute_capability: Option<String>,  // CUDA compute capability
    pub vram_mb: Option<u64>,
    pub pcie_generation: Option<u8>,
    pub pcie_lanes: Option<u8>,
    pub sriov_capable: bool,
    pub vgpu_capable: bool,
    pub reset_bug: bool,  // Known NVIDIA reset bug
}

/// GPU Passthrough configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPassthroughConfig {
    pub device_address: String,
    pub mode: PassthroughMode,
    pub romfile: Option<PathBuf>,
    pub multifunction: bool,
    pub audio_device: Option<String>,  // GPU audio device address
    pub usb_controller: Option<String>, // USB controller for looking glass
    pub x_vga: bool,  // Primary VGA device
    pub display: DisplayMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PassthroughMode {
    Full,           // Full device passthrough
    SrIov,          // SR-IOV virtual function
    Vgpu,           // NVIDIA vGPU
    ManagedVfio,    // Managed by Nova
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisplayMode {
    None,           // Headless
    Spice,          // SPICE display
    LookingGlass,   // Looking Glass for near-native
    VirtioGpu,      // Virtio GPU (software)
}

/// GPU Manager - handles all GPU passthrough operations
pub struct GpuManager {
    /// Available GPUs detected on the system
    gpus: Vec<PciDevice>,

    /// IOMMU groups
    iommu_groups: Vec<IommuGroup>,

    /// GPU reservations (device_address -> vm_name)
    reservations: HashMap<String, String>,

    /// nvbind integration enabled
    pub nvbind_available: bool,

    /// System configuration
    pub config: GpuSystemConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSystemConfig {
    pub vfio_enabled: bool,
    pub iommu_enabled: bool,
    pub iommu_mode: Option<String>,  // intel_iommu or amd_iommu
    pub kernel_modules: Vec<String>,
    pub blacklisted_drivers: Vec<String>,
}

impl GpuManager {
    pub fn new() -> Self {
        Self {
            gpus: Vec::new(),
            iommu_groups: Vec::new(),
            reservations: HashMap::new(),
            nvbind_available: Self::check_nvbind(),
            config: GpuSystemConfig::detect(),
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

        // Discover PCI devices
        self.discover_pci_devices()?;

        // Discover IOMMU groups
        self.discover_iommu_groups()?;

        // Check GPU capabilities
        self.discover_gpu_capabilities()?;

        log_info!("Discovered {} GPUs in {} IOMMU groups",
                  self.gpus.len(), self.iommu_groups.len());

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
                || line.contains("Display controller") {

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
        let driver = self.get_device_driver(&address);

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
    fn get_device_driver(&self, address: &str) -> Option<String> {
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
    fn discover_capabilities_via_nvbind(&self) -> Result<()> {
        log_info!("Using nvbind for GPU capability discovery");

        let output = Command::new("nvbind")
            .arg("info")
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if output.status.success() {
            let info = String::from_utf8_lossy(&output.stdout);
            log_debug!("nvbind info: {}", info);
            // TODO: Parse nvbind JSON output
        }

        Ok(())
    }

    /// Discover GPU capabilities via nvidia-smi
    fn discover_capabilities_via_nvidia_smi(&self) -> Result<()> {
        let output = Command::new("nvidia-smi")
            .args(&["--query-gpu=index,name,memory.total,pcie.link.gen.current,pcie.link.width.current", "--format=csv,noheader"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout);
                log_debug!("nvidia-smi output: {}", info);
                // TODO: Parse and store GPU capabilities
            }
        }

        Ok(())
    }

    /// Configure a GPU for passthrough
    pub fn configure_passthrough(&mut self, device_address: &str, vm_name: &str) -> Result<()> {
        log_info!("Configuring GPU {} for passthrough to VM '{}'", device_address, vm_name);

        // Find the GPU
        let gpu = self.gpus.iter()
            .find(|g| g.address == device_address)
            .ok_or_else(|| NovaError::ConfigError(format!("GPU {} not found", device_address)))?;

        // Check if GPU is already reserved
        if self.reservations.contains_key(device_address) {
            return Err(NovaError::ConfigError(
                format!("GPU {} is already reserved by VM '{}'",
                        device_address, self.reservations[device_address])
            ));
        }

        // Unbind from current driver
        self.unbind_driver(device_address)?;

        // Bind to vfio-pci
        self.bind_vfio_pci(device_address)?;

        // Reserve the GPU
        self.reservations.insert(device_address.to_string(), vm_name.to_string());

        log_info!("GPU {} successfully configured for passthrough", device_address);
        Ok(())
    }

    /// Unbind a device from its current driver
    fn unbind_driver(&self, device_address: &str) -> Result<()> {
        let driver_path = format!("/sys/bus/pci/devices/{}/driver/unbind", device_address);

        if Path::new(&driver_path).exists() {
            fs::write(&driver_path, device_address)
                .map_err(|e| {
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
        let _ = Command::new("modprobe")
            .arg("vfio-pci")
            .output();

        // Write device IDs to vfio-pci new_id
        let gpu = self.gpus.iter()
            .find(|g| g.address == device_address)
            .ok_or_else(|| NovaError::ConfigError(format!("GPU {} not found", device_address)))?;

        let new_id_path = "/sys/bus/pci/drivers/vfio-pci/new_id";
        let device_ids = format!("{} {}", gpu.vendor_id, gpu.device_id);

        fs::write(new_id_path, device_ids)
            .map_err(|e| {
                log_error!("Failed to bind to vfio-pci: {}", e);
                NovaError::SystemCommandFailed
            })?;

        log_debug!("Bound {} to vfio-pci", device_address);
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
            return Err(NovaError::ConfigError("Invalid PCI address format".to_string()));
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
        if let Some(audio_address) = &config.audio_device {
            xml.push_str("    <hostdev mode='subsystem' type='pci' managed='yes'>\n");
            xml.push_str("      <source>\n");
            xml.push_str(&format!("        <address domain='0x0000' bus='0x{}' slot='0x{}' function='0x{}'/>\n",
                                  "01", "00", "1")); // Simplified
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
        Ok(())
    }

    /// List all available GPUs
    pub fn list_gpus(&self) -> &[PciDevice] {
        &self.gpus
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
        let required = ["vfio", "vfio_pci", "vfio_iommu_type1"];

        for module in required {
            if !Path::new(&format!("/sys/module/{}", module)).exists() {
                return false;
            }
        }

        true
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

        if self.iommu_groups.iter().filter(|g| g.viable_for_passthrough).count() == 0 {
            issues.push("No viable IOMMU groups for GPU passthrough".to_string());
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
