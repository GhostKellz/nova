use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchNetworkConfig {
    pub use_systemd_networkd: bool,
    pub use_network_manager: bool,
    pub bridge_configs: HashMap<String, SystemdNetworkdConfig>,
    pub detected_interfaces: Vec<ArchInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdNetworkdConfig {
    pub name: String,
    pub network_file: String,
    pub netdev_file: String,
    pub dhcp: bool,
    pub static_ip: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchInterface {
    pub name: String,
    pub driver: Option<String>,
    pub speed: Option<String>,
    pub managed_by: InterfaceManager,
    pub systemd_config: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterfaceManager {
    SystemdNetworkd,
    NetworkManager,
    Manual,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkManagerProfile {
    pub name: String,
    pub uuid: String,
    pub connection_type: String,
    pub interface: Option<String>,
    pub autoconnect: bool,
}

pub struct ArchNetworkManager {
    config: ArchNetworkConfig,
    systemd_configs: HashMap<String, SystemdNetworkdConfig>,
    nm_profiles: Vec<NetworkManagerProfile>,
}

impl ArchNetworkManager {
    pub fn new() -> Self {
        Self {
            config: ArchNetworkConfig {
                use_systemd_networkd: false,
                use_network_manager: false,
                bridge_configs: HashMap::new(),
                detected_interfaces: Vec::new(),
            },
            systemd_configs: HashMap::new(),
            nm_profiles: Vec::new(),
        }
    }

    // Detect current network management system
    pub async fn detect_network_manager(&mut self) -> Result<()> {
        log_info!("Detecting Arch Linux network management system");

        // Check if systemd-networkd is active
        self.config.use_systemd_networkd = self.is_systemd_networkd_active().await;

        // Check if NetworkManager is active
        self.config.use_network_manager = self.is_network_manager_active().await;

        if self.config.use_systemd_networkd {
            log_info!("Detected systemd-networkd as active network manager");
            self.discover_systemd_configs().await?;
        }

        if self.config.use_network_manager {
            log_info!("Detected NetworkManager as active network manager");
            self.discover_nm_profiles().await?;
        }

        if !self.config.use_systemd_networkd && !self.config.use_network_manager {
            log_warn!("No managed network system detected, using manual configuration");
        }

        self.discover_interfaces().await?;
        Ok(())
    }

    async fn is_systemd_networkd_active(&self) -> bool {
        Command::new("systemctl")
            .args(&["is-active", "systemd-networkd"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    async fn is_network_manager_active(&self) -> bool {
        Command::new("systemctl")
            .args(&["is-active", "NetworkManager"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    // SystemD-networkd integration
    async fn discover_systemd_configs(&mut self) -> Result<()> {
        log_debug!("Discovering systemd-networkd configurations");

        let config_dirs = [
            "/etc/systemd/network",
            "/run/systemd/network",
            "/usr/lib/systemd/network",
        ];

        for config_dir in &config_dirs {
            if let Ok(entries) = fs::read_dir(config_dir) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if let Some(extension) = path.extension() {
                            if extension == "network" || extension == "netdev" {
                                if let Ok(config) = self.parse_systemd_config(&path).await {
                                    self.systemd_configs.insert(config.name.clone(), config);
                                }
                            }
                        }
                    }
                }
            }
        }

        log_info!(
            "Found {} systemd-networkd configurations",
            self.systemd_configs.len()
        );
        Ok(())
    }

    async fn parse_systemd_config(&self, config_path: &Path) -> Result<SystemdNetworkdConfig> {
        let content = fs::read_to_string(config_path).map_err(|_| NovaError::InvalidConfig)?;

        let name = config_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut config = SystemdNetworkdConfig {
            name: name.clone(),
            network_file: config_path.to_string_lossy().to_string(),
            netdev_file: String::new(),
            dhcp: false,
            static_ip: None,
            gateway: None,
            dns: Vec::new(),
        };

        // Parse systemd network configuration
        let mut in_network_section = false;
        let mut in_address_section = false;

        for line in content.lines() {
            let line = line.trim();

            if line == "[Network]" {
                in_network_section = true;
                in_address_section = false;
                continue;
            } else if line == "[Address]" {
                in_address_section = true;
                in_network_section = false;
                continue;
            } else if line.starts_with('[') {
                in_network_section = false;
                in_address_section = false;
                continue;
            }

            if in_network_section {
                if line.starts_with("DHCP=") {
                    config.dhcp = line
                        .split('=')
                        .nth(1)
                        .map(|v| v.trim().to_lowercase() == "yes" || v.trim() == "true")
                        .unwrap_or(false);
                } else if line.starts_with("Gateway=") {
                    config.gateway = line.split('=').nth(1).map(|v| v.trim().to_string());
                } else if line.starts_with("DNS=") {
                    if let Some(dns_list) = line.split('=').nth(1) {
                        config
                            .dns
                            .extend(dns_list.split_whitespace().map(|s| s.to_string()));
                    }
                }
            }

            if in_address_section {
                if line.starts_with("Address=") {
                    config.static_ip = line.split('=').nth(1).map(|v| v.trim().to_string());
                }
            }
        }

        Ok(config)
    }

    pub async fn create_systemd_bridge(
        &self,
        bridge_name: &str,
        interfaces: &[String],
    ) -> Result<()> {
        log_info!("Creating systemd-networkd bridge: {}", bridge_name);

        // Create .netdev file for bridge
        let netdev_content = format!(
            "[NetDev]
Name={}
Kind=bridge

[Bridge]
STP=yes
",
            bridge_name
        );

        let netdev_path = format!("/etc/systemd/network/25-{}.netdev", bridge_name);
        fs::write(&netdev_path, netdev_content).map_err(|e| {
            log_error!("Failed to write netdev file: {}", e);
            NovaError::SystemCommandFailed
        })?;

        // Create .network file for bridge
        let network_content = format!(
            "[Match]
Name={}

[Network]
DHCP=yes
IPForward=yes
",
            bridge_name
        );

        let network_path = format!("/etc/systemd/network/25-{}.network", bridge_name);
        fs::write(&network_path, network_content).map_err(|e| {
            log_error!("Failed to write network file: {}", e);
            NovaError::SystemCommandFailed
        })?;

        // Create bind files for each interface
        for interface in interfaces {
            let bind_content = format!(
                "[Match]
Name={}

[Network]
Bridge={}
",
                interface, bridge_name
            );

            let bind_path = format!("/etc/systemd/network/25-{}-bind.network", interface);
            fs::write(&bind_path, bind_content).map_err(|e| {
                log_error!("Failed to write bind file for {}: {}", interface, e);
                NovaError::SystemCommandFailed
            })?;
        }

        // Restart systemd-networkd
        self.restart_systemd_networkd().await?;

        log_info!(
            "systemd-networkd bridge {} created successfully",
            bridge_name
        );
        Ok(())
    }

    async fn restart_systemd_networkd(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .args(&["restart", "systemd-networkd"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to restart systemd-networkd");
            return Err(NovaError::SystemCommandFailed);
        }

        // Wait a moment for networkd to settle
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        Ok(())
    }

    // NetworkManager integration
    async fn discover_nm_profiles(&mut self) -> Result<()> {
        log_debug!("Discovering NetworkManager profiles");

        let output = Command::new("nmcli")
            .args(&[
                "-t",
                "-f",
                "NAME,UUID,TYPE,DEVICE,AUTOCONNECT",
                "connection",
                "show",
            ])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Ok(());
        }

        let connections = String::from_utf8_lossy(&output.stdout);
        for line in connections.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 5 {
                let profile = NetworkManagerProfile {
                    name: parts[0].to_string(),
                    uuid: parts[1].to_string(),
                    connection_type: parts[2].to_string(),
                    interface: if parts[3].is_empty() {
                        None
                    } else {
                        Some(parts[3].to_string())
                    },
                    autoconnect: parts[4] == "yes",
                };
                self.nm_profiles.push(profile);
            }
        }

        log_info!("Found {} NetworkManager profiles", self.nm_profiles.len());
        Ok(())
    }

    pub async fn create_nm_bridge(&self, bridge_name: &str, interfaces: &[String]) -> Result<()> {
        log_info!("Creating NetworkManager bridge: {}", bridge_name);

        // Create bridge connection
        let output = Command::new("nmcli")
            .args(&[
                "connection",
                "add",
                "type",
                "bridge",
                "con-name",
                bridge_name,
                "ifname",
                bridge_name,
            ])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to create NetworkManager bridge: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        // Add interfaces to bridge
        for interface in interfaces {
            let slave_name = format!("{}-slave-{}", bridge_name, interface);
            let output = Command::new("nmcli")
                .args(&[
                    "connection",
                    "add",
                    "type",
                    "bridge-slave",
                    "con-name",
                    &slave_name,
                    "ifname",
                    interface,
                    "master",
                    bridge_name,
                ])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                log_warn!(
                    "Failed to add interface {} to bridge {}",
                    interface,
                    bridge_name
                );
            }
        }

        // Bring up the bridge
        let output = Command::new("nmcli")
            .args(&["connection", "up", bridge_name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to bring up NetworkManager bridge {}", bridge_name);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("NetworkManager bridge {} created successfully", bridge_name);
        Ok(())
    }

    // Interface discovery
    async fn discover_interfaces(&mut self) -> Result<()> {
        log_debug!("Discovering network interfaces");

        // Get interface list
        let output = Command::new("ip")
            .args(&["-j", "link", "show"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let interfaces: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|_| NovaError::InvalidConfig)?;

        if let Some(interfaces_array) = interfaces.as_array() {
            for interface_data in interfaces_array {
                if let Some(name) = interface_data["ifname"].as_str() {
                    // Skip loopback and some virtual interfaces
                    if name == "lo" || name.starts_with("veth") {
                        continue;
                    }

                    let driver = self.get_interface_driver(name).await.ok();
                    let speed = self.get_interface_speed(name).await.ok();
                    let managed_by = self.detect_interface_manager(name).await;
                    let systemd_config = self.find_systemd_config_for_interface(name);

                    let arch_interface = ArchInterface {
                        name: name.to_string(),
                        driver,
                        speed,
                        managed_by,
                        systemd_config,
                    };

                    self.config.detected_interfaces.push(arch_interface);
                }
            }
        }

        log_info!(
            "Discovered {} network interfaces",
            self.config.detected_interfaces.len()
        );
        Ok(())
    }

    async fn get_interface_driver(&self, interface: &str) -> Result<String> {
        let driver_path = format!("/sys/class/net/{}/device/driver", interface);
        if let Ok(link) = fs::read_link(&driver_path) {
            if let Some(driver_name) = link.file_name() {
                return Ok(driver_name.to_string_lossy().to_string());
            }
        }
        Err(NovaError::NetworkNotFound(interface.to_string()))
    }

    async fn get_interface_speed(&self, interface: &str) -> Result<String> {
        let speed_path = format!("/sys/class/net/{}/speed", interface);
        if let Ok(speed_content) = fs::read_to_string(&speed_path) {
            let speed = speed_content.trim();
            if speed != "-1" {
                return Ok(format!("{}Mbps", speed));
            }
        }
        Err(NovaError::NetworkNotFound(interface.to_string()))
    }

    async fn detect_interface_manager(&self, interface: &str) -> InterfaceManager {
        // Check if managed by NetworkManager
        if self.config.use_network_manager {
            let output = Command::new("nmcli")
                .args(&["-t", "-f", "DEVICE", "device", "status"])
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    let devices = String::from_utf8_lossy(&output.stdout);
                    if devices.lines().any(|line| line.trim() == interface) {
                        return InterfaceManager::NetworkManager;
                    }
                }
            }
        }

        // Check if has systemd-networkd config
        if self.find_systemd_config_for_interface(interface).is_some() {
            return InterfaceManager::SystemdNetworkd;
        }

        // Check if manually configured
        let output = Command::new("ip")
            .args(&["addr", "show", interface])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let addr_info = String::from_utf8_lossy(&output.stdout);
                if addr_info.contains("inet ") {
                    return InterfaceManager::Manual;
                }
            }
        }

        InterfaceManager::Unknown
    }

    fn find_systemd_config_for_interface(&self, interface: &str) -> Option<String> {
        for config in self.systemd_configs.values() {
            if config.name.contains(interface) || config.network_file.contains(interface) {
                return Some(config.network_file.clone());
            }
        }
        None
    }

    // Arch-specific optimizations
    pub async fn optimize_for_virtualization(&self) -> Result<()> {
        log_info!("Applying Arch Linux virtualization optimizations");

        // Check and enable required kernel modules
        self.ensure_kvm_modules().await?;

        // Optimize systemd settings for virtualization
        self.optimize_systemd_settings().await?;

        // Configure user groups for KVM access
        self.configure_kvm_groups().await?;

        log_info!("Arch Linux optimizations applied successfully");
        Ok(())
    }

    async fn ensure_kvm_modules(&self) -> Result<()> {
        let modules = [
            "kvm",
            "kvm_intel",
            "kvm_amd",
            "vhost_net",
            "bridge",
            "br_netfilter",
        ];

        for module in &modules {
            let output = Command::new("modprobe")
                .arg(module)
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if output.status.success() {
                log_debug!("Loaded kernel module: {}", module);
            } else {
                log_warn!("Failed to load kernel module: {}", module);
            }
        }

        // Persist module loading
        let modules_conf = modules.join(
            "
",
        );
        fs::write("/etc/modules-load.d/nova-kvm.conf", modules_conf).map_err(|e| {
            log_error!("Failed to write modules config: {}", e);
            NovaError::SystemCommandFailed
        })?;

        Ok(())
    }

    async fn optimize_systemd_settings(&self) -> Result<()> {
        // Create systemd drop-in for improved virtualization performance
        let systemd_conf = r#"[Manager]
DefaultLimitNOFILE=65536
DefaultLimitMEMLOCK=infinity
"#;

        fs::create_dir_all("/etc/systemd/system.conf.d")
            .map_err(|_| NovaError::SystemCommandFailed)?;
        fs::write(
            "/etc/systemd/system.conf.d/nova-virtualization.conf",
            systemd_conf,
        )
        .map_err(|e| {
            log_error!("Failed to write systemd config: {}", e);
            NovaError::SystemCommandFailed
        })?;

        Ok(())
    }

    async fn configure_kvm_groups(&self) -> Result<()> {
        // Ensure kvm and libvirt groups exist
        let groups = ["kvm", "libvirt"];

        for group in &groups {
            let output = Command::new("getent")
                .args(&["group", group])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                // Create group if it doesn't exist
                let _ = Command::new("groupadd").arg(group).output();
            }
        }

        Ok(())
    }

    // Getters
    pub fn get_config(&self) -> &ArchNetworkConfig {
        &self.config
    }

    pub fn get_systemd_configs(&self) -> &HashMap<String, SystemdNetworkdConfig> {
        &self.systemd_configs
    }

    pub fn get_nm_profiles(&self) -> &Vec<NetworkManagerProfile> {
        &self.nm_profiles
    }

    pub fn is_using_systemd_networkd(&self) -> bool {
        self.config.use_systemd_networkd
    }

    pub fn is_using_network_manager(&self) -> bool {
        self.config.use_network_manager
    }
}

impl Default for ArchNetworkManager {
    fn default() -> Self {
        Self::new()
    }
}
