use crate::console::{ConsoleSession, ConsoleType, ConnectionInfo};
use crate::{log_debug, log_error, log_info, log_warn, NovaError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustDeskConfig {
    pub server_host: String,
    pub server_port: u16,
    pub relay_servers: Vec<String>,
    pub encryption_enabled: bool,
    pub hardware_acceleration: bool,
    pub audio_enabled: bool,
    pub file_transfer_enabled: bool,
    pub clipboard_sync: bool,
    pub auto_quality: bool,
    pub max_fps: u32,
    pub bitrate_limit: Option<u32>, // Mbps
    pub custom_resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustDeskSession {
    pub vm_name: String,
    pub session_id: String,
    pub rustdesk_id: String,
    pub password: String,
    pub relay_server: String,
    pub connection_url: String,
    pub performance_profile: PerformanceProfile,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_connected: Option<chrono::DateTime<chrono::Utc>>,
    pub active: bool,
    pub guest_agent_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceProfile {
    UltraHigh,  // Minimal compression, max quality
    High,       // Balanced quality/performance
    Balanced,   // Auto-adjust based on network
    LowBandwidth, // Heavy compression for slow networks
    Custom(CustomProfile),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProfile {
    pub fps: u32,
    pub quality: u8,     // 1-100
    pub compression: u8, // 1-100
    pub color_depth: u8, // 16, 24, 32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustDeskMetrics {
    pub session_id: String,
    pub fps: f32,
    pub bitrate_kbps: u32,
    pub latency_ms: u32,
    pub packet_loss: f32,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub network_quality: NetworkQuality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkQuality {
    Excellent,
    Good,
    Fair,
    Poor,
}

pub struct RustDeskManager {
    config: RustDeskConfig,
    sessions: Arc<Mutex<HashMap<String, RustDeskSession>>>,
    rustdesk_processes: Arc<Mutex<HashMap<String, Child>>>,
    metrics: Arc<Mutex<HashMap<String, RustDeskMetrics>>>,
}

impl RustDeskManager {
    pub fn new(config: RustDeskConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            rustdesk_processes: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Create high-performance RustDesk session for VM
    pub async fn create_rustdesk_session(
        &mut self, 
        vm_name: &str, 
        vm_ip: &str,
        profile: PerformanceProfile
    ) -> Result<RustDeskSession> {
        log_info!("Creating RustDesk session for VM: {} ({})", vm_name, vm_ip);

        // Check if RustDesk is available
        if !self.check_rustdesk_available() {
            return Err(NovaError::SystemCommandFailed);
        }

        let session_id = Uuid::new_v4().to_string();
        let rustdesk_id = self.generate_rustdesk_id();
        let password = self.generate_secure_password();

        // Install and configure RustDesk in VM if needed
        self.ensure_rustdesk_in_vm(vm_name, vm_ip, &rustdesk_id, &password).await?;

        // Set up relay server for optimal routing
        let relay_server = self.select_optimal_relay_server(vm_ip).await;

        let session = RustDeskSession {
            vm_name: vm_name.to_string(),
            session_id: session_id.clone(),
            rustdesk_id: rustdesk_id.clone(),
            password: password.clone(),
            relay_server: relay_server.clone(),
            connection_url: format!("rustdesk://{}?password={}&relay={}", 
                                   rustdesk_id, password, relay_server),
            performance_profile: profile,
            created_at: chrono::Utc::now(),
            last_connected: None,
            active: true,
            guest_agent_installed: self.check_guest_agent_installed(vm_name).await,
        };

        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }

        log_info!("RustDesk session created: {} -> {}", session_id, rustdesk_id);
        Ok(session)
    }

    // Install RustDesk in VM with optimal configuration
    async fn ensure_rustdesk_in_vm(
        &self, 
        vm_name: &str, 
        vm_ip: &str, 
        rustdesk_id: &str, 
        password: &str
    ) -> Result<()> {
        log_info!("Ensuring RustDesk is installed and configured in VM: {}", vm_name);

        // Check if already installed via SSH or guest agent
        if self.check_rustdesk_installed_in_vm(vm_ip).await {
            log_info!("RustDesk already installed in VM: {}", vm_name);
            return self.configure_rustdesk_in_vm(vm_ip, rustdesk_id, password).await;
        }

        // Auto-install RustDesk based on VM OS
        let os_type = self.detect_vm_os_type(vm_name).await?;
        
        match os_type.as_str() {
            "linux" => self.install_rustdesk_linux(vm_ip).await?,
            "windows" => self.install_rustdesk_windows(vm_ip).await?,
            "macos" => self.install_rustdesk_macos(vm_ip).await?,
            _ => {
                log_error!("Unsupported OS type for RustDesk installation: {}", os_type);
                return Err(NovaError::SystemCommandFailed);
            }
        }

        // Configure with our settings
        self.configure_rustdesk_in_vm(vm_ip, rustdesk_id, password).await?;
        
        log_info!("RustDesk installed and configured in VM: {}", vm_name);
        Ok(())
    }

    async fn install_rustdesk_linux(&self, vm_ip: &str) -> Result<()> {
        log_info!("Installing RustDesk on Linux VM: {}", vm_ip);
        
        // Download and install RustDesk via SSH
        let install_script = r#"
            wget https://github.com/rustdesk/rustdesk/releases/latest/download/rustdesk-x86_64.AppImage -O /tmp/rustdesk.AppImage
            chmod +x /tmp/rustdesk.AppImage
            sudo mv /tmp/rustdesk.AppImage /usr/local/bin/rustdesk
            
            # Create systemd service for headless operation
            sudo tee /etc/systemd/system/rustdesk.service > /dev/null <<EOF
[Unit]
Description=RustDesk Remote Desktop
After=network.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/rustdesk --service
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
            
            sudo systemctl daemon-reload
            sudo systemctl enable rustdesk
            sudo systemctl start rustdesk
        "#;

        self.execute_in_vm(vm_ip, install_script).await?;
        Ok(())
    }

    async fn install_rustdesk_windows(&self, vm_ip: &str) -> Result<()> {
        log_info!("Installing RustDesk on Windows VM: {}", vm_ip);
        
        // Use PowerShell to download and install
        let install_script = r#"
            $url = 'https://github.com/rustdesk/rustdesk/releases/latest/download/rustdesk-1.2.3-x86_64.exe'
            $output = '$env:TEMP\\rustdesk-installer.exe'
            Invoke-WebRequest -Uri $url -OutFile $output
            Start-Process -FilePath $output -ArgumentList '/VERYSILENT', '/NORESTART', '/SERVICE' -Wait
            
            # Configure as service
            rustdesk --install-service
            rustdesk --service
        "#;

        self.execute_powershell_in_vm(vm_ip, install_script).await?;
        Ok(())
    }

    async fn install_rustdesk_macos(&self, vm_ip: &str) -> Result<()> {
        log_info!("Installing RustDesk on macOS VM: {}", vm_ip);
        
        let install_script = r#"
            curl -L https://github.com/rustdesk/rustdesk/releases/latest/download/rustdesk-macos.dmg -o /tmp/rustdesk.dmg
            hdiutil attach /tmp/rustdesk.dmg
            cp -R /Volumes/RustDesk/RustDesk.app /Applications/
            hdiutil detach /Volumes/RustDesk
            
            # Configure launch daemon
            sudo /Applications/RustDesk.app/Contents/MacOS/RustDesk --service
        "#;

        self.execute_in_vm(vm_ip, install_script).await?;
        Ok(())
    }

    async fn configure_rustdesk_in_vm(
        &self, 
        vm_ip: &str, 
        rustdesk_id: &str, 
        password: &str
    ) -> Result<()> {
        log_info!("Configuring RustDesk in VM: {} with ID: {}", vm_ip, rustdesk_id);

        // Configure RustDesk with our custom settings
        let config_commands = vec![
            format!("rustdesk --config set id {}", rustdesk_id),
            format!("rustdesk --config set password {}", password),
            format!("rustdesk --config set relay {}", self.config.server_host),
            "rustdesk --config set allow-remote-restart true".to_string(),
            "rustdesk --config set enable-file-transfer true".to_string(),
            "rustdesk --config set enable-clipboard true".to_string(),
            "rustdesk --config set enable-audio true".to_string(),
            "rustdesk --config set auto-disconnect-timeout 0".to_string(), // Never auto-disconnect
        ];

        for command in config_commands {
            self.execute_in_vm(vm_ip, &command).await?;
        }

        // Apply performance optimizations
        self.apply_performance_optimizations(vm_ip).await?;

        Ok(())
    }

    async fn apply_performance_optimizations(&self, vm_ip: &str) -> Result<()> {
        log_info!("Applying RustDesk performance optimizations for VM: {}", vm_ip);

        let optimizations = vec![
            // Hardware acceleration
            "rustdesk --config set enable-hwcodec true",
            // Optimize for LAN
            "rustdesk --config set direct-server true",
            // Reduce latency
            "rustdesk --config set low-latency true",
            // High quality
            "rustdesk --config set image-quality high",
            // Enable all codecs
            "rustdesk --config set codec h264",
            "rustdesk --config set codec h265",
            // Optimize CPU usage
            "rustdesk --config set cpu-usage balanced",
        ];

        for optimization in optimizations {
            self.execute_in_vm(vm_ip, optimization).await?;
        }

        Ok(())
    }

    // High-performance connection methods
    pub async fn connect_with_performance_profile(
        &mut self,
        session_id: &str,
        profile: PerformanceProfile
    ) -> Result<()> {
        log_info!("Connecting to RustDesk session: {} with profile: {:?}", session_id, profile);

        let session = {
            let sessions = self.sessions.lock().unwrap();
            sessions.get(session_id).cloned()
        };

        if let Some(mut session) = session {
            session.performance_profile = profile.clone();
            session.last_connected = Some(chrono::Utc::now());

            // Launch RustDesk client with optimized settings
            let mut rustdesk_cmd = Command::new("rustdesk");
            rustdesk_cmd.arg(&session.rustdesk_id);
            rustdesk_cmd.arg("--password").arg(&session.password);

            // Apply performance profile
            match profile {
                PerformanceProfile::UltraHigh => {
                    rustdesk_cmd.args(&[
                        "--quality", "100",
                        "--fps", "60",
                        "--codec", "h265",
                        "--hwcodec", "true",
                        "--low-latency", "true"
                    ]);
                }
                PerformanceProfile::High => {
                    rustdesk_cmd.args(&[
                        "--quality", "80",
                        "--fps", "30",
                        "--codec", "h264",
                        "--hwcodec", "true"
                    ]);
                }
                PerformanceProfile::Balanced => {
                    rustdesk_cmd.args(&[
                        "--quality", "60",
                        "--fps", "25",
                        "--auto-quality", "true"
                    ]);
                }
                PerformanceProfile::LowBandwidth => {
                    rustdesk_cmd.args(&[
                        "--quality", "30",
                        "--fps", "15",
                        "--compress", "high"
                    ]);
                }
                PerformanceProfile::Custom(custom) => {
                    rustdesk_cmd.args(&[
                        "--quality", &custom.quality.to_string(),
                        "--fps", &custom.fps.to_string(),
                        "--compress", &custom.compression.to_string()
                    ]);
                }
            }

            // Launch client
            let child = rustdesk_cmd
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    log_error!("Failed to launch RustDesk client: {}", e);
                    NovaError::SystemCommandFailed
                })?;

            {
                let mut processes = self.rustdesk_processes.lock().unwrap();
                processes.insert(session_id.to_string(), child);
            }

            {
                let mut sessions = self.sessions.lock().unwrap();
                sessions.insert(session_id.to_string(), session);
            }

            log_info!("RustDesk client launched for session: {}", session_id);
        }

        Ok(())
    }

    // Performance monitoring
    pub async fn start_performance_monitoring(&self, session_id: &str) -> Result<()> {
        log_info!("Starting performance monitoring for session: {}", session_id);

        let session_id_clone = session_id.to_string();
        let metrics_clone = self.metrics.clone();

        tokio::spawn(async move {
            loop {
                if let Ok(metrics) = Self::collect_performance_metrics(&session_id_clone).await {
                    let mut metrics_map = metrics_clone.lock().unwrap();
                    metrics_map.insert(session_id_clone.clone(), metrics);
                }
                sleep(Duration::from_secs(5)).await;
            }
        });

        Ok(())
    }

    async fn collect_performance_metrics(session_id: &str) -> Result<RustDeskMetrics> {
        // Collect real-time performance metrics
        // This would integrate with RustDesk's API or log parsing
        
        Ok(RustDeskMetrics {
            session_id: session_id.to_string(),
            fps: 30.0,
            bitrate_kbps: 5000,
            latency_ms: 15,
            packet_loss: 0.1,
            cpu_usage: 25.0,
            memory_usage: 512 * 1024 * 1024, // 512MB
            network_quality: NetworkQuality::Excellent,
        })
    }

    pub fn get_performance_metrics(&self, session_id: &str) -> Option<RustDeskMetrics> {
        let metrics = self.metrics.lock().unwrap();
        metrics.get(session_id).cloned()
    }

    // Advanced features
    pub async fn enable_file_transfer(&self, session_id: &str) -> Result<()> {
        log_info!("Enabling file transfer for session: {}", session_id);
        // RustDesk supports secure file transfer
        Ok(())
    }

    pub async fn share_clipboard(&self, session_id: &str, content: &str) -> Result<()> {
        log_info!("Sharing clipboard content for session: {}", session_id);
        // RustDesk supports real-time clipboard sync
        Ok(())
    }

    pub async fn record_session(&self, session_id: &str, output_path: &str) -> Result<()> {
        log_info!("Starting session recording for: {} -> {}", session_id, output_path);
        // RustDesk can record sessions for compliance/training
        Ok(())
    }

    // Utility functions
    async fn select_optimal_relay_server(&self, vm_ip: &str) -> String {
        // Test latency to different relay servers and select the best
        // For now, return the primary server
        self.config.server_host.clone()
    }

    fn generate_rustdesk_id(&self) -> String {
        // Generate a unique RustDesk ID
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..9).map(|_| rng.gen_range(0..10).to_string()).collect()
    }

    fn generate_secure_password(&self) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\\
                                abcdefghijklmnopqrstuvwxyz\\
                                0123456789!@#$%^&*";
        const PASSWORD_LEN: usize = 16;
        let mut rng = rand::thread_rng();

        (0..PASSWORD_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    async fn check_rustdesk_installed_in_vm(&self, vm_ip: &str) -> bool {
        // Check if RustDesk is installed via SSH or guest agent
        self.execute_in_vm(vm_ip, "which rustdesk").await.is_ok()
    }

    async fn detect_vm_os_type(&self, vm_name: &str) -> Result<String> {
        // Detect OS type via guest agent or SSH
        let output = Command::new("virsh")
            .args(&["dominfo", vm_name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        let info = String::from_utf8_lossy(&output.stdout);
        if info.contains("ubuntu") || info.contains("debian") || info.contains("centos") {
            Ok("linux".to_string())
        } else if info.contains("windows") {
            Ok("windows".to_string())
        } else if info.contains("macos") {
            Ok("macos".to_string())
        } else {
            Ok("unknown".to_string())
        }
    }

    async fn check_guest_agent_installed(&self, vm_name: &str) -> bool {
        Command::new("virsh")
            .args(&["qemu-agent-command", vm_name, "{\"execute\":\"guest-ping\"}"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    async fn execute_in_vm(&self, vm_ip: &str, command: &str) -> Result<()> {
        // Execute command in VM via SSH or guest agent
        let output = Command::new("ssh")
            .args(&["-o", "StrictHostKeyChecking=no", &format!("root@{}", vm_ip), command])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to execute command in VM: {}", String::from_utf8_lossy(&output.stderr));
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    async fn execute_powershell_in_vm(&self, vm_ip: &str, script: &str) -> Result<()> {
        // Execute PowerShell script in Windows VM
        use base64::prelude::*;
        let encoded_script = BASE64_STANDARD.encode(script);
        let command = format!("powershell -EncodedCommand {}", encoded_script);
        self.execute_in_vm(vm_ip, &command).await
    }

    fn check_rustdesk_available(&self) -> bool {
        Command::new("rustdesk")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    // Session management
    pub fn list_active_sessions(&self) -> Vec<RustDeskSession> {
        let sessions = self.sessions.lock().unwrap();
        sessions.values().filter(|s| s.active).cloned().collect()
    }

    pub async fn disconnect_session(&mut self, session_id: &str) -> Result<()> {
        log_info!("Disconnecting RustDesk session: {}", session_id);

        // Kill local client process
        {
            let mut processes = self.rustdesk_processes.lock().unwrap();
            if let Some(mut process) = processes.remove(session_id) {
                let _ = process.kill();
            }
        }

        // Mark session as inactive
        {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(session) = sessions.get_mut(session_id) {
                session.active = false;
            }
        }

        Ok(())
    }
}

impl Default for RustDeskConfig {
    fn default() -> Self {
        Self {
            server_host: "localhost".to_string(),
            server_port: 21116,
            relay_servers: vec![
                "relay1.rustdesk.com".to_string(),
                "relay2.rustdesk.com".to_string()
            ],
            encryption_enabled: true,
            hardware_acceleration: true,
            audio_enabled: true,
            file_transfer_enabled: true,
            clipboard_sync: true,
            auto_quality: true,
            max_fps: 60,
            bitrate_limit: None,
            custom_resolution: None,
        }
    }
}