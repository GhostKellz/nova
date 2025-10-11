// SR-IOV (Single Root I/O Virtualization) Support
// Allows sharing of PCIe devices (GPUs, NICs) across multiple VMs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SriovDevice {
    pub pf_address: String,          // Physical Function address (e.g., "0000:01:00.0")
    pub device_type: DeviceType,
    pub vendor_id: String,
    pub device_id: String,
    pub vendor_name: String,
    pub device_name: String,
    pub max_vfs: u32,                // Maximum number of Virtual Functions
    pub current_vfs: u32,            // Currently active VFs
    pub vf_list: Vec<VirtualFunction>,
    pub driver: Option<String>,
    pub sriov_capable: bool,
    pub sriov_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    GPU,
    NetworkCard,
    Storage,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualFunction {
    pub vf_index: u32,
    pub vf_address: String,          // e.g., "0000:01:00.1"
    pub assigned_to_vm: Option<String>,
    pub driver: Option<String>,
    pub mac_address: Option<String>, // For network VFs
}

pub struct SriovManager {
    devices: HashMap<String, SriovDevice>,
    vf_assignments: HashMap<String, String>, // VF address -> VM name
}

impl SriovManager {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
            vf_assignments: HashMap::new(),
        }
    }

    /// Discover SR-IOV capable devices
    pub fn discover_sriov_devices(&mut self) -> Result<Vec<SriovDevice>, String> {
        let mut devices = Vec::new();

        // Scan /sys/bus/pci/devices for SR-IOV capable devices
        let pci_devices_path = Path::new("/sys/bus/pci/devices");

        if !pci_devices_path.exists() {
            return Err("PCI devices path not found".to_string());
        }

        for entry in fs::read_dir(pci_devices_path)
            .map_err(|e| format!("Failed to read PCI devices: {}", e))? {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let device_path = entry.path();
            let address = entry.file_name().to_string_lossy().to_string();

            // Check if device supports SR-IOV
            let sriov_totalvfs_path = device_path.join("sriov_totalvfs");
            if sriov_totalvfs_path.exists() {
                if let Ok(max_vfs_str) = fs::read_to_string(&sriov_totalvfs_path) {
                    if let Ok(max_vfs) = max_vfs_str.trim().parse::<u32>() {
                        if max_vfs > 0 {
                            // This device supports SR-IOV
                            let device = self.parse_sriov_device(&device_path, &address, max_vfs)?;
                            devices.push(device.clone());
                            self.devices.insert(address.clone(), device);
                        }
                    }
                }
            }
        }

        println!("Discovered {} SR-IOV capable devices", devices.len());
        Ok(devices)
    }

    /// Parse SR-IOV device information
    fn parse_sriov_device(&self, device_path: &Path, address: &str, max_vfs: u32)
        -> Result<SriovDevice, String> {
        // Read vendor and device IDs
        let vendor_id = Self::read_sysfs_file(&device_path.join("vendor"))?
            .trim()
            .trim_start_matches("0x")
            .to_string();

        let device_id = Self::read_sysfs_file(&device_path.join("device"))?
            .trim()
            .trim_start_matches("0x")
            .to_string();

        // Read driver
        let driver = device_path
            .join("driver")
            .read_link()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()));

        // Read current VF count
        let current_vfs = Self::read_sysfs_file(&device_path.join("sriov_numvfs"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);

        // Determine device type
        let device_type = Self::determine_device_type(&vendor_id, &device_id);

        // Get VF list if any are active
        let vf_list = if current_vfs > 0 {
            self.enumerate_virtual_functions(device_path, current_vfs)?
        } else {
            Vec::new()
        };

        let vendor_name = Self::lookup_vendor_name(&vendor_id);
        let device_name = Self::lookup_device_name(&vendor_id, &device_id);

        Ok(SriovDevice {
            pf_address: address.to_string(),
            device_type,
            vendor_id,
            device_id,
            vendor_name,
            device_name,
            max_vfs,
            current_vfs,
            vf_list,
            driver,
            sriov_capable: true,
            sriov_enabled: current_vfs > 0,
        })
    }

    /// Enable SR-IOV and create Virtual Functions
    pub fn enable_sriov(&mut self, pf_address: &str, num_vfs: u32)
        -> Result<(), String> {
        let device = self.devices.get(pf_address)
            .ok_or_else(|| format!("Device {} not found", pf_address))?;

        if num_vfs > device.max_vfs {
            return Err(format!("Requested {} VFs but device only supports {}",
                              num_vfs, device.max_vfs));
        }

        println!("Enabling SR-IOV on {} with {} VFs", pf_address, num_vfs);

        // Write to sriov_numvfs sysfs file
        let sysfs_path = format!("/sys/bus/pci/devices/{}/sriov_numvfs", pf_address);

        // First disable any existing VFs
        fs::write(&sysfs_path, "0")
            .map_err(|e| format!("Failed to disable VFs: {}. Try running with sudo.", e))?;

        // Enable requested number of VFs
        fs::write(&sysfs_path, num_vfs.to_string())
            .map_err(|e| format!("Failed to enable VFs: {}. Try running with sudo.", e))?;

        // Wait for VFs to initialize
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Refresh device state
        self.refresh_device(pf_address)?;

        println!("✅ SR-IOV enabled: {} VFs created", num_vfs);
        Ok(())
    }

    /// Disable SR-IOV (remove all VFs)
    pub fn disable_sriov(&mut self, pf_address: &str) -> Result<(), String> {
        println!("Disabling SR-IOV on {}", pf_address);

        let sysfs_path = format!("/sys/bus/pci/devices/{}/sriov_numvfs", pf_address);

        fs::write(&sysfs_path, "0")
            .map_err(|e| format!("Failed to disable VFs: {}", e))?;

        // Update device state
        if let Some(device) = self.devices.get_mut(pf_address) {
            device.current_vfs = 0;
            device.vf_list.clear();
            device.sriov_enabled = false;
        }

        println!("✅ SR-IOV disabled");
        Ok(())
    }

    /// Enumerate Virtual Functions
    fn enumerate_virtual_functions(&self, pf_path: &Path, num_vfs: u32)
        -> Result<Vec<VirtualFunction>, String> {
        let mut vfs = Vec::new();

        for vf_index in 0..num_vfs {
            let virtfn_link = pf_path.join(format!("virtfn{}", vf_index));

            if let Ok(vf_path) = virtfn_link.read_link() {
                let vf_address = vf_path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Read VF driver
                let driver = pf_path.join(format!("virtfn{}/driver", vf_index))
                    .read_link()
                    .ok()
                    .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()));

                // For network VFs, read MAC address
                let mac_address = Self::read_vf_mac_address(&pf_path, vf_index);

                vfs.push(VirtualFunction {
                    vf_index,
                    vf_address,
                    assigned_to_vm: None,
                    driver,
                    mac_address,
                });
            }
        }

        Ok(vfs)
    }

    /// Assign VF to a VM
    pub fn assign_vf_to_vm(&mut self, pf_address: &str, vf_index: u32, vm_name: &str)
        -> Result<String, String> {
        // Get VF address first
        let vf_address = {
            let device = self.devices.get(pf_address)
                .ok_or_else(|| format!("Device {} not found", pf_address))?;

            let vf = device.vf_list.iter()
                .find(|vf| vf.vf_index == vf_index)
                .ok_or_else(|| format!("VF {} not found", vf_index))?;

            if vf.assigned_to_vm.is_some() {
                return Err(format!("VF {} already assigned to VM", vf_index));
            }

            vf.vf_address.clone()
        };

        // Bind VF to vfio-pci driver for passthrough
        self.bind_vf_to_vfio(&vf_address)?;

        // Now update VF state
        let device = self.devices.get_mut(pf_address).unwrap();
        let vf = device.vf_list.iter_mut()
            .find(|vf| vf.vf_index == vf_index)
            .unwrap();

        vf.assigned_to_vm = Some(vm_name.to_string());
        self.vf_assignments.insert(vf_address.clone(), vm_name.to_string());

        println!("✅ VF {} assigned to VM '{}'", vf_address, vm_name);
        Ok(vf_address)
    }

    /// Release VF from VM
    pub fn release_vf(&mut self, vf_address: &str) -> Result<(), String> {
        // Find the VF in our devices
        for device in self.devices.values_mut() {
            if let Some(vf) = device.vf_list.iter_mut().find(|vf| vf.vf_address == vf_address) {
                vf.assigned_to_vm = None;
                self.vf_assignments.remove(vf_address);

                // Unbind from vfio-pci
                self.unbind_vf_from_vfio(vf_address)?;

                println!("✅ VF {} released", vf_address);
                return Ok(());
            }
        }

        Err(format!("VF {} not found", vf_address))
    }

    /// Bind VF to vfio-pci driver
    fn bind_vf_to_vfio(&self, vf_address: &str) -> Result<(), String> {
        let device_path = format!("/sys/bus/pci/devices/{}", vf_address);

        // Read vendor and device IDs
        let vendor = Self::read_sysfs_file(&PathBuf::from(&device_path).join("vendor"))?
            .trim()
            .trim_start_matches("0x")
            .to_string();
        let device = Self::read_sysfs_file(&PathBuf::from(&device_path).join("device"))?
            .trim()
            .trim_start_matches("0x")
            .to_string();

        // Unbind from current driver if any
        let driver_path = PathBuf::from(&device_path).join("driver/unbind");
        if driver_path.exists() {
            let _ = fs::write(&driver_path, vf_address);
        }

        // Bind to vfio-pci
        let vfio_new_id = "/sys/bus/pci/drivers/vfio-pci/new_id";
        fs::write(vfio_new_id, format!("{} {}", vendor, device))
            .map_err(|e| format!("Failed to bind to vfio-pci: {}", e))?;

        Ok(())
    }

    /// Unbind VF from vfio-pci driver
    fn unbind_vf_from_vfio(&self, vf_address: &str) -> Result<(), String> {
        let unbind_path = "/sys/bus/pci/drivers/vfio-pci/unbind";
        fs::write(unbind_path, vf_address)
            .map_err(|e| format!("Failed to unbind from vfio-pci: {}", e))?;
        Ok(())
    }

    /// Generate libvirt XML for VF passthrough
    pub fn generate_vf_xml(&self, vf_address: &str) -> String {
        // Parse PCI address (e.g., "0000:01:00.1")
        let parts: Vec<&str> = vf_address.split(&[':', '.']).collect();
        let domain = parts.get(0).unwrap_or(&"0000");
        let bus = parts.get(1).unwrap_or(&"00");
        let slot = parts.get(2).unwrap_or(&"00");
        let function = parts.get(3).unwrap_or(&"0");

        format!(r#"
    <hostdev mode='subsystem' type='pci' managed='yes'>
      <source>
        <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>
      </source>
    </hostdev>
"#, domain, bus, slot, function)
    }

    /// Refresh device state
    fn refresh_device(&mut self, pf_address: &str) -> Result<(), String> {
        let device_path = PathBuf::from(format!("/sys/bus/pci/devices/{}", pf_address));

        let max_vfs = Self::read_sysfs_file(&device_path.join("sriov_totalvfs"))?
            .trim()
            .parse::<u32>()
            .map_err(|e| format!("Failed to parse max VFs: {}", e))?;

        let updated_device = self.parse_sriov_device(&device_path, pf_address, max_vfs)?;
        self.devices.insert(pf_address.to_string(), updated_device);

        Ok(())
    }

    /// List all SR-IOV devices
    pub fn list_devices(&self) -> Vec<&SriovDevice> {
        self.devices.values().collect()
    }

    /// Get device by PF address
    pub fn get_device(&self, pf_address: &str) -> Option<&SriovDevice> {
        self.devices.get(pf_address)
    }

    /// Get VF assignments
    pub fn get_vf_assignments(&self) -> &HashMap<String, String> {
        &self.vf_assignments
    }

    // Helper methods
    fn read_sysfs_file(path: &Path) -> Result<String, String> {
        fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {:?}: {}", path, e))
    }

    fn read_vf_mac_address(_pf_path: &Path, _vf_index: u32) -> Option<String> {
        // This would read from netdev sysfs for network devices
        // Placeholder for now
        None
    }

    fn determine_device_type(vendor_id: &str, device_id: &str) -> DeviceType {
        match vendor_id {
            "10de" => DeviceType::GPU,              // NVIDIA
            "1002" => DeviceType::GPU,              // AMD
            "8086" if device_id.starts_with("15") => DeviceType::NetworkCard, // Intel NICs
            "14e4" => DeviceType::NetworkCard,      // Broadcom
            _ => DeviceType::Other,
        }
    }

    fn lookup_vendor_name(vendor_id: &str) -> String {
        match vendor_id {
            "10de" => "NVIDIA Corporation".to_string(),
            "1002" => "Advanced Micro Devices, Inc. [AMD/ATI]".to_string(),
            "8086" => "Intel Corporation".to_string(),
            "14e4" => "Broadcom Inc.".to_string(),
            "15b3" => "Mellanox Technologies".to_string(),
            _ => format!("Vendor {}", vendor_id),
        }
    }

    fn lookup_device_name(vendor_id: &str, device_id: &str) -> String {
        // Simplified lookup - in production, use pci.ids database
        match (vendor_id, &device_id[0..2]) {
            ("10de", "20" | "21" | "22" | "23" | "24" | "25") => "NVIDIA GPU (RTX Series)".to_string(),
            ("10de", _) => "NVIDIA GPU".to_string(),
            ("1002", _) => "AMD GPU".to_string(),
            ("8086", "15") => "Intel Network Adapter".to_string(),
            _ => "Unknown Device".to_string(),
        }
    }

    /// Generate SR-IOV setup instructions
    pub fn generate_setup_instructions(&self) -> String {
        r#"
# SR-IOV Setup Instructions

## 1. Enable IOMMU in BIOS/UEFI
- Intel: Enable VT-d
- AMD: Enable AMD-Vi

## 2. Enable IOMMU in kernel parameters
# For Intel:
GRUB_CMDLINE_LINUX="intel_iommu=on iommu=pt"

# For AMD:
GRUB_CMDLINE_LINUX="amd_iommu=on iommu=pt"

# Update GRUB and reboot
sudo grub-mkconfig -o /boot/grub/grub.cfg
sudo reboot

## 3. Load vfio-pci module
sudo modprobe vfio-pci

# Auto-load on boot:
echo "vfio-pci" | sudo tee /etc/modules-load.d/vfio-pci.conf

## 4. Enable SR-IOV on device
# Example: Enable 4 VFs on GPU at 0000:01:00.0
nova sriov enable 0000:01:00.0 --num-vfs 4

## 5. Assign VF to VM
nova sriov assign 0000:01:00.0 --vf 0 --vm myvm

## 6. Verify in VM
# The VF should appear as a separate GPU/NIC in the guest

## Benefits:
- Share expensive GPUs across multiple VMs
- Better resource utilization
- Lower latency than emulated devices
- Near-native performance

## Limitations:
- Not all devices support SR-IOV
- Requires hardware and driver support
- Maximum VFs limited by device (typically 8-64)
"#.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sriov_manager_creation() {
        let manager = SriovManager::new();
        assert_eq!(manager.list_devices().len(), 0);
    }

    #[test]
    fn test_device_type_detection() {
        assert!(matches!(
            SriovManager::determine_device_type("10de", "1234"),
            DeviceType::GPU
        ));

        assert!(matches!(
            SriovManager::determine_device_type("1002", "5678"),
            DeviceType::GPU
        ));

        assert!(matches!(
            SriovManager::determine_device_type("8086", "1521"),
            DeviceType::NetworkCard
        ));
    }

    #[test]
    fn test_vf_xml_generation() {
        let manager = SriovManager::new();
        let xml = manager.generate_vf_xml("0000:01:00.1");

        assert!(xml.contains("<hostdev"));
        assert!(xml.contains("type='pci'"));
        assert!(xml.contains("managed='yes'"));
        assert!(xml.contains("domain='0x0000'"));
        assert!(xml.contains("bus='0x01'"));
        assert!(xml.contains("slot='0x00'"));
        assert!(xml.contains("function='0x1'"));
    }
}
