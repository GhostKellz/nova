// Generic PCI Device Passthrough
// Supports GPUs, NICs, NVMe drives, sound cards, and any PCIe device

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
// Removed unused import: use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PciDevice {
    pub address: String, // e.g., "0000:01:00.0"
    pub vendor_id: String,
    pub device_id: String,
    pub subsystem_vendor_id: String,
    pub subsystem_device_id: String,
    pub vendor_name: String,
    pub device_name: String,
    pub device_class: PciDeviceClass,
    pub iommu_group: Option<u32>,
    pub driver: Option<String>,
    pub numa_node: Option<u32>,
    pub assigned_to_vm: Option<String>,
    pub sysfs_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PciDeviceClass {
    GPU,
    NetworkController,
    StorageController, // NVMe, SATA, SAS
    AudioDevice,
    USBController,
    SATAController,
    EthernetController,
    WirelessController,
    Bridge,
    Other(String),
}

pub struct PciPassthroughManager {
    devices: HashMap<String, PciDevice>,
    assignments: HashMap<String, String>, // pci_address -> vm_name
}

impl PciPassthroughManager {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
            assignments: HashMap::new(),
        }
    }

    /// Discover all PCI devices
    pub fn discover_devices(&mut self) -> Result<Vec<PciDevice>, String> {
        let mut devices = Vec::new();
        let pci_devices_path = Path::new("/sys/bus/pci/devices");

        if !pci_devices_path.exists() {
            return Err("PCI devices path not found".to_string());
        }

        for entry in fs::read_dir(pci_devices_path)
            .map_err(|e| format!("Failed to read PCI devices: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let device_path = entry.path();
            let address = entry.file_name().to_string_lossy().to_string();

            if let Ok(device) = self.parse_pci_device(&device_path, &address) {
                devices.push(device.clone());
                self.devices.insert(address, device);
            }
        }

        println!("Discovered {} PCI devices", devices.len());
        Ok(devices)
    }

    /// Parse PCI device information from sysfs
    fn parse_pci_device(&self, device_path: &Path, address: &str) -> Result<PciDevice, String> {
        // Read device IDs
        let vendor_id = Self::read_hex_file(&device_path.join("vendor"))?;
        let device_id = Self::read_hex_file(&device_path.join("device"))?;
        let subsystem_vendor = Self::read_hex_file(&device_path.join("subsystem_vendor"))?;
        let subsystem_device = Self::read_hex_file(&device_path.join("subsystem_device"))?;

        // Read device class
        let class_code = Self::read_hex_file(&device_path.join("class"))?;
        let device_class = Self::classify_pci_device(&class_code);

        // Read driver
        let driver = device_path
            .join("driver")
            .read_link()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()));

        // Read IOMMU group
        let iommu_group = device_path
            .join("iommu_group")
            .read_link()
            .ok()
            .and_then(|p| {
                p.file_name()
                    .and_then(|f| f.to_str())
                    .and_then(|s| s.parse::<u32>().ok())
            });

        // Read NUMA node
        let numa_node = Self::read_file(&device_path.join("numa_node"))
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .and_then(|n| if n >= 0 { Some(n as u32) } else { None });

        let vendor_name = Self::lookup_vendor_name(&vendor_id);
        let device_name = Self::lookup_device_name(&vendor_id, &device_id, &device_class);

        Ok(PciDevice {
            address: address.to_string(),
            vendor_id,
            device_id,
            subsystem_vendor_id: subsystem_vendor,
            subsystem_device_id: subsystem_device,
            vendor_name,
            device_name,
            device_class,
            iommu_group,
            driver,
            numa_node,
            assigned_to_vm: None,
            sysfs_path: device_path.to_path_buf(),
        })
    }

    /// Bind PCI device to vfio-pci driver
    pub fn bind_to_vfio(&self, pci_address: &str) -> Result<(), String> {
        let device = self
            .devices
            .get(pci_address)
            .ok_or_else(|| format!("Device {} not found", pci_address))?;

        println!("Binding {} to vfio-pci driver", pci_address);

        // Unbind from current driver if any
        if device.driver.is_some() {
            let unbind_path = format!("/sys/bus/pci/devices/{}/driver/unbind", pci_address);
            let _ = fs::write(&unbind_path, pci_address);
        }

        // Override driver to vfio-pci
        let driver_override = format!("/sys/bus/pci/devices/{}/driver_override", pci_address);
        fs::write(&driver_override, "vfio-pci")
            .map_err(|e| format!("Failed to set driver override: {}", e))?;

        // Bind to vfio-pci
        let bind_path = "/sys/bus/pci/drivers/vfio-pci/bind";
        fs::write(bind_path, pci_address)
            .map_err(|e| format!("Failed to bind to vfio-pci: {}", e))?;

        println!("✅ Device bound to vfio-pci");
        Ok(())
    }

    /// Unbind from vfio-pci driver
    pub fn unbind_from_vfio(&self, pci_address: &str) -> Result<(), String> {
        println!("Unbinding {} from vfio-pci", pci_address);

        let unbind_path = "/sys/bus/pci/drivers/vfio-pci/unbind";
        fs::write(unbind_path, pci_address)
            .map_err(|e| format!("Failed to unbind from vfio-pci: {}", e))?;

        // Clear driver override
        let driver_override = format!("/sys/bus/pci/devices/{}/driver_override", pci_address);
        let _ = fs::write(&driver_override, "\n");

        // Trigger rescan to rebind to original driver
        let rescan_path = "/sys/bus/pci/rescan";
        let _ = fs::write(rescan_path, "1");

        println!("✅ Device unbound from vfio-pci");
        Ok(())
    }

    /// Assign PCI device to VM
    pub fn assign_to_vm(&mut self, pci_address: &str, vm_name: &str) -> Result<(), String> {
        // Check device exists and current driver
        let needs_vfio_bind = {
            let device = self
                .devices
                .get(pci_address)
                .ok_or_else(|| format!("Device {} not found", pci_address))?;

            if device.assigned_to_vm.is_some() {
                return Err(format!("Device {} already assigned", pci_address));
            }

            device.driver.as_deref() != Some("vfio-pci")
        };

        // Ensure device is bound to vfio-pci
        if needs_vfio_bind {
            self.bind_to_vfio(pci_address)?;
        }

        // Now update device state
        let device = self.devices.get_mut(pci_address).unwrap();
        device.assigned_to_vm = Some(vm_name.to_string());
        self.assignments
            .insert(pci_address.to_string(), vm_name.to_string());

        println!("✅ Device {} assigned to VM '{}'", pci_address, vm_name);
        Ok(())
    }

    /// Release PCI device from VM
    pub fn release_from_vm(&mut self, pci_address: &str) -> Result<(), String> {
        let device = self
            .devices
            .get_mut(pci_address)
            .ok_or_else(|| format!("Device {} not found", pci_address))?;

        device.assigned_to_vm = None;
        self.assignments.remove(pci_address);

        println!("✅ Device {} released", pci_address);
        Ok(())
    }

    /// Generate libvirt XML for PCI passthrough
    pub fn generate_libvirt_xml(&self, pci_address: &str) -> Result<String, String> {
        let _device = self
            .devices
            .get(pci_address)
            .ok_or_else(|| format!("Device {} not found", pci_address))?;

        // Parse PCI address components
        let parts: Vec<&str> = pci_address.split(&[':', '.']).collect();
        if parts.len() != 4 {
            return Err("Invalid PCI address format".to_string());
        }

        let domain = parts[0];
        let bus = parts[1];
        let slot = parts[2];
        let function = parts[3];

        Ok(format!(
            r#"
    <hostdev mode='subsystem' type='pci' managed='yes'>
      <source>
        <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>
      </source>
    </hostdev>
"#,
            domain, bus, slot, function
        ))
    }

    /// Check if device can be safely passed through
    pub fn check_passthrough_viability(
        &self,
        pci_address: &str,
    ) -> Result<PassthroughViability, String> {
        let device = self
            .devices
            .get(pci_address)
            .ok_or_else(|| format!("Device {} not found", pci_address))?;

        let mut viability = PassthroughViability {
            viable: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        };

        // Check IOMMU group
        if device.iommu_group.is_none() {
            viability.viable = false;
            viability
                .errors
                .push("Device not in IOMMU group (IOMMU not enabled?)".to_string());
        } else {
            // Check if IOMMU group has multiple devices
            let group_devices = self.get_iommu_group_devices(device.iommu_group.unwrap());
            if group_devices.len() > 1 {
                viability.warnings.push(format!(
                    "IOMMU group {} contains {} devices - all must be passed through together",
                    device.iommu_group.unwrap(),
                    group_devices.len()
                ));
            }
        }

        // Check if device is critical for system
        if Self::is_critical_device(&device.device_class) {
            viability.warnings.push(
                "This is a critical system device - passthrough may affect host stability"
                    .to_string(),
            );
        }

        // Check driver
        if device.driver.as_deref() == Some("vfio-pci") {
            // Already bound, good
        } else if device.driver.is_some() {
            viability.warnings.push(format!(
                "Device currently bound to {} driver - will be unbound for passthrough",
                device.driver.as_ref().unwrap()
            ));
        }

        Ok(viability)
    }

    /// Get all devices in the same IOMMU group
    pub fn get_iommu_group_devices(&self, group_id: u32) -> Vec<&PciDevice> {
        self.devices
            .values()
            .filter(|d| d.iommu_group == Some(group_id))
            .collect()
    }

    /// List devices by class
    pub fn list_by_class(&self, device_class: &PciDeviceClass) -> Vec<&PciDevice> {
        self.devices
            .values()
            .filter(|d| &d.device_class == device_class)
            .collect()
    }

    /// List all devices
    pub fn list_devices(&self) -> Vec<&PciDevice> {
        self.devices.values().collect()
    }

    /// Get device by address
    pub fn get_device(&self, address: &str) -> Option<&PciDevice> {
        self.devices.get(address)
    }

    // Helper methods
    fn read_file(path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))
    }

    fn read_hex_file(path: &Path) -> Result<String, String> {
        let content = Self::read_file(path)?;
        Ok(content.trim().trim_start_matches("0x").to_string())
    }

    fn classify_pci_device(class_code: &str) -> PciDeviceClass {
        // PCI class code format: CCSSPP (Class, Subclass, Programming Interface)
        let class = &class_code[0..2];

        match class {
            "01" => PciDeviceClass::StorageController,
            "02" => PciDeviceClass::NetworkController,
            "03" => PciDeviceClass::GPU,
            "04" => PciDeviceClass::AudioDevice,
            "06" => PciDeviceClass::Bridge,
            "0c" => {
                let subclass = &class_code[2..4];
                match subclass {
                    "03" => PciDeviceClass::USBController,
                    _ => PciDeviceClass::Other(class_code.to_string()),
                }
            }
            _ => PciDeviceClass::Other(class_code.to_string()),
        }
    }

    fn is_critical_device(device_class: &PciDeviceClass) -> bool {
        matches!(
            device_class,
            PciDeviceClass::Bridge
                | PciDeviceClass::SATAController
                | PciDeviceClass::StorageController
        )
    }

    fn lookup_vendor_name(vendor_id: &str) -> String {
        match vendor_id {
            "10de" => "NVIDIA Corporation".to_string(),
            "1002" => "Advanced Micro Devices, Inc. [AMD/ATI]".to_string(),
            "8086" => "Intel Corporation".to_string(),
            "1022" => "Advanced Micro Devices, Inc. [AMD]".to_string(),
            "1b21" => "ASMedia Technology Inc.".to_string(),
            "15b3" => "Mellanox Technologies".to_string(),
            "14e4" => "Broadcom Inc.".to_string(),
            "1af4" => "Red Hat, Inc.".to_string(),
            "1912" => "Renesas Technology Corp.".to_string(),
            _ => format!("Vendor {}", vendor_id),
        }
    }

    fn lookup_device_name(vendor_id: &str, device_id: &str, class: &PciDeviceClass) -> String {
        // Simplified device name lookup
        match (vendor_id, class) {
            ("10de", PciDeviceClass::GPU) => format!("NVIDIA GPU [{}]", device_id),
            ("1002", PciDeviceClass::GPU) => format!("AMD GPU [{}]", device_id),
            ("8086", PciDeviceClass::NetworkController) => {
                format!("Intel Network Adapter [{}]", device_id)
            }
            ("8086", PciDeviceClass::StorageController) => {
                format!("Intel NVMe Controller [{}]", device_id)
            }
            ("1022", PciDeviceClass::StorageController) => {
                format!("AMD NVMe Controller [{}]", device_id)
            }
            ("1b21", PciDeviceClass::SATAController) => {
                format!("ASMedia SATA Controller [{}]", device_id)
            }
            _ => format!("{:?} [{}]", class, device_id),
        }
    }

    /// Print device information
    pub fn print_device_info(device: &PciDevice) {
        println!("PCI Device: {}", device.address);
        println!("  Vendor:Device  {}:{}", device.vendor_id, device.device_id);
        println!("  Vendor         {}", device.vendor_name);
        println!("  Device         {}", device.device_name);
        println!("  Class          {:?}", device.device_class);
        println!(
            "  IOMMU Group    {}",
            device
                .iommu_group
                .map(|g| g.to_string())
                .unwrap_or_else(|| "None".to_string())
        );
        println!(
            "  Driver         {}",
            device.driver.as_deref().unwrap_or("None")
        );
        if let Some(numa) = device.numa_node {
            println!("  NUMA Node      {}", numa);
        }
        if let Some(vm) = &device.assigned_to_vm {
            println!("  Assigned to    {}", vm);
        } else {
            println!("  Status         Available");
        }
    }
}

#[derive(Debug)]
pub struct PassthroughViability {
    pub viable: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl PassthroughViability {
    pub fn print(&self) {
        if self.viable {
            println!("✅ Device can be passed through");
        } else {
            println!("❌ Device cannot be passed through");
        }

        if !self.errors.is_empty() {
            println!("\nErrors:");
            for error in &self.errors {
                println!("  ❌ {}", error);
            }
        }

        if !self.warnings.is_empty() {
            println!("\nWarnings:");
            for warning in &self.warnings {
                println!("  ⚠️  {}", warning);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_creation() {
        let manager = PciPassthroughManager::new();
        assert_eq!(manager.list_devices().len(), 0);
    }

    #[test]
    fn test_device_classification() {
        assert_eq!(
            PciPassthroughManager::classify_pci_device("030000"),
            PciDeviceClass::GPU
        );
        assert_eq!(
            PciPassthroughManager::classify_pci_device("020000"),
            PciDeviceClass::NetworkController
        );
        assert_eq!(
            PciPassthroughManager::classify_pci_device("0c0330"),
            PciDeviceClass::USBController
        );
    }

    #[test]
    fn test_xml_generation() {
        let mut manager = PciPassthroughManager::new();

        // Create a mock device
        let device = PciDevice {
            address: "0000:01:00.0".to_string(),
            vendor_id: "10de".to_string(),
            device_id: "1234".to_string(),
            subsystem_vendor_id: "1043".to_string(),
            subsystem_device_id: "5678".to_string(),
            vendor_name: "NVIDIA".to_string(),
            device_name: "GPU".to_string(),
            device_class: PciDeviceClass::GPU,
            iommu_group: Some(1),
            driver: None,
            numa_node: None,
            assigned_to_vm: None,
            sysfs_path: PathBuf::from("/sys/bus/pci/devices/0000:01:00.0"),
        };

        manager.devices.insert("0000:01:00.0".to_string(), device);

        let xml = manager.generate_libvirt_xml("0000:01:00.0").unwrap();

        assert!(xml.contains("<hostdev"));
        assert!(xml.contains("type='pci'"));
        assert!(xml.contains("domain='0x0000'"));
        assert!(xml.contains("bus='0x01'"));
    }
}
