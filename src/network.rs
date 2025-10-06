use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use dirs;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, Instant};

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

const NETWORK_STATE_DIR_FALLBACK: &str = "/var/lib/nova/networks";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwitchProfile {
    Internal,
    External {
        uplink: String,
    },
    Nat {
        uplink: String,
        subnet_cidr: String,
        dhcp_range_start: Option<Ipv4Addr>,
        dhcp_range_end: Option<Ipv4Addr>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualSwitch {
    pub name: String,
    pub switch_type: SwitchType,
    pub interfaces: Vec<String>,
    pub vlan_id: Option<u16>,
    pub stp_enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: SwitchStatus,
    pub origin: SwitchOrigin,
    #[serde(default)]
    pub profile: Option<SwitchProfile>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SwitchOrigin {
    Nova,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterfaceState {
    Up,
    Down,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: String,
    pub ip_address: Option<Ipv4Addr>,
    pub state: InterfaceState,
    pub bridge: Option<String>,
    pub speed: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkSummary {
    pub total_switches: usize,
    pub active_switches: usize,
    pub nova_managed_switches: usize,
    pub system_switches: usize,
    pub total_interfaces: usize,
    pub interfaces_up: usize,
    pub interfaces_down: usize,
    pub interfaces_unknown: usize,
    pub last_refresh_at: Option<chrono::DateTime<chrono::Utc>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSwitch {
    name: String,
    switch_type: SwitchType,
    profile: Option<SwitchProfile>,
}

fn network_state_dir() -> PathBuf {
    if let Some(mut dir) = dirs::data_dir() {
        dir.push("nova");
        dir.push("networks");
        dir
    } else {
        PathBuf::from(NETWORK_STATE_DIR_FALLBACK)
    }
}

fn network_state_file(name: &str) -> PathBuf {
    let mut path = network_state_dir();
    path.push(format!("{}.json", name));
    path
}

fn load_all_persisted_switches() -> Result<Vec<PersistedSwitch>> {
    let dir = network_state_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut persisted = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(entry.path())?;
        if let Ok(state) = serde_json::from_str::<PersistedSwitch>(&content) {
            persisted.push(state);
        }
    }

    Ok(persisted)
}

fn load_persisted_switch(name: &str) -> Result<Option<PersistedSwitch>> {
    let path = network_state_file(name);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let state: PersistedSwitch = serde_json::from_str(&content)?;
    Ok(Some(state))
}

fn persist_switch_state(state: &PersistedSwitch) -> Result<()> {
    let dir = network_state_dir();
    fs::create_dir_all(&dir)?;
    let path = network_state_file(&state.name);
    let payload = serde_json::to_string_pretty(state)?;
    fs::write(path, payload)?;
    Ok(())
}

fn remove_persisted_switch(name: &str) -> Result<()> {
    let path = network_state_file(name);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub struct NetworkManager {
    switches: HashMap<String, VirtualSwitch>,
    interfaces: HashMap<String, NetworkInterface>,
    restored_profiles: HashSet<String>,
    last_refresh_at: Option<chrono::DateTime<chrono::Utc>>,
    last_refresh_instant: Option<Instant>,
    refresh_interval: Duration,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            switches: HashMap::new(),
            interfaces: HashMap::new(),
            restored_profiles: HashSet::new(),
            last_refresh_at: None,
            last_refresh_instant: None,
            refresh_interval: Duration::from_secs(10),
        }
    }

    pub async fn refresh_state(&mut self) -> Result<()> {
        let mut retained = HashMap::new();
        for (name, switch) in self.switches.iter() {
            if switch.origin == SwitchOrigin::Nova {
                retained.insert(name.clone(), switch.clone());
            }
        }

        self.switches = retained;
        self.restored_profiles
            .retain(|name| self.switches.contains_key(name));
        self.interfaces.clear();

        self.discover_interfaces().await?;
        self.rebuild_switch_memberships();
        self.hydrate_persisted_switches().await?;

        self.last_refresh_instant = Some(Instant::now());
        self.last_refresh_at = Some(chrono::Utc::now());

        Ok(())
    }

    pub fn set_refresh_interval(&mut self, interval: Duration) {
        self.refresh_interval = interval;
    }

    async fn hydrate_persisted_switches(&mut self) -> Result<()> {
        let persisted = load_all_persisted_switches()?;
        for state in persisted {
            let switch_name = state.name.clone();

            if let Some(existing) = self.switches.get_mut(&switch_name) {
                existing.origin = SwitchOrigin::Nova;
                existing.switch_type = state.switch_type.clone();
                existing.profile = state.profile.clone();
            } else {
                self.switches.insert(
                    switch_name.clone(),
                    VirtualSwitch {
                        name: switch_name.clone(),
                        switch_type: state.switch_type.clone(),
                        interfaces: Vec::new(),
                        vlan_id: None,
                        stp_enabled: false,
                        created_at: chrono::Utc::now(),
                        status: SwitchStatus::Inactive,
                        origin: SwitchOrigin::Nova,
                        profile: state.profile.clone(),
                    },
                );
            }

            let mut bridge_ready = bridge_exists(&switch_name);
            if !bridge_ready {
                match state.switch_type {
                    SwitchType::LinuxBridge => {
                        if let Err(err) = self.create_linux_bridge(&switch_name).await {
                            log_error!("Failed to recreate Linux bridge {}: {}", switch_name, err);
                            continue;
                        }
                        bridge_ready = true;
                    }
                    SwitchType::OpenVSwitch => {
                        if let Err(err) = self.create_ovs_bridge(&switch_name).await {
                            log_error!("Failed to recreate OVS bridge {}: {}", switch_name, err);
                            continue;
                        }
                        bridge_ready = true;
                    }
                }
            }

            if let Some(switch) = self.switches.get_mut(&switch_name) {
                if bridge_ready || is_test_mode() {
                    switch.status = SwitchStatus::Active;
                }
            }

            if let Some(profile) = state.profile.clone() {
                if self.restored_profiles.contains(&switch_name) {
                    continue;
                }

                #[cfg(test)]
                record_restore_attempt(&state.name);

                match self.restore_persisted_profile(&state, &profile).await {
                    Ok(()) => {
                        self.restored_profiles.insert(switch_name.clone());
                        if let Some(switch) = self.switches.get_mut(&switch_name) {
                            switch.status = SwitchStatus::Active;
                        }
                    }
                    Err(err) => {
                        log_error!("Failed to restore profile for {}: {}", switch_name, err);
                    }
                }
            }
        }

        Ok(())
    }

    async fn restore_persisted_profile(
        &mut self,
        state: &PersistedSwitch,
        profile: &SwitchProfile,
    ) -> Result<()> {
        match profile {
            SwitchProfile::Internal => Ok(()),
            SwitchProfile::External { uplink } => {
                let already_attached = self
                    .interfaces
                    .get(uplink)
                    .and_then(|iface| iface.bridge.as_ref())
                    .map_or(false, |bridge| bridge == &state.name);

                if !already_attached {
                    self.add_interface_to_switch(&state.name, uplink).await?;
                } else if let Some(switch) = self.switches.get_mut(&state.name) {
                    if !switch.interfaces.iter().any(|iface| iface == uplink) {
                        switch.interfaces.push(uplink.clone());
                    }
                }

                if let Some(iface) = self.interfaces.get_mut(uplink) {
                    iface.bridge = Some(state.name.clone());
                }

                if let Some(switch) = self.switches.get_mut(&state.name) {
                    switch.status = SwitchStatus::Active;
                }

                Ok(())
            }
            SwitchProfile::Nat {
                uplink,
                subnet_cidr,
                dhcp_range_start,
                dhcp_range_end,
            } => {
                self.assign_bridge_address(&state.name, subnet_cidr, None)
                    .await?;

                let (gateway_ip, prefix) = parse_cidr(subnet_cidr)?;
                let mask = prefix_to_mask(prefix)
                    .ok_or_else(|| NovaError::ConfigError("Invalid subnet prefix".to_string()))?;
                let subnet_mask = mask_to_ipv4(mask);

                let (range_start, range_end) = match (dhcp_range_start, dhcp_range_end) {
                    (Some(start), Some(end)) => (*start, *end),
                    (None, None) => default_dhcp_range(gateway_ip, prefix).ok_or_else(|| {
                        NovaError::ConfigError(
                            "Unable to derive DHCP range from provided subnet".to_string(),
                        )
                    })?,
                    _ => {
                        return Err(NovaError::ConfigError(
                            "DHCP range requires both start and end addresses".to_string(),
                        ));
                    }
                };

                let dhcp_config = DhcpConfig {
                    enabled: true,
                    range_start,
                    range_end,
                    subnet_mask,
                    gateway: gateway_ip,
                    dns_servers: vec![gateway_ip],
                    lease_time: 86_400,
                };

                let _ = self.stop_dhcp(&state.name).await;
                self.configure_dhcp(&dhcp_config, &state.name).await?;

                let nat_config = NatConfig {
                    enabled: true,
                    internal_interface: state.name.clone(),
                    external_interface: uplink.clone(),
                    masquerade: true,
                };

                self.configure_nat(&nat_config).await?;

                if let Some(switch) = self.switches.get_mut(&state.name) {
                    switch.status = SwitchStatus::Active;
                }

                Ok(())
            }
        }
    }

    pub async fn ensure_fresh_state(&mut self) -> Result<()> {
        let should_refresh = self
            .last_refresh_instant
            .map(|instant| instant.elapsed() >= self.refresh_interval)
            .unwrap_or(true);

        if should_refresh {
            self.refresh_state().await?;
        }

        Ok(())
    }

    pub fn summary(&self) -> NetworkSummary {
        let total_switches = self.switches.len();
        let active_switches = self
            .switches
            .values()
            .filter(|switch| matches!(switch.status, SwitchStatus::Active))
            .count();
        let nova_managed_switches = self
            .switches
            .values()
            .filter(|switch| switch.origin == SwitchOrigin::Nova)
            .count();
        let system_switches = self
            .switches
            .values()
            .filter(|switch| switch.origin == SwitchOrigin::System)
            .count();

        let total_interfaces = self.interfaces.len();
        let interfaces_up = self
            .interfaces
            .values()
            .filter(|iface| matches!(iface.state, InterfaceState::Up))
            .count();
        let interfaces_down = self
            .interfaces
            .values()
            .filter(|iface| matches!(iface.state, InterfaceState::Down))
            .count();
        let interfaces_unknown = self
            .interfaces
            .values()
            .filter(|iface| matches!(iface.state, InterfaceState::Unknown))
            .count();

        NetworkSummary {
            total_switches,
            active_switches,
            nova_managed_switches,
            system_switches,
            total_interfaces,
            interfaces_up,
            interfaces_down,
            interfaces_unknown,
            last_refresh_at: self.last_refresh_at.clone(),
        }
    }

    fn rebuild_switch_memberships(&mut self) {
        for switch in self.switches.values_mut() {
            switch.interfaces.clear();
        }

        for (iface_name, interface) in &self.interfaces {
            if let Some(master) = &interface.bridge {
                if let Some(switch) = self.switches.get_mut(master) {
                    if !switch.interfaces.contains(iface_name) {
                        switch.interfaces.push(iface_name.clone());
                    }
                }
            }
        }
    }

    // Virtual Switch Management
    pub async fn create_virtual_switch(
        &mut self,
        name: &str,
        switch_type: SwitchType,
        profile: Option<SwitchProfile>,
    ) -> Result<()> {
        log_info!("Creating virtual switch: {} ({:?})", name, switch_type);

        let has_profile = profile.is_some();
        match &switch_type {
            SwitchType::LinuxBridge => {
                self.create_linux_bridge(name).await?;
            }
            SwitchType::OpenVSwitch => {
                self.create_ovs_bridge(name).await?;
            }
        }

        let profile_to_apply = profile.clone();

        let switch = VirtualSwitch {
            name: name.to_string(),
            switch_type: switch_type.clone(),
            interfaces: Vec::new(),
            vlan_id: None,
            stp_enabled: false,
            created_at: chrono::Utc::now(),
            status: SwitchStatus::Active,
            origin: SwitchOrigin::Nova,
            profile: profile.clone(),
        };

        self.switches.insert(name.to_string(), switch);

        if let Some(profile) = profile_to_apply {
            match profile {
                SwitchProfile::Internal => {}
                SwitchProfile::External { uplink } => {
                    if let Err(err) = self.add_interface_to_switch(name, &uplink).await {
                        let _ = self.delete_virtual_switch(name).await;
                        return Err(err);
                    }
                }
                SwitchProfile::Nat {
                    uplink,
                    subnet_cidr,
                    dhcp_range_start,
                    dhcp_range_end,
                } => {
                    if let Err(err) = self.assign_bridge_address(name, &subnet_cidr, None).await {
                        let _ = self.delete_virtual_switch(name).await;
                        return Err(err);
                    }

                    let (gateway_ip, prefix) = match parse_cidr(&subnet_cidr) {
                        Ok(values) => values,
                        Err(err) => {
                            let _ = self.delete_virtual_switch(name).await;
                            return Err(err);
                        }
                    };
                    let mask = match prefix_to_mask(prefix) {
                        Some(mask) => mask,
                        None => {
                            let _ = self.delete_virtual_switch(name).await;
                            return Err(NovaError::ConfigError(
                                "Invalid subnet prefix".to_string(),
                            ));
                        }
                    };
                    let subnet_mask = mask_to_ipv4(mask);

                    let (range_start, range_end) = match (dhcp_range_start, dhcp_range_end) {
                        (Some(start), Some(end)) => (start, end),
                        (None, None) => match default_dhcp_range(gateway_ip, prefix) {
                            Some(range) => range,
                            None => {
                                let _ = self.delete_virtual_switch(name).await;
                                return Err(NovaError::ConfigError(
                                    "Unable to derive DHCP range from provided subnet".to_string(),
                                ));
                            }
                        },
                        _ => {
                            let _ = self.delete_virtual_switch(name).await;
                            return Err(NovaError::ConfigError(
                                "DHCP range requires both start and end addresses".to_string(),
                            ));
                        }
                    };

                    let dhcp_config = DhcpConfig {
                        enabled: true,
                        range_start,
                        range_end,
                        subnet_mask,
                        gateway: gateway_ip,
                        dns_servers: vec![gateway_ip],
                        lease_time: 86_400,
                    };

                    if let Err(err) = self.configure_dhcp(&dhcp_config, name).await {
                        let _ = self.delete_virtual_switch(name).await;
                        return Err(err);
                    }

                    let nat_config = NatConfig {
                        enabled: true,
                        internal_interface: name.to_string(),
                        external_interface: uplink.clone(),
                        masquerade: true,
                    };

                    if let Err(err) = self.configure_nat(&nat_config).await {
                        let _ = self.stop_dhcp(name).await;
                        let _ = self.delete_virtual_switch(name).await;
                        return Err(err);
                    }
                }
            }
        }

        let persisted = PersistedSwitch {
            name: name.to_string(),
            switch_type,
            profile,
        };
        persist_switch_state(&persisted)?;
        if has_profile {
            self.restored_profiles.insert(name.to_string());
        }
        Ok(())
    }

    async fn create_linux_bridge(&self, name: &str) -> Result<()> {
        if is_test_mode() {
            log_debug!("[test] Pretending to create Linux bridge {}", name);
            return Ok(());
        }

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

        if is_test_mode() {
            log_debug!("[test] Pretending to create OVS bridge {}", name);
            return Ok(());
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

        let persisted_state = load_persisted_switch(name)?;

        let profile = self
            .switches
            .get(name)
            .and_then(|switch| switch.profile.clone())
            .or_else(|| {
                persisted_state
                    .as_ref()
                    .and_then(|state| state.profile.clone())
            });

        if let Some(profile) = profile {
            match profile {
                SwitchProfile::Nat { uplink, .. } => {
                    let nat_config = NatConfig {
                        enabled: false,
                        internal_interface: name.to_string(),
                        external_interface: uplink.clone(),
                        masquerade: true,
                    };
                    let _ = self.configure_nat(&nat_config).await;
                    let _ = self.stop_dhcp(name).await;
                    let _ = Command::new("ip")
                        .args(&["addr", "flush", "dev", name])
                        .output();
                }
                SwitchProfile::External { uplink } => {
                    let _ = Command::new("ip")
                        .args(&["link", "set", "dev", &uplink, "nomaster"])
                        .output();
                }
                SwitchProfile::Internal => {}
            }
        }

        if let Some(switch) = self.switches.get(name) {
            match switch.switch_type {
                SwitchType::LinuxBridge => {
                    self.delete_linux_bridge(name).await?;
                }
                SwitchType::OpenVSwitch => {
                    self.delete_ovs_bridge(name).await?;
                }
            }
        } else if let Some(state) = &persisted_state {
            match state.switch_type {
                SwitchType::LinuxBridge => {
                    self.delete_linux_bridge(name).await?;
                }
                SwitchType::OpenVSwitch => {
                    self.delete_ovs_bridge(name).await?;
                }
            }
        }

        self.switches.remove(name);
        remove_persisted_switch(name)?;
        self.restored_profiles.remove(name);
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

    pub async fn add_interface_to_switch(
        &mut self,
        switch_name: &str,
        interface: &str,
    ) -> Result<()> {
        log_info!("Adding interface {} to switch {}", interface, switch_name);

        let switch_type = if let Some(switch) = self.switches.get(switch_name) {
            switch.switch_type.clone()
        } else {
            return Err(NovaError::NetworkNotFound(switch_name.to_string()));
        };

        match switch_type {
            SwitchType::LinuxBridge => {
                self.add_interface_to_linux_bridge(switch_name, interface)
                    .await?;
            }
            SwitchType::OpenVSwitch => {
                self.add_interface_to_ovs_bridge(switch_name, interface)
                    .await?;
            }
        }

        if let Some(switch) = self.switches.get_mut(switch_name) {
            if !switch.interfaces.iter().any(|i| i == interface) {
                switch.interfaces.push(interface.to_string());
            }
        }

        Ok(())
    }

    pub async fn remove_interface_from_switch(
        &mut self,
        switch_name: &str,
        interface: &str,
    ) -> Result<()> {
        log_info!(
            "Removing interface {} from switch {}",
            interface,
            switch_name
        );

        let switch_type = if let Some(switch) = self.switches.get(switch_name) {
            switch.switch_type.clone()
        } else {
            return Err(NovaError::NetworkNotFound(switch_name.to_string()));
        };

        match switch_type {
            SwitchType::LinuxBridge => {
                let output = Command::new("ip")
                    .args(&["link", "set", "dev", interface, "nomaster"])
                    .output()
                    .map_err(|_| NovaError::SystemCommandFailed)?;

                if !output.status.success() {
                    log_error!(
                        "Failed to remove interface {} from bridge {}",
                        interface,
                        switch_name
                    );
                    return Err(NovaError::SystemCommandFailed);
                }
            }
            SwitchType::OpenVSwitch => {
                let output = Command::new("ovs-vsctl")
                    .args(&["del-port", switch_name, interface])
                    .output()
                    .map_err(|_| NovaError::SystemCommandFailed)?;

                if !output.status.success() {
                    log_error!(
                        "Failed to remove interface {} from OVS bridge {}",
                        interface,
                        switch_name
                    );
                    return Err(NovaError::SystemCommandFailed);
                }
            }
        }

        if let Some(switch) = self.switches.get_mut(switch_name) {
            switch.interfaces.retain(|iface| iface != interface);
        }

        Ok(())
    }

    async fn add_interface_to_linux_bridge(&self, bridge: &str, interface: &str) -> Result<()> {
        if is_test_mode() {
            log_debug!(
                "[test] Pretending to add interface {} to Linux bridge {}",
                interface,
                bridge
            );
            return Ok(());
        }

        let output = Command::new("ip")
            .args(&["link", "set", "dev", interface, "master", bridge])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to add interface {} to bridge {}", interface, bridge);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!(
            "Interface {} added to bridge {} successfully",
            interface,
            bridge
        );
        Ok(())
    }

    async fn add_interface_to_ovs_bridge(&self, bridge: &str, interface: &str) -> Result<()> {
        if is_test_mode() {
            log_debug!(
                "[test] Pretending to add interface {} to OVS bridge {}",
                interface,
                bridge
            );
            return Ok(());
        }

        let output = Command::new("ovs-vsctl")
            .args(&["add-port", bridge, interface])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!(
                "Failed to add interface {} to OVS bridge {}",
                interface,
                bridge
            );
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!(
            "Interface {} added to OVS bridge {} successfully",
            interface,
            bridge
        );
        Ok(())
    }

    // Bridge Configuration
    pub async fn configure_bridge(&self, config: &BridgeConfig) -> Result<()> {
        log_info!("Configuring bridge: {}", config.name);

        // Configure STP
        if config.stp {
            self.enable_stp(&config.name).await?;
            self.set_bridge_parameter(
                &config.name,
                "forward_delay",
                &config.forward_delay.to_string(),
            )
            .await?;
            self.set_bridge_parameter(&config.name, "hello_time", &config.hello_time.to_string())
                .await?;
            self.set_bridge_parameter(&config.name, "max_age", &config.max_age.to_string())
                .await?;
        }

        // Configure aging time
        self.set_bridge_parameter(&config.name, "ageing_time", &config.aging_time.to_string())
            .await?;

        // Configure multicast snooping
        if config.multicast_snooping {
            self.set_bridge_parameter(&config.name, "multicast_snooping", "1")
                .await?;
        }

        log_info!("Bridge {} configured successfully", config.name);
        Ok(())
    }

    async fn enable_stp(&self, bridge: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(&[
                "link",
                "set",
                "dev",
                bridge,
                "type",
                "bridge",
                "stp_state",
                "1",
            ])
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
            log_warn!(
                "Failed to set bridge parameter {}={} for {}",
                param,
                value,
                bridge
            );
        } else {
            log_debug!("Set bridge parameter {}={} for {}", param, value, bridge);
        }

        Ok(())
    }

    pub async fn assign_bridge_address(
        &self,
        bridge: &str,
        cidr: &str,
        flush_from: Option<&str>,
    ) -> Result<()> {
        if is_test_mode() {
            log_debug!(
                "[test] Pretending to assign {} to bridge {} (flush {:?})",
                cidr,
                bridge,
                flush_from
            );
            return Ok(());
        }

        if let Some(source) = flush_from {
            log_info!(
                "Flushing IP configuration from {} before assigning to {}",
                source,
                bridge
            );
            let _ = Command::new("ip")
                .args(&["addr", "flush", "dev", source])
                .output();
        }

        let output = Command::new("ip")
            .args(&["addr", "replace", cidr, "dev", bridge])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to assign address {} to bridge {}", cidr, bridge);
            return Err(NovaError::SystemCommandFailed);
        }

        let _ = Command::new("ip")
            .args(&["link", "set", "dev", bridge, "up"])
            .output();

        log_info!("Assigned address {} to bridge {}", cidr, bridge);
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
        let interfaces: serde_json::Value =
            serde_json::from_str(json_str).map_err(|_| NovaError::InvalidConfig)?;

        if let Some(interfaces_array) = interfaces.as_array() {
            use std::collections::hash_map::Entry;

            for interface_data in interfaces_array {
                if let Some(name) = interface_data["ifname"].as_str() {
                    // Skip loopback and ephemeral veth pairs, but keep docker/libvirt bridges
                    if name == "lo" || name.starts_with("veth") {
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

                    let ip_address = interface_data["addr_info"].as_array().and_then(|infos| {
                        infos.iter().find_map(|addr| {
                            if addr["family"].as_str() == Some("inet") {
                                addr["local"]
                                    .as_str()
                                    .and_then(|ip| Ipv4Addr::from_str(ip).ok())
                            } else {
                                None
                            }
                        })
                    });

                    let bridge = interface_data["master"].as_str().map(|s| s.to_string());

                    let speed = interface_data["linkinfo"]["info_data"]["speed"]
                        .as_u64()
                        .or_else(|| interface_data["speed"].as_u64());

                    let interface = NetworkInterface {
                        name: name.to_string(),
                        mac_address,
                        ip_address,
                        state: state.clone(),
                        bridge,
                        speed,
                    };

                    self.interfaces.insert(name.to_string(), interface);

                    if interface_data["linkinfo"]["info_kind"].as_str() == Some("bridge") {
                        let stp_enabled = interface_data["linkinfo"]["info_data"]["stp_state"]
                            .as_u64()
                            .map(|val| val == 1)
                            .unwrap_or(false);

                        let status = match state {
                            InterfaceState::Up => SwitchStatus::Active,
                            InterfaceState::Down => SwitchStatus::Inactive,
                            InterfaceState::Unknown => SwitchStatus::Inactive,
                        };

                        match self.switches.entry(name.to_string()) {
                            Entry::Occupied(mut entry) => {
                                let switch = entry.get_mut();
                                switch.status = status.clone();
                                switch.stp_enabled = stp_enabled;
                                switch.switch_type = SwitchType::LinuxBridge;
                                if switch.origin != SwitchOrigin::Nova {
                                    switch.origin = SwitchOrigin::System;
                                }
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(VirtualSwitch {
                                    name: name.to_string(),
                                    switch_type: SwitchType::LinuxBridge,
                                    interfaces: Vec::new(),
                                    vlan_id: None,
                                    stp_enabled,
                                    created_at: chrono::Utc::now(),
                                    status,
                                    origin: SwitchOrigin::System,
                                    profile: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // VLAN Management
    pub async fn create_vlan_interface(
        &self,
        base_interface: &str,
        vlan_id: u16,
    ) -> Result<String> {
        let vlan_name = format!("{}.{}", base_interface, vlan_id);
        log_info!("Creating VLAN interface: {}", vlan_name);

        let output = Command::new("ip")
            .args(&[
                "link",
                "add",
                "link",
                base_interface,
                "name",
                &vlan_name,
                "type",
                "vlan",
                "id",
                &vlan_id.to_string(),
            ])
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

    pub fn switch_exists(&self, name: &str) -> bool {
        self.switches.contains_key(name)
            || Path::new(&format!("/sys/class/net/{}/bridge", name)).exists()
    }

    pub fn list_interfaces(&self) -> Vec<&NetworkInterface> {
        self.interfaces.values().collect()
    }

    pub fn get_interface(&self, name: &str) -> Option<&NetworkInterface> {
        self.interfaces.get(name)
    }

    pub fn interface_exists(&self, name: &str) -> bool {
        self.interfaces.contains_key(name)
            || Path::new(&format!("/sys/class/net/{}", name)).exists()
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
    pub async fn enable_port_mirroring(
        &self,
        bridge: &str,
        source_port: &str,
        target_port: &str,
    ) -> Result<()> {
        log_info!(
            "Enabling port mirroring on bridge {}: {} -> {}",
            bridge,
            source_port,
            target_port
        );

        if self.check_ovs_available() {
            // Use OVS for advanced port mirroring
            let output = Command::new("ovs-vsctl")
                .args(&[
                    "--",
                    "--id=@m",
                    "create",
                    "mirror",
                    &format!("name=mirror-{}-{}", source_port, target_port),
                ])
                .arg("--")
                .args(&["--id=@in", "get", "port", source_port])
                .arg("--")
                .args(&["--id=@out", "get", "port", target_port])
                .arg("--")
                .args(&["set", "bridge", bridge, "mirrors=@m"])
                .arg("--")
                .args(&[
                    "set",
                    "mirror",
                    "@m",
                    "select_src_port=@in",
                    "output_port=@out",
                ])
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

    pub async fn configure_bridge_filters(
        &self,
        bridge: &str,
        rules: &[BridgeFilter],
    ) -> Result<()> {
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
                    .args(&[
                        "-A",
                        "FORWARD",
                        "-i",
                        bridge,
                        "-p",
                        &filter.protocol,
                        "-j",
                        "ACCEPT",
                    ])
                    .output()
                    .map_err(|_| NovaError::SystemCommandFailed)?;

                if !output.status.success() {
                    log_warn!("Failed to apply bridge filter rule");
                }
            }
            FilterAction::Deny => {
                let output = Command::new("ebtables")
                    .args(&[
                        "-A",
                        "FORWARD",
                        "-i",
                        bridge,
                        "-p",
                        &filter.protocol,
                        "-j",
                        "DROP",
                    ])
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

        if is_test_mode() {
            log_debug!(
                "[test] Skipping dnsmasq launch for interface {} with config {:?}",
                interface,
                config
            );
            return Ok(());
        }

        let conf_file = format!("/tmp/nova-dhcp-{}.conf", interface);
        let mut dhcp_conf = String::new();

        // Basic configuration
        dhcp_conf.push_str(&format!(
            "interface={}
",
            interface
        ));
        dhcp_conf.push_str(&format!(
            "dhcp-range={},{},{},{}s
",
            config.range_start, config.range_end, config.subnet_mask, config.lease_time
        ));
        dhcp_conf.push_str(&format!(
            "dhcp-option=3,{}
",
            config.gateway
        )); // Gateway

        // DNS servers
        for dns in config.dns_servers.iter() {
            dhcp_conf.push_str(&format!(
                "dhcp-option=6,{}
",
                dns
            ));
        }

        // No daemon mode, bind to interface
        dhcp_conf.push_str(
            "bind-interfaces
",
        );
        dhcp_conf.push_str(
            "no-daemon
",
        );
        dhcp_conf.push_str(
            "log-dhcp
",
        );

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

        if is_test_mode() {
            log_debug!("[test] No DHCP teardown needed for {}", interface);
            return Ok(());
        }

        let pid_file = format!("/tmp/nova-dhcp-{}.pid", interface);

        if let Ok(pid_content) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_content.trim().parse::<u32>() {
                let _ = Command::new("kill").arg(&pid.to_string()).output();
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
        log_info!(
            "Configuring NAT: {} -> {}",
            config.internal_interface,
            config.external_interface
        );

        if is_test_mode() {
            log_debug!(
                "[test] Skipping NAT configuration for {}",
                config.internal_interface
            );
            return Ok(());
        }

        if !config.enabled {
            if self.check_nft_available() {
                let _ = self.remove_nat_with_nftables(config).await;
            }
            if self.check_iptables_available() {
                let _ = self.remove_nat_with_iptables(config).await;
            }
            return Ok(());
        }

        // Ensure clean slate before applying rules
        if self.check_nft_available() {
            let _ = self.remove_nat_with_nftables(config).await;
        }
        if self.check_iptables_available() {
            let _ = self.remove_nat_with_iptables(config).await;
        }

        // Enable IP forwarding globally
        let output = Command::new("sysctl")
            .args(&["-w", "net.ipv4.ip_forward=1"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to enable IP forwarding");
            return Err(NovaError::SystemCommandFailed);
        }

        if self.check_nft_available() {
            self.apply_nat_with_nftables(config).await
        } else if self.check_iptables_available() {
            self.apply_nat_with_iptables(config).await
        } else {
            Err(NovaError::NetworkError(
                "No nftables or iptables backend available for NAT".to_string(),
            ))
        }
    }

    async fn apply_nat_with_iptables(&self, config: &NatConfig) -> Result<()> {
        if config.masquerade {
            let output = Command::new("iptables")
                .args(&[
                    "-t",
                    "nat",
                    "-A",
                    "POSTROUTING",
                    "-o",
                    &config.external_interface,
                    "-j",
                    "MASQUERADE",
                ])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                log_error!("Failed to add masquerade rule via iptables");
                return Err(NovaError::SystemCommandFailed);
            }
        }

        let output = Command::new("iptables")
            .args(&[
                "-A",
                "FORWARD",
                "-i",
                &config.internal_interface,
                "-o",
                &config.external_interface,
                "-j",
                "ACCEPT",
            ])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to add forward rule via iptables");
            return Err(NovaError::SystemCommandFailed);
        }

        let output = Command::new("iptables")
            .args(&[
                "-A",
                "FORWARD",
                "-i",
                &config.external_interface,
                "-o",
                &config.internal_interface,
                "-m",
                "state",
                "--state",
                "RELATED,ESTABLISHED",
                "-j",
                "ACCEPT",
            ])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to add return forward rule via iptables");
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    async fn remove_nat_with_iptables(&self, config: &NatConfig) -> Result<()> {
        if config.masquerade {
            let _ = Command::new("iptables")
                .args(&[
                    "-t",
                    "nat",
                    "-D",
                    "POSTROUTING",
                    "-o",
                    &config.external_interface,
                    "-j",
                    "MASQUERADE",
                ])
                .output();
        }

        let _ = Command::new("iptables")
            .args(&[
                "-D",
                "FORWARD",
                "-i",
                &config.internal_interface,
                "-o",
                &config.external_interface,
                "-j",
                "ACCEPT",
            ])
            .output();

        let _ = Command::new("iptables")
            .args(&[
                "-D",
                "FORWARD",
                "-i",
                &config.external_interface,
                "-o",
                &config.internal_interface,
                "-m",
                "state",
                "--state",
                "RELATED,ESTABLISHED",
                "-j",
                "ACCEPT",
            ])
            .output();

        Ok(())
    }

    async fn apply_nat_with_nftables(&self, config: &NatConfig) -> Result<()> {
        let table_name = Self::nft_table_name(&config.internal_interface);
        let _ = Command::new("nft")
            .args(&["delete", "table", "inet", &table_name])
            .output();

        let script = format!(
            r#"table inet {table_name} {{
    chain prerouting {{
        type nat hook prerouting priority -100;
    }}

    chain postrouting {{
        type nat hook postrouting priority 100;
        oifname "{external}" masquerade
    }}

    chain forward {{
        type filter hook forward priority 0;
        iifname "{internal}" oifname "{external}" accept
        iifname "{external}" oifname "{internal}" ct state related,established accept
    }}
}}
"#,
            table_name = table_name,
            internal = config.internal_interface,
            external = config.external_interface
        );

        let mut child = Command::new("nft")
            .arg("-f")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(script.as_bytes())?;
        }

        let output = child
            .wait_with_output()
            .map_err(|_| NovaError::SystemCommandFailed)?;
        if !output.status.success() {
            log_error!(
                "Failed to apply nftables NAT rules: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    async fn remove_nat_with_nftables(&self, config: &NatConfig) -> Result<()> {
        let table_name = Self::nft_table_name(&config.internal_interface);
        let _ = Command::new("nft")
            .args(&["delete", "table", "inet", &table_name])
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

    pub fn check_nft_available(&self) -> bool {
        Command::new("nft")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn nft_table_name(internal: &str) -> String {
        format!("nova_{}", internal.replace('-', "_"))
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

fn parse_cidr(cidr: &str) -> Result<(Ipv4Addr, u8)> {
    let mut parts = cidr.split('/');
    let addr_part = parts
        .next()
        .ok_or_else(|| NovaError::ConfigError("Invalid CIDR".to_string()))?;
    let prefix_part = parts
        .next()
        .ok_or_else(|| NovaError::ConfigError("Invalid CIDR".to_string()))?;

    let address = addr_part
        .parse::<Ipv4Addr>()
        .map_err(|_| NovaError::ConfigError("Invalid IPv4 address".to_string()))?;
    let prefix = prefix_part
        .parse::<u8>()
        .map_err(|_| NovaError::ConfigError("Invalid prefix length".to_string()))?;

    if prefix > 32 {
        return Err(NovaError::ConfigError(
            "CIDR prefix out of range".to_string(),
        ));
    }

    Ok((address, prefix))
}

fn prefix_to_mask(prefix: u8) -> Option<u32> {
    if prefix > 32 {
        return None;
    }
    if prefix == 0 {
        Some(0)
    } else {
        Some(!0u32 << (32 - prefix))
    }
}

fn mask_to_ipv4(mask: u32) -> Ipv4Addr {
    Ipv4Addr::from(mask)
}

fn default_dhcp_range(ip: Ipv4Addr, prefix: u8) -> Option<(Ipv4Addr, Ipv4Addr)> {
    if prefix >= 31 {
        return None;
    }

    let mask = prefix_to_mask(prefix)?;
    let network = u32::from(ip) & mask;
    let broadcast = network | !mask;

    let start = network.saturating_add(10);
    let end = broadcast.saturating_sub(10);

    if start >= end {
        return None;
    }

    Some((Ipv4Addr::from(start), Ipv4Addr::from(end)))
}

fn bridge_exists(name: &str) -> bool {
    Path::new(&format!("/sys/class/net/{}", name)).exists()
}

fn is_test_mode() -> bool {
    cfg!(test) || matches!(env::var("NOVA_TEST_MODE"), Ok(val) if val == "1")
}

#[cfg(test)]
static RESTORE_ATTEMPTS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

#[cfg(test)]
fn record_restore_attempt(name: &str) {
    RESTORE_ATTEMPTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(name.to_string());
}

#[cfg(test)]
fn restore_attempts_snapshot() -> Vec<String> {
    RESTORE_ATTEMPTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .clone()
}

#[cfg(test)]
fn clear_restore_attempts() {
    if let Some(store) = RESTORE_ATTEMPTS.get() {
        store.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_test_env() -> tempfile::TempDir {
        clear_restore_attempts();
        unsafe {
            std::env::set_var("NOVA_TEST_MODE", "1");
        }
        let tmp = tempdir().expect("temp dir");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", tmp.path());
        }
        tmp
    }

    fn teardown_test_env(tmp: tempfile::TempDir) {
        drop(tmp);
        unsafe {
            std::env::remove_var("NOVA_TEST_MODE");
            std::env::remove_var("XDG_DATA_HOME");
        }
        clear_restore_attempts();
    }

    #[tokio::test]
    async fn hydrate_restoration_behaviors() {
        let tmp = setup_test_env();

        let nat_state = PersistedSwitch {
            name: "nova-nat-test".to_string(),
            switch_type: SwitchType::LinuxBridge,
            profile: Some(SwitchProfile::Nat {
                uplink: "eth0".to_string(),
                subnet_cidr: "192.168.200.1/24".to_string(),
                dhcp_range_start: None,
                dhcp_range_end: None,
            }),
        };

        persist_switch_state(&nat_state).expect("persist nat state");

        let mut manager = NetworkManager::new();
        manager
            .hydrate_persisted_switches()
            .await
            .expect("hydrate nat first");

        let nat_attempts = restore_attempts_snapshot();
        assert_eq!(nat_attempts, vec!["nova-nat-test".to_string()]);
        assert!(manager.restored_profiles.contains("nova-nat-test"));

        let nat_switch = manager
            .switches
            .get("nova-nat-test")
            .expect("nat switch inserted");
        assert!(matches!(nat_switch.status, SwitchStatus::Active));
        assert!(matches!(
            nat_switch.profile,
            Some(SwitchProfile::Nat { .. })
        ));

        manager
            .hydrate_persisted_switches()
            .await
            .expect("hydrate nat second");
        let nat_attempts_after = restore_attempts_snapshot();
        assert_eq!(nat_attempts_after, vec!["nova-nat-test".to_string()]);

        teardown_test_env(tmp);

        let tmp = setup_test_env();

        let ext_state = PersistedSwitch {
            name: "nova-ext-test".to_string(),
            switch_type: SwitchType::LinuxBridge,
            profile: Some(SwitchProfile::External {
                uplink: "enp3s0".to_string(),
            }),
        };

        persist_switch_state(&ext_state).expect("persist ext state");

        let mut manager = NetworkManager::new();
        manager
            .hydrate_persisted_switches()
            .await
            .expect("hydrate ext first");

        let ext_attempts = restore_attempts_snapshot();
        assert_eq!(ext_attempts, vec!["nova-ext-test".to_string()]);

        let ext_switch = manager
            .switches
            .get("nova-ext-test")
            .expect("ext switch exists");
        assert!(matches!(ext_switch.status, SwitchStatus::Active));
        assert!(ext_switch.interfaces.contains(&"enp3s0".to_string()));
        assert!(matches!(
            ext_switch.profile,
            Some(SwitchProfile::External { .. })
        ));

        manager
            .hydrate_persisted_switches()
            .await
            .expect("hydrate ext second");

        let ext_attempts_after = restore_attempts_snapshot();
        assert_eq!(ext_attempts_after, vec!["nova-ext-test".to_string()]);

        teardown_test_env(tmp);
    }
}
