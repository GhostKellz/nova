//! Bolt Container Runtime Integration
//!
//! Integrates Bolt's high-performance container runtime with Nova.
//! Bolt provides ultra-fast GPU passthrough (<100μs vs Docker's ~10ms),
//! gaming optimizations, and BTRFS/ZFS snapshots.

use crate::container_runtime::*;
use crate::{log_debug, log_error, log_info};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Bolt runtime implementation
pub struct BoltRuntime {
    available: bool,
    version: Option<String>,
}

impl BoltRuntime {
    pub fn new() -> Self {
        let available = Self::check_bolt_installed();
        let version = if available {
            Self::get_bolt_version()
        } else {
            None
        };

        if available {
            log_info!(
                "Bolt runtime initialized (version: {})",
                version.as_deref().unwrap_or("unknown")
            );
        } else {
            log_debug!("Bolt runtime not available");
        }

        Self { available, version }
    }

    fn check_bolt_installed() -> bool {
        Command::new("bolt")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn get_bolt_version() -> Option<String> {
        Command::new("bolt")
            .arg("--version")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }

    /// Convert Nova container config to Bolt run arguments
    fn build_bolt_args(&self, name: Option<&str>, config: &ContainerConfig) -> Vec<String> {
        let mut args = vec!["run".to_string()];

        // Name
        if let Some(n) = name {
            args.push("--name".to_string());
            args.push(n.to_string());
        }

        // Detach
        if config.detach {
            args.push("-d".to_string());
        }

        // Ports
        for port in &config.ports {
            args.push("-p".to_string());
            args.push(port.clone());
        }

        // Volumes
        for volume in &config.volumes {
            args.push("-v".to_string());
            args.push(volume.clone());
        }

        // Environment variables
        for (key, value) in &config.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Network
        if let Some(network) = &config.network {
            args.push("--network".to_string());
            args.push(network.clone());
        }

        // GPU passthrough
        if config.gpu_passthrough {
            args.push("--gpu".to_string());
            args.push("all".to_string());
            // Enable nvbind for ultra-fast GPU passthrough
            args.push("--gpu-runtime".to_string());
            args.push("nvbind".to_string());
        }

        // Memory limit
        if let Some(mem) = config.memory_mb {
            args.push("--memory".to_string());
            args.push(format!("{}m", mem));
        }

        // CPU limit
        if let Some(cpus) = config.cpus {
            args.push("--cpus".to_string());
            args.push(cpus.to_string());
        }

        // Restart policy
        match config.restart_policy {
            RestartPolicy::Always => {
                args.push("--restart".to_string());
                args.push("always".to_string());
            }
            RestartPolicy::OnFailure => {
                args.push("--restart".to_string());
                args.push("on-failure".to_string());
            }
            RestartPolicy::UnlessStopped => {
                args.push("--restart".to_string());
                args.push("unless-stopped".to_string());
            }
            RestartPolicy::No => {}
        }

        // Image
        args.push(config.capsule.clone());

        args
    }

    /// Parse Bolt ps output to ContainerInfo
    fn parse_bolt_ps_line(&self, line: &str) -> Option<ContainerInfo> {
        // Bolt ps format: ID NAME IMAGE STATUS PORTS
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        let status = Self::parse_status(parts.get(3).unwrap_or(&""));

        Some(ContainerInfo {
            id: parts[0].to_string(),
            name: parts.get(1).unwrap_or(&"").to_string(),
            image: parts.get(2).unwrap_or(&"").to_string(),
            status,
            created: chrono::Utc::now(), // Bolt doesn't provide this in ps
            ports: Vec::new(),           // Would need to parse port info
            network: None,
            pid: None,
            ip_address: None,
        })
    }

    fn parse_status(status_str: &str) -> ContainerStatus {
        let status_lower = status_str.to_lowercase();
        if status_lower.contains("running") || status_lower == "up" {
            ContainerStatus::Running
        } else if status_lower.contains("exited") || status_lower.contains("stopped") {
            ContainerStatus::Stopped
        } else if status_lower.contains("paused") {
            ContainerStatus::Paused
        } else if status_lower.contains("restarting") {
            ContainerStatus::Restarting
        } else if status_lower.contains("starting") || status_lower.contains("created") {
            ContainerStatus::Starting
        } else if status_lower.contains("dead") {
            ContainerStatus::Dead
        } else {
            ContainerStatus::Unknown
        }
    }
}

impl ContainerRuntime for BoltRuntime {
    fn is_available(&self) -> bool {
        self.available
    }

    fn name(&self) -> &str {
        "Bolt"
    }

    fn version<'a>(&'a self) -> RuntimeFuture<'a, String> {
        Box::pin(async move {
            self.version.clone().ok_or_else(|| {
                ContainerRuntimeError::RuntimeNotAvailable("Bolt version unknown".to_string())
            })
        })
    }

    fn run_container<'a>(
        &'a self,
        image: &'a str,
        name: Option<&'a str>,
        config: &'a ContainerConfig,
    ) -> RuntimeFuture<'a, String> {
        Box::pin(async move {
            if !self.available {
                return Err(ContainerRuntimeError::RuntimeNotAvailable(
                    "Bolt is not installed".to_string(),
                ));
            }

            log_info!(
                "Starting Bolt container: {} from image {}",
                name.unwrap_or("<unnamed>"),
                image
            );

            // Build command arguments
            let mut bolt_config = config.clone();
            bolt_config.capsule = image.to_string();
            let args = self.build_bolt_args(name, &bolt_config);

            // Execute bolt run command
            let output = Command::new("bolt").args(&args).output().map_err(|e| {
                ContainerRuntimeError::StartFailed(format!("Failed to execute bolt: {}", e))
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log_error!("Bolt run failed: {}", stderr);
                return Err(ContainerRuntimeError::StartFailed(stderr.to_string()));
            }

            // Get container ID/name from output
            let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

            log_info!("Bolt container started: {}", container_id);
            Ok(container_id)
        })
    }

    fn stop_container<'a>(&'a self, id_or_name: &'a str) -> RuntimeFuture<'a, ()> {
        Box::pin(async move {
            log_info!("Stopping Bolt container: {}", id_or_name);

            let output = Command::new("bolt")
                .args(&["stop", id_or_name])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::StopFailed(format!("Failed to execute bolt stop: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ContainerRuntimeError::StopFailed(stderr.to_string()));
            }

            Ok(())
        })
    }

    fn remove_container<'a>(&'a self, id_or_name: &'a str, force: bool) -> RuntimeFuture<'a, ()> {
        Box::pin(async move {
            log_info!("Removing Bolt container: {}", id_or_name);

            let mut args = vec!["rm"];
            if force {
                args.push("-f");
            }
            args.push(id_or_name);

            let output = Command::new("bolt").args(&args).output().map_err(|e| {
                ContainerRuntimeError::Other(format!("Failed to execute bolt rm: {}", e))
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ContainerRuntimeError::Other(stderr.to_string()));
            }

            Ok(())
        })
    }

    fn list_containers<'a>(&'a self, all: bool) -> RuntimeFuture<'a, Vec<ContainerInfo>> {
        Box::pin(async move {
            let mut args = vec!["ps"];
            if all {
                args.push("-a");
            }

            let output = Command::new("bolt").args(&args).output().map_err(|e| {
                ContainerRuntimeError::Other(format!("Failed to execute bolt ps: {}", e))
            })?;

            if !output.status.success() {
                return Ok(Vec::new());
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let containers: Vec<ContainerInfo> = stdout
                .lines()
                .skip(1)
                .filter_map(|line| self.parse_bolt_ps_line(line))
                .collect();

            Ok(containers)
        })
    }

    fn inspect_container<'a>(&'a self, id_or_name: &'a str) -> RuntimeFuture<'a, ContainerInfo> {
        Box::pin(async move {
            let output = Command::new("bolt")
                .args(&["inspect", id_or_name])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::Other(format!("Failed to execute bolt inspect: {}", e))
                })?;

            if !output.status.success() {
                return Err(ContainerRuntimeError::ContainerNotFound(
                    id_or_name.to_string(),
                ));
            }

            let json_str = String::from_utf8_lossy(&output.stdout);
            let inspect_data: BoltInspectData = serde_json::from_str(&json_str)?;

            Ok(ContainerInfo {
                id: inspect_data.id,
                name: inspect_data.name,
                image: inspect_data.image,
                status: Self::parse_status(&inspect_data.status),
                created: inspect_data.created.unwrap_or_else(chrono::Utc::now),
                ports: Vec::new(),
                network: inspect_data.network,
                pid: inspect_data.pid,
                ip_address: inspect_data.ip_address,
            })
        })
    }

    fn pull_image<'a>(&'a self, image: &'a str) -> RuntimeFuture<'a, ()> {
        Box::pin(async move {
            log_info!("Pulling Bolt image: {}", image);

            let output = Command::new("bolt")
                .args(&["pull", image])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::Other(format!("Failed to execute bolt pull: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ContainerRuntimeError::Other(stderr.to_string()));
            }

            Ok(())
        })
    }

    fn list_images<'a>(&'a self) -> RuntimeFuture<'a, Vec<ImageInfo>> {
        Box::pin(async move {
            let output = Command::new("bolt")
                .args(&["images"])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::Other(format!("Failed to execute bolt images: {}", e))
                })?;

            if !output.status.success() {
                return Ok(Vec::new());
            }

            Ok(Vec::new())
        })
    }

    fn get_logs<'a>(&'a self, id_or_name: &'a str, lines: usize) -> RuntimeFuture<'a, Vec<String>> {
        Box::pin(async move {
            let output = Command::new("bolt")
                .args(&["logs", "--tail", &lines.to_string(), id_or_name])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::Other(format!("Failed to execute bolt logs: {}", e))
                })?;

            if !output.status.success() {
                return Err(ContainerRuntimeError::ContainerNotFound(
                    id_or_name.to_string(),
                ));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.lines().map(|s| s.to_string()).collect())
        })
    }

    fn get_stats<'a>(&'a self, id_or_name: &'a str) -> RuntimeFuture<'a, ContainerStats> {
        Box::pin(async move {
            let output = Command::new("bolt")
                .args(&["stats", "--no-stream", id_or_name])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::Other(format!("Failed to execute bolt stats: {}", e))
                })?;

            if !output.status.success() {
                return Err(ContainerRuntimeError::ContainerNotFound(
                    id_or_name.to_string(),
                ));
            }

            Ok(ContainerStats {
                cpu_usage_percent: 0.0,
                memory_usage_mb: 0,
                memory_limit_mb: 0,
                network_rx_bytes: 0,
                network_tx_bytes: 0,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
            })
        })
    }
}

/// Bolt inspect data structure
#[derive(Debug, Deserialize, Serialize)]
struct BoltInspectData {
    id: String,
    name: String,
    image: String,
    status: String,
    created: Option<chrono::DateTime<chrono::Utc>>,
    network: Option<String>,
    pid: Option<u32>,
    ip_address: Option<String>,
}

impl Default for BoltRuntime {
    fn default() -> Self {
        Self::new()
    }
}
