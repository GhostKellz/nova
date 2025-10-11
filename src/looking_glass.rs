// Looking Glass Integration for Low-Latency GPU Passthrough Display
// https://looking-glass.io/

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookingGlassConfig {
    pub enabled: bool,
    pub resolution: Resolution,
    pub framebuffer_size: u64,        // In MB (default: 128MB for 4K)
    pub shmem_path: PathBuf,          // /dev/shm/looking-glass
    pub socket_path: Option<PathBuf>, // KVMFR socket
    pub audio_enabled: bool,
    pub audio_buffer_latency: u32,    // In milliseconds
    pub vsync_enabled: bool,
    pub mouse_input: MouseInputMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseInputMode {
    Relative,   // Best for gaming
    Absolute,   // Best for desktop work
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LookingGlassProfile {
    Gaming,       // Optimized for gaming: low latency, relative mouse, vsync off
    Productivity, // Optimized for productivity: absolute mouse, vsync on
    Streaming,    // Optimized for streaming/recording: balanced settings
    Custom,       // User-defined settings
}

impl LookingGlassProfile {
    pub fn to_config(&self) -> LookingGlassConfig {
        match self {
            LookingGlassProfile::Gaming => LookingGlassConfig {
                enabled: true,
                resolution: Resolution { width: 1920, height: 1080 },
                framebuffer_size: 64,
                shmem_path: PathBuf::from("/dev/shm/looking-glass"),
                socket_path: None,
                audio_enabled: true,
                audio_buffer_latency: 10, // Low latency
                vsync_enabled: false,
                mouse_input: MouseInputMode::Relative,
            },
            LookingGlassProfile::Productivity => LookingGlassConfig {
                enabled: true,
                resolution: Resolution { width: 2560, height: 1440 },
                framebuffer_size: 128,
                shmem_path: PathBuf::from("/dev/shm/looking-glass"),
                socket_path: None,
                audio_enabled: true,
                audio_buffer_latency: 20,
                vsync_enabled: true,
                mouse_input: MouseInputMode::Absolute,
            },
            LookingGlassProfile::Streaming => LookingGlassConfig {
                enabled: true,
                resolution: Resolution { width: 1920, height: 1080 },
                framebuffer_size: 64,
                shmem_path: PathBuf::from("/dev/shm/looking-glass"),
                socket_path: None,
                audio_enabled: true,
                audio_buffer_latency: 15,
                vsync_enabled: true,
                mouse_input: MouseInputMode::Relative,
            },
            LookingGlassProfile::Custom => LookingGlassConfig::default(),
        }
    }
}

impl Default for LookingGlassConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution: Resolution { width: 1920, height: 1080 },
            framebuffer_size: 64, // 64MB for 1080p, 128MB for 4K
            shmem_path: PathBuf::from("/dev/shm/looking-glass"),
            socket_path: None,
            audio_enabled: true,
            audio_buffer_latency: 13, // ~13ms latency
            vsync_enabled: true,
            mouse_input: MouseInputMode::Relative,
        }
    }
}

impl LookingGlassConfig {
    /// Calculate required framebuffer size based on resolution
    pub fn calculate_framebuffer_size(&self) -> u64 {
        // Formula: width * height * 4 bytes per pixel * 2 buffers + overhead
        let pixels = self.resolution.width as u64 * self.resolution.height as u64;
        let bytes = pixels * 4 * 2; // RGBA, double buffered
        let mb = (bytes / (1024 * 1024)) + 10; // Add 10MB overhead

        // Round up to nearest power of 2 for alignment
        mb.next_power_of_two()
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.resolution.width < 640 || self.resolution.height < 480 {
            return Err("Resolution too small (minimum 640x480)".to_string());
        }

        if self.resolution.width > 7680 || self.resolution.height > 4320 {
            return Err("Resolution too large (maximum 8K)".to_string());
        }

        if self.framebuffer_size < 32 || self.framebuffer_size > 512 {
            return Err("Framebuffer size must be between 32MB and 512MB".to_string());
        }

        Ok(())
    }
}

pub struct LookingGlassManager {
    configs: std::collections::HashMap<String, LookingGlassConfig>,
}

impl LookingGlassManager {
    pub fn new() -> Self {
        Self {
            configs: std::collections::HashMap::new(),
        }
    }

    /// Parse PCI address from format "0000:01:00.0" to components
    fn parse_pci_address(address: &str) -> Result<(u8, u8, u8), String> {
        let parts: Vec<&str> = address.split(':').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid PCI address format: {}", address));
        }

        let bus = u8::from_str_radix(parts[1], 16)
            .map_err(|_| format!("Invalid bus number: {}", parts[1]))?;

        let slot_func: Vec<&str> = parts[2].split('.').collect();
        if slot_func.len() != 2 {
            return Err(format!("Invalid slot.function format: {}", parts[2]))?;
        }

        let slot = u8::from_str_radix(slot_func[0], 16)
            .map_err(|_| format!("Invalid slot number: {}", slot_func[0]))?;

        let function = u8::from_str_radix(slot_func[1], 16)
            .map_err(|_| format!("Invalid function number: {}", slot_func[1]))?;

        Ok((bus, slot, function))
    }

    /// Check if huge pages are configured
    pub fn check_hugepages(&self) -> (bool, String) {
        let hugepages_path = Path::new("/sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages");

        if !hugepages_path.exists() {
            return (false, "Huge pages not available".to_string());
        }

        match std::fs::read_to_string(hugepages_path) {
            Ok(content) => {
                let count: u32 = content.trim().parse().unwrap_or(0);
                if count >= 512 {
                    (true, format!("{} huge pages configured", count))
                } else {
                    (false, format!("Only {} huge pages (need at least 512)", count))
                }
            }
            Err(_) => (false, "Cannot read huge pages configuration".to_string()),
        }
    }

    /// Generate instructions for Windows guest driver installation
    pub fn generate_windows_driver_instructions(&self) -> String {
        r#"
# Looking Glass Guest Driver Installation (Windows)

## 1. Download the Looking Glass Host Application
Visit: https://looking-glass.io/downloads
Download: looking-glass-host-setup.exe

## 2. Install on Windows Guest
1. Run looking-glass-host-setup.exe as Administrator
2. Follow the installation wizard
3. The IVSHMEM driver will be installed automatically

## 3. Configure the Host Application
Create: C:\Program Files\Looking Glass (host)\looking-glass-host.ini

```ini
[app]
shmFile=looking-glass

[os]
shmSize=64

[capture]
; Use DXGI for best performance
interface=dxgi

[dxgi]
; Enable these for NVIDIA GPUs
nvfbc=true
useAcquireLock=true
```

## 4. Start the Host Application
Run: "Looking Glass (host)" from Start Menu
The application will minimize to system tray

## 5. Verify Connection
- Check system tray for Looking Glass icon
- Icon should show "Capturing" status
- If not, check Event Viewer for errors

## Troubleshooting
- If IVSHMEM device not detected: Check VM XML has correct shmem device
- If capture fails: Try disabling Secure Boot in guest
- For NVIDIA: Install latest GeForce drivers
- For AMD: Ensure guest GPU is set as primary in BIOS
"#.to_string()
    }

    /// Setup huge pages for better performance
    pub async fn setup_hugepages(&self, count: u32) -> Result<(), String> {
        // Calculate required huge pages (2MB each)
        // For 128MB IVSHMEM: need 64 pages
        let required = count.max(512);

        println!("Setting up {} huge pages ({}MB)...", required, required * 2);

        // Set huge pages
        let set_cmd = Command::new("sh")
            .arg("-c")
            .arg(format!("echo {} | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages", required))
            .output()
            .map_err(|e| format!("Failed to set huge pages: {}", e))?;

        if !set_cmd.status.success() {
            return Err("Failed to configure huge pages. Run with sudo".to_string());
        }

        // Make persistent across reboots
        let persist_cmd = Command::new("sh")
            .arg("-c")
            .arg(format!("echo 'vm.nr_hugepages = {}' | sudo tee -a /etc/sysctl.d/99-hugepages.conf", required))
            .output()
            .map_err(|e| format!("Failed to make persistent: {}", e))?;

        if persist_cmd.status.success() {
            println!("✅ Huge pages configured and made persistent");
            println!("   Pages: {}, Size: {}MB", required, required * 2);
        }

        Ok(())
    }

    /// Generate libvirt XML for IVSHMEM device
    pub fn generate_ivshmem_xml(&self, config: &LookingGlassConfig) -> String {
        let size_mb = config.framebuffer_size;

        format!(r#"
    <!-- Looking Glass IVSHMEM Device -->
    <shmem name='looking-glass'>
      <model type='ivshmem-plain'/>
      <size unit='M'>{}</size>
    </shmem>
"#, size_mb)
    }

    /// Generate QEMU command line arguments for Looking Glass
    pub fn generate_qemu_args(&self, config: &LookingGlassConfig) -> Vec<String> {
        let size_mb = config.framebuffer_size;
        let shmem_path = config.shmem_path.display().to_string();

        vec![
            "-device".to_string(),
            format!("ivshmem-plain,memdev=ivshmem,bus=pcie.0"),
            "-object".to_string(),
            format!("memory-backend-file,id=ivshmem,share=on,mem-path={},size={}M",
                    shmem_path, size_mb),
        ]
    }

    /// Setup shared memory file
    pub async fn setup_shmem(&self, config: &LookingGlassConfig,
                              _vm_name: &str) -> Result<(), String> {
        let shmem_path = &config.shmem_path;
        let _size_bytes = config.framebuffer_size * 1024 * 1024;

        // Create shared memory file
        let output = Command::new("dd")
            .args(&[
                "if=/dev/zero",
                &format!("of={}", shmem_path.display()),
                "bs=1M",
                &format!("count={}", config.framebuffer_size),
            ])
            .output()
            .map_err(|e| format!("Failed to create shmem file: {}", e))?;

        if !output.status.success() {
            return Err(format!("dd failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        // Set permissions for QEMU access
        Command::new("chmod")
            .args(&["660", &shmem_path.display().to_string()])
            .output()
            .map_err(|e| format!("Failed to set permissions: {}", e))?;

        // Change ownership to libvirt-qemu user
        Command::new("chown")
            .args(&["libvirt-qemu:kvm", &shmem_path.display().to_string()])
            .output()
            .map_err(|e| format!("Failed to set ownership: {}", e))?;

        println!("✅ Looking Glass shared memory setup: {} ({}MB)",
                 shmem_path.display(), config.framebuffer_size);

        Ok(())
    }

    /// Cleanup shared memory after VM shutdown
    pub async fn cleanup_shmem(&self, config: &LookingGlassConfig) -> Result<(), String> {
        if config.shmem_path.exists() {
            fs::remove_file(&config.shmem_path)
                .await
                .map_err(|e| format!("Failed to remove shmem file: {}", e))?;
        }
        Ok(())
    }

    /// Generate Looking Glass client configuration file
    pub fn generate_client_config(&self, config: &LookingGlassConfig,
                                   vm_name: &str) -> String {
        format!(r#"
# Looking Glass Client Configuration for {}

[app]
shmFile={}
renderer=opengl

[win]
size={}x{}
fullScreen=no
jitRender=yes
keepAspect=yes

[input]
rawMouse={}
autoCapture=yes
captureOnFocus=yes
releaseKeysOnFocusLoss=yes

[spice]
enable={}
host=127.0.0.1
port=5900
audio={}

[egl]
vsync={}
doubleBuffer=yes

[opengl]
vsync={}
"#,
            vm_name,
            config.shmem_path.display(),
            config.resolution.width,
            config.resolution.height,
            matches!(config.mouse_input, MouseInputMode::Relative),
            config.audio_enabled,
            config.audio_enabled,
            config.vsync_enabled,
            config.vsync_enabled,
        )
    }

    /// Check if Looking Glass client is installed
    pub fn check_client_installed(&self) -> bool {
        Command::new("which")
            .arg("looking-glass-client")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Install Looking Glass client (Arch Linux)
    pub async fn install_client_arch(&self) -> Result<(), String> {
        println!("Installing Looking Glass from AUR...");

        let output = Command::new("yay")
            .args(&["-S", "--noconfirm", "looking-glass"])
            .output()
            .map_err(|e| format!("Failed to install: {}", e))?;

        if output.status.success() {
            println!("✅ Looking Glass client installed successfully");
            Ok(())
        } else {
            Err(format!("Installation failed: {}",
                       String::from_utf8_lossy(&output.stderr)))
        }
    }

    /// Launch Looking Glass client
    pub async fn launch_client(&self, config: &LookingGlassConfig) -> Result<(), String> {
        if !self.check_client_installed() {
            return Err("Looking Glass client not installed. Run: yay -S looking-glass".to_string());
        }

        let mut cmd = Command::new("looking-glass-client");

        // Add configuration arguments
        cmd.arg("-f").arg(config.shmem_path.display().to_string());
        cmd.arg("-m").arg(format!("{}x{}", config.resolution.width, config.resolution.height));

        if config.vsync_enabled {
            cmd.arg("--opengl-vsync");
        }

        match config.mouse_input {
            MouseInputMode::Relative => cmd.arg("--input-rawMouse"),
            MouseInputMode::Absolute => &mut cmd,
        };

        if config.audio_enabled {
            cmd.arg("--spice-audio");
        }

        // Launch client in background
        cmd.spawn()
            .map_err(|e| format!("Failed to launch client: {}", e))?;

        println!("✅ Looking Glass client launched");
        Ok(())
    }

    /// Setup KVMFR kernel module (if available)
    pub async fn setup_kvmfr_module(&self) -> Result<(), String> {
        // Check if kvmfr module is available
        let module_check = Command::new("modprobe")
            .args(&["-n", "kvmfr"])
            .output()
            .map_err(|e| format!("Failed to check module: {}", e))?;

        if !module_check.status.success() {
            return Err("KVMFR module not available. Install looking-glass-module-dkms".to_string());
        }

        // Load module
        let load = Command::new("modprobe")
            .arg("kvmfr")
            .output()
            .map_err(|e| format!("Failed to load module: {}", e))?;

        if !load.status.success() {
            return Err(format!("Failed to load kvmfr: {}",
                              String::from_utf8_lossy(&load.stderr)));
        }

        println!("✅ KVMFR module loaded");
        Ok(())
    }

    /// Generate complete VM configuration with Looking Glass
    pub fn generate_complete_vm_config(&self, config: &LookingGlassConfig,
                                       vm_name: &str,
                                       gpu_address: &str) -> Result<String, String> {
        // Parse GPU address
        let (bus, slot, function) = Self::parse_pci_address(gpu_address)?;

        Ok(format!(r#"
<!-- Complete VM Configuration with Looking Glass -->
<domain type='kvm'>
  <name>{}</name>
  <memory unit='GiB'>16</memory>
  <vcpu placement='static'>8</vcpu>

  <os>
    <type arch='x86_64' machine='q35'>hvm</type>
    <loader readonly='yes' type='pflash'>/usr/share/edk2-ovmf/x64/OVMF_CODE.fd</loader>
    <nvram>/var/lib/libvirt/qemu/nvram/{}_VARS.fd</nvram>
  </os>

  <features>
    <acpi/>
    <apic/>
    <hyperv>
      <relaxed state='on'/>
      <vapic state='on'/>
      <spinlocks state='on' retries='8191'/>
      <vendor_id state='on' value='1234567890ab'/>
    </hyperv>
    <kvm>
      <hidden state='on'/>
    </kvm>
    <vmport state='off'/>
    <ioapic driver='kvm'/>
  </features>

  <cpu mode='host-passthrough' check='none' migratable='on'>
    <topology sockets='1' dies='1' cores='4' threads='2'/>
    <cache mode='passthrough'/>
    <feature policy='require' name='topoext'/>
  </cpu>

  <clock offset='localtime'>
    <timer name='rtc' tickpolicy='catchup'/>
    <timer name='pit' tickpolicy='delay'/>
    <timer name='hpet' present='no'/>
    <timer name='hypervclock' present='yes'/>
  </clock>

  <devices>
    <!-- GPU Passthrough -->
    <hostdev mode='subsystem' type='pci' managed='yes'>
      <source>
        <address domain='0x0000' bus='0x{:02x}' slot='0x{:02x}' function='0x{:x}'/>
      </source>
      <address type='pci' domain='0x0000' bus='0x05' slot='0x00' function='0x0'/>
    </hostdev>

    {}

    <!-- Virtio devices for performance -->
    <controller type='scsi' index='0' model='virtio-scsi'>
      <driver queues='8'/>
    </controller>

    <interface type='network'>
      <source network='default'/>
      <model type='virtio'/>
      <driver name='vhost' queues='8'/>
    </interface>

    <!-- Audio -->
    <sound model='ich9'>
      <codec type='micro'/>
      <audio id='1'/>
    </sound>
    <audio id='1' type='pulseaudio' serverName='/run/user/1000/pulse/native'/>

    <!-- SPICE for audio/USB (optional with Looking Glass) -->
    <graphics type='spice' autoport='yes'>
      <listen type='address'/>
      <image compression='off'/>
      <streaming mode='off'/>
    </graphics>

    <!-- Virtio input devices -->
    <input type='mouse' bus='virtio'/>
    <input type='keyboard' bus='virtio'/>

    <!-- Virtio video (for Looking Glass) -->
    <video>
      <model type='none'/>
    </video>
  </devices>
</domain>
"#,
            vm_name,
            vm_name,
            bus, slot, function,
            self.generate_ivshmem_xml(config),
        ))
    }

    /// Validate system requirements for Looking Glass
    pub fn check_system_requirements(&self) -> SystemRequirements {
        let mut reqs = SystemRequirements::default();

        // Check IOMMU
        reqs.iommu_enabled = Path::new("/sys/kernel/iommu_groups").exists();

        // Check KVM
        reqs.kvm_available = Path::new("/dev/kvm").exists();

        // Check libvirt
        reqs.libvirt_installed = Command::new("virsh")
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Check Looking Glass client
        reqs.lg_client_installed = self.check_client_installed();

        // Check KVMFR module
        reqs.kvmfr_available = Command::new("modprobe")
            .args(&["-n", "kvmfr"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Check shared memory
        reqs.shmem_available = Path::new("/dev/shm").exists();

        // Overall status
        reqs.ready = reqs.iommu_enabled && reqs.kvm_available &&
                     reqs.libvirt_installed && reqs.shmem_available;

        reqs
    }

    /// Register Looking Glass configuration for a VM
    pub fn register_config(&mut self, vm_name: String, config: LookingGlassConfig) {
        self.configs.insert(vm_name, config);
    }

    /// Get Looking Glass configuration for a VM
    pub fn get_config(&self, vm_name: &str) -> Option<&LookingGlassConfig> {
        self.configs.get(vm_name)
    }

    /// Generate installation instructions for Arch Linux
    pub fn generate_arch_install_instructions(&self) -> String {
        r#"
# Looking Glass Setup for Arch Linux

## 1. Install Looking Glass
yay -S looking-glass
yay -S looking-glass-module-dkms  # Optional: KVMFR kernel module

## 2. Load KVMFR module (if installed)
sudo modprobe kvmfr
sudo sh -c "echo 'kvmfr' > /etc/modules-load.d/kvmfr.conf"

## 3. Setup udev rules for shared memory
sudo tee /etc/udev/rules.d/99-kvmfr.rules << EOF
SUBSYSTEM=="kvmfr", OWNER="libvirt-qemu", GROUP="kvm", MODE="0660"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger

## 4. Add your user to KVM group
sudo usermod -aG kvm $USER
sudo usermod -aG libvirt $USER

## 5. Reboot to apply changes
sudo reboot

## 6. Launch Looking Glass client
looking-glass-client -f /dev/shm/looking-glass

## For best performance:
# - Use "performance" CPU governor
# - Disable compositor (for X11)
# - Use dedicated NVIDIA GPU for guest
# - Enable huge pages in host
"#.to_string()
    }
}

#[derive(Debug, Default)]
pub struct SystemRequirements {
    pub iommu_enabled: bool,
    pub kvm_available: bool,
    pub libvirt_installed: bool,
    pub lg_client_installed: bool,
    pub kvmfr_available: bool,
    pub shmem_available: bool,
    pub ready: bool,
}

impl SystemRequirements {
    pub fn print_status(&self) {
        println!("\n=== Looking Glass System Requirements ===");
        println!("IOMMU Enabled:           {}", if self.iommu_enabled { "✅" } else { "❌" });
        println!("KVM Available:           {}", if self.kvm_available { "✅" } else { "❌" });
        println!("Libvirt Installed:       {}", if self.libvirt_installed { "✅" } else { "❌" });
        println!("LG Client Installed:     {}", if self.lg_client_installed { "✅" } else { "❌" });
        println!("KVMFR Module Available:  {}", if self.kvmfr_available { "✅" } else { "⚠️  Optional" });
        println!("Shared Memory Available: {}", if self.shmem_available { "✅" } else { "❌" });
        println!("\nOverall Status:          {}", if self.ready { "✅ READY" } else { "❌ NOT READY" });

        if !self.ready {
            println!("\n⚠️  Some requirements are missing. Run:");
            println!("   nova looking-glass setup");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framebuffer_calculation() {
        let config = LookingGlassConfig {
            resolution: Resolution { width: 1920, height: 1080 },
            ..Default::default()
        };

        let size = config.calculate_framebuffer_size();
        assert!(size >= 32, "1080p should need at least 32MB");
        assert!(size <= 128, "1080p shouldn't need more than 128MB");
    }

    #[test]
    fn test_4k_framebuffer_calculation() {
        let config = LookingGlassConfig {
            resolution: Resolution { width: 3840, height: 2160 },
            ..Default::default()
        };

        let size = config.calculate_framebuffer_size();
        assert!(size >= 128, "4K should need at least 128MB");
    }

    #[test]
    fn test_config_validation() {
        let config = LookingGlassConfig::default();
        assert!(config.validate().is_ok());

        let invalid_config = LookingGlassConfig {
            resolution: Resolution { width: 100, height: 100 },
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_ivshmem_xml_generation() {
        let manager = LookingGlassManager::new();
        let config = LookingGlassConfig::default();

        let xml = manager.generate_ivshmem_xml(&config);
        assert!(xml.contains("<shmem name='looking-glass'>"));
        assert!(xml.contains("<model type='ivshmem-plain'/>"));
    }
}
