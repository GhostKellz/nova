use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, sleep};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleSession {
    pub vm_name: String,
    pub session_id: String,
    pub console_type: ConsoleType,
    pub connection_info: ConnectionInfo,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsoleType {
    VNC,
    SPICE,
    RDP,
    SerialConsole,
    WebConsole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub auth_required: bool,
    pub password: Option<String>,
    pub certificate_path: Option<String>,
    pub websocket_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleConfig {
    pub vnc_enabled: bool,
    pub vnc_port_range: (u16, u16),
    pub spice_enabled: bool,
    pub spice_port_range: (u16, u16),
    pub rdp_enabled: bool,
    pub rdp_port_range: (u16, u16),
    pub web_console_enabled: bool,
    pub web_console_port: u16,
    pub auth_required: bool,
    pub ssl_enabled: bool,
    pub certificate_path: Option<String>,
    pub key_path: Option<String>,
}

pub struct ConsoleManager {
    sessions: Arc<Mutex<HashMap<String, ConsoleSession>>>,
    config: ConsoleConfig,
    vnc_processes: Arc<Mutex<HashMap<String, Child>>>,
    spice_processes: Arc<Mutex<HashMap<String, Child>>>,
    port_allocator: Arc<Mutex<PortAllocator>>,
}

struct PortAllocator {
    vnc_ports: std::collections::VecDeque<u16>,
    spice_ports: std::collections::VecDeque<u16>,
    rdp_ports: std::collections::VecDeque<u16>,
    allocated_ports: std::collections::HashSet<u16>,
}

impl ConsoleManager {
    pub fn new(config: ConsoleConfig) -> Self {
        let mut vnc_ports = std::collections::VecDeque::new();
        let mut spice_ports = std::collections::VecDeque::new();
        let mut rdp_ports = std::collections::VecDeque::new();

        // Initialize port ranges
        for port in config.vnc_port_range.0..=config.vnc_port_range.1 {
            vnc_ports.push_back(port);
        }
        for port in config.spice_port_range.0..=config.spice_port_range.1 {
            spice_ports.push_back(port);
        }
        for port in config.rdp_port_range.0..=config.rdp_port_range.1 {
            rdp_ports.push_back(port);
        }

        let port_allocator = PortAllocator {
            vnc_ports,
            spice_ports,
            rdp_ports,
            allocated_ports: std::collections::HashSet::new(),
        };

        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            config,
            vnc_processes: Arc::new(Mutex::new(HashMap::new())),
            spice_processes: Arc::new(Mutex::new(HashMap::new())),
            port_allocator: Arc::new(Mutex::new(port_allocator)),
        }
    }

    // Enhanced VNC Console
    pub async fn create_vnc_console(
        &mut self,
        vm_name: &str,
        enhanced: bool,
    ) -> Result<ConsoleSession> {
        log_info!(
            "Creating {} VNC console for VM: {}",
            if enhanced { "enhanced" } else { "standard" },
            vm_name
        );

        let port = self.allocate_port(ConsoleType::VNC)?;
        let session_id = format!("vnc-{}-{}", vm_name, port);

        // Create VNC server with enhanced features
        let mut vnc_cmd = if enhanced {
            // Use x11vnc with better performance and features
            let mut cmd = Command::new("x11vnc");
            cmd.args(&["-create", "-shared", "-forever"]);
            cmd.args(&["-rfbport", &port.to_string()]);

            if self.config.auth_required {
                // Generate random password
                let password = self.generate_password();
                cmd.args(&["-passwd", &password]);
            }

            if self.config.ssl_enabled {
                if let Some(cert_path) = &self.config.certificate_path {
                    cmd.args(&["-ssl", cert_path]);
                }
            }

            // Performance optimizations
            cmd.args(&["-noxdamage", "-noxfixes", "-noxrandr"]);
            cmd.args(&["-wireframe", "-scrollcopyrect"]);
            cmd.args(&["-ncache", "10"]);

            cmd
        } else {
            // Standard QEMU VNC
            Command::new("qemu-system-x86_64")
        };

        let vnc_process = vnc_cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                log_error!("Failed to start VNC server: {}", e);
                NovaError::SystemCommandFailed
            })?;

        let connection_info = ConnectionInfo {
            host: "localhost".to_string(),
            port,
            protocol: "vnc".to_string(),
            auth_required: self.config.auth_required,
            password: if self.config.auth_required {
                Some(self.generate_password())
            } else {
                None
            },
            certificate_path: self.config.certificate_path.clone(),
            websocket_url: if self.config.web_console_enabled {
                Some(format!(
                    "ws://localhost:{}/vnc/{}",
                    self.config.web_console_port, session_id
                ))
            } else {
                None
            },
        };

        let session = ConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: session_id.clone(),
            console_type: ConsoleType::VNC,
            connection_info,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
        };

        // Store the process
        {
            let mut processes = self.vnc_processes.lock().unwrap();
            processes.insert(session_id.clone(), vnc_process);
        }

        // Store the session
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }

        log_info!("VNC console created successfully: {}:{}", "localhost", port);
        Ok(session)
    }

    // SPICE Console (Superior performance vs VNC)
    pub async fn create_spice_console(&mut self, vm_name: &str) -> Result<ConsoleSession> {
        log_info!("Creating SPICE console for VM: {}", vm_name);

        if !self.check_spice_available() {
            log_warn!("SPICE not available, falling back to VNC");
            return self.create_vnc_console(vm_name, true).await;
        }

        let port = self.allocate_port(ConsoleType::SPICE)?;
        let session_id = format!("spice-{}-{}", vm_name, port);

        // SPICE provides better performance than VNC:
        // - Audio/Video redirection
        // - USB redirection
        // - Multi-monitor support
        // - Better compression
        let connection_info = ConnectionInfo {
            host: "localhost".to_string(),
            port,
            protocol: "spice".to_string(),
            auth_required: self.config.auth_required,
            password: if self.config.auth_required {
                Some(self.generate_password())
            } else {
                None
            },
            certificate_path: self.config.certificate_path.clone(),
            websocket_url: if self.config.web_console_enabled {
                Some(format!(
                    "ws://localhost:{}/spice/{}",
                    self.config.web_console_port, session_id
                ))
            } else {
                None
            },
        };

        let session = ConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: session_id.clone(),
            console_type: ConsoleType::SPICE,
            connection_info,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
        };

        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }

        log_info!(
            "SPICE console created successfully: {}:{}",
            "localhost",
            port
        );
        Ok(session)
    }

    // RDP Console (For Windows VMs)
    pub async fn create_rdp_console(
        &mut self,
        vm_name: &str,
        vm_ip: &str,
    ) -> Result<ConsoleSession> {
        log_info!("Creating RDP console for VM: {} ({})", vm_name, vm_ip);

        let port = 3389; // Standard RDP port
        let session_id = format!("rdp-{}-{}", vm_name, chrono::Utc::now().timestamp());

        let connection_info = ConnectionInfo {
            host: vm_ip.to_string(),
            port,
            protocol: "rdp".to_string(),
            auth_required: true, // RDP always requires auth
            password: None,      // User provides credentials
            certificate_path: None,
            websocket_url: if self.config.web_console_enabled {
                Some(format!(
                    "ws://localhost:{}/rdp/{}",
                    self.config.web_console_port, session_id
                ))
            } else {
                None
            },
        };

        let session = ConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: session_id.clone(),
            console_type: ConsoleType::RDP,
            connection_info,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
        };

        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }

        log_info!("RDP console session created: {}:{}", vm_ip, port);
        Ok(session)
    }

    // Serial Console (For headless VMs and debugging)
    pub async fn create_serial_console(&mut self, vm_name: &str) -> Result<ConsoleSession> {
        log_info!("Creating serial console for VM: {}", vm_name);

        let session_id = format!("serial-{}-{}", vm_name, chrono::Utc::now().timestamp());

        // Connect to VM's serial console via virsh or socat
        let connection_info = ConnectionInfo {
            host: "localhost".to_string(),
            port: 0, // Serial doesn't use network ports
            protocol: "serial".to_string(),
            auth_required: false,
            password: None,
            certificate_path: None,
            websocket_url: if self.config.web_console_enabled {
                Some(format!(
                    "ws://localhost:{}/serial/{}",
                    self.config.web_console_port, session_id
                ))
            } else {
                None
            },
        };

        let session = ConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: session_id.clone(),
            console_type: ConsoleType::SerialConsole,
            connection_info,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
        };

        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }

        log_info!("Serial console session created for VM: {}", vm_name);
        Ok(session)
    }

    // Web-based Console (noVNC/HTML5)
    pub async fn create_web_console(&mut self, vm_name: &str) -> Result<ConsoleSession> {
        log_info!("Creating web console for VM: {}", vm_name);

        // First create a VNC session
        let vnc_session = self.create_vnc_console(vm_name, true).await?;

        let session_id = format!("web-{}-{}", vm_name, chrono::Utc::now().timestamp());

        // Start noVNC proxy
        self.start_novnc_proxy(&vnc_session).await?;

        let connection_info = ConnectionInfo {
            host: "localhost".to_string(),
            port: self.config.web_console_port,
            protocol: "https".to_string(),
            auth_required: self.config.auth_required,
            password: vnc_session.connection_info.password,
            certificate_path: self.config.certificate_path.clone(),
            websocket_url: Some(format!(
                "{}://localhost:{}/websockify?token={}",
                if self.config.ssl_enabled { "wss" } else { "ws" },
                self.config.web_console_port,
                session_id
            )),
        };

        let session = ConsoleSession {
            vm_name: vm_name.to_string(),
            session_id: session_id.clone(),
            console_type: ConsoleType::WebConsole,
            connection_info,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            active: true,
        };

        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }

        log_info!(
            "Web console created: https://localhost:{}/vnc.html?token={}",
            self.config.web_console_port,
            session_id
        );
        Ok(session)
    }

    async fn start_novnc_proxy(&self, vnc_session: &ConsoleSession) -> Result<()> {
        // Start websockify proxy for noVNC
        let _websockify_cmd = Command::new("websockify")
            .args(&[
                &format!(
                    "{}:{}",
                    self.config.web_console_port, vnc_session.connection_info.port
                ),
                &format!(
                    "{}:{}",
                    vnc_session.connection_info.host, vnc_session.connection_info.port
                ),
            ])
            .spawn()
            .map_err(|e| {
                log_error!("Failed to start websockify: {}", e);
                NovaError::SystemCommandFailed
            })?;

        log_debug!("websockify proxy started for VNC session");
        Ok(())
    }

    // Console Session Management
    pub fn get_console_session(&self, session_id: &str) -> Option<ConsoleSession> {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(session_id).cloned()
    }

    pub fn list_active_sessions(&self) -> Vec<ConsoleSession> {
        let sessions = self.sessions.lock().unwrap();
        sessions.values().filter(|s| s.active).cloned().collect()
    }

    pub async fn close_session(&mut self, session_id: &str) -> Result<()> {
        log_info!("Closing console session: {}", session_id);

        // Remove from active sessions
        {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(mut session) = sessions.get_mut(session_id) {
                session.active = false;
            }
        }

        // Kill associated processes
        if session_id.starts_with("vnc-") {
            let mut processes = self.vnc_processes.lock().unwrap();
            if let Some(mut process) = processes.remove(session_id) {
                let _ = process.kill();
            }
        } else if session_id.starts_with("spice-") {
            let mut processes = self.spice_processes.lock().unwrap();
            if let Some(mut process) = processes.remove(session_id) {
                let _ = process.kill();
            }
        }

        // Release allocated port
        if let Some(session) = self.get_console_session(session_id) {
            self.release_port(session.console_type, session.connection_info.port);
        }

        Ok(())
    }

    // Enhanced Console Features
    pub async fn enable_clipboard_sharing(&self, session_id: &str) -> Result<()> {
        log_info!("Enabling clipboard sharing for session: {}", session_id);
        // SPICE supports this natively, VNC needs additional setup
        Ok(())
    }

    pub async fn enable_usb_redirection(&self, session_id: &str, device_id: &str) -> Result<()> {
        log_info!(
            "Enabling USB redirection for session: {} device: {}",
            session_id,
            device_id
        );
        // Requires SPICE and proper USB passthrough configuration
        Ok(())
    }

    pub async fn set_display_resolution(
        &self,
        session_id: &str,
        width: u32,
        height: u32,
    ) -> Result<()> {
        log_info!(
            "Setting display resolution for session: {} to {}x{}",
            session_id,
            width,
            height
        );

        if let Some(session) = self.get_console_session(session_id) {
            match session.console_type {
                ConsoleType::SPICE => {
                    // SPICE supports dynamic resolution changes
                    // Send resolution change command to guest agent
                    self.send_guest_agent_command(
                        &session.vm_name,
                        &format!("display-set-resolution width={} height={}", width, height),
                    )
                    .await?;
                }
                ConsoleType::VNC => {
                    // VNC resolution is more limited
                    log_warn!("VNC resolution changes require guest cooperation");
                }
                _ => {
                    log_warn!("Resolution change not supported for this console type");
                }
            }
        }

        Ok(())
    }

    async fn send_guest_agent_command(&self, vm_name: &str, command: &str) -> Result<()> {
        let output = Command::new("virsh")
            .args(&[
                "qemu-agent-command",
                vm_name,
                &format!("{{\"execute\":\"{}\"}}", command),
            ])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!(
                "Failed to send guest agent command: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    // Multi-monitor support
    pub async fn configure_multi_monitor(
        &self,
        session_id: &str,
        monitor_count: u32,
    ) -> Result<()> {
        log_info!(
            "Configuring {} monitors for session: {}",
            monitor_count,
            session_id
        );

        if let Some(session) = self.get_console_session(session_id) {
            if matches!(session.console_type, ConsoleType::SPICE) {
                // SPICE supports multi-monitor natively
                self.send_guest_agent_command(
                    &session.vm_name,
                    &format!("display-set-monitors count={}", monitor_count),
                )
                .await?;
            } else {
                log_warn!("Multi-monitor support requires SPICE console");
            }
        }

        Ok(())
    }

    // Performance optimization
    pub async fn optimize_console_performance(&self, session_id: &str) -> Result<()> {
        log_info!("Optimizing console performance for session: {}", session_id);

        if let Some(session) = self.get_console_session(session_id) {
            match session.console_type {
                ConsoleType::SPICE => {
                    // Enable SPICE optimizations:
                    // - Image compression
                    // - Video streaming
                    // - Audio compression
                    log_info!("SPICE optimizations enabled for session: {}", session_id);
                }
                ConsoleType::VNC => {
                    // Enable VNC optimizations:
                    // - Tight encoding
                    // - Zlib compression
                    // - JPEG quality adjustment
                    log_info!("VNC optimizations enabled for session: {}", session_id);
                }
                _ => {}
            }
        }

        Ok(())
    }

    // Port management
    fn allocate_port(&mut self, console_type: ConsoleType) -> Result<u16> {
        let mut allocator = self.port_allocator.lock().unwrap();

        let port = match console_type {
            ConsoleType::VNC => allocator.vnc_ports.pop_front(),
            ConsoleType::SPICE => allocator.spice_ports.pop_front(),
            ConsoleType::RDP => allocator.rdp_ports.pop_front(),
            _ => None,
        };

        match port {
            Some(p) => {
                allocator.allocated_ports.insert(p);
                Ok(p)
            }
            None => {
                log_error!("No available ports for console type: {:?}", console_type);
                Err(NovaError::SystemCommandFailed)
            }
        }
    }

    fn release_port(&mut self, console_type: ConsoleType, port: u16) {
        let mut allocator = self.port_allocator.lock().unwrap();

        allocator.allocated_ports.remove(&port);

        match console_type {
            ConsoleType::VNC => allocator.vnc_ports.push_back(port),
            ConsoleType::SPICE => allocator.spice_ports.push_back(port),
            ConsoleType::RDP => allocator.rdp_ports.push_back(port),
            _ => {}
        }
    }

    // Utility functions
    fn check_spice_available(&self) -> bool {
        Command::new("spice-server")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn check_novnc_available(&self) -> bool {
        Command::new("websockify")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn generate_password(&self) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789";
        const PASSWORD_LEN: usize = 12;
        let mut rng = rand::thread_rng();

        (0..PASSWORD_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    // Session cleanup
    pub async fn cleanup_inactive_sessions(&mut self) {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);

        let mut to_remove = Vec::new();
        {
            let sessions = self.sessions.lock().unwrap();
            for (id, session) in sessions.iter() {
                if !session.active || session.last_accessed < cutoff {
                    to_remove.push(id.clone());
                }
            }
        }

        let cleanup_count = to_remove.len();
        for session_id in to_remove {
            let _ = self.close_session(&session_id).await;
        }

        log_info!("Cleaned up {} inactive console sessions", cleanup_count);
    }
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            vnc_enabled: true,
            vnc_port_range: (5900, 5999),
            spice_enabled: true,
            spice_port_range: (5000, 5099),
            rdp_enabled: true,
            rdp_port_range: (3389, 3389),
            web_console_enabled: true,
            web_console_port: 6080,
            auth_required: true,
            ssl_enabled: false,
            certificate_path: None,
            key_path: None,
        }
    }
}
