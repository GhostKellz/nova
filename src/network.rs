use crate::{log_debug, log_error, log_info, log_warn, NovaError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::net::Ipv4Addr;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualSwitch {
    pub name: String,
    pub switch_type: SwitchType,
    pub interfaces: Vec<String>,
    pub vlan_id: Option<u16>,
    pub stp_enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: SwitchStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SwitchType {
    LinuxBridge,
    OpenVSwitch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwitchStatus {
    Active,
    Inactive,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: String,
    pub ip_address: Option<Ipv4Addr>,
    pub state: InterfaceState,
    pub bridge: Option<String>,
    pub speed: Option<u64>, // Mbps
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterfaceState {
    Up,
    Down,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub name: String,
    pub stp: bool,
    pub forward_delay: u32,
    pub hello_time: u32,
    pub max_age: u32,
    pub aging_time: u32,
    pub multicast_snooping: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub range_start: Ipv4Addr,
    pub range_end: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
    pub gateway: Ipv4Addr,
    pub dns_servers: Vec<Ipv4Addr>,
    pub lease_time: u32, // seconds
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatConfig {
    pub enabled: bool,
    pub internal_interface: String,
    pub external_interface: String,
    pub masquerade: bool,
}

pub struct NetworkManager {
    switches: HashMap<String, VirtualSwitch>,
    interfaces: HashMap<String, NetworkInterface>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            switches: HashMap::new(),
            interfaces: HashMap::new(),
        }
    }

    // Virtual Switch Management
    pub async fn create_virtual_switch(&mut self, name: &str, switch_type: SwitchType) -> Result<()> {
        log_info!("Creating virtual switch: {} ({:?})", name, switch_type);

        match switch_type {
            SwitchType::LinuxBridge => {
                self.create_linux_bridge(name).await?;
            }
            SwitchType::OpenVSwitch => {
                self.create_ovs_bridge(name).await?;
            }
        }

        let switch = VirtualSwitch {
            name: name.to_string(),
            switch_type,
            interfaces: Vec::new(),
            vlan_id: None,
            stp_enabled: false,
            created_at: chrono::Utc::now(),
            status: SwitchStatus::Active,
        };

        self.switches.insert(name.to_string(), switch);
        Ok(())
    }

    async fn create_linux_bridge(&self, name: &str) -> Result<()> {
        // Create bridge using ip command (modern approach)
        let output = Command::new("ip")
            .args(&["link", "add", "name", name, "type", "bridge"])
            .output()
            .map_err(|e| {
                log_error!("Failed to create bridge {}: {}", name, e);
                NovaError::SystemCommandFailed
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Bridge creation failed: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        // Bring bridge up
        let output = Command::new("ip")
            .args(&["link", "set", "dev", name, "up"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to bring bridge {} up", name);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("Linux bridge {} created successfully", name);
        Ok(())
    }

    async fn create_ovs_bridge(&self, name: &str) -> Result<()> {
        // Check if OVS is available
        if !self.check_ovs_available() {
            log_warn!("Open vSwitch not available, falling back to Linux bridge");
            return self.create_linux_bridge(name).await;
        }

        let output = Command::new("ovs-vsctl")
            .args(&["add-br", name])
            .output()
            .map_err(|e| {
                log_error!("Failed to create OVS bridge {}: {}", name, e);
                NovaError::SystemCommandFailed
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("OVS bridge creation failed: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("OVS bridge {} created successfully", name);
        Ok(())
    }

    pub async fn delete_virtual_switch(&mut self, name: &str) -> Result<()> {
        log_info!("Deleting virtual switch: {}", name);

        if let Some(switch) = self.switches.get(name) {
            match switch.switch_type {
                SwitchType::LinuxBridge => {
                    self.delete_linux_bridge(name).await?;
                }
                SwitchType::OpenVSwitch => {
                    self.delete_ovs_bridge(name).await?;
                }
            }
        }

        self.switches.remove(name);
        Ok(())
    }

    async fn delete_linux_bridge(&self, name: &str) -> Result<()> {
        // Bring bridge down first
        let _ = Command::new("ip")
            .args(&["link", "set", "dev", name, "down"])
            .output();

        // Delete bridge
        let output = Command::new("ip")
            .args(&["link", "delete", name, "type", "bridge"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to delete bridge {}", name);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("Linux bridge {} deleted successfully", name);
        Ok(())
    }

    async fn delete_ovs_bridge(&self, name: &str) -> Result<()> {
        let output = Command::new("ovs-vsctl")
            .args(&["del-br", name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to delete OVS bridge {}", name);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("OVS bridge {} deleted successfully", name);
        Ok(())
    }

    pub async fn add_interface_to_switch(&mut self, switch_name: &str, interface: &str) -> Result<()> {
        log_info!("Adding interface {} to switch {}", interface, switch_name);

        let switch_type = if let Some(switch) = self.switches.get(switch_name) {
            switch.switch_type.clone()
        } else {
            return Err(NovaError::NetworkNotFound(switch_name.to_string()));
        };

        match switch_type {
            SwitchType::LinuxBridge => {
                self.add_interface_to_linux_bridge(switch_name, interface).await?;
            }
            SwitchType::OpenVSwitch => {
                self.add_interface_to_ovs_bridge(switch_name, interface).await?;
            }
        }

        if let Some(switch) = self.switches.get_mut(switch_name) {
            switch.interfaces.push(interface.to_string());
        }

        Ok(())
    }

    async fn add_interface_to_linux_bridge(&self, bridge: &str, interface: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(&["link", "set", "dev", interface, "master", bridge])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to add interface {} to bridge {}", interface, bridge);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("Interface {} added to bridge {} successfully", interface, bridge);
        Ok(())
    }

    async fn add_interface_to_ovs_bridge(&self, bridge: &str, interface: &str) -> Result<()> {
        let output = Command::new("ovs-vsctl")
            .args(&["add-port", bridge, interface])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to add interface {} to OVS bridge {}", interface, bridge);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("Interface {} added to OVS bridge {} successfully", interface, bridge);
        Ok(())
    }

    // Bridge Configuration
    pub async fn configure_bridge(&self, config: &BridgeConfig) -> Result<()> {
        log_info!("Configuring bridge: {}", config.name);

        // Configure STP
        if config.stp {
            self.enable_stp(&config.name).await?;
            self.set_bridge_parameter(&config.name, "forward_delay", &config.forward_delay.to_string()).await?;
            self.set_bridge_parameter(&config.name, "hello_time", &config.hello_time.to_string()).await?;
            self.set_bridge_parameter(&config.name, "max_age", &config.max_age.to_string()).await?;
        }

        // Configure aging time
        self.set_bridge_parameter(&config.name, "ageing_time", &config.aging_time.to_string()).await?;

        // Configure multicast snooping
        if config.multicast_snooping {
            self.set_bridge_parameter(&config.name, "multicast_snooping", "1").await?;
        }

        log_info!("Bridge {} configured successfully", config.name);
        Ok(())
    }

    async fn enable_stp(&self, bridge: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(&["link", "set", "dev", bridge, "type", "bridge", "stp_state", "1"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to enable STP on bridge {}", bridge);
            return Err(NovaError::SystemCommandFailed);
        }

        log_debug!("STP enabled on bridge {}", bridge);
        Ok(())
    }

    async fn set_bridge_parameter(&self, bridge: &str, param: &str, value: &str) -> Result<()> {
        let sysfs_path = format!("/sys/class/net/{}/bridge/{}", bridge, param);

        let output = Command::new("sh")
            .arg("-c")
            .arg(&format!("echo {} > {}", value, sysfs_path))
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_warn!("Failed to set bridge parameter {}={} for {}", param, value, bridge);
        } else {
            log_debug!("Set bridge parameter {}={} for {}", param, value, bridge);
        }

        Ok(())
    }

    // Interface Discovery and Management
    pub async fn discover_interfaces(&mut self) -> Result<()> {
        log_info!("Discovering network interfaces");

        let output = Command::new("ip")
            .args(&["-j", "link", "show"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        self.parse_interface_json(&json_str)?;

        log_info!("Discovered {} network interfaces", self.interfaces.len());
        Ok(())
    }

    fn parse_interface_json(&mut self, json_str: &str) -> Result<()> {
        // Parse JSON output from ip command
        let interfaces: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|_| NovaError::InvalidConfig)?;

        if let Some(interfaces_array) = interfaces.as_array() {
            for interface_data in interfaces_array {
                if let Some(name) = interface_data["ifname"].as_str() {
                    // Skip loopback and virtual interfaces for now
                    if name == "lo" || name.starts_with("veth") || name.starts_with("docker") {
                        continue;
                    }

                    let mac_address = interface_data["address"]
                        .as_str()
                        .unwrap_or("00:00:00:00:00:00")
                        .to_string();

                    let state = match interface_data["operstate"].as_str() {
                        Some("UP") => InterfaceState::Up,
                        Some("DOWN") => InterfaceState::Down,
                        _ => InterfaceState::Unknown,
                    };

                    let interface = NetworkInterface {
                        name: name.to_string(),
                        mac_address,
                        ip_address: None, // Will be populated separately
                        state,
                        bridge: None, // Will be detected separately
                        speed: None,
                    };

                    self.interfaces.insert(name.to_string(), interface);
                }
            }
        }

        Ok(())
    }

    // VLAN Management
    pub async fn create_vlan_interface(&self, base_interface: &str, vlan_id: u16) -> Result<String> {
        let vlan_name = format!("{}.{}", base_interface, vlan_id);
        log_info!("Creating VLAN interface: {}", vlan_name);

        let output = Command::new("ip")
            .args(&["link", "add", "link", base_interface, "name", &vlan_name, "type", "vlan", "id", &vlan_id.to_string()])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to create VLAN interface {}", vlan_name);
            return Err(NovaError::SystemCommandFailed);
        }

        // Bring VLAN interface up
        let _ = Command::new("ip")
            .args(&["link", "set", "dev", &vlan_name, "up"])
            .output();

        log_info!("VLAN interface {} created successfully", vlan_name);
        Ok(vlan_name)
    }

    // Utility functions
    pub fn list_switches(&self) -> Vec<&VirtualSwitch> {
        self.switches.values().collect()
    }

    pub fn get_switch(&self, name: &str) -> Option<&VirtualSwitch> {
        self.switches.get(name)
    }

    pub fn list_interfaces(&self) -> Vec<&NetworkInterface> {
        self.interfaces.values().collect()
    }

    pub fn get_interface(&self, name: &str) -> Option<&NetworkInterface> {
        self.interfaces.get(name)
    }

    fn check_ovs_available(&self) -> bool {
        Command::new("ovs-vsctl")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn check_bridge_utils_available(&self) -> bool {
        Command::new("ip")
            .args(&["link", "help"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

impl NetworkManager {
    // Advanced Bridge Features
    pub async fn enable_port_mirroring(&self, bridge: &str, source_port: &str, target_port: &str) -> Result<()> {
        log_info!("Enabling port mirroring on bridge {}: {} -> {}", bridge, source_port, target_port);

        if self.check_ovs_available() {
            // Use OVS for advanced port mirroring
            let output = Command::new("ovs-vsctl")
                .args(&["--", "--id=@m", "create", "mirror", &format!("name=mirror-{}-{}", source_port, target_port)])
                .arg("--")
                .args(&["--id=@in", "get", "port", source_port])
                .arg("--")
                .args(&["--id=@out", "get", "port", target_port])
                .arg("--")
                .args(&["set", "bridge", bridge, "mirrors=@m"])
                .arg("--")
                .args(&["set", "mirror", "@m", "select_src_port=@in", "output_port=@out"])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                log_error!("Failed to enable OVS port mirroring");
                return Err(NovaError::SystemCommandFailed);
            }
        } else {
            log_warn!("Port mirroring requires Open vSwitch, falling back to basic bridge");
        }

        Ok(())
    }

    pub async fn configure_bridge_filters(&self, bridge: &str, rules: &[BridgeFilter]) -> Result<()> {
        log_info!("Configuring bridge filters for {}", bridge);

        for rule in rules {
            self.apply_bridge_filter(bridge, rule).await?;
        }

        Ok(())
    }

    async fn apply_bridge_filter(&self, bridge: &str, filter: &BridgeFilter) -> Result<()> {
        match filter.action {
            FilterAction::Allow => {
                // Use ebtables for bridge-level filtering
                let output = Command::new("ebtables")
                    .args(&["-A", "FORWARD", "-i", bridge, "-p", &filter.protocol, "-j", "ACCEPT"])
                    .output()
                    .map_err(|_| NovaError::SystemCommandFailed)?;

                if !output.status.success() {
                    log_warn!("Failed to apply bridge filter rule");
                }
            }
            FilterAction::Deny => {
                let output = Command::new("ebtables")
                    .args(&["-A", "FORWARD", "-i", bridge, "-p", &filter.protocol, "-j", "DROP"])
                    .output()
                    .map_err(|_| NovaError::SystemCommandFailed)?;

                if !output.status.success() {
                    log_warn!("Failed to apply bridge filter rule");
                }
            }
        }

        Ok(())
    }

    // DHCP Management with dnsmasq
    pub async fn configure_dhcp(&self, config: &DhcpConfig, interface: &str) -> Result<()> {
        log_info!("Configuring DHCP for interface {}", interface);

        if !config.enabled {
            return self.stop_dhcp(interface).await;
        }

        let conf_file = format!("/tmp/nova-dhcp-{}.conf", interface);
        let mut dhcp_conf = String::new();

        // Basic configuration
        dhcp_conf.push_str(&format!("interface={}
", interface));
        dhcp_conf.push_str(&format!("dhcp-range={},{},{},{}s
",
                                   config.range_start, config.range_end,
                                   config.subnet_mask, config.lease_time));
        dhcp_conf.push_str(&format!("dhcp-option=3,{}
", config.gateway)); // Gateway

        // DNS servers
        for (i, dns) in config.dns_servers.iter().enumerate() {
            dhcp_conf.push_str(&format!("dhcp-option=6,{}
", dns));
        }

        // No daemon mode, bind to interface
        dhcp_conf.push_str("bind-interfaces
");
        dhcp_conf.push_str("no-daemon
");
        dhcp_conf.push_str("log-dhcp
");

        // Write configuration
        std::fs::write(&conf_file, dhcp_conf).map_err(|e| {
            log_error!("Failed to write DHCP config: {}", e);
            NovaError::SystemCommandFailed
        })?;

        // Start dnsmasq
        let pid_file = format!("/tmp/nova-dhcp-{}.pid", interface);
        let log_file = format!("/tmp/nova-dhcp-{}.log", interface);

        let output = Command::new("dnsmasq")
            .args(&["-C", &conf_file])
            .args(&["--pid-file", &pid_file])
            .args(&["--log-facility", &log_file])
            .output()
            .map_err(|e| {
                log_error!("Failed to start dnsmasq: {}", e);
                NovaError::SystemCommandFailed
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("dnsmasq failed to start: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("DHCP server started for interface {}", interface);
        Ok(())
    }

    pub async fn stop_dhcp(&self, interface: &str) -> Result<()> {
        log_info!("Stopping DHCP for interface {}", interface);

        let pid_file = format!("/tmp/nova-dhcp-{}.pid", interface);

        if let Ok(pid_content) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_content.trim().parse::<u32>() {
                let _ = Command::new("kill")
                    .arg(&pid.to_string())
                    .output();
            }
        }

        // Clean up files
        let _ = std::fs::remove_file(&pid_file);
        let _ = std::fs::remove_file(&format!("/tmp/nova-dhcp-{}.conf", interface));
        let _ = std::fs::remove_file(&format!("/tmp/nova-dhcp-{}.log", interface));

        Ok(())
    }

    // NAT Management with iptables
    pub async fn configure_nat(&self, config: &NatConfig) -> Result<()> {
        log_info!("Configuring NAT: {} -> {}", config.internal_interface, config.external_interface);

        if !config.enabled {
            return self.remove_nat_rules(config).await;
        }

        // Enable IP forwarding
        let output = Command::new("sysctl")
            .args(&["-w", "net.ipv4.ip_forward=1"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to enable IP forwarding");
            return Err(NovaError::SystemCommandFailed);
        }

        if config.masquerade {
            // Add masquerade rule
            let output = Command::new("iptables")
                .args(&["-t", "nat", "-A", "POSTROUTING", "-o", &config.external_interface, "-j", "MASQUERADE"])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                log_error!("Failed to add masquerade rule");
                return Err(NovaError::SystemCommandFailed);
            }
        }

        // Add forward rules
        let output = Command::new("iptables")
            .args(&["-A", "FORWARD", "-i", &config.internal_interface, "-o", &config.external_interface, "-j", "ACCEPT"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to add forward rule");
            return Err(NovaError::SystemCommandFailed);
        }

        let output = Command::new("iptables")
            .args(&["-A", "FORWARD", "-i", &config.external_interface, "-o", &config.internal_interface, "-m", "state", "--state", "RELATED,ESTABLISHED", "-j", "ACCEPT"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to add return forward rule");
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("NAT configuration applied successfully");
        Ok(())
    }

    async fn remove_nat_rules(&self, config: &NatConfig) -> Result<()> {
        log_info!("Removing NAT rules for {}", config.internal_interface);

        // Remove masquerade rule
        if config.masquerade {
            let _ = Command::new("iptables")
                .args(&["-t", "nat", "-D", "POSTROUTING", "-o", &config.external_interface, "-j", "MASQUERADE"])
                .output();
        }

        // Remove forward rules
        let _ = Command::new("iptables")
            .args(&["-D", "FORWARD", "-i", &config.internal_interface, "-o", &config.external_interface, "-j", "ACCEPT"])
            .output();

        let _ = Command::new("iptables")
            .args(&["-D", "FORWARD", "-i", &config.external_interface, "-o", &config.internal_interface, "-m", "state", "--state", "RELATED,ESTABLISHED", "-j", "ACCEPT"])
            .output();

        Ok(())
    }

    // Check if required tools are available
    pub fn check_dhcp_available(&self) -> bool {
        Command::new("dnsmasq")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn check_iptables_available(&self) -> bool {
        Command::new("iptables")
            .args(&["-L", "-n"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn check_ebtables_available(&self) -> bool {
        Command::new("ebtables")
            .args(&["-L"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeFilter {
    pub protocol: String, // "ip", "arp", "ipv6", etc.
    pub source_mac: Option<String>,
    pub dest_mac: Option<String>,
    pub action: FilterAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterAction {
    Allow,
    Deny,
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}