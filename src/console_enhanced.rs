// Enhanced Console Manager for Nova
// Supports SPICE, VNC, and Looking Glass protocols

use crate::console::{ConsoleConfig, ConsoleManager};
use crate::looking_glass::{LookingGlassConfig, LookingGlassManager, LookingGlassProfile};
use crate::{NovaError, Result, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedConsoleConfig {
    pub console_config: ConsoleConfig,
    pub looking_glass_config: LookingGlassConfig,
    pub preferred_protocol: PreferredProtocol,
    pub performance_monitoring: bool,
    pub multi_monitor_support: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PreferredProtocol {
    LookingGlass, // Best for GPU passthrough VMs
    SPICE,        // Good performance, native libvirt integration
    VNC,          // Universal compatibility
    Auto,         // Auto-select based on VM capabilities
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedConsoleSession {
    pub vm_name: String,
    pub session_id: String,
    pub protocol_used: ActiveProtocol,
    pub connection_info: ConnectionDetails,
    pub performance_score: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub active: bool,
    pub features: SessionFeatures,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActiveProtocol {
    LookingGlass,
    SPICE,
    VNC,
    Serial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionDetails {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub viewer_command: String,
    pub shmem_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFeatures {
    pub clipboard_sync: bool,
    pub audio_enabled: bool,
    pub multi_monitor: bool,
    pub hardware_acceleration: bool,
    pub usb_redirect: bool,
    pub low_latency: bool,
}

pub struct EnhancedConsoleManager {
    config: EnhancedConsoleConfig,
    console_manager: ConsoleManager,
    looking_glass_manager: LookingGlassManager,
    unified_sessions: Arc<Mutex<HashMap<String, UnifiedConsoleSession>>>,
    performance_scores: Arc<Mutex<HashMap<String, f32>>>,
}

impl EnhancedConsoleManager {
    pub fn new(config: EnhancedConsoleConfig) -> Self {
        let console_manager = ConsoleManager::new(config.console_config.clone());
        let looking_glass_manager = LookingGlassManager::new();

        Self {
            config,
            console_manager,
            looking_glass_manager,
            unified_sessions: Arc::new(Mutex::new(HashMap::new())),
            performance_scores: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create the best console connection for a VM
    pub async fn create_optimal_console(
        &mut self,
        vm_name: &str,
        _vm_ip: Option<&str>,
    ) -> Result<UnifiedConsoleSession> {
        log_info!("Creating console session for VM: {}", vm_name);

        // Analyze VM to determine best protocol
        let analysis = self.analyze_vm_capabilities(vm_name).await?;
        let selected_protocol = self.select_optimal_protocol(&analysis);

        log_info!(
            "Selected protocol for '{}': {:?}",
            vm_name,
            selected_protocol
        );

        let session = match selected_protocol {
            PreferredProtocol::LookingGlass => {
                self.create_looking_glass_session(vm_name, &analysis).await?
            }
            PreferredProtocol::SPICE => {
                self.create_spice_session(vm_name, &analysis).await?
            }
            PreferredProtocol::VNC => {
                self.create_vnc_session(vm_name).await?
            }
            PreferredProtocol::Auto => {
                // Auto already resolved by select_optimal_protocol
                self.create_spice_session(vm_name, &analysis).await?
            }
        };

        // Start performance monitoring if enabled
        if self.config.performance_monitoring {
            self.start_performance_monitoring(&session.session_id).await?;
        }

        // Store session
        {
            let mut sessions = self.unified_sessions.lock().unwrap();
            sessions.insert(session.session_id.clone(), session.clone());
        }

        log_info!(
            "Console session created: {} (score: {:.0})",
            session.session_id,
            session.performance_score
        );

        Ok(session)
    }

    async fn create_looking_glass_session(
        &mut self,
        vm_name: &str,
        analysis: &VmAnalysis,
    ) -> Result<UnifiedConsoleSession> {
        log_info!("Creating Looking Glass session for VM: {}", vm_name);

        // Select profile based on VM capabilities
        let profile = if analysis.has_gpu && analysis.cpu_cores >= 4 {
            LookingGlassProfile::Gaming
        } else {
            LookingGlassProfile::Productivity
        };

        let lg_config = profile.to_config();
        self.looking_glass_manager.register_config(vm_name.to_string(), lg_config.clone());

        let session_id = format!("lg-{}-{}", vm_name, chrono::Utc::now().timestamp());

        let connection = ConnectionDetails {
            host: "localhost".to_string(),
            port: 0, // Looking Glass uses shared memory, not network
            protocol: "looking-glass".to_string(),
            viewer_command: format!(
                "looking-glass-client -f {}",
                lg_config.shmem_path.display()
            ),
            shmem_path: Some(lg_config.shmem_path.display().to_string()),
        };

        let features = SessionFeatures {
            clipboard_sync: true,
            audio_enabled: lg_config.audio_enabled,
            multi_monitor: analysis.supports_multi_monitor,
            hardware_acceleration: true,
            usb_redirect: false,
            low_latency: true,
        };

        Ok(UnifiedConsoleSession {
            vm_name: vm_name.to_string(),
            session_id,
            protocol_used: ActiveProtocol::LookingGlass,
            connection_info: connection,
            performance_score: 95.0,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
            features,
        })
    }

    async fn create_spice_session(
        &mut self,
        vm_name: &str,
        analysis: &VmAnalysis,
    ) -> Result<UnifiedConsoleSession> {
        log_info!("Creating SPICE session for VM: {}", vm_name);

        let console_session = self.console_manager.create_spice_console(vm_name).await?;

        let connection = ConnectionDetails {
            host: console_session.connection_info.host.clone(),
            port: console_session.connection_info.port,
            protocol: "spice".to_string(),
            viewer_command: format!(
                "remote-viewer spice://{}:{}",
                console_session.connection_info.host,
                console_session.connection_info.port
            ),
            shmem_path: None,
        };

        let features = SessionFeatures {
            clipboard_sync: true,
            audio_enabled: true,
            multi_monitor: analysis.supports_multi_monitor,
            hardware_acceleration: analysis.has_gpu,
            usb_redirect: true,
            low_latency: false,
        };

        Ok(UnifiedConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: console_session.session_id,
            protocol_used: ActiveProtocol::SPICE,
            connection_info: connection,
            performance_score: 75.0,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
            features,
        })
    }

    async fn create_vnc_session(&mut self, vm_name: &str) -> Result<UnifiedConsoleSession> {
        log_info!("Creating VNC session for VM: {}", vm_name);

        let console_session = self
            .console_manager
            .create_vnc_console(vm_name, true)
            .await?;

        let connection = ConnectionDetails {
            host: console_session.connection_info.host.clone(),
            port: console_session.connection_info.port,
            protocol: "vnc".to_string(),
            viewer_command: format!(
                "vncviewer {}:{}",
                console_session.connection_info.host,
                console_session.connection_info.port
            ),
            shmem_path: None,
        };

        let features = SessionFeatures {
            clipboard_sync: false,
            audio_enabled: false,
            multi_monitor: false,
            hardware_acceleration: false,
            usb_redirect: false,
            low_latency: false,
        };

        Ok(UnifiedConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: console_session.session_id,
            protocol_used: ActiveProtocol::VNC,
            connection_info: connection,
            performance_score: 50.0,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
            features,
        })
    }

    async fn analyze_vm_capabilities(&self, vm_name: &str) -> Result<VmAnalysis> {
        log_info!("Analyzing VM capabilities: {}", vm_name);

        let mut analysis = VmAnalysis::default();

        // Check VM specs via libvirt
        if let Ok(output) = tokio::process::Command::new("virsh")
            .args(["dominfo", vm_name])
            .output()
            .await
        {
            let info = String::from_utf8_lossy(&output.stdout);

            if let Some(cpu_line) = info.lines().find(|line| line.contains("CPU(s)")) {
                if let Some(cpu_str) = cpu_line.split_whitespace().nth(1) {
                    analysis.cpu_cores = cpu_str.parse().unwrap_or(1);
                }
            }

            if let Some(mem_line) = info.lines().find(|line| line.contains("Max memory")) {
                if let Some(mem_str) = mem_line.split_whitespace().nth(2) {
                    analysis.memory_mb = mem_str.parse().unwrap_or(1024);
                }
            }
        }

        // Check for GPU passthrough and Looking Glass IVSHMEM
        if let Ok(output) = tokio::process::Command::new("virsh")
            .args(["dumpxml", vm_name])
            .output()
            .await
        {
            let xml = String::from_utf8_lossy(&output.stdout);
            analysis.has_gpu = xml.contains("<hostdev") && xml.contains("type='pci'");
            analysis.has_looking_glass = xml.contains("looking-glass") || xml.contains("ivshmem");
            analysis.has_spice = xml.contains("<graphics type='spice'");
            analysis.has_vnc = xml.contains("<graphics type='vnc'");
        }

        // Check guest agent
        analysis.supports_guest_agent = self.check_guest_agent(vm_name).await;

        // Multi-monitor based on config
        analysis.supports_multi_monitor = analysis.has_gpu || analysis.has_spice;

        log_info!(
            "VM '{}': {} cores, {}MB, GPU={}, LG={}, SPICE={}, VNC={}",
            vm_name,
            analysis.cpu_cores,
            analysis.memory_mb,
            analysis.has_gpu,
            analysis.has_looking_glass,
            analysis.has_spice,
            analysis.has_vnc
        );

        Ok(analysis)
    }

    fn select_optimal_protocol(&self, analysis: &VmAnalysis) -> PreferredProtocol {
        match &self.config.preferred_protocol {
            PreferredProtocol::Auto => {
                // Looking Glass for GPU passthrough VMs with IVSHMEM configured
                if analysis.has_looking_glass && analysis.has_gpu {
                    if self.looking_glass_manager.check_client_installed() {
                        return PreferredProtocol::LookingGlass;
                    }
                    log_warn!("Looking Glass configured but client not installed");
                }

                // SPICE for VMs with SPICE graphics
                if analysis.has_spice && analysis.supports_guest_agent {
                    return PreferredProtocol::SPICE;
                }

                // VNC as universal fallback
                PreferredProtocol::VNC
            }
            other => other.clone(),
        }
    }

    async fn check_guest_agent(&self, vm_name: &str) -> bool {
        tokio::process::Command::new("virsh")
            .args([
                "qemu-agent-command",
                vm_name,
                "{\"execute\":\"guest-ping\"}",
            ])
            .output()
            .await
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    async fn start_performance_monitoring(&self, session_id: &str) -> Result<()> {
        log_info!("Starting performance monitoring for: {}", session_id);

        let session_id_clone = session_id.to_string();
        let scores_clone = self.performance_scores.clone();

        tokio::spawn(async move {
            loop {
                let score = 85.0; // Placeholder - would measure actual metrics
                {
                    let mut scores = scores_clone.lock().unwrap();
                    scores.insert(session_id_clone.clone(), score);
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });

        Ok(())
    }

    // Public API
    pub fn list_active_sessions(&self) -> Vec<UnifiedConsoleSession> {
        let sessions = self.unified_sessions.lock().unwrap();
        sessions.values().filter(|s| s.active).cloned().collect()
    }

    pub fn get_session(&self, session_id: &str) -> Option<UnifiedConsoleSession> {
        let sessions = self.unified_sessions.lock().unwrap();
        sessions.get(session_id).cloned()
    }

    pub fn get_performance_score(&self, session_id: &str) -> Option<f32> {
        let scores = self.performance_scores.lock().unwrap();
        scores.get(session_id).copied()
    }

    /// Launch the viewer application for a session
    pub async fn launch_session_client(&self, session_id: &str) -> Result<()> {
        let session = self
            .get_session(session_id)
            .ok_or(NovaError::NetworkNotFound(session_id.to_string()))?;

        log_info!("Launching viewer: {}", session.connection_info.viewer_command);

        match session.protocol_used {
            ActiveProtocol::LookingGlass => {
                if let Some(shmem) = &session.connection_info.shmem_path {
                    Command::new("looking-glass-client")
                        .arg("-f")
                        .arg(shmem)
                        .spawn()
                        .map_err(|_| NovaError::SystemCommandFailed)?;
                }
            }
            ActiveProtocol::SPICE => {
                Command::new("remote-viewer")
                    .arg(format!(
                        "spice://{}:{}",
                        session.connection_info.host, session.connection_info.port
                    ))
                    .spawn()
                    .map_err(|_| NovaError::SystemCommandFailed)?;
            }
            ActiveProtocol::VNC => {
                // Try virt-viewer first, fallback to vncviewer
                let result = Command::new("remote-viewer")
                    .arg(format!(
                        "vnc://{}:{}",
                        session.connection_info.host, session.connection_info.port
                    ))
                    .spawn();

                if result.is_err() {
                    Command::new("vncviewer")
                        .arg(format!(
                            "{}:{}",
                            session.connection_info.host, session.connection_info.port
                        ))
                        .spawn()
                        .map_err(|_| NovaError::SystemCommandFailed)?;
                }
            }
            ActiveProtocol::Serial => {
                Command::new("virsh")
                    .args(["console", &session.vm_name])
                    .spawn()
                    .map_err(|_| NovaError::SystemCommandFailed)?;
            }
        }

        Ok(())
    }

    pub async fn close_session(&mut self, session_id: &str) -> Result<()> {
        log_info!("Closing session: {}", session_id);

        if let Some(session) = self.get_session(session_id) {
            match session.protocol_used {
                ActiveProtocol::SPICE | ActiveProtocol::VNC => {
                    self.console_manager.close_session(session_id).await?;
                }
                _ => {}
            }
        }

        {
            let mut sessions = self.unified_sessions.lock().unwrap();
            sessions.remove(session_id);
        }

        {
            let mut scores = self.performance_scores.lock().unwrap();
            scores.remove(session_id);
        }

        Ok(())
    }

    /// Get Looking Glass manager for direct access
    pub fn looking_glass(&self) -> &LookingGlassManager {
        &self.looking_glass_manager
    }

    /// Check system requirements for Looking Glass
    pub fn check_looking_glass_ready(&self) -> bool {
        let reqs = self.looking_glass_manager.check_system_requirements();
        reqs.ready
    }
}

#[derive(Debug, Clone)]
struct VmAnalysis {
    cpu_cores: u32,
    memory_mb: u64,
    has_gpu: bool,
    has_looking_glass: bool,
    has_spice: bool,
    has_vnc: bool,
    supports_guest_agent: bool,
    supports_multi_monitor: bool,
}

impl Default for VmAnalysis {
    fn default() -> Self {
        Self {
            cpu_cores: 2,
            memory_mb: 2048,
            has_gpu: false,
            has_looking_glass: false,
            has_spice: true,
            has_vnc: true,
            supports_guest_agent: false,
            supports_multi_monitor: false,
        }
    }
}

impl Default for EnhancedConsoleConfig {
    fn default() -> Self {
        Self {
            console_config: ConsoleConfig::default(),
            looking_glass_config: LookingGlassConfig::default(),
            preferred_protocol: PreferredProtocol::Auto,
            performance_monitoring: true,
            multi_monitor_support: true,
        }
    }
}
