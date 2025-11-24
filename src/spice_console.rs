use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpiceConfig {
    pub enabled: bool,
    pub listen_address: IpAddr,
    pub port: u16,
    pub tls_port: Option<u16>,
    pub password: Option<String>,
    pub autoport: bool,

    // Graphics features
    pub monitors: u32,
    pub opengl: bool,
    pub qxl_vram_mb: u32, // QXL video RAM (MB)
    pub qxl_ram_mb: u32,  // QXL RAM (MB)

    // I/O features
    pub audio: bool,
    pub clipboard_sharing: bool,
    pub file_transfer: bool,
    pub usb_redirection: bool,
    pub usb_redirector_count: u32,

    // Security
    pub tls_enabled: bool,
    pub sasl_enabled: bool,
    pub disable_ticketing: bool,

    // Performance
    pub image_compression: ImageCompression,
    pub jpeg_compression: JpegCompression,
    pub zlib_compression: ZlibCompression,
    pub streaming_mode: StreamingMode,
    pub playback_compression: bool,

    // Client configuration
    pub client_config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageCompression {
    Auto,
    Off,
    AutoGlz,
    AutoLz,
    Quic,
    Glz,
    Lz,
    Lz4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JpegCompression {
    Auto,
    Never,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZlibCompression {
    Auto,
    Never,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamingMode {
    Filter,
    All,
    Off,
}

impl Default for SpiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 5900,
            tls_port: None,
            password: None,
            autoport: true,

            monitors: 1,
            opengl: false,
            qxl_vram_mb: 64,
            qxl_ram_mb: 64,

            audio: true,
            clipboard_sharing: true,
            file_transfer: true,
            usb_redirection: true,
            usb_redirector_count: 4,

            tls_enabled: false,
            sasl_enabled: false,
            disable_ticketing: false,

            image_compression: ImageCompression::Auto,
            jpeg_compression: JpegCompression::Auto,
            zlib_compression: ZlibCompression::Auto,
            streaming_mode: StreamingMode::Filter,
            playback_compression: true,

            client_config_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpiceConnectionInfo {
    pub vm_name: String,
    pub host: String,
    pub port: u16,
    pub tls_port: Option<u16>,
    pub password: Option<String>,
    pub uri: String,
}

pub struct SpiceManager {
    configs: HashMap<String, SpiceConfig>,
    active_connections: HashMap<String, SpiceConnectionInfo>,
}

impl SpiceManager {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            active_connections: HashMap::new(),
        }
    }

    /// Set SPICE configuration for a VM
    pub fn set_config(&mut self, vm_name: &str, config: SpiceConfig) {
        self.configs.insert(vm_name.to_string(), config);
    }

    /// Get SPICE configuration for a VM
    pub fn get_config(&self, vm_name: &str) -> Option<&SpiceConfig> {
        self.configs.get(vm_name)
    }

    /// Generate libvirt XML for SPICE graphics
    pub fn generate_graphics_xml(&self, vm_name: &str) -> Result<String, String> {
        let config = self
            .configs
            .get(vm_name)
            .ok_or_else(|| format!("No SPICE config for VM '{}'", vm_name))?;

        if !config.enabled {
            return Ok(String::new());
        }

        let mut xml = String::new();

        // Graphics device
        xml.push_str("  <graphics type='spice'");

        if config.autoport {
            xml.push_str(" autoport='yes'");
        } else {
            xml.push_str(&format!(" port='{}'", config.port));
            if let Some(tls_port) = config.tls_port {
                xml.push_str(&format!(" tlsPort='{}'", tls_port));
            }
        }

        xml.push_str(&format!(" listen='{}'", config.listen_address));

        if let Some(password) = &config.password {
            xml.push_str(&format!(" passwd='{}'", password));
        }

        xml.push_str(">\n");

        // Listen element
        xml.push_str(&format!(
            "    <listen type='address' address='{}'/>\n",
            config.listen_address
        ));

        // Image compression
        let compression_str = match config.image_compression {
            ImageCompression::Auto => "auto_glz",
            ImageCompression::Off => "off",
            ImageCompression::AutoGlz => "auto_glz",
            ImageCompression::AutoLz => "auto_lz",
            ImageCompression::Quic => "quic",
            ImageCompression::Glz => "glz",
            ImageCompression::Lz => "lz",
            ImageCompression::Lz4 => "lz4",
        };
        xml.push_str(&format!("    <image compression='{}'/>\n", compression_str));

        // JPEG compression
        let jpeg_str = match config.jpeg_compression {
            JpegCompression::Auto => "auto",
            JpegCompression::Never => "never",
            JpegCompression::Always => "always",
        };
        xml.push_str(&format!("    <jpeg compression='{}'/>\n", jpeg_str));

        // Zlib compression
        let zlib_str = match config.zlib_compression {
            ZlibCompression::Auto => "auto",
            ZlibCompression::Never => "never",
            ZlibCompression::Always => "always",
        };
        xml.push_str(&format!("    <zlib compression='{}'/>\n", zlib_str));

        // Streaming mode
        let streaming_str = match config.streaming_mode {
            StreamingMode::Filter => "filter",
            StreamingMode::All => "all",
            StreamingMode::Off => "off",
        };
        xml.push_str(&format!("    <streaming mode='{}'/>\n", streaming_str));

        // Clipboard sharing
        xml.push_str(&format!(
            "    <clipboard copypaste='{}'/>\n",
            if config.clipboard_sharing {
                "yes"
            } else {
                "no"
            }
        ));

        // File transfer
        xml.push_str(&format!(
            "    <filetransfer enable='{}'/>\n",
            if config.file_transfer { "yes" } else { "no" }
        ));

        // OpenGL
        if config.opengl {
            xml.push_str("    <gl enable='yes'/>\n");
        }

        xml.push_str("  </graphics>\n");

        // Video device (QXL)
        xml.push_str("  <video>\n");
        xml.push_str("    <model type='qxl'");
        xml.push_str(&format!(" ram='{}'", config.qxl_ram_mb * 1024));
        xml.push_str(&format!(" vram='{}'", config.qxl_vram_mb * 1024));
        xml.push_str(&format!(" heads='{}'", config.monitors));
        xml.push_str("/>\n");
        xml.push_str("  </video>\n");

        // Audio device
        if config.audio {
            xml.push_str("  <sound model='ich9'>\n");
            xml.push_str("    <codec type='micro'/>\n");
            xml.push_str("  </sound>\n");
        }

        // USB redirection channels
        if config.usb_redirection {
            for i in 0..config.usb_redirector_count {
                xml.push_str(&format!("  <redirdev bus='usb' type='spicevmc'>\n"));
                xml.push_str(&format!(
                    "    <address type='usb' bus='0' port='{}'/>\n",
                    i + 1
                ));
                xml.push_str("  </redirdev>\n");
            }

            // USB controller for redirection
            xml.push_str("  <controller type='usb' model='ich9-ehci1'/>\n");
        }

        // Spice channel for agent communication
        xml.push_str("  <channel type='spicevmc'>\n");
        xml.push_str("    <target type='virtio' name='com.redhat.spice.0'/>\n");
        xml.push_str("  </channel>\n");

        Ok(xml)
    }

    /// Apply SPICE configuration to a running VM
    pub async fn apply_config(&mut self, vm_name: &str) -> Result<(), String> {
        let xml = self.generate_graphics_xml(vm_name)?;

        // Write XML to temp file
        let temp_file = format!("/tmp/spice-{}.xml", vm_name);
        std::fs::write(&temp_file, xml).map_err(|e| format!("Failed to write XML: {}", e))?;

        // Update VM definition
        let output = Command::new("virsh")
            .args(&["define", &temp_file])
            .output()
            .map_err(|e| format!("Failed to execute virsh: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "virsh define failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_file);

        Ok(())
    }

    /// Get SPICE connection info for a running VM
    pub async fn get_connection_info(
        &mut self,
        vm_name: &str,
    ) -> Result<SpiceConnectionInfo, String> {
        // Get SPICE port from virsh
        let output = Command::new("virsh")
            .args(&["domdisplay", "--type", "spice", vm_name])
            .output()
            .map_err(|e| format!("Failed to execute virsh: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to get SPICE info: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let uri = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if uri.is_empty() {
            return Err("VM does not have SPICE graphics enabled".to_string());
        }

        // Parse URI (format: spice://host:port)
        let parts: Vec<&str> = uri.split("://").collect();
        if parts.len() != 2 {
            return Err(format!("Invalid SPICE URI: {}", uri));
        }

        let host_port: Vec<&str> = parts[1].split(':').collect();
        let host = host_port[0].to_string();
        let port = if host_port.len() > 1 {
            host_port[1]
                .parse::<u16>()
                .map_err(|_| format!("Invalid port in URI: {}", uri))?
        } else {
            5900
        };

        let config = self.configs.get(vm_name);

        let info = SpiceConnectionInfo {
            vm_name: vm_name.to_string(),
            host,
            port,
            tls_port: config.and_then(|c| c.tls_port),
            password: config.and_then(|c| c.password.clone()),
            uri,
        };

        self.active_connections
            .insert(vm_name.to_string(), info.clone());

        Ok(info)
    }

    /// Launch SPICE client (remote-viewer)
    pub async fn launch_client(&self, vm_name: &str) -> Result<(), String> {
        let info = self
            .active_connections
            .get(vm_name)
            .ok_or_else(|| format!("No active SPICE connection for VM '{}'", vm_name))?;

        // Check if remote-viewer is installed
        if !self.is_client_installed() {
            return Err("SPICE client (remote-viewer) not installed. Install with: sudo pacman -S virt-viewer".to_string());
        }

        // Generate .vv file for remote-viewer
        let vv_content = self.generate_vv_file(info)?;
        let vv_path = format!("/tmp/{}.vv", vm_name);
        std::fs::write(&vv_path, vv_content)
            .map_err(|e| format!("Failed to write .vv file: {}", e))?;

        // Launch remote-viewer in background
        Command::new("remote-viewer")
            .arg(&vv_path)
            .spawn()
            .map_err(|e| format!("Failed to launch remote-viewer: {}", e))?;

        Ok(())
    }

    /// Generate .vv file for remote-viewer
    fn generate_vv_file(&self, info: &SpiceConnectionInfo) -> Result<String, String> {
        let mut content = String::new();
        content.push_str("[virt-viewer]\n");
        content.push_str("type=spice\n");
        content.push_str(&format!("host={}\n", info.host));
        content.push_str(&format!("port={}\n", info.port));

        if let Some(tls_port) = info.tls_port {
            content.push_str(&format!("tls-port={}\n", tls_port));
        }

        if let Some(password) = &info.password {
            content.push_str(&format!("password={}\n", password));
        }

        // Additional options
        content.push_str("fullscreen=0\n");
        content.push_str("title=Nova - {}\n");
        content.push_str("enable-smartcard=1\n");
        content.push_str("enable-usbredir=1\n");
        content.push_str("enable-usb-autoshare=1\n");
        content.push_str("usb-filter=-1,-1,-1,-1,0\n");
        content.push_str("secure-attention=ctrl+alt+end\n");
        content.push_str("release-cursor=shift+f12\n");

        Ok(content)
    }

    /// Check if SPICE client is installed
    pub fn is_client_installed(&self) -> bool {
        Command::new("which")
            .arg("remote-viewer")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Install SPICE client on Arch Linux
    pub async fn install_client_arch(&self) -> Result<(), String> {
        let output = Command::new("sudo")
            .args(&["pacman", "-S", "--noconfirm", "virt-viewer"])
            .output()
            .map_err(|e| format!("Failed to install virt-viewer: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Installation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    /// Check system requirements for SPICE
    pub fn check_requirements(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Check for remote-viewer
        if !self.is_client_installed() {
            issues.push(
                "SPICE client (remote-viewer) not installed. Install: sudo pacman -S virt-viewer"
                    .to_string(),
            );
        }

        // Check for spice-vdagent (guest agent)
        let agent_check = Command::new("which")
            .arg("spice-vdagent")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !agent_check {
            issues.push(
                "Note: Install spice-vdagent in guest for clipboard/resolution features"
                    .to_string(),
            );
        }

        // Check for QXL driver availability
        let qxl_check = Command::new("modinfo")
            .arg("qxl")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !qxl_check {
            issues.push(
                "Warning: QXL kernel module not available (may reduce graphics performance)"
                    .to_string(),
            );
        }

        issues
    }

    /// Get SPICE statistics for a VM
    pub async fn get_statistics(&self, vm_name: &str) -> Result<SpiceStats, String> {
        // This would normally query libvirt for SPICE channel statistics
        // For now, return basic stats

        let _info = self
            .active_connections
            .get(vm_name)
            .ok_or_else(|| format!("No active SPICE connection for VM '{}'", vm_name))?;

        Ok(SpiceStats {
            vm_name: vm_name.to_string(),
            connected: true,
            channels: 5, // main, display, inputs, cursor, playback
            bytes_sent: 0,
            bytes_received: 0,
        })
    }

    /// Enable/disable SPICE features dynamically
    pub async fn set_feature(
        &mut self,
        vm_name: &str,
        feature: SpiceFeature,
        enabled: bool,
    ) -> Result<(), String> {
        let config = self
            .configs
            .get_mut(vm_name)
            .ok_or_else(|| format!("No SPICE config for VM '{}'", vm_name))?;

        match feature {
            SpiceFeature::Audio => config.audio = enabled,
            SpiceFeature::ClipboardSharing => config.clipboard_sharing = enabled,
            SpiceFeature::FileTransfer => config.file_transfer = enabled,
            SpiceFeature::UsbRedirection => config.usb_redirection = enabled,
            SpiceFeature::OpenGL => config.opengl = enabled,
        }

        // Reapply configuration
        self.apply_config(vm_name).await
    }

    /// Configure multi-monitor setup
    pub async fn set_monitors(&mut self, vm_name: &str, count: u32) -> Result<(), String> {
        if count == 0 || count > 16 {
            return Err("Monitor count must be between 1 and 16".to_string());
        }

        let config = self
            .configs
            .get_mut(vm_name)
            .ok_or_else(|| format!("No SPICE config for VM '{}'", vm_name))?;

        config.monitors = count;

        // Adjust VRAM based on monitor count (more monitors = more VRAM needed)
        config.qxl_vram_mb = (64 * count).min(512);

        self.apply_config(vm_name).await
    }

    /// Set password for SPICE access
    pub fn set_password(&mut self, vm_name: &str, password: Option<String>) {
        if let Some(config) = self.configs.get_mut(vm_name) {
            config.password = password;
        }
    }

    /// List all VMs with SPICE enabled
    pub fn list_spice_vms(&self) -> Vec<String> {
        self.configs
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpiceStats {
    pub vm_name: String,
    pub connected: bool,
    pub channels: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum SpiceFeature {
    Audio,
    ClipboardSharing,
    FileTransfer,
    UsbRedirection,
    OpenGL,
}

impl Default for SpiceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SpiceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.port, 5900);
        assert!(config.audio);
        assert!(config.clipboard_sharing);
    }

    #[test]
    fn test_spice_manager() {
        let mut manager = SpiceManager::new();
        let config = SpiceConfig::default();

        manager.set_config("test-vm", config);
        assert!(manager.get_config("test-vm").is_some());
    }

    #[test]
    fn test_xml_generation() {
        let mut manager = SpiceManager::new();
        let config = SpiceConfig {
            enabled: true,
            autoport: false,
            port: 5901,
            monitors: 2,
            audio: true,
            usb_redirection: true,
            ..Default::default()
        };

        manager.set_config("test-vm", config);
        let xml = manager.generate_graphics_xml("test-vm").unwrap();

        assert!(xml.contains("type='spice'"));
        assert!(xml.contains("port='5901'"));
        assert!(xml.contains("heads='2'"));
        assert!(xml.contains("sound model='ich9'"));
        assert!(xml.contains("redirdev bus='usb'"));
    }

    #[test]
    fn test_vv_file_generation() {
        let manager = SpiceManager::new();
        let info = SpiceConnectionInfo {
            vm_name: "test-vm".to_string(),
            host: "localhost".to_string(),
            port: 5900,
            tls_port: None,
            password: Some("secret".to_string()),
            uri: "spice://localhost:5900".to_string(),
        };

        let vv = manager.generate_vv_file(&info).unwrap();
        assert!(vv.contains("type=spice"));
        assert!(vv.contains("host=localhost"));
        assert!(vv.contains("port=5900"));
        assert!(vv.contains("password=secret"));
    }
}
