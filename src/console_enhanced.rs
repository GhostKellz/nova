use crate::console::{ConnectionInfo, ConsoleConfig, ConsoleManager, ConsoleSession, ConsoleType};
use crate::rustdesk_integration::{
    PerformanceProfile, RustDeskConfig, RustDeskManager, RustDeskSession,
};
use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedConsoleConfig {
    pub standard_console: ConsoleConfig,
    pub rustdesk_config: RustDeskConfig,
    pub preferred_protocol: PreferredProtocol,
    pub auto_install_agents: bool,
    pub performance_monitoring: bool,
    pub session_recording: bool,
    pub multi_monitor_support: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreferredProtocol {
    RustDesk, // Highest performance
    SPICE,    // Good performance, native libvirt
    VNC,      // Universal compatibility
    Auto,     // Auto-select based on VM capabilities
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedConsoleSession {
    pub vm_name: String,
    pub session_id: String,
    pub protocol_used: ActiveProtocol,
    pub performance_score: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub active: bool,
    pub features: SessionFeatures,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActiveProtocol {
    RustDesk(RustDeskSession),
    Standard(ConsoleSession),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFeatures {
    pub file_transfer: bool,
    pub clipboard_sync: bool,
    pub audio_enabled: bool,
    pub multi_monitor: bool,
    pub hardware_acceleration: bool,
    pub encryption: bool,
    pub recording: bool,
}

pub struct EnhancedConsoleManager {
    config: EnhancedConsoleConfig,
    console_manager: ConsoleManager,
    rustdesk_manager: RustDeskManager,
    unified_sessions: Arc<Mutex<HashMap<String, UnifiedConsoleSession>>>,
    performance_scores: Arc<Mutex<HashMap<String, f32>>>,
}

impl EnhancedConsoleManager {
    pub fn new(config: EnhancedConsoleConfig) -> Self {
        let console_manager = ConsoleManager::new(config.standard_console.clone());
        let rustdesk_manager = RustDeskManager::new(config.rustdesk_config.clone());

        Self {
            config,
            console_manager,
            rustdesk_manager,
            unified_sessions: Arc::new(Mutex::new(HashMap::new())),
            performance_scores: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create the best possible console connection for a VM
    pub async fn create_optimal_console(
        &mut self,
        vm_name: &str,
        vm_ip: Option<&str>,
    ) -> Result<UnifiedConsoleSession> {
        log_info!("Creating optimal console connection for VM: {}", vm_name);

        // Analyze VM capabilities and network conditions
        let vm_analysis = self.analyze_vm_capabilities(vm_name, vm_ip).await?;

        // Select the best protocol based on analysis
        let selected_protocol = self.select_optimal_protocol(&vm_analysis).await;

        log_info!(
            "Selected protocol for VM '{}': {:?}",
            vm_name,
            selected_protocol
        );

        let session = match selected_protocol {
            PreferredProtocol::RustDesk => {
                self.create_rustdesk_session(vm_name, vm_ip, vm_analysis)
                    .await?
            }
            PreferredProtocol::SPICE => self.create_spice_session(vm_name, vm_analysis).await?,
            PreferredProtocol::VNC => self.create_vnc_session(vm_name, vm_analysis).await?,
            PreferredProtocol::Auto => {
                // This should not happen as select_optimal_protocol returns specific protocol
                self.create_rustdesk_session(vm_name, vm_ip, vm_analysis)
                    .await?
            }
        };

        // Start performance monitoring
        if self.config.performance_monitoring {
            self.start_performance_monitoring(&session.session_id)
                .await?;
        }

        // Store session
        {
            let mut sessions = self.unified_sessions.lock().unwrap();
            sessions.insert(session.session_id.clone(), session.clone());
        }

        log_info!(
            "Optimal console session created: {} (score: {:.2})",
            session.session_id,
            session.performance_score
        );

        Ok(session)
    }

    async fn create_rustdesk_session(
        &mut self,
        vm_name: &str,
        vm_ip: Option<&str>,
        analysis: VmAnalysis,
    ) -> Result<UnifiedConsoleSession> {
        log_info!("Creating RustDesk session for VM: {}", vm_name);

        let vm_ip = vm_ip.ok_or_else(|| {
            log_error!("VM IP required for RustDesk connection");
            NovaError::SystemCommandFailed
        })?;

        // Determine optimal performance profile
        let performance_profile = self.determine_performance_profile(&analysis);

        let rustdesk_session = self
            .rustdesk_manager
            .create_rustdesk_session(vm_name, vm_ip, performance_profile)
            .await?;

        let features = SessionFeatures {
            file_transfer: true,
            clipboard_sync: true,
            audio_enabled: true,
            multi_monitor: analysis.supports_multi_monitor,
            hardware_acceleration: analysis.has_gpu,
            encryption: true,
            recording: self.config.session_recording,
        };

        let session = UnifiedConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: rustdesk_session.session_id.clone(),
            protocol_used: ActiveProtocol::RustDesk(rustdesk_session),
            performance_score: 95.0, // RustDesk gets highest score
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
            features,
        };

        Ok(session)
    }

    async fn create_spice_session(
        &mut self,
        vm_name: &str,
        analysis: VmAnalysis,
    ) -> Result<UnifiedConsoleSession> {
        log_info!("Creating SPICE session for VM: {}", vm_name);

        let console_session = self.console_manager.create_spice_console(vm_name).await?;

        let features = SessionFeatures {
            file_transfer: false, // SPICE doesn't have native file transfer
            clipboard_sync: true,
            audio_enabled: true,
            multi_monitor: analysis.supports_multi_monitor,
            hardware_acceleration: analysis.has_gpu,
            encryption: false,
            recording: false,
        };

        let session = UnifiedConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: console_session.session_id.clone(),
            protocol_used: ActiveProtocol::Standard(console_session),
            performance_score: 75.0, // SPICE gets good score
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
            features,
        };

        Ok(session)
    }

    async fn create_vnc_session(
        &mut self,
        vm_name: &str,
        analysis: VmAnalysis,
    ) -> Result<UnifiedConsoleSession> {
        log_info!("Creating enhanced VNC session for VM: {}", vm_name);

        let console_session = self
            .console_manager
            .create_vnc_console(vm_name, true) // Enhanced VNC
            .await?;

        let features = SessionFeatures {
            file_transfer: false,
            clipboard_sync: false,
            audio_enabled: false,
            multi_monitor: false,
            hardware_acceleration: false,
            encryption: self.config.standard_console.ssl_enabled,
            recording: false,
        };

        let session = UnifiedConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: console_session.session_id.clone(),
            protocol_used: ActiveProtocol::Standard(console_session),
            performance_score: 50.0, // VNC gets basic score
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
            features,
        };

        Ok(session)
    }

    async fn analyze_vm_capabilities(
        &self,
        vm_name: &str,
        vm_ip: Option<&str>,
    ) -> Result<VmAnalysis> {
        log_info!("Analyzing VM capabilities: {}", vm_name);

        let mut analysis = VmAnalysis::default();

        // Check VM specs via libvirt
        if let Ok(output) = tokio::process::Command::new("virsh")
            .args(&["dominfo", vm_name])
            .output()
            .await
        {
            let info = String::from_utf8_lossy(&output.stdout);

            // Parse CPU and memory info
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

        // Check for GPU passthrough
        if let Ok(output) = tokio::process::Command::new("virsh")
            .args(&["dumpxml", vm_name])
            .output()
            .await
        {
            let xml = String::from_utf8_lossy(&output.stdout);
            analysis.has_gpu = xml.contains("<hostdev") && xml.contains("type='pci'");
        }

        // Check network connectivity and latency if IP provided
        if let Some(ip) = vm_ip {
            analysis.network_latency_ms = self.measure_network_latency(ip).await;
            analysis.network_bandwidth_mbps = self.measure_network_bandwidth(ip).await;
        }

        // Detect OS type and capabilities
        analysis.os_type = self.detect_os_type(vm_name).await;
        analysis.supports_guest_agent = self.check_guest_agent(vm_name).await;

        // Multi-monitor support (mainly for SPICE and RustDesk)
        analysis.supports_multi_monitor =
            analysis.has_gpu || matches!(analysis.os_type.as_str(), "windows" | "linux");

        log_info!(
            "VM analysis complete for '{}': {} cores, {}MB RAM, GPU: {}, OS: {}",
            vm_name,
            analysis.cpu_cores,
            analysis.memory_mb,
            analysis.has_gpu,
            analysis.os_type
        );

        Ok(analysis)
    }

    async fn select_optimal_protocol(&self, analysis: &VmAnalysis) -> PreferredProtocol {
        match &self.config.preferred_protocol {
            PreferredProtocol::Auto => {
                // Intelligent protocol selection based on VM capabilities

                // RustDesk is preferred for high-performance scenarios
                if analysis.cpu_cores >= 2
                    && analysis.memory_mb >= 2048
                    && analysis.network_latency_ms < 50.0
                {
                    return PreferredProtocol::RustDesk;
                }

                // SPICE for good balance with libvirt integration
                if analysis.supports_guest_agent && analysis.cpu_cores >= 1 {
                    return PreferredProtocol::SPICE;
                }

                // VNC as fallback
                PreferredProtocol::VNC
            }
            other => other.clone(),
        }
    }

    fn determine_performance_profile(&self, analysis: &VmAnalysis) -> PerformanceProfile {
        // Select optimal RustDesk performance profile based on VM capabilities

        if analysis.has_gpu
            && analysis.cpu_cores >= 4
            && analysis.memory_mb >= 4096
            && analysis.network_bandwidth_mbps >= 100.0
        {
            PerformanceProfile::UltraHigh
        } else if analysis.cpu_cores >= 2
            && analysis.memory_mb >= 2048
            && analysis.network_bandwidth_mbps >= 50.0
        {
            PerformanceProfile::High
        } else if analysis.network_bandwidth_mbps < 10.0 {
            PerformanceProfile::LowBandwidth
        } else {
            PerformanceProfile::Balanced
        }
    }

    async fn measure_network_latency(&self, ip: &str) -> f32 {
        // Measure ping latency to VM
        if let Ok(output) = tokio::process::Command::new("ping")
            .args(&["-c", "3", ip])
            .output()
            .await
        {
            let ping_output = String::from_utf8_lossy(&output.stdout);
            if let Some(avg_line) = ping_output.lines().find(|line| line.contains("avg")) {
                if let Some(avg_str) = avg_line.split('/').nth(4) {
                    return avg_str.parse().unwrap_or(100.0);
                }
            }
        }
        100.0 // Default high latency
    }

    async fn measure_network_bandwidth(&self, ip: &str) -> f32 {
        // Quick bandwidth test (simplified)
        // In production, could use iperf3 or similar
        if self.measure_network_latency(ip).await < 10.0 {
            1000.0 // Assume gigabit for low latency (LAN)
        } else if self.measure_network_latency(ip).await < 50.0 {
            100.0 // Assume 100Mbps for moderate latency
        } else {
            10.0 // Assume 10Mbps for high latency
        }
    }

    async fn detect_os_type(&self, vm_name: &str) -> String {
        // Try to detect OS via guest agent or XML analysis
        if let Ok(output) = tokio::process::Command::new("virsh")
            .args(&[
                "qemu-agent-command",
                vm_name,
                "{\"execute\":\"guest-get-osinfo\"}",
            ])
            .output()
            .await
        {
            if output.status.success() {
                let response = String::from_utf8_lossy(&output.stdout);
                if response.contains("Windows") {
                    return "windows".to_string();
                } else if response.contains("Linux") {
                    return "linux".to_string();
                } else if response.contains("Darwin") {
                    return "macos".to_string();
                }
            }
        }
        "unknown".to_string()
    }

    async fn check_guest_agent(&self, vm_name: &str) -> bool {
        tokio::process::Command::new("virsh")
            .args(&[
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
        log_info!(
            "Starting performance monitoring for session: {}",
            session_id
        );

        let session_id_clone = session_id.to_string();
        let scores_clone = self.performance_scores.clone();

        tokio::spawn(async move {
            loop {
                // Collect performance metrics and calculate score
                let score = Self::calculate_performance_score(&session_id_clone).await;

                {
                    let mut scores = scores_clone.lock().unwrap();
                    scores.insert(session_id_clone.clone(), score);
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });

        Ok(())
    }

    async fn calculate_performance_score(session_id: &str) -> f32 {
        // Calculate performance score based on multiple factors:
        // - Latency
        // - FPS
        // - Bandwidth usage
        // - CPU/Memory usage

        // Simplified scoring for now
        85.0
    }

    // Public API methods
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

    pub async fn close_session(&mut self, session_id: &str) -> Result<()> {
        log_info!("Closing unified console session: {}", session_id);

        if let Some(session) = self.get_session(session_id) {
            match session.protocol_used {
                ActiveProtocol::RustDesk(_) => {
                    self.rustdesk_manager.disconnect_session(session_id).await?
                }
                ActiveProtocol::Standard(_) => {
                    self.console_manager.close_session(session_id).await?
                }
            }
        }

        // Remove from unified sessions
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

    /// Switch protocols for an active session (if possible)
    pub async fn switch_protocol(
        &mut self,
        session_id: &str,
        new_protocol: PreferredProtocol,
    ) -> Result<UnifiedConsoleSession> {
        log_info!(
            "Switching protocol for session: {} to {:?}",
            session_id,
            new_protocol
        );

        // Get current session
        let current_session = self
            .get_session(session_id)
            .ok_or(NovaError::NetworkNotFound(session_id.to_string()))?;

        // Close current session
        self.close_session(session_id).await?;

        // Create new session with different protocol
        self.config.preferred_protocol = new_protocol;

        // Determine VM IP from current session if needed
        let vm_ip = match &current_session.protocol_used {
            ActiveProtocol::RustDesk(rd_session) => {
                // Extract IP from RustDesk session if available
                Some("192.168.1.100") // Placeholder
            }
            ActiveProtocol::Standard(_) => None,
        };

        self.create_optimal_console(&current_session.vm_name, vm_ip)
            .await
    }
}

#[derive(Debug, Clone)]
struct VmAnalysis {
    cpu_cores: u32,
    memory_mb: u64,
    has_gpu: bool,
    network_latency_ms: f32,
    network_bandwidth_mbps: f32,
    os_type: String,
    supports_guest_agent: bool,
    supports_multi_monitor: bool,
}

impl Default for VmAnalysis {
    fn default() -> Self {
        Self {
            cpu_cores: 2,
            memory_mb: 2048,
            has_gpu: false,
            network_latency_ms: 10.0,
            network_bandwidth_mbps: 100.0,
            os_type: "unknown".to_string(),
            supports_guest_agent: false,
            supports_multi_monitor: false,
        }
    }
}

impl Default for EnhancedConsoleConfig {
    fn default() -> Self {
        Self {
            standard_console: ConsoleConfig::default(),
            rustdesk_config: RustDeskConfig::default(),
            preferred_protocol: PreferredProtocol::Auto,
            auto_install_agents: true,
            performance_monitoring: true,
            session_recording: false,
            multi_monitor_support: true,
        }
    }
}
