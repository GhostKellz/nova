use crate::{config::ContainerConfig, instance::Instance, log_debug, log_error, log_info, log_warn, NovaError, Result};
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

// TODO: Replace with Bolt runtime integration once available
// See BOLT_INT.md for integration requirements

pub struct ContainerManager {
    instances: Arc<Mutex<HashMap<String, Instance>>>,
    // TODO: Replace with bolt_runtime::BoltRuntime
    // bolt_runtime: Arc<Mutex<BoltRuntime>>,
}

impl ContainerManager {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start_container(&self, name: &str, config: Option<&ContainerConfig>) -> Result<()> {
        log_info!("Starting container: {}", name);

        // Check if container is already running
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(name) {
                if instance.is_running() {
                    log_warn!("Container '{}' is already running", name);
                    return Ok(());
                }
            }
        }

        let container_config = config.cloned().unwrap_or_default();

        // Create container using unshare for namespace isolation
        // This is a simplified implementation - production would use proper container runtime
        let script_content = self.generate_container_script(name, &container_config)?;
        let script_path = format!("/tmp/nova_container_{}.sh", name);

        // Write the container script
        tokio::fs::write(&script_path, script_content).await?;

        // Make script executable
        Command::new("chmod")
            .args(&["+x", &script_path])
            .status()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        // Start the container
        let mut cmd = Command::new("bash");
        cmd.arg(&script_path)
           .stdin(Stdio::null())
           .stdout(Stdio::null())
           .stderr(Stdio::piped());

        log_debug!("Container command: {:?}", cmd);

        let child = cmd.spawn().map_err(|e| {
            log_error!("Failed to start container '{}': {}", name, e);
            NovaError::SystemCommandFailed
        })?;

        let pid = child.id();
        log_info!("Container '{}' started with PID: {}", name, pid);

        // Note: In a full implementation, we would store the process handle
        // For now, we just track the PID in the instance

        // Create or update instance
        let mut instance = Instance::new(name.to_string(), crate::instance::InstanceType::Container);
        instance.set_pid(Some(pid));
        instance.update_status(crate::instance::InstanceStatus::Starting);
        instance.cpu_cores = 1; // Containers typically share CPU
        instance.memory_mb = 512; // Default container memory
        instance.network = container_config.network.clone();

        {
            let mut instances = self.instances.lock().unwrap();
            instances.insert(name.to_string(), instance);
        }

        // Monitor container startup
        tokio::spawn({
            let instances = self.instances.clone();
            let name = name.to_string();
            async move {
                sleep(Duration::from_secs(2)).await;
                let mut instances = instances.lock().unwrap();
                if let Some(instance) = instances.get_mut(&name) {
                    instance.update_status(crate::instance::InstanceStatus::Running);
                    log_info!("Container '{}' is now running", name);
                }
            }
        });

        Ok(())
    }

    pub async fn stop_container(&self, name: &str) -> Result<()> {
        log_info!("Stopping container: {}", name);

        // Update instance status
        {
            let mut instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get_mut(name) {
                instance.update_status(crate::instance::InstanceStatus::Stopping);
            } else {
                return Err(NovaError::ContainerNotFound(name.to_string()));
            }
        }

        // Note: In a full implementation, we would kill the stored process handle
        // For now, we rely on pkill to terminate the container

        // Alternative: use pkill to find and kill container process
        let output = Command::new("pkill")
            .arg("-f")
            .arg(&format!("nova-container-{}", name))
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if output.status.success() {
            log_info!("Container '{}' stopped successfully", name);
        } else {
            log_warn!("Container '{}' may not have been running", name);
        }

        // Clean up script file
        let script_path = format!("/tmp/nova_container_{}.sh", name);
        let _ = tokio::fs::remove_file(script_path).await;

        // Update instance status
        {
            let mut instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get_mut(name) {
                instance.update_status(crate::instance::InstanceStatus::Stopped);
                instance.set_pid(None);
            }
        }

        Ok(())
    }

    pub fn list_containers(&self) -> Vec<Instance> {
        let instances = self.instances.lock().unwrap();
        instances.values().cloned().collect()
    }

    pub fn get_container(&self, name: &str) -> Option<Instance> {
        let instances = self.instances.lock().unwrap();
        instances.get(name).cloned()
    }

    pub async fn get_container_status(&self, name: &str) -> Result<crate::instance::InstanceStatus> {
        let instances = self.instances.lock().unwrap();
        if let Some(instance) = instances.get(name) {
            Ok(instance.status)
        } else {
            Err(NovaError::ContainerNotFound(name.to_string()))
        }
    }

    fn generate_container_script(&self, name: &str, config: &ContainerConfig) -> Result<String> {
        let mut script = String::new();

        script.push_str("#!/bin/bash
");
        script.push_str("set -e

");

        script.push_str(&format!("# Nova container script for '{}'
", name));
        script.push_str(&format!("echo \"Starting Nova container: {}\"

", name));

        // Set process name for easy identification
        script.push_str(&format!("exec -a nova-container-{} ", name));

        // Use unshare for basic namespace isolation
        script.push_str("unshare ");
        script.push_str("--pid --fork --mount-proc ");
        script.push_str("--net --uts --ipc ");

        // If we have a capsule (base image), try to use it
        if let Some(capsule) = &config.capsule {
            log_debug!("Using capsule '{}' for container '{}'", capsule, name);

            // For now, just run a simple command
            // In production, this would integrate with a proper container runtime
            if capsule.contains("ubuntu") {
                script.push_str("bash -c \"");
                script.push_str("echo 'Container running with Ubuntu base'; ");

                // Set environment variables
                for (key, value) in &config.env {
                    script.push_str(&format!("export {}='{}'; ", key, value));
                }

                script.push_str("sleep infinity\"");
            } else {
                // Generic capsule
                script.push_str("bash -c \"");
                script.push_str(&format!("echo 'Container running with {} base'; ", capsule));

                // Set environment variables
                for (key, value) in &config.env {
                    script.push_str(&format!("export {}='{}'; ", key, value));
                }

                script.push_str("sleep infinity\"");
            }
        } else {
            // Default container without specific capsule
            script.push_str("bash -c \"");
            script.push_str("echo 'Nova container started'; ");

            // Set environment variables
            for (key, value) in &config.env {
                script.push_str(&format!("export {}='{}'; ", key, value));
            }

            script.push_str("sleep infinity\"");
        }

        Ok(script)
    }

    // Check available container runtimes (priority order: Bolt > Docker > Podman)
    pub fn check_container_runtime(&self) -> ContainerRuntime {
        if self.check_bolt_available() {
            ContainerRuntime::Bolt
        } else if self.check_docker_available() {
            ContainerRuntime::Docker
        } else if self.check_podman_available() {
            ContainerRuntime::Podman
        } else {
            ContainerRuntime::None
        }
    }

    pub fn check_bolt_available(&self) -> bool {
        // Check if Bolt is installed and available
        Command::new("bolt")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn check_docker_available(&self) -> bool {
        Command::new("docker")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn check_podman_available(&self) -> bool {
        Command::new("podman")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Bolt,    // Primary: High-performance Rust container runtime
    Docker,  // Fallback: Industry standard
    Podman,  // Fallback: Daemonless alternative
    None,
}

impl Default for ContainerManager {
    fn default() -> Self {
        Self::new()
    }
}