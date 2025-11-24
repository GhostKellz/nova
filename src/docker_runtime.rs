//! Docker Container Runtime Integration
//!
//! Provides Docker as a fallback container runtime when Bolt is not available.
//! Uses Docker's standard CLI for container management.

use crate::container_runtime::*;
use crate::{log_debug, log_error, log_info};
use std::process::Command;

/// Docker runtime implementation
pub struct DockerRuntime {
    available: bool,
    version: Option<String>,
}

impl DockerRuntime {
    pub fn new() -> Self {
        let available = Self::check_docker_installed();
        let version = if available {
            Self::get_docker_version()
        } else {
            None
        };

        if available {
            log_info!(
                "Docker runtime initialized (version: {})",
                version.as_deref().unwrap_or("unknown")
            );
        } else {
            log_debug!("Docker runtime not available");
        }

        Self { available, version }
    }

    fn check_docker_installed() -> bool {
        Command::new("docker")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn get_docker_version() -> Option<String> {
        Command::new("docker")
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

    /// Convert Nova container config to Docker run arguments
    fn build_docker_args(&self, name: Option<&str>, config: &ContainerConfig) -> Vec<String> {
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

        // GPU passthrough (NVIDIA runtime)
        if config.gpu_passthrough {
            args.push("--gpus".to_string());
            args.push("all".to_string());
            // Note: Docker uses NVIDIA Container Toolkit which is slower than Bolt's nvbind
            log_debug!("Using Docker with NVIDIA Container Toolkit (slower than Bolt+nvbind)");
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

    /// Parse Docker ps output (pipe-delimited format)
    fn parse_docker_ps_line(&self, line: &str) -> Option<ContainerInfo> {
        // Docker ps --format output: ID|NAME|IMAGE|STATUS
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            return None;
        }

        let status = Self::parse_status(parts[3]);

        Some(ContainerInfo {
            id: parts[0].to_string(),
            name: parts[1].to_string(),
            image: parts[2].to_string(),
            status,
            created: chrono::Utc::now(),
            ports: Vec::new(),
            network: None,
            pid: None,
            ip_address: None,
        })
    }

    fn parse_status(status_str: &str) -> ContainerStatus {
        let status_lower = status_str.to_lowercase();
        if status_lower.contains("up") || status_lower.contains("running") {
            ContainerStatus::Running
        } else if status_lower.contains("exited") {
            ContainerStatus::Stopped
        } else if status_lower.contains("paused") {
            ContainerStatus::Paused
        } else if status_lower.contains("restarting") {
            ContainerStatus::Restarting
        } else if status_lower.contains("created") {
            ContainerStatus::Starting
        } else if status_lower.contains("dead") {
            ContainerStatus::Dead
        } else {
            ContainerStatus::Unknown
        }
    }
}

impl ContainerRuntime for DockerRuntime {
    fn is_available(&self) -> bool {
        self.available
    }

    fn name(&self) -> &str {
        "Docker"
    }

    fn version<'a>(&'a self) -> RuntimeFuture<'a, String> {
        Box::pin(async move {
            self.version.clone().ok_or_else(|| {
                ContainerRuntimeError::RuntimeNotAvailable("Docker version unknown".to_string())
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
                    "Docker is not installed".to_string(),
                ));
            }

            log_info!(
                "Starting Docker container: {} from image {}",
                name.unwrap_or("<unnamed>"),
                image
            );

            let mut docker_config = config.clone();
            docker_config.capsule = image.to_string();
            let args = self.build_docker_args(name, &docker_config);

            let output = Command::new("docker").args(&args).output().map_err(|e| {
                ContainerRuntimeError::StartFailed(format!("Failed to execute docker: {}", e))
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log_error!("Docker run failed: {}", stderr);
                return Err(ContainerRuntimeError::StartFailed(stderr.to_string()));
            }

            let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

            log_info!("Docker container started: {}", container_id);
            Ok(container_id)
        })
    }

    fn stop_container<'a>(&'a self, id_or_name: &'a str) -> RuntimeFuture<'a, ()> {
        Box::pin(async move {
            log_info!("Stopping Docker container: {}", id_or_name);

            let output = Command::new("docker")
                .args(&["stop", id_or_name])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::StopFailed(format!(
                        "Failed to execute docker stop: {}",
                        e
                    ))
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
            log_info!("Removing Docker container: {}", id_or_name);

            let mut args = vec!["rm"];
            if force {
                args.push("-f");
            }
            args.push(id_or_name);

            let output = Command::new("docker").args(&args).output().map_err(|e| {
                ContainerRuntimeError::Other(format!("Failed to execute docker rm: {}", e))
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
            let mut args = vec![
                "ps",
                "--format",
                "{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}",
            ];
            if all {
                args.push("-a");
            }

            let output = Command::new("docker").args(&args).output().map_err(|e| {
                ContainerRuntimeError::Other(format!("Failed to execute docker ps: {}", e))
            })?;

            if !output.status.success() {
                return Ok(Vec::new());
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let containers: Vec<ContainerInfo> = stdout
                .lines()
                .filter_map(|line| self.parse_docker_ps_line(line))
                .collect();

            Ok(containers)
        })
    }

    fn inspect_container<'a>(&'a self, id_or_name: &'a str) -> RuntimeFuture<'a, ContainerInfo> {
        Box::pin(async move {
            let containers = self.list_containers(true).await?;
            containers
                .into_iter()
                .find(|c| c.id == id_or_name || c.name == id_or_name)
                .ok_or_else(|| ContainerRuntimeError::ContainerNotFound(id_or_name.to_string()))
        })
    }

    fn pull_image<'a>(&'a self, image: &'a str) -> RuntimeFuture<'a, ()> {
        Box::pin(async move {
            log_info!("Pulling Docker image: {}", image);

            let output = Command::new("docker")
                .args(&["pull", image])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::Other(format!("Failed to execute docker pull: {}", e))
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
            // Simple implementation - would need proper parsing
            Ok(Vec::new())
        })
    }

    fn get_logs<'a>(&'a self, id_or_name: &'a str, lines: usize) -> RuntimeFuture<'a, Vec<String>> {
        Box::pin(async move {
            let output = Command::new("docker")
                .args(&["logs", "--tail", &lines.to_string(), id_or_name])
                .output()
                .map_err(|e| {
                    ContainerRuntimeError::Other(format!("Failed to execute docker logs: {}", e))
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

    fn get_stats<'a>(&'a self, _id_or_name: &'a str) -> RuntimeFuture<'a, ContainerStats> {
        Box::pin(async move {
            // Placeholder - would need proper stats parsing
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

impl Default for DockerRuntime {
    fn default() -> Self {
        Self::new()
    }
}
