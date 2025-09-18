use crate::{log_debug, log_error, log_info, log_warn, NovaError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub interface: String,
    pub timestamp: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_drops: u64,
    pub tx_drops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthUsage {
    pub interface: String,
    pub timestamp: u64,
    pub rx_bps: f64, // bytes per second
    pub tx_bps: f64,
    pub rx_pps: f64, // packets per second
    pub tx_pps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub local_addr: String,
    pub remote_addr: String,
    pub state: String,
    pub protocol: String,
    pub process: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketCaptureConfig {
    pub interface: String,
    pub filter: Option<String>, // BPF filter
    pub duration: Option<u64>,  // seconds
    pub packet_count: Option<u64>,
    pub output_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopology {
    pub bridges: Vec<TopologyBridge>,
    pub connections: Vec<TopologyConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyBridge {
    pub name: String,
    pub bridge_type: String, // "linux", "ovs"
    pub interfaces: Vec<String>,
    pub ip_address: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyConnection {
    pub from: String,
    pub to: String,
    pub connection_type: String, // "bridge", "veth", "tap"
    pub bandwidth: Option<f64>,
}

pub struct NetworkMonitor {
    stats_history: HashMap<String, Vec<NetworkStats>>,
    bandwidth_history: HashMap<String, Vec<BandwidthUsage>>,
    monitoring_active: bool,
}

impl NetworkMonitor {
    pub fn new() -> Self {
        Self {
            stats_history: HashMap::new(),
            bandwidth_history: HashMap::new(),
            monitoring_active: false,
        }
    }

    // Start continuous monitoring
    pub async fn start_monitoring(&mut self, interfaces: Vec<String>, interval_seconds: u64) -> Result<()> {
        log_info!("Starting network monitoring for {} interfaces", interfaces.len());

        self.monitoring_active = true;

        // Note: In a real implementation, this would need proper async handling
        // For now, we'll just store the config and implement polling differently
        log_info!("Network monitoring configured for interfaces: {:?}", interfaces);

        Ok(())
    }

    pub fn stop_monitoring(&mut self) {
        log_info!("Stopping network monitoring");
        self.monitoring_active = false;
    }

    // Get current network statistics for an interface
    pub async fn get_interface_stats(&self, interface: &str) -> Result<NetworkStats> {
        let proc_net_dev = std::fs::read_to_string("/proc/net/dev")
            .map_err(|_| NovaError::SystemCommandFailed)?;

        for line in proc_net_dev.lines() {
            if line.contains(interface) && line.contains(":") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 17 {
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    return Ok(NetworkStats {
                        interface: interface.to_string(),
                        timestamp,
                        rx_bytes: parts[1].parse().unwrap_or(0),
                        rx_packets: parts[2].parse().unwrap_or(0),
                        rx_errors: parts[3].parse().unwrap_or(0),
                        rx_drops: parts[4].parse().unwrap_or(0),
                        tx_bytes: parts[9].parse().unwrap_or(0),
                        tx_packets: parts[10].parse().unwrap_or(0),
                        tx_errors: parts[11].parse().unwrap_or(0),
                        tx_drops: parts[12].parse().unwrap_or(0),
                    });
                }
            }
        }

        Err(NovaError::NetworkNotFound(interface.to_string()))
    }

    // Calculate bandwidth between two stat samples
    fn calculate_bandwidth(&self, current: &NetworkStats, previous: &NetworkStats) -> Result<BandwidthUsage> {
        let time_diff = current.timestamp.saturating_sub(previous.timestamp) as f64;
        if time_diff == 0.0 {
            return Err(NovaError::InvalidConfig);
        }

        let rx_bytes_diff = current.rx_bytes.saturating_sub(previous.rx_bytes) as f64;
        let tx_bytes_diff = current.tx_bytes.saturating_sub(previous.tx_bytes) as f64;
        let rx_packets_diff = current.rx_packets.saturating_sub(previous.rx_packets) as f64;
        let tx_packets_diff = current.tx_packets.saturating_sub(previous.tx_packets) as f64;

        Ok(BandwidthUsage {
            interface: current.interface.clone(),
            timestamp: current.timestamp,
            rx_bps: rx_bytes_diff / time_diff,
            tx_bps: tx_bytes_diff / time_diff,
            rx_pps: rx_packets_diff / time_diff,
            tx_pps: tx_packets_diff / time_diff,
        })
    }

    // Get current bandwidth usage
    pub fn get_current_bandwidth(&self, interface: &str) -> Option<&BandwidthUsage> {
        self.bandwidth_history.get(interface)?.last()
    }

    // Get bandwidth history for an interface
    pub fn get_bandwidth_history(&self, interface: &str, limit: Option<usize>) -> Vec<&BandwidthUsage> {
        if let Some(history) = self.bandwidth_history.get(interface) {
            if let Some(limit) = limit {
                history.iter().rev().take(limit).collect()
            } else {
                history.iter().collect()
            }
        } else {
            Vec::new()
        }
    }

    // Packet Capture Integration
    pub async fn start_packet_capture(&self, config: &PacketCaptureConfig) -> Result<String> {
        log_info!("Starting packet capture on interface: {}", config.interface);

        let mut cmd = Command::new("tcpdump");
        cmd.args(&["-i", &config.interface]);
        cmd.args(&["-w", &config.output_file]);

        // Add filter if specified
        if let Some(filter) = &config.filter {
            cmd.arg(filter);
        }

        // Add packet count limit if specified
        if let Some(count) = config.packet_count {
            cmd.args(&["-c", &count.to_string()]);
        }

        // Set capture duration if specified
        if let Some(duration) = config.duration {
            cmd.args(&["-G", &duration.to_string()]);
            cmd.args(&["-W", "1"]); // Only one file
        }

        // Run in background
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                log_error!("Failed to start tcpdump: {}", e);
                NovaError::SystemCommandFailed
            })?;

        let pid = child.id();
        log_info!("Packet capture started with PID: {}", pid);

        Ok(format!("nova-capture-{}", pid))
    }

    pub async fn stop_packet_capture(&self, capture_id: &str) -> Result<()> {
        log_info!("Stopping packet capture: {}", capture_id);

        // Extract PID from capture_id
        if let Some(pid_str) = capture_id.strip_prefix("nova-capture-") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                let output = Command::new("kill")
                    .args(&["-TERM", &pid.to_string()])
                    .output()
                    .map_err(|_| NovaError::SystemCommandFailed)?;

                if output.status.success() {
                    log_info!("Packet capture {} stopped", capture_id);
                } else {
                    log_warn!("Failed to stop packet capture {}", capture_id);
                }
            }
        }

        Ok(())
    }

    // Launch Wireshark for analysis
    pub async fn launch_wireshark(&self, pcap_file: &str) -> Result<()> {
        log_info!("Launching Wireshark for file: {}", pcap_file);

        let _output = Command::new("wireshark")
            .arg(pcap_file)
            .spawn()
            .map_err(|e| {
                log_error!("Failed to launch Wireshark: {}", e);
                NovaError::SystemCommandFailed
            })?;

        log_info!("Wireshark launched for packet analysis");
        Ok(())
    }

    // Network Topology Discovery
    pub async fn discover_topology(&self) -> Result<NetworkTopology> {
        log_info!("Discovering network topology");

        let mut topology = NetworkTopology {
            bridges: Vec::new(),
            connections: Vec::new(),
        };

        // Discover Linux bridges
        self.discover_linux_bridges(&mut topology).await?;

        // Discover OVS bridges if available
        if self.check_ovs_available() {
            self.discover_ovs_bridges(&mut topology).await?;
        }

        // Discover connections between bridges and interfaces
        self.discover_connections(&mut topology).await?;

        log_info!("Discovered {} bridges and {} connections",
                 topology.bridges.len(), topology.connections.len());

        Ok(topology)
    }

    async fn discover_linux_bridges(&self, topology: &mut NetworkTopology) -> Result<()> {
        // Get list of bridges from /sys/class/net
        let sys_net = std::fs::read_dir("/sys/class/net")
            .map_err(|_| NovaError::SystemCommandFailed)?;

        for entry in sys_net {
            if let Ok(entry) = entry {
                let bridge_path = entry.path().join("bridge");
                if bridge_path.exists() {
                    let bridge_name = entry.file_name().to_string_lossy().to_string();

                    // Get bridge interfaces
                    let mut interfaces = Vec::new();
                    if let Ok(brif_dir) = std::fs::read_dir(bridge_path.join("brif")) {
                        for iface_entry in brif_dir {
                            if let Ok(iface_entry) = iface_entry {
                                interfaces.push(iface_entry.file_name().to_string_lossy().to_string());
                            }
                        }
                    }

                    // Get bridge IP address
                    let ip_address = self.get_interface_ip(&bridge_name).await.ok();

                    topology.bridges.push(TopologyBridge {
                        name: bridge_name,
                        bridge_type: "linux".to_string(),
                        interfaces,
                        ip_address,
                        status: "active".to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    async fn discover_ovs_bridges(&self, topology: &mut NetworkTopology) -> Result<()> {
        let output = Command::new("ovs-vsctl")
            .arg("list-br")
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Ok(());
        }

        let bridges = String::from_utf8_lossy(&output.stdout);
        for bridge_name in bridges.lines() {
            let bridge_name = bridge_name.trim();
            if !bridge_name.is_empty() {
                // Get bridge ports
                let port_output = Command::new("ovs-vsctl")
                    .args(&["list-ports", bridge_name])
                    .output()
                    .unwrap_or_else(|_| std::process::Output {
                        status: std::process::ExitStatus::from_raw(1),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    });

                let mut interfaces = Vec::new();
                if port_output.status.success() {
                    let ports = String::from_utf8_lossy(&port_output.stdout);
                    for port in ports.lines() {
                        let port = port.trim();
                        if !port.is_empty() {
                            interfaces.push(port.to_string());
                        }
                    }
                }

                let ip_address = self.get_interface_ip(bridge_name).await.ok();

                topology.bridges.push(TopologyBridge {
                    name: bridge_name.to_string(),
                    bridge_type: "ovs".to_string(),
                    interfaces,
                    ip_address,
                    status: "active".to_string(),
                });
            }
        }

        Ok(())
    }

    async fn discover_connections(&self, topology: &mut NetworkTopology) -> Result<()> {
        // Discover connections between bridges and their interfaces
        for bridge in &topology.bridges {
            for interface in &bridge.interfaces {
                topology.connections.push(TopologyConnection {
                    from: bridge.name.clone(),
                    to: interface.clone(),
                    connection_type: "bridge".to_string(),
                    bandwidth: None, // Could be populated from monitoring data
                });
            }
        }

        Ok(())
    }

    async fn get_interface_ip(&self, interface: &str) -> Result<String> {
        let output = Command::new("ip")
            .args(&["-4", "addr", "show", interface])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::NetworkNotFound(interface.to_string()));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines() {
            if line.contains("inet ") && !line.contains("127.0.0.1") {
                if let Some(start) = line.find("inet ") {
                    let ip_part = &line[start + 5..];
                    if let Some(end) = ip_part.find(' ') {
                        return Ok(ip_part[..end].to_string());
                    }
                }
            }
        }

        Err(NovaError::NetworkNotFound(interface.to_string()))
    }

    // Connection tracking
    pub async fn get_active_connections(&self) -> Result<Vec<ConnectionInfo>> {
        log_debug!("Getting active network connections");

        let output = Command::new("netstat")
            .args(&["-tuln"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let mut connections = Vec::new();
        let output_str = String::from_utf8_lossy(&output.stdout);

        for line in output_str.lines() {
            if line.starts_with("tcp") || line.starts_with("udp") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    connections.push(ConnectionInfo {
                        protocol: parts[0].to_string(),
                        local_addr: parts[3].to_string(),
                        remote_addr: if parts.len() > 4 { parts[4].to_string() } else { "*".to_string() },
                        state: if parts.len() > 5 { parts[5].to_string() } else { "UNKNOWN".to_string() },
                        process: None, // Would need additional parsing with -p flag
                    });
                }
            }
        }

        Ok(connections)
    }

    // Utility functions
    fn check_ovs_available(&self) -> bool {
        Command::new("ovs-vsctl")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn check_tcpdump_available(&self) -> bool {
        Command::new("tcpdump")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn check_wireshark_available(&self) -> bool {
        Command::new("wireshark")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}