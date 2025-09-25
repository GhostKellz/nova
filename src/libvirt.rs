use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::Ipv4Addr;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibvirtNetwork {
    pub name: String,
    pub uuid: Option<String>,
    pub forward: Option<ForwardMode>,
    pub bridge: Option<BridgeConfig>,
    pub ip: Option<IpConfig>,
    pub dns: Option<DnsConfig>,
    pub autostart: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardMode {
    pub mode: String, // nat, route, bridge, passthrough, private, vepa, hostdev
    pub dev: Option<String>,
    pub interfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub name: String,
    pub stp: Option<bool>,
    pub delay: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpConfig {
    pub address: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub dhcp: Option<DhcpRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpRange {
    pub start: Ipv4Addr,
    pub end: Ipv4Addr,
    pub hosts: Vec<DhcpHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpHost {
    pub mac: String,
    pub name: String,
    pub ip: Ipv4Addr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub forwarders: Vec<Ipv4Addr>,
    pub hosts: Vec<DnsHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsHost {
    pub ip: Ipv4Addr,
    pub hostname: String,
}

pub struct LibvirtManager {
    networks: Vec<LibvirtNetwork>,
}

impl LibvirtManager {
    pub fn new() -> Self {
        Self {
            networks: Vec::new(),
        }
    }

    // Check if libvirt is available
    pub fn check_libvirt_available(&self) -> bool {
        Command::new("virsh")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    // Discover existing libvirt networks
    pub async fn discover_networks(&mut self) -> Result<()> {
        log_info!("Discovering libvirt networks");

        if !self.check_libvirt_available() {
            log_warn!("libvirt not available, skipping network discovery");
            return Ok(());
        }

        self.networks.clear();

        // Get list of all networks (active and inactive)
        let output = Command::new("virsh")
            .args(&["net-list", "--all", "--name"])
            .output()
            .map_err(|e| {
                log_error!("Failed to list libvirt networks: {}", e);
                NovaError::SystemCommandFailed
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("virsh net-list failed: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        let network_names = String::from_utf8_lossy(&output.stdout);

        for line in network_names.lines() {
            let name = line.trim();
            if !name.is_empty() {
                if let Ok(network) = self.get_network_info(name).await {
                    self.networks.push(network);
                }
            }
        }

        log_info!("Discovered {} libvirt networks", self.networks.len());
        Ok(())
    }

    pub async fn set_network_autostart(&self, name: &str, enable: bool) -> Result<()> {
        log_info!("Setting libvirt network autostart {} -> {}", name, enable);

        let mut args = vec!["net-autostart", name];
        if !enable {
            args.insert(1, "--disable");
        }

        let output = Command::new("virsh")
            .args(&args)
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to update autostart: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    // Get detailed information about a specific network
    pub async fn get_network_info(&self, name: &str) -> Result<LibvirtNetwork> {
        log_debug!("Getting info for libvirt network: {}", name);

        // Get network XML definition
        let output = Command::new("virsh")
            .args(&["net-dumpxml", name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::NetworkNotFound(name.to_string()));
        }

        let xml_content = String::from_utf8_lossy(&output.stdout);
        self.parse_network_xml(&xml_content, name)
    }

    // Parse libvirt network XML
    fn parse_network_xml(&self, xml_content: &str, name: &str) -> Result<LibvirtNetwork> {
        // For now, we'll do basic XML parsing
        // In a production system, you'd want to use a proper XML parser like roxmltree

        let mut network = LibvirtNetwork {
            name: name.to_string(),
            uuid: None,
            forward: None,
            bridge: None,
            ip: None,
            dns: None,
            autostart: false,
            active: self.is_network_active(name),
        };

        // Extract UUID
        if let Some(start) = xml_content.find("<uuid>") {
            if let Some(end) = xml_content[start..].find("</uuid>") {
                let uuid_start = start + 6; // len("<uuid>")
                let uuid_end = start + end;
                network.uuid = Some(xml_content[uuid_start..uuid_end].to_string());
            }
        }

        // Extract bridge information
        if let Some(bridge_start) = xml_content.find("<bridge") {
            if let Some(bridge_end) = xml_content[bridge_start..].find("/>") {
                let bridge_tag = &xml_content[bridge_start..bridge_start + bridge_end + 2];

                let bridge_name = self
                    .extract_attribute(bridge_tag, "name")
                    .unwrap_or_else(|| format!("virbr-{}", name));

                let stp = self
                    .extract_attribute(bridge_tag, "stp")
                    .and_then(|s| s.parse::<bool>().ok());

                let delay = self
                    .extract_attribute(bridge_tag, "delay")
                    .and_then(|s| s.parse::<u32>().ok());

                network.bridge = Some(BridgeConfig {
                    name: bridge_name,
                    stp,
                    delay,
                });
            }
        }

        // Extract forward mode
        if let Some(forward_start) = xml_content.find("<forward") {
            if let Some(forward_end) = xml_content[forward_start..].find("/>") {
                let forward_tag = &xml_content[forward_start..forward_start + forward_end + 2];

                if let Some(mode) = self.extract_attribute(forward_tag, "mode") {
                    let dev = self.extract_attribute(forward_tag, "dev");

                    network.forward = Some(ForwardMode {
                        mode,
                        dev,
                        interfaces: Vec::new(), // Would need more complex parsing for interfaces
                    });
                }
            }
        }

        // Extract IP configuration
        if let Some(ip_start) = xml_content.find("<ip") {
            if let Some(ip_end) = xml_content[ip_start..].find(">") {
                let ip_tag = &xml_content[ip_start..ip_start + ip_end + 1];

                if let Some(address_str) = self.extract_attribute(ip_tag, "address") {
                    if let Ok(address) = Ipv4Addr::from_str(&address_str) {
                        if let Some(netmask_str) = self.extract_attribute(ip_tag, "netmask") {
                            if let Ok(netmask) = Ipv4Addr::from_str(&netmask_str) {
                                // Look for DHCP range
                                let dhcp = self.extract_dhcp_config(&xml_content);

                                network.ip = Some(IpConfig {
                                    address,
                                    netmask,
                                    dhcp,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(network)
    }

    fn extract_attribute(&self, tag: &str, attr_name: &str) -> Option<String> {
        let pattern = format!("{}=\"", attr_name);
        if let Some(start) = tag.find(&pattern) {
            let value_start = start + pattern.len();
            if let Some(end) = tag[value_start..].find('"') {
                return Some(tag[value_start..value_start + end].to_string());
            }
        }
        None
    }

    fn extract_dhcp_config(&self, xml_content: &str) -> Option<DhcpRange> {
        if let Some(dhcp_start) = xml_content.find("<dhcp>") {
            if let Some(dhcp_end) = xml_content[dhcp_start..].find("</dhcp>") {
                let dhcp_section = &xml_content[dhcp_start..dhcp_start + dhcp_end];

                // Look for range
                if let Some(range_start) = dhcp_section.find("<range") {
                    if let Some(range_end) = dhcp_section[range_start..].find("/>") {
                        let range_tag = &dhcp_section[range_start..range_start + range_end + 2];

                        if let (Some(start_str), Some(end_str)) = (
                            self.extract_attribute(range_tag, "start"),
                            self.extract_attribute(range_tag, "end"),
                        ) {
                            if let (Ok(start), Ok(end)) =
                                (Ipv4Addr::from_str(&start_str), Ipv4Addr::from_str(&end_str))
                            {
                                return Some(DhcpRange {
                                    start,
                                    end,
                                    hosts: Vec::new(), // Would need more parsing for static hosts
                                });
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn is_network_active(&self, name: &str) -> bool {
        Command::new("virsh")
            .args(&["net-info", name])
            .output()
            .map(|output| {
                if output.status.success() {
                    let info = String::from_utf8_lossy(&output.stdout);
                    info.contains("Active:         yes")
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    // Create a new libvirt network
    pub async fn create_network(&mut self, network: &LibvirtNetwork) -> Result<()> {
        log_info!("Creating libvirt network: {}", network.name);

        let xml_content = self.generate_network_xml(network)?;

        // Write XML to temporary file
        let temp_file = format!("/tmp/nova-network-{}.xml", network.name);
        fs::write(&temp_file, xml_content).map_err(|e| {
            log_error!("Failed to write network XML: {}", e);
            NovaError::SystemCommandFailed
        })?;

        // Define the network
        let output = Command::new("virsh")
            .args(&["net-define", &temp_file])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to define network: {}", error);
            // Clean up temp file
            let _ = fs::remove_file(&temp_file);
            return Err(NovaError::SystemCommandFailed);
        }

        // Start the network
        let output = Command::new("virsh")
            .args(&["net-start", &network.name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to start network: {}", error);
        }

        // Set autostart if requested
        if network.autostart {
            let _ = Command::new("virsh")
                .args(&["net-autostart", &network.name])
                .output();
        }

        // Clean up temp file
        let _ = fs::remove_file(&temp_file);

        log_info!("Libvirt network {} created successfully", network.name);
        Ok(())
    }

    // Generate libvirt network XML
    fn generate_network_xml(&self, network: &LibvirtNetwork) -> Result<String> {
        let mut xml = String::new();

        xml.push_str(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
",
        );
        xml.push_str(&format!(
            "<network>
  <name>{}</name>
",
            network.name
        ));

        if let Some(uuid) = &network.uuid {
            xml.push_str(&format!(
                "  <uuid>{}</uuid>
",
                uuid
            ));
        }

        // Forward mode
        if let Some(forward) = &network.forward {
            if forward.mode == "nat" {
                xml.push_str(
                    "  <forward mode='nat'/>
",
                );
            } else if forward.mode == "bridge" {
                if let Some(dev) = &forward.dev {
                    xml.push_str(&format!(
                        "  <forward mode='bridge'>
    <interface dev='{}'/>
  </forward>
",
                        dev
                    ));
                } else {
                    xml.push_str(
                        "  <forward mode='bridge'/>
",
                    );
                }
            } else {
                xml.push_str(&format!(
                    "  <forward mode='{}'/>
",
                    forward.mode
                ));
            }
        }

        // Bridge configuration
        if let Some(bridge) = &network.bridge {
            let mut bridge_attrs = format!("name='{}'", bridge.name);
            if let Some(stp) = bridge.stp {
                bridge_attrs.push_str(&format!(" stp='{}'", if stp { "on" } else { "off" }));
            }
            if let Some(delay) = bridge.delay {
                bridge_attrs.push_str(&format!(" delay='{}'", delay));
            }
            xml.push_str(&format!(
                "  <bridge {}/>
",
                bridge_attrs
            ));
        }

        // IP configuration
        if let Some(ip) = &network.ip {
            xml.push_str(&format!(
                "  <ip address='{}' netmask='{}'>
",
                ip.address, ip.netmask
            ));

            if let Some(dhcp) = &ip.dhcp {
                xml.push_str(
                    "    <dhcp>
",
                );
                xml.push_str(&format!(
                    "      <range start='{}' end='{}'/>
",
                    dhcp.start, dhcp.end
                ));

                for host in &dhcp.hosts {
                    xml.push_str(&format!(
                        "      <host mac='{}' name='{}' ip='{}'/>
",
                        host.mac, host.name, host.ip
                    ));
                }

                xml.push_str(
                    "    </dhcp>
",
                );
            }

            xml.push_str(
                "  </ip>
",
            );
        }

        xml.push_str(
            "</network>
",
        );

        Ok(xml)
    }

    // Delete a libvirt network
    pub async fn delete_network(&mut self, name: &str) -> Result<()> {
        log_info!("Deleting libvirt network: {}", name);

        // Stop the network if it's running
        let _ = Command::new("virsh").args(&["net-destroy", name]).output();

        // Undefine the network
        let output = Command::new("virsh")
            .args(&["net-undefine", name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to undefine network: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        // Remove from our list
        self.networks.retain(|n| n.name != name);

        log_info!("Libvirt network {} deleted successfully", name);
        Ok(())
    }

    // Start/stop networks
    pub async fn start_network(&self, name: &str) -> Result<()> {
        log_info!("Starting libvirt network: {}", name);

        let output = Command::new("virsh")
            .args(&["net-start", name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to start network: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    pub async fn stop_network(&self, name: &str) -> Result<()> {
        log_info!("Stopping libvirt network: {}", name);

        let output = Command::new("virsh")
            .args(&["net-destroy", name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to stop network: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    // Get list of networks
    pub fn list_networks(&self) -> &Vec<LibvirtNetwork> {
        &self.networks
    }

    pub fn get_network(&self, name: &str) -> Option<&LibvirtNetwork> {
        self.networks.iter().find(|n| n.name == name)
    }

    // Create a default NAT network (similar to libvirt's default network)
    pub fn create_default_nat_network(&self, name: &str, subnet: &str) -> LibvirtNetwork {
        let network_addr =
            Ipv4Addr::from_str(&format!("{}.1", &subnet[..subnet.rfind('.').unwrap()])).unwrap();
        let netmask = Ipv4Addr::new(255, 255, 255, 0);
        let dhcp_start =
            Ipv4Addr::from_str(&format!("{}.2", &subnet[..subnet.rfind('.').unwrap()])).unwrap();
        let dhcp_end =
            Ipv4Addr::from_str(&format!("{}.254", &subnet[..subnet.rfind('.').unwrap()])).unwrap();

        LibvirtNetwork {
            name: name.to_string(),
            uuid: None,
            forward: Some(ForwardMode {
                mode: "nat".to_string(),
                dev: None,
                interfaces: Vec::new(),
            }),
            bridge: Some(BridgeConfig {
                name: format!("virbr-{}", name),
                stp: Some(true),
                delay: Some(0),
            }),
            ip: Some(IpConfig {
                address: network_addr,
                netmask,
                dhcp: Some(DhcpRange {
                    start: dhcp_start,
                    end: dhcp_end,
                    hosts: Vec::new(),
                }),
            }),
            dns: None,
            autostart: true,
            active: false,
        }
    }
}

impl Default for LibvirtManager {
    fn default() -> Self {
        Self::new()
    }
}
