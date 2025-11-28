use crate::{
    NovaError, Result,
    bolt_runtime::BoltRuntime,
    config::ContainerConfig as NovaContainerConfig,
    container_runtime::{
        ContainerConfig, ContainerInfo, ContainerRuntime as Runtime, ContainerStats, RestartPolicy,
    },
    docker_runtime::DockerRuntime,
    instance::Instance,
    log_error, log_info, log_warn,
};
use std::sync::Arc;

/// Container manager with runtime selection (Bolt > Docker > Fallback)
pub struct ContainerManager {
    runtime: Arc<dyn Runtime>,
    runtime_name: String,
}

impl ContainerManager {
    pub fn new() -> Self {
        // Auto-select runtime: Bolt > Docker
        let (runtime, runtime_name): (Arc<dyn Runtime>, String) = {
            let bolt = BoltRuntime::new();
            if bolt.is_available() {
                log_info!("Using Bolt container runtime (ultra-fast GPU passthrough)");
                (Arc::new(bolt), "Bolt".to_string())
            } else {
                let docker = DockerRuntime::new();
                if docker.is_available() {
                    log_info!("Using Docker container runtime (fallback)");
                    (Arc::new(docker), "Docker".to_string())
                } else {
                    log_warn!("No container runtime available, functionality will be limited");
                    // Return a dummy runtime - we could implement a "no-op" runtime here
                    (Arc::new(BoltRuntime::new()), "None".to_string())
                }
            }
        };

        Self {
            runtime,
            runtime_name,
        }
    }

    /// Get the active runtime name
    pub fn get_runtime_name(&self) -> &str {
        &self.runtime_name
    }

    pub async fn start_container(
        &self,
        name: &str,
        config: Option<&NovaContainerConfig>,
    ) -> Result<()> {
        log_info!("Starting container: {}", name);

        let nova_config = config.cloned().unwrap_or_default();

        // Convert Nova config to runtime config
        let runtime_config = ContainerConfig {
            capsule: nova_config
                .capsule
                .unwrap_or_else(|| "ubuntu:latest".to_string()),
            ports: Vec::new(),
            volumes: nova_config.volumes,
            env: nova_config.env,
            network: nova_config.network,
            gpu_passthrough: nova_config.bolt.gpu_access,
            memory_mb: None,
            cpus: None,
            restart_policy: RestartPolicy::No,
            detach: true,
        };

        // Use runtime to start container
        let container_id = self
            .runtime
            .run_container(&runtime_config.capsule, Some(name), &runtime_config)
            .await
            .map_err(|e| {
                log_error!("Failed to start container '{}': {:?}", name, e);
                NovaError::SystemCommandFailed
            })?;

        log_info!("Container '{}' started with ID: {}", name, container_id);
        Ok(())
    }

    pub async fn stop_container(&self, name: &str) -> Result<()> {
        log_info!("Stopping container: {}", name);

        self.runtime.stop_container(name).await.map_err(|e| {
            log_error!("Failed to stop container '{}': {:?}", name, e);
            NovaError::SystemCommandFailed
        })?;

        log_info!("Container '{}' stopped successfully", name);
        Ok(())
    }

    pub async fn remove_container(&self, name: &str, force: bool) -> Result<()> {
        log_info!("Removing container: {}", name);

        self.runtime
            .remove_container(name, force)
            .await
            .map_err(|e| {
                log_error!("Failed to remove container '{}': {:?}", name, e);
                NovaError::SystemCommandFailed
            })?;

        log_info!("Container '{}' removed successfully", name);
        Ok(())
    }

    /// Async version of list_containers (for CLI use)
    pub async fn list_containers_async(&self) -> Vec<Instance> {
        match self.runtime.list_containers(true).await {
            Ok(containers) => {
                // Convert ContainerInfo to Instance
                containers
                    .iter()
                    .map(|c| {
                        let mut instance =
                            Instance::new(c.name.clone(), crate::instance::InstanceType::Container);
                        instance.update_status(match c.status {
                            crate::container_runtime::ContainerStatus::Running => {
                                crate::instance::InstanceStatus::Running
                            }
                            crate::container_runtime::ContainerStatus::Stopped => {
                                crate::instance::InstanceStatus::Stopped
                            }
                            crate::container_runtime::ContainerStatus::Paused => {
                                crate::instance::InstanceStatus::Suspended
                            }
                            crate::container_runtime::ContainerStatus::Starting => {
                                crate::instance::InstanceStatus::Starting
                            }
                            crate::container_runtime::ContainerStatus::Restarting => {
                                crate::instance::InstanceStatus::Starting
                            }
                            _ => crate::instance::InstanceStatus::Error,
                        });
                        if let Some(pid) = c.pid {
                            instance.set_pid(Some(pid));
                        }
                        instance.network = c.network.clone();
                        instance
                    })
                    .collect()
            }
            Err(e) => {
                log_warn!("Failed to list containers: {:?}", e);
                Vec::new()
            }
        }
    }

    /// Sync version of list_containers (for GUI use)
    pub fn list_containers(&self) -> Vec<Instance> {
        // Check if we're already in an async runtime
        if tokio::runtime::Handle::try_current().is_ok() {
            // We're in an async context - can't use block_on
            // Return empty for now - GUI should use spawn_blocking or separate thread
            log_warn!(
                "list_containers() called from async context - use list_containers_async() instead"
            );
            Vec::new()
        } else {
            // Not in async context - create runtime and block
            match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(async { self.list_containers_async().await }),
                Err(_) => {
                    log_warn!("Failed to create tokio runtime for list_containers");
                    Vec::new()
                }
            }
        }
    }

    pub fn get_container(&self, name: &str) -> Option<Instance> {
        // Use tokio runtime to block on async call
        let runtime = tokio::runtime::Handle::try_current()
            .or_else(|_| tokio::runtime::Runtime::new().map(|rt| rt.handle().clone()));

        match runtime {
            Ok(handle) => {
                match handle.block_on(async { self.runtime.inspect_container(name).await }) {
                    Ok(container) => {
                        let mut instance = Instance::new(
                            container.name.clone(),
                            crate::instance::InstanceType::Container,
                        );
                        instance.update_status(match container.status {
                            crate::container_runtime::ContainerStatus::Running => {
                                crate::instance::InstanceStatus::Running
                            }
                            crate::container_runtime::ContainerStatus::Stopped => {
                                crate::instance::InstanceStatus::Stopped
                            }
                            crate::container_runtime::ContainerStatus::Paused => {
                                crate::instance::InstanceStatus::Suspended
                            }
                            crate::container_runtime::ContainerStatus::Starting => {
                                crate::instance::InstanceStatus::Starting
                            }
                            crate::container_runtime::ContainerStatus::Restarting => {
                                crate::instance::InstanceStatus::Starting
                            }
                            _ => crate::instance::InstanceStatus::Error,
                        });
                        if let Some(pid) = container.pid {
                            instance.set_pid(Some(pid));
                        }
                        instance.network = container.network.clone();
                        Some(instance)
                    }
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    }

    pub async fn inspect_container(&self, name: &str) -> Result<ContainerInfo> {
        self.runtime.inspect_container(name).await.map_err(|e| {
            log_error!("Failed to inspect container '{}': {:?}", name, e);
            NovaError::ContainerNotFound(name.to_string())
        })
    }

    pub async fn container_stats(&self, name: &str) -> Result<ContainerStats> {
        self.runtime.get_stats(name).await.map_err(|e| {
            log_error!("Failed to collect stats for container '{}': {:?}", name, e);
            NovaError::SystemCommandFailed
        })
    }

    pub async fn get_container_status(
        &self,
        name: &str,
    ) -> Result<crate::instance::InstanceStatus> {
        let container = self.runtime.inspect_container(name).await.map_err(|e| {
            log_error!("Failed to get container status for '{}': {:?}", name, e);
            NovaError::ContainerNotFound(name.to_string())
        })?;

        Ok(match container.status {
            crate::container_runtime::ContainerStatus::Running => {
                crate::instance::InstanceStatus::Running
            }
            crate::container_runtime::ContainerStatus::Stopped => {
                crate::instance::InstanceStatus::Stopped
            }
            crate::container_runtime::ContainerStatus::Paused => {
                crate::instance::InstanceStatus::Suspended
            }
            crate::container_runtime::ContainerStatus::Starting => {
                crate::instance::InstanceStatus::Starting
            }
            crate::container_runtime::ContainerStatus::Restarting => {
                crate::instance::InstanceStatus::Starting
            }
            _ => crate::instance::InstanceStatus::Error,
        })
    }

    pub async fn get_container_logs(&self, name: &str, lines: usize) -> Result<Vec<String>> {
        self.runtime.get_logs(name, lines).await.map_err(|e| {
            log_error!("Failed to get logs for container '{}': {:?}", name, e);
            NovaError::SystemCommandFailed
        })
    }

    pub async fn pull_image(&self, image: &str) -> Result<()> {
        log_info!("Pulling image: {}", image);

        self.runtime.pull_image(image).await.map_err(|e| {
            log_error!("Failed to pull image '{}': {:?}", image, e);
            NovaError::SystemCommandFailed
        })?;

        log_info!("Image '{}' pulled successfully", image);
        Ok(())
    }

    // Runtime availability checks
    pub fn check_container_runtime(&self) -> &str {
        &self.runtime_name
    }

    pub fn check_bolt_available(&self) -> bool {
        BoltRuntime::new().is_available()
    }

    pub fn check_docker_available(&self) -> bool {
        DockerRuntime::new().is_available()
    }

    pub fn check_podman_available(&self) -> bool {
        // Podman check: would need to implement Podman runtime
        // For now, just check if podman binary exists
        std::process::Command::new("podman")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

impl Default for ContainerManager {
    fn default() -> Self {
        Self::new()
    }
}
