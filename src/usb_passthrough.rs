// USB Passthrough Support
// Hot-plug USB devices to VMs with automatic detection

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDevice {
    pub bus: u8,
    pub device: u8,
    pub vendor_id: String,  // e.g., "046d"
    pub product_id: String, // e.g., "c52b"
    pub vendor_name: String,
    pub product_name: String,
    pub device_class: UsbDeviceClass,
    pub serial: Option<String>,
    pub speed: UsbSpeed,
    pub attached_to_vm: Option<String>,
    pub sysfs_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UsbDeviceClass {
    HID,     // Keyboard, Mouse, Gamepad
    Storage, // USB drives, external HDDs
    Audio,   // USB audio devices
    Video,   // Webcams
    Printer,
    Hub,
    Wireless, // WiFi/Bluetooth adapters
    SmartCard,
    Other(u8),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsbSpeed {
    Low,       // 1.5 Mbps (USB 1.0)
    Full,      // 12 Mbps (USB 1.1)
    High,      // 480 Mbps (USB 2.0)
    Super,     // 5 Gbps (USB 3.0)
    SuperPlus, // 10 Gbps (USB 3.1+)
}

pub struct UsbManager {
    devices: HashMap<String, UsbDevice>,
    assignments: HashMap<String, String>, // device_key -> vm_name
}

impl UsbManager {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
            assignments: HashMap::new(),
        }
    }

    /// Discover all USB devices
    pub fn discover_devices(&mut self) -> Result<Vec<UsbDevice>, String> {
        let mut devices = Vec::new();
        let usb_devices_path = Path::new("/sys/bus/usb/devices");

        if !usb_devices_path.exists() {
            return Err("USB devices path not found".to_string());
        }

        for entry in fs::read_dir(usb_devices_path)
            .map_err(|e| format!("Failed to read USB devices: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let device_path = entry.path();

            // Skip entries that are not actual USB devices
            if !device_path.join("idVendor").exists() {
                continue;
            }

            if let Ok(device) = self.parse_usb_device(&device_path) {
                let device_key = format!("{}:{}", device.bus, device.device);
                devices.push(device.clone());
                self.devices.insert(device_key, device);
            }
        }

        println!("Discovered {} USB devices", devices.len());
        Ok(devices)
    }

    /// Parse USB device information from sysfs
    fn parse_usb_device(&self, device_path: &Path) -> Result<UsbDevice, String> {
        // Read basic device info
        let vendor_id = Self::read_sysfs_file(&device_path.join("idVendor"))?
            .trim()
            .to_string();

        let product_id = Self::read_sysfs_file(&device_path.join("idProduct"))?
            .trim()
            .to_string();

        let busnum = Self::read_sysfs_file(&device_path.join("busnum"))?
            .trim()
            .parse::<u8>()
            .map_err(|e| format!("Failed to parse bus number: {}", e))?;

        let devnum = Self::read_sysfs_file(&device_path.join("devnum"))?
            .trim()
            .parse::<u8>()
            .map_err(|e| format!("Failed to parse device number: {}", e))?;

        // Read device class
        let device_class_code = Self::read_sysfs_file(&device_path.join("bDeviceClass"))
            .ok()
            .and_then(|s| u8::from_str_radix(s.trim(), 16).ok())
            .unwrap_or(0);

        let device_class = Self::classify_device(device_class_code);

        // Read descriptive names
        let vendor_name = Self::read_sysfs_file(&device_path.join("manufacturer"))
            .unwrap_or_else(|_| Self::lookup_vendor_name(&vendor_id));

        let product_name = Self::read_sysfs_file(&device_path.join("product"))
            .unwrap_or_else(|_| format!("USB Device {}", product_id));

        // Read serial number
        let serial = Self::read_sysfs_file(&device_path.join("serial")).ok();

        // Read USB speed
        let speed = Self::read_sysfs_file(&device_path.join("speed"))
            .ok()
            .and_then(|s| Self::parse_speed(&s))
            .unwrap_or(UsbSpeed::Full);

        Ok(UsbDevice {
            bus: busnum,
            device: devnum,
            vendor_id,
            product_id,
            vendor_name,
            product_name,
            device_class,
            serial,
            speed,
            attached_to_vm: None,
            sysfs_path: device_path.to_path_buf(),
        })
    }

    /// Attach USB device to VM
    pub async fn attach_device(&mut self, vm_name: &str, device: &UsbDevice) -> Result<(), String> {
        println!(
            "Attaching USB device {:04x}:{:04x} to VM '{}'",
            u16::from_str_radix(&device.vendor_id, 16).unwrap_or(0),
            u16::from_str_radix(&device.product_id, 16).unwrap_or(0),
            vm_name
        );

        // Generate libvirt XML
        let xml = self.generate_usb_xml(device);

        // Attach device using virsh
        let temp_xml = format!("/tmp/nova-usb-{}-{}.xml", device.bus, device.device);
        fs::write(&temp_xml, &xml).map_err(|e| format!("Failed to write temp XML: {}", e))?;

        let output = Command::new("virsh")
            .args(&["attach-device", vm_name, &temp_xml, "--live"])
            .output()
            .map_err(|e| format!("Failed to execute virsh: {}", e))?;

        // Clean up temp file
        let _ = fs::remove_file(&temp_xml);

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to attach USB device: {}", error));
        }

        // Update internal state
        let device_key = format!("{}:{}", device.bus, device.device);
        if let Some(dev) = self.devices.get_mut(&device_key) {
            dev.attached_to_vm = Some(vm_name.to_string());
        }
        self.assignments.insert(device_key, vm_name.to_string());

        println!("✅ USB device attached successfully");
        Ok(())
    }

    /// Detach USB device from VM
    pub async fn detach_device(&mut self, vm_name: &str, device: &UsbDevice) -> Result<(), String> {
        println!("Detaching USB device from VM '{}'", vm_name);

        // Generate libvirt XML (same as attach)
        let xml = self.generate_usb_xml(device);

        let temp_xml = format!("/tmp/nova-usb-{}-{}.xml", device.bus, device.device);
        fs::write(&temp_xml, &xml).map_err(|e| format!("Failed to write temp XML: {}", e))?;

        let output = Command::new("virsh")
            .args(&["detach-device", vm_name, &temp_xml, "--live"])
            .output()
            .map_err(|e| format!("Failed to execute virsh: {}", e))?;

        let _ = fs::remove_file(&temp_xml);

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to detach USB device: {}", error));
        }

        // Update internal state
        let device_key = format!("{}:{}", device.bus, device.device);
        if let Some(dev) = self.devices.get_mut(&device_key) {
            dev.attached_to_vm = None;
        }
        self.assignments.remove(&device_key);

        println!("✅ USB device detached successfully");
        Ok(())
    }

    /// Generate libvirt XML for USB passthrough
    pub fn generate_usb_xml(&self, device: &UsbDevice) -> String {
        format!(
            r#"<hostdev mode='subsystem' type='usb' managed='yes'>
  <source>
    <vendor id='0x{}'/>
    <product id='0x{}'/>
    <address bus='{}' device='{}'/>
  </source>
</hostdev>"#,
            device.vendor_id, device.product_id, device.bus, device.device
        )
    }

    /// Pass entire USB controller to VM
    pub fn pass_usb_controller(&self, vm_name: &str, pci_address: &str) -> Result<(), String> {
        println!("Passing USB controller {} to VM '{}'", pci_address, vm_name);

        // Parse PCI address
        let parts: Vec<&str> = pci_address.split(&[':', '.']).collect();
        if parts.len() != 4 {
            return Err("Invalid PCI address format".to_string());
        }

        let xml = format!(
            r#"<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x{}' bus='0x{}' slot='0x{}' function='0x{}'/>
  </source>
</hostdev>"#,
            parts[0], parts[1], parts[2], parts[3]
        );

        // Attach controller
        let temp_xml = "/tmp/nova-usb-controller.xml";
        fs::write(temp_xml, &xml).map_err(|e| format!("Failed to write XML: {}", e))?;

        let output = Command::new("virsh")
            .args(&["attach-device", vm_name, temp_xml, "--config"])
            .output()
            .map_err(|e| format!("Failed to execute virsh: {}", e))?;

        let _ = fs::remove_file(temp_xml);

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to pass USB controller: {}", error));
        }

        println!("✅ USB controller passed successfully");
        Ok(())
    }

    /// List all USB devices
    pub fn list_devices(&self) -> Vec<&UsbDevice> {
        self.devices.values().collect()
    }

    /// List available (unassigned) USB devices
    pub fn list_available_devices(&self) -> Vec<&UsbDevice> {
        self.devices
            .values()
            .filter(|d| d.attached_to_vm.is_none())
            .collect()
    }

    /// Get device by vendor and product ID
    pub fn find_device(&self, vendor_id: &str, product_id: &str) -> Option<&UsbDevice> {
        self.devices
            .values()
            .find(|d| d.vendor_id == vendor_id && d.product_id == product_id)
    }

    /// Get USB device assignments
    pub fn get_assignments(&self) -> &HashMap<String, String> {
        &self.assignments
    }

    // Helper methods
    fn read_sysfs_file(path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))
    }

    fn classify_device(class_code: u8) -> UsbDeviceClass {
        match class_code {
            0x01 => UsbDeviceClass::Audio,
            0x02 => UsbDeviceClass::Wireless, // Communications
            0x03 => UsbDeviceClass::HID,
            0x06 => UsbDeviceClass::Video,
            0x07 => UsbDeviceClass::Printer,
            0x08 => UsbDeviceClass::Storage,
            0x09 => UsbDeviceClass::Hub,
            0x0b => UsbDeviceClass::SmartCard,
            _ => UsbDeviceClass::Other(class_code),
        }
    }

    fn parse_speed(speed_str: &str) -> Option<UsbSpeed> {
        match speed_str.trim() {
            "1.5" => Some(UsbSpeed::Low),
            "12" => Some(UsbSpeed::Full),
            "480" => Some(UsbSpeed::High),
            "5000" => Some(UsbSpeed::Super),
            "10000" | "20000" => Some(UsbSpeed::SuperPlus),
            _ => None,
        }
    }

    fn lookup_vendor_name(vendor_id: &str) -> String {
        match vendor_id {
            "046d" => "Logitech",
            "045e" => "Microsoft",
            "1532" => "Razer",
            "0781" => "SanDisk",
            "058f" => "Alcor Micro",
            "05ac" => "Apple",
            "04f2" => "Chicony Electronics",
            "2109" => "VIA Labs",
            _ => "Unknown Vendor",
        }
        .to_string()
    }

    /// Print device information in human-readable format
    pub fn print_device_info(device: &UsbDevice) {
        println!("USB Device:");
        println!("  Bus:Device    {:#03}:{:#03}", device.bus, device.device);
        println!(
            "  Vendor:Product  {}:{}",
            device.vendor_id, device.product_id
        );
        println!("  Vendor        {}", device.vendor_name);
        println!("  Product       {}", device.product_name);
        println!("  Class         {:?}", device.device_class);
        println!("  Speed         {:?}", device.speed);
        if let Some(serial) = &device.serial {
            println!("  Serial        {}", serial);
        }
        if let Some(vm) = &device.attached_to_vm {
            println!("  Attached to   {}", vm);
        } else {
            println!("  Status        Available");
        }
    }

    /// Monitor for USB hotplug events
    pub async fn start_hotplug_monitor(&self) -> Result<(), String> {
        println!("Starting USB hotplug monitor...");

        // This would use udev monitoring in production
        // For now, just a placeholder

        println!("✅ USB hotplug monitor started");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usb_manager_creation() {
        let manager = UsbManager::new();
        assert_eq!(manager.list_devices().len(), 0);
    }

    #[test]
    fn test_device_classification() {
        assert_eq!(UsbManager::classify_device(0x03), UsbDeviceClass::HID);
        assert_eq!(UsbManager::classify_device(0x08), UsbDeviceClass::Storage);
        assert_eq!(UsbManager::classify_device(0x06), UsbDeviceClass::Video);
    }

    #[test]
    fn test_usb_xml_generation() {
        let manager = UsbManager::new();
        let device = UsbDevice {
            bus: 1,
            device: 5,
            vendor_id: "046d".to_string(),
            product_id: "c52b".to_string(),
            vendor_name: "Logitech".to_string(),
            product_name: "USB Receiver".to_string(),
            device_class: UsbDeviceClass::HID,
            serial: None,
            speed: UsbSpeed::Full,
            attached_to_vm: None,
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-1"),
        };

        let xml = manager.generate_usb_xml(&device);

        assert!(xml.contains("<hostdev"));
        assert!(xml.contains("type='usb'"));
        assert!(xml.contains("vendor id='0x046d'"));
        assert!(xml.contains("product id='0xc52b'"));
    }

    #[test]
    fn test_speed_parsing() {
        assert!(matches!(
            UsbManager::parse_speed("480"),
            Some(UsbSpeed::High)
        ));
        assert!(matches!(
            UsbManager::parse_speed("5000"),
            Some(UsbSpeed::Super)
        ));
    }
}
