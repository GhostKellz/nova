use crate::{config::VmConfig, instance::Instance, log_debug, log_error, log_info, log_warn, NovaError, Result};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

pub struct VmManager {
    instances: Arc<Mutex<HashMap<String, Instance>>>,
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl VmManager {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(Mutex::new(HashMap::new())),
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start_vm(&self, name: &str, config: Option<&VmConfig>) -> Result<()> {
        log_info!("Starting VM: {}", name);

        // Check if VM is already running
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(name) {
                if instance.is_running() {
                    log_warn!("VM '{}' is already running", name);
                    return Ok(());
                }
            }
        }

        let vm_config = config.cloned().unwrap_or_default();

        // Create QEMU command
        let mut cmd = Command::new("qemu-system-x86_64");

        // Basic configuration
        cmd.arg("-name").arg(name)
           .arg("-m").arg(format!("{}M", self.parse_memory_mb(&vm_config.memory)?))
           .arg("-cpu").arg("host")
           .arg("-enable-kvm")
           .arg("-smp").arg(vm_config.cpu.to_string())
           .arg("-daemonize")
           .arg("-monitor").arg("none")
           .arg("-display").arg("none");

        // Add disk image if specified
        if let Some(image_path) = &vm_config.image {
            cmd.arg("-drive").arg(format!("file={},format=qcow2,if=virtio", image_path));
        } else {
            // Create a temporary disk for testing
            self.create_test_disk(name).await?;
            let temp_disk = format!("/tmp/nova_{}.qcow2", name);
            cmd.arg("-drive").arg(format!("file={},format=qcow2,if=virtio", temp_disk));
        }

        // GPU passthrough
        if vm_config.gpu_passthrough {
            log_info!("Enabling GPU passthrough for VM '{}'", name);
            cmd.arg("-vga").arg("none")
               .arg("-device").arg("vfio-pci,host=01:00.0"); // Example GPU device
        }

        // Network configuration
        if let Some(network) = &vm_config.network {
            cmd.arg("-netdev").arg(format!("bridge,id=net0,br={}", network))
               .arg("-device").arg("virtio-net-pci,netdev=net0");
        } else {
            cmd.arg("-netdev").arg("user,id=net0")
               .arg("-device").arg("virtio-net-pci,netdev=net0");
        }

        log_debug!("QEMU command: {:?}", cmd);

        // Start the VM process
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                log_error!("Failed to start QEMU for VM '{}': {}", name, e);
                NovaError::SystemCommandFailed
            })?;

        let pid = child.id();
        log_info!("VM '{}' started with PID: {}", name, pid);

        // Store the process
        {
            let mut processes = self.processes.lock().unwrap();
            processes.insert(name.to_string(), child);
        }

        // Create or update instance
        let mut instance = Instance::new(name.to_string(), crate::instance::InstanceType::Vm);
        instance.set_pid(Some(pid));
        instance.update_status(crate::instance::InstanceStatus::Starting);
        instance.cpu_cores = vm_config.cpu;
        instance.memory_mb = self.parse_memory_mb(&vm_config.memory)?;
        instance.network = vm_config.network.clone();

        {
            let mut instances = self.instances.lock().unwrap();
            instances.insert(name.to_string(), instance);
        }

        // Monitor VM startup
        tokio::spawn({
            let instances = self.instances.clone();
            let name = name.to_string();
            async move {
                sleep(Duration::from_secs(3)).await;
                let mut instances = instances.lock().unwrap();
                if let Some(instance) = instances.get_mut(&name) {
                    instance.update_status(crate::instance::InstanceStatus::Running);
                    log_info!("VM '{}' is now running", name);
                }
            }
        });

        Ok(())
    }

    pub async fn stop_vm(&self, name: &str) -> Result<()> {
        log_info!("Stopping VM: {}", name);

        // Update instance status
        {
            let mut instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get_mut(name) {
                instance.update_status(crate::instance::InstanceStatus::Stopping);
            } else {
                return Err(NovaError::VmNotFound(name.to_string()));
            }
        }

        // Kill the QEMU process
        {
            let mut processes = self.processes.lock().unwrap();
            if let Some(mut child) = processes.remove(name) {
                if let Err(e) = child.kill() {
                    log_error!("Failed to kill VM process '{}': {}", name, e);
                }
                let _ = child.wait();
            }
        }

        // Alternative: use pkill to find and kill QEMU process
        let output = Command::new("pkill")
            .arg("-f")
            .arg(&format!("qemu.*{}", name))
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if output.status.success() {
            log_info!("VM '{}' stopped successfully", name);
        } else {
            log_warn!("VM '{}' may not have been running", name);
        }

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

    pub fn list_vms(&self) -> Vec<Instance> {
        let instances = self.instances.lock().unwrap();
        instances.values().cloned().collect()
    }

    pub fn get_vm(&self, name: &str) -> Option<Instance> {
        let instances = self.instances.lock().unwrap();
        instances.get(name).cloned()
    }

    pub async fn get_vm_status(&self, name: &str) -> Result<crate::instance::InstanceStatus> {
        let instances = self.instances.lock().unwrap();
        if let Some(instance) = instances.get(name) {
            Ok(instance.status)
        } else {
            Err(NovaError::VmNotFound(name.to_string()))
        }
    }

    fn parse_memory_mb(&self, memory_str: &str) -> Result<u64> {
        let bytes = crate::config::parse_memory_to_bytes(memory_str)?;
        Ok(bytes / (1024 * 1024)) // Convert to MB
    }

    async fn create_test_disk(&self, vm_name: &str) -> Result<()> {
        let disk_path = format!("/tmp/nova_{}.qcow2", vm_name);

        // Check if disk already exists
        if tokio::fs::metadata(&disk_path).await.is_ok() {
            return Ok(());
        }

        log_info!("Creating test disk for VM '{}' at {}", vm_name, disk_path);

        let output = Command::new("qemu-img")
            .args(&["create", "-f", "qcow2", &disk_path, "1G"])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_error!("Failed to create test disk: {}", String::from_utf8_lossy(&output.stderr));
            return Err(NovaError::SystemCommandFailed);
        }

        log_debug!("Test disk created successfully at {}", disk_path);
        Ok(())
    }

    // Check if libvirt is available and try to use it
    pub fn check_libvirt(&self) -> bool {
        Command::new("virsh")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

impl Default for VmManager {
    fn default() -> Self {
        Self::new()
    }
}