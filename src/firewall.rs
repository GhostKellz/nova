use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FirewallBackend {
    Iptables,
    Nftables,
    Firewalld,
    Ufw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    pub id: String,
    pub chain: String,
    pub table: String,
    pub target: RuleTarget,
    pub protocol: Option<Protocol>,
    pub source: Option<NetworkAddress>,
    pub destination: Option<NetworkAddress>,
    pub port_range: Option<PortRange>,
    pub interface: Option<String>,
    pub state: Option<ConnectionState>,
    pub comment: Option<String>,
    pub enabled: bool,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuleTarget {
    Accept,
    Drop,
    Reject,
    Log,
    Return,
    Jump(String),
    Goto(String),
    Masquerade,
    Snat(String),
    Dnat(String),
    Redirect(u16),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    All,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkAddress {
    pub address: String,
    pub cidr: Option<u8>,
    pub negated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    pub start: u16,
    pub end: Option<u16>,
    pub negated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionState {
    New,
    Established,
    Related,
    Invalid,
    Untracked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallChain {
    pub name: String,
    pub table: String,
    pub policy: ChainPolicy,
    pub rules: Vec<FirewallRule>,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChainPolicy {
    Accept,
    Drop,
    Return,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallTable {
    pub name: String,
    pub chains: HashMap<String, FirewallChain>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficFlow {
    pub id: String,
    pub source_ip: String,
    pub dest_ip: String,
    pub source_port: u16,
    pub dest_port: u16,
    pub protocol: Protocol,
    pub packets_per_second: u64,
    pub bytes_per_second: u64,
    pub rule_path: Vec<String>, // Which rules this traffic matched
    pub verdict: RuleTarget,
    pub interface_in: Option<String>,
    pub interface_out: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ip_addresses: Vec<String>,
    pub mac_address: String,
    pub mtu: u16,
    pub state: InterfaceState,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InterfaceState {
    Up,
    Down,
    Unknown,
}

pub struct FirewallManager {
    backend: FirewallBackend,
    tables: HashMap<String, FirewallTable>,
    active_flows: Vec<TrafficFlow>,
    interfaces: HashMap<String, NetworkInterface>,
    rule_conflicts: Vec<RuleConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConflict {
    pub rule1_id: String,
    pub rule2_id: String,
    pub conflict_type: ConflictType,
    pub description: String,
    pub severity: ConflictSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictType {
    Shadowing,     // Rule never matches due to earlier rule
    Redundant,     // Rule duplicates existing functionality
    Contradictory, // Rules have opposing effects
    Performance,   // Rule ordering causes performance issues
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl FirewallManager {
    pub fn new() -> Result<Self> {
        let backend = Self::detect_backend()?;
        log_info!("Detected firewall backend: {:?}", backend);

        Ok(Self {
            backend,
            tables: HashMap::new(),
            active_flows: Vec::new(),
            interfaces: HashMap::new(),
            rule_conflicts: Vec::new(),
        })
    }

    fn detect_backend() -> Result<FirewallBackend> {
        // Check for firewalld first (most user-friendly)
        if Self::command_exists("firewall-cmd") {
            return Ok(FirewallBackend::Firewalld);
        }

        // Check for nftables
        if Self::command_exists("nft") {
            return Ok(FirewallBackend::Nftables);
        }

        // Check for iptables
        if Self::command_exists("iptables") {
            return Ok(FirewallBackend::Iptables);
        }

        // Check for ufw
        if Self::command_exists("ufw") {
            return Ok(FirewallBackend::Ufw);
        }

        Err(NovaError::SystemCommandFailed)
    }

    fn command_exists(command: &str) -> bool {
        Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub async fn load_current_rules(&mut self) -> Result<()> {
        log_info!("Loading current firewall rules from {:?}", self.backend);

        match self.backend {
            FirewallBackend::Iptables => self.load_iptables_rules().await,
            FirewallBackend::Nftables => self.load_nftables_rules().await,
            FirewallBackend::Firewalld => self.load_firewalld_rules().await,
            FirewallBackend::Ufw => self.load_ufw_rules().await,
        }
    }

    async fn load_iptables_rules(&mut self) -> Result<()> {
        // Load filter table
        self.load_iptables_table("filter").await?;

        // Load nat table
        self.load_iptables_table("nat").await?;

        // Load mangle table
        self.load_iptables_table("mangle").await?;

        Ok(())
    }

    async fn load_iptables_table(&mut self, table_name: &str) -> Result<()> {
        let output = Command::new("iptables")
            .args(&["-t", table_name, "-L", "-n", "-v", "--line-numbers"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let rules_text = String::from_utf8_lossy(&output.stdout);
        let table = self.parse_iptables_output(table_name, &rules_text)?;
        self.tables.insert(table_name.to_string(), table);

        Ok(())
    }

    fn parse_iptables_output(&self, table_name: &str, output: &str) -> Result<FirewallTable> {
        let mut chains = HashMap::new();
        let mut current_chain: Option<String> = None;
        let mut current_rules = Vec::new();

        for line in output.lines() {
            if line.starts_with("Chain ") {
                // Save previous chain if exists
                if let Some(chain_name) = current_chain.take() {
                    let chain = FirewallChain {
                        name: chain_name.clone(),
                        table: table_name.to_string(),
                        policy: ChainPolicy::Accept, // Parse from line
                        rules: current_rules,
                        packets: 0,
                        bytes: 0,
                    };
                    chains.insert(chain_name, chain);
                    current_rules = Vec::new();
                }

                // Parse new chain
                if let Some(chain_name) = line.split_whitespace().nth(1) {
                    current_chain = Some(chain_name.to_string());
                }
            } else if line.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                // This is a rule line, parse it
                if let Ok(rule) = self.parse_iptables_rule_line(line, table_name) {
                    current_rules.push(rule);
                }
            }
        }

        // Save last chain
        if let Some(chain_name) = current_chain {
            let chain = FirewallChain {
                name: chain_name.clone(),
                table: table_name.to_string(),
                policy: ChainPolicy::Accept,
                rules: current_rules,
                packets: 0,
                bytes: 0,
            };
            chains.insert(chain_name, chain);
        }

        Ok(FirewallTable {
            name: table_name.to_string(),
            chains,
        })
    }

    fn parse_iptables_rule_line(&self, line: &str, table: &str) -> Result<FirewallRule> {
        let parts: Vec<&str> = line.split_whitespace().collect();

        // Basic parsing - would need more sophisticated parsing for real implementation
        let rule = FirewallRule {
            id: uuid::Uuid::new_v4().to_string(),
            chain: "INPUT".to_string(), // Would parse from context
            table: table.to_string(),
            target: RuleTarget::Accept, // Parse from parts
            protocol: None,             // Parse from parts
            source: None,               // Parse from parts
            destination: None,          // Parse from parts
            port_range: None,
            interface: None,
            state: None,
            comment: None,
            enabled: true,
            priority: 0,
        };

        Ok(rule)
    }

    async fn load_nftables_rules(&mut self) -> Result<()> {
        let output = Command::new("nft")
            .args(&["list", "ruleset"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let rules_text = String::from_utf8_lossy(&output.stdout);
        self.parse_nftables_output(&rules_text)?;

        Ok(())
    }

    fn parse_nftables_output(&mut self, _output: &str) -> Result<()> {
        // Complex nftables parsing would go here
        // For now, just create placeholder data
        Ok(())
    }

    async fn load_firewalld_rules(&mut self) -> Result<()> {
        // Load zones
        let zones = self.get_firewalld_zones().await?;

        for zone in zones {
            self.load_firewalld_zone_rules(&zone).await?;
        }

        Ok(())
    }

    async fn get_firewalld_zones(&self) -> Result<Vec<String>> {
        let output = Command::new("firewall-cmd")
            .args(&["--get-zones"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let zones_text = String::from_utf8_lossy(&output.stdout);
        Ok(zones_text
            .split_whitespace()
            .map(|s| s.to_string())
            .collect())
    }

    async fn load_firewalld_zone_rules(&mut self, _zone: &str) -> Result<()> {
        // Load firewalld zone rules
        Ok(())
    }

    async fn load_ufw_rules(&mut self) -> Result<()> {
        let output = Command::new("ufw")
            .args(&["status", "numbered"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let rules_text = String::from_utf8_lossy(&output.stdout);
        self.parse_ufw_output(&rules_text)?;

        Ok(())
    }

    fn parse_ufw_output(&mut self, _output: &str) -> Result<()> {
        // Parse UFW output
        Ok(())
    }

    pub async fn add_rule(&mut self, rule: FirewallRule) -> Result<()> {
        log_info!(
            "Adding firewall rule: {:?}",
            rule.comment.as_deref().unwrap_or("unnamed")
        );

        // Validate rule
        self.validate_rule(&rule)?;

        // Apply rule to backend
        match self.backend {
            FirewallBackend::Iptables => self.add_iptables_rule(&rule).await,
            FirewallBackend::Nftables => self.add_nftables_rule(&rule).await,
            FirewallBackend::Firewalld => self.add_firewalld_rule(&rule).await,
            FirewallBackend::Ufw => self.add_ufw_rule(&rule).await,
        }?;

        // Add to local state
        if let Some(table) = self.tables.get_mut(&rule.table) {
            if let Some(chain) = table.chains.get_mut(&rule.chain) {
                chain.rules.push(rule);
            }
        }

        // Reanalyze for conflicts
        self.analyze_rule_conflicts().await?;

        Ok(())
    }

    fn validate_rule(&self, rule: &FirewallRule) -> Result<()> {
        // Validate rule syntax and parameters
        if rule.chain.is_empty() {
            return Err(NovaError::ConfigError(
                "Chain name cannot be empty".to_string(),
            ));
        }

        if rule.table.is_empty() {
            return Err(NovaError::ConfigError(
                "Table name cannot be empty".to_string(),
            ));
        }

        // Additional validation logic
        Ok(())
    }

    async fn add_iptables_rule(&self, rule: &FirewallRule) -> Result<()> {
        let mut cmd = Command::new("iptables");
        cmd.args(&["-t", &rule.table, "-A", &rule.chain]);

        // Build iptables command from rule
        if let Some(protocol) = &rule.protocol {
            match protocol {
                Protocol::Tcp => cmd.args(&["-p", "tcp"]),
                Protocol::Udp => cmd.args(&["-p", "udp"]),
                Protocol::Icmp => cmd.args(&["-p", "icmp"]),
                Protocol::All => &mut cmd,
                Protocol::Custom(p) => cmd.args(&["-p", p]),
            };
        }

        if let Some(source) = &rule.source {
            if source.negated {
                cmd.arg("!");
            }
            cmd.args(&["-s", &source.address]);
        }

        if let Some(dest) = &rule.destination {
            if dest.negated {
                cmd.arg("!");
            }
            cmd.args(&["-d", &dest.address]);
        }

        if let Some(port_range) = &rule.port_range {
            if port_range.negated {
                cmd.arg("!");
            }
            cmd.args(&["--dport", &port_range.start.to_string()]);
        }

        match &rule.target {
            RuleTarget::Accept => cmd.args(&["-j", "ACCEPT"]),
            RuleTarget::Drop => cmd.args(&["-j", "DROP"]),
            RuleTarget::Reject => cmd.args(&["-j", "REJECT"]),
            RuleTarget::Log => cmd.args(&["-j", "LOG"]),
            _ => &mut cmd,
        };

        let output = cmd.output().map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to add iptables rule: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    async fn add_nftables_rule(&self, _rule: &FirewallRule) -> Result<()> {
        // Implement nftables rule addition
        Ok(())
    }

    async fn add_firewalld_rule(&self, _rule: &FirewallRule) -> Result<()> {
        // Implement firewalld rule addition
        Ok(())
    }

    async fn add_ufw_rule(&self, _rule: &FirewallRule) -> Result<()> {
        // Implement UFW rule addition
        Ok(())
    }

    pub async fn remove_rule(&mut self, rule_id: &str) -> Result<()> {
        log_info!("Removing firewall rule: {}", rule_id);

        // Find and remove rule from local state
        for table in self.tables.values_mut() {
            for chain in table.chains.values_mut() {
                if let Some(pos) = chain.rules.iter().position(|r| r.id == rule_id) {
                    let rule = chain.rules.remove(pos);

                    // Remove from backend
                    match self.backend {
                        FirewallBackend::Iptables => self.remove_iptables_rule(&rule).await?,
                        FirewallBackend::Nftables => self.remove_nftables_rule(&rule).await?,
                        FirewallBackend::Firewalld => self.remove_firewalld_rule(&rule).await?,
                        FirewallBackend::Ufw => self.remove_ufw_rule(&rule).await?,
                    }

                    return Ok(());
                }
            }
        }

        Err(NovaError::NetworkNotFound(format!(
            "Rule {} not found",
            rule_id
        )))
    }

    async fn remove_iptables_rule(&self, _rule: &FirewallRule) -> Result<()> {
        // Implement iptables rule removal
        Ok(())
    }

    async fn remove_nftables_rule(&self, _rule: &FirewallRule) -> Result<()> {
        Ok(())
    }

    async fn remove_firewalld_rule(&self, _rule: &FirewallRule) -> Result<()> {
        Ok(())
    }

    async fn remove_ufw_rule(&self, _rule: &FirewallRule) -> Result<()> {
        Ok(())
    }

    pub async fn analyze_rule_conflicts(&mut self) -> Result<()> {
        log_debug!("Analyzing firewall rule conflicts");

        self.rule_conflicts.clear();

        // Collect chains to analyze to avoid borrowing conflicts
        let mut chains_to_analyze = Vec::new();
        for table in self.tables.values() {
            for chain in table.chains.values() {
                chains_to_analyze.push(chain.clone());
            }
        }

        // Analyze conflicts without holding immutable borrow
        for chain in chains_to_analyze {
            self.analyze_chain_conflicts(&chain)?;
        }

        log_info!("Found {} rule conflicts", self.rule_conflicts.len());
        Ok(())
    }

    fn analyze_chain_conflicts(&mut self, chain: &FirewallChain) -> Result<()> {
        let rules = &chain.rules;

        for (i, rule1) in rules.iter().enumerate() {
            for (j, rule2) in rules.iter().enumerate() {
                if i >= j {
                    continue;
                }

                if let Some(conflict) = self.detect_conflict(rule1, rule2, i < j) {
                    self.rule_conflicts.push(conflict);
                }
            }
        }

        Ok(())
    }

    fn detect_conflict(
        &self,
        rule1: &FirewallRule,
        rule2: &FirewallRule,
        rule1_first: bool,
    ) -> Option<RuleConflict> {
        // Check for shadowing
        if rule1_first && self.rules_overlap(rule1, rule2) && rule1.target != rule2.target {
            return Some(RuleConflict {
                rule1_id: rule1.id.clone(),
                rule2_id: rule2.id.clone(),
                conflict_type: ConflictType::Shadowing,
                description: "Rule is shadowed by earlier rule".to_string(),
                severity: ConflictSeverity::High,
            });
        }

        // Check for redundancy
        if self.rules_equivalent(rule1, rule2) {
            return Some(RuleConflict {
                rule1_id: rule1.id.clone(),
                rule2_id: rule2.id.clone(),
                conflict_type: ConflictType::Redundant,
                description: "Rules are functionally equivalent".to_string(),
                severity: ConflictSeverity::Medium,
            });
        }

        None
    }

    fn rules_overlap(&self, _rule1: &FirewallRule, _rule2: &FirewallRule) -> bool {
        // Complex logic to determine if rules overlap
        false
    }

    fn rules_equivalent(&self, rule1: &FirewallRule, rule2: &FirewallRule) -> bool {
        rule1.protocol == rule2.protocol
            && rule1.source == rule2.source
            && rule1.destination == rule2.destination
            && rule1.target == rule2.target
    }

    pub async fn monitor_traffic_flows(&mut self) -> Result<()> {
        log_debug!("Starting traffic flow monitoring");

        // Use netstat/ss to get active connections
        self.capture_active_connections().await?;

        // Use netfilter logs to track rule matches
        self.analyze_netfilter_logs().await?;

        Ok(())
    }

    async fn capture_active_connections(&mut self) -> Result<()> {
        let output = Command::new("ss")
            .args(&["-tuln"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if output.status.success() {
            let connections = String::from_utf8_lossy(&output.stdout);
            self.parse_connections(&connections)?;
        }

        Ok(())
    }

    fn parse_connections(&mut self, _connections: &str) -> Result<()> {
        // Parse ss output and create TrafficFlow objects
        Ok(())
    }

    async fn analyze_netfilter_logs(&mut self) -> Result<()> {
        // Monitor /proc/net/netfilter/nfnetlink_log or similar
        Ok(())
    }

    pub fn get_tables(&self) -> &HashMap<String, FirewallTable> {
        &self.tables
    }

    pub fn get_active_flows(&self) -> &[TrafficFlow] {
        &self.active_flows
    }

    pub fn get_rule_conflicts(&self) -> &[RuleConflict] {
        &self.rule_conflicts
    }

    pub fn get_backend(&self) -> &FirewallBackend {
        &self.backend
    }

    pub async fn optimize_rules(&mut self) -> Result<Vec<String>> {
        log_info!("Optimizing firewall rules");

        let mut suggestions = Vec::new();

        // Analyze rule ordering for performance
        suggestions.extend(self.analyze_rule_performance());

        // Suggest rule consolidation
        suggestions.extend(self.suggest_rule_consolidation());

        // Detect unused rules
        suggestions.extend(self.detect_unused_rules().await?);

        Ok(suggestions)
    }

    fn analyze_rule_performance(&self) -> Vec<String> {
        let mut suggestions = Vec::new();

        for table in self.tables.values() {
            for chain in table.chains.values() {
                if chain.rules.len() > 20 {
                    suggestions.push(format!(
                        "Chain {}.{} has {} rules - consider optimization",
                        table.name,
                        chain.name,
                        chain.rules.len()
                    ));
                }
            }
        }

        suggestions
    }

    fn suggest_rule_consolidation(&self) -> Vec<String> {
        // Analyze rules that could be consolidated
        Vec::new()
    }

    async fn detect_unused_rules(&self) -> Result<Vec<String>> {
        // Analyze traffic to detect rules that never match
        Ok(Vec::new())
    }
}

impl Default for FirewallManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            backend: FirewallBackend::Iptables,
            tables: HashMap::new(),
            active_flows: Vec::new(),
            interfaces: HashMap::new(),
            rule_conflicts: Vec::new(),
        })
    }
}

// Network visualization data structures for GUI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopology {
    pub nodes: Vec<NetworkNode>,
    pub connections: Vec<NetworkConnection>,
    pub traffic_flows: Vec<VisualTrafficFlow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNode {
    pub id: String,
    pub name: String,
    pub node_type: NodeType,
    pub position: (f32, f32, f32), // 3D position for visualization
    pub status: NodeStatus,
    pub interfaces: Vec<String>,
    pub rules_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    Router,
    Switch,
    Bridge,
    Firewall,
    Host,
    Container,
    VM,
    Internet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeStatus {
    Active,
    Inactive,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConnection {
    pub id: String,
    pub from_node: String,
    pub to_node: String,
    pub connection_type: ConnectionType,
    pub bandwidth: u64,
    pub latency: f32,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionType {
    Physical,
    Virtual,
    Tunnel,
    Bridge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionStatus {
    Up,
    Down,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualTrafficFlow {
    pub id: String,
    pub path: Vec<String>, // Node IDs
    pub protocol: Protocol,
    pub bandwidth_usage: f32, // 0.0 to 1.0
    pub packet_count: u64,
    pub flow_color: (u8, u8, u8), // RGB color for visualization
    pub animation_speed: f32,
}
