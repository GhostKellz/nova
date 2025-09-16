use crate::{
    config::VmConfig,
    instance::Instance,
    libvirt::{LibvirtManager, LibvirtNetwork},
    network::NetworkManager,
    log_debug, log_error, log_info, log_warn,
    NovaError, Result
};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

pub struct EnhancedVmManager {
    instances: Arc<Mutex<HashMap<String, Instance>>>,
    processes: Arc<Mutex<HashMap<String, Child>>>,
    libvirt_manager: Arc<Mutex<LibvirtManager>>,
    network_manager: Arc<Mutex<NetworkManager>>,
    use_libvirt: bool,
}

impl EnhancedVmManager {
    pub fn new() -> Self {
        let libvirt_available = Self::check_libvirt_available();
        log_info!("Libvirt available: {}", libvirt_available);

        Self {
            instances: Arc::new(Mutex::new(HashMap::new())),
            processes: Arc::new(Mutex::new(HashMap::new())),
            libvirt_manager: Arc::new(Mutex::new(LibvirtManager::new())),
            network_manager: Arc::new(Mutex::new(NetworkManager::new())),
            use_libvirt: libvirt_available,
        }
    }

    fn check_libvirt_available() -> bool {
        Command::new("virsh")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub async fn start_vm(&self, name: &str, config: Option<&VmConfig>) -> Result<()> {
        if self.use_libvirt {
            self.start_vm_libvirt(name, config).await
        } else {
            self.start_vm_qemu(name, config).await
        }
    }

    async fn start_vm_libvirt(&self, name: &str, config: Option<&VmConfig>) -> Result<()> {
        log_info!("Starting VM with libvirt: {}", name);

        let vm_config = config.cloned().unwrap_or_default();

        // Check if VM domain already exists
        let domain_exists = self.check_libvirt_domain_exists(name).await;

        if !domain_exists {
            // Create VM domain XML and define it
            self.create_libvirt_domain(name, &vm_config).await?;
        }

        // Start the domain
        let output = Command::new("virsh")
            .args(&["start", name])
            .output()
            .map_err(|e| {
                log_error!("Failed to start libvirt domain '{}': {}", name, e);
                NovaError::SystemCommandFailed
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to start VM '{}': {}", name, error);
            return Err(NovaError::SystemCommandFailed);
        }

        // Update instance tracking
        let mut instance = Instance::new(name.to_string(), crate::instance::InstanceType::Vm);
        instance.update_status(crate::instance::InstanceStatus::Starting);
        instance.cpu_cores = vm_config.cpu;
        instance.memory_mb = self.parse_memory_mb(&vm_config.memory)?;
        instance.network = vm_config.network.clone();

        {
            let mut instances = self.instances.lock().unwrap();
            instances.insert(name.to_string(), instance);
        }

        // Monitor startup
        tokio::spawn({
            let instances = self.instances.clone();
            let name = name.to_string();
            async move {
                sleep(Duration::from_secs(5)).await;
                let mut instances = instances.lock().unwrap();
                if let Some(instance) = instances.get_mut(&name) {
                    instance.update_status(crate::instance::InstanceStatus::Running);
                    log_info!("VM '{}' is now running via libvirt", name);
                }
            }
        });

        Ok(())
    }

    async fn start_vm_qemu(&self, name: &str, config: Option<&VmConfig>) -> Result<()> {
        log_info!("Starting VM with QEMU: {}", name);

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

        // Enhanced network configuration
        if let Some(network) = &vm_config.network {
            // Check if network exists in our network manager
            let network_manager = self.network_manager.lock().unwrap();
            if let Some(_switch) = network_manager.get_switch(network) {
                cmd.arg("-netdev").arg(format!("bridge,id=net0,br={}", network))
                   .arg("-device").arg("virtio-net-pci,netdev=net0,mac=52:54:00:12:34:56");
                log_info!("Using bridge network '{}' for VM '{}'", network, name);
            } else {
                log_warn!("Network '{}' not found, using default bridge", network);
                cmd.arg("-netdev").arg("bridge,id=net0,br=virbr0")
                   .arg("-device").arg("virtio-net-pci,netdev=net0");
            }
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
        if self.use_libvirt {
            self.stop_vm_libvirt(name).await
        } else {
            self.stop_vm_qemu(name).await
        }
    }

    async fn stop_vm_libvirt(&self, name: &str) -> Result<()> {
        log_info!("Stopping VM with libvirt: {}", name);

        // Update instance status
        {
            let mut instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get_mut(name) {
                instance.update_status(crate::instance::InstanceStatus::Stopping);
            } else {
                return Err(NovaError::VmNotFound(name.to_string()));
            }
        }

        // Shutdown the domain gracefully
        let output = Command::new("virsh")
            .args(&["shutdown", name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            log_warn!("Graceful shutdown failed, forcing destruction of VM '{}'", name);

            // Force destroy if graceful shutdown fails
            let output = Command::new("virsh")
                .args(&["destroy", name])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                log_error!("Failed to destroy VM '{}'", name);
                return Err(NovaError::SystemCommandFailed);
            }
        }

        // Update instance status
        {
            let mut instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get_mut(name) {
                instance.update_status(crate::instance::InstanceStatus::Stopped);
                instance.set_pid(None);
            }
        }

        log_info!("VM '{}' stopped successfully via libvirt", name);
        Ok(())
    }

    async fn stop_vm_qemu(&self, name: &str) -> Result<()> {
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

    async fn check_libvirt_domain_exists(&self, name: &str) -> bool {
        Command::new("virsh")
            .args(&["dominfo", name])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    async fn create_libvirt_domain(&self, name: &str, config: &VmConfig) -> Result<()> {
        log_info!("Creating libvirt domain for VM: {}", name);

        let domain_xml = self.generate_libvirt_domain_xml(name, config)?;

        // Write XML to temporary file
        let temp_file = format!("/tmp/nova-vm-{}.xml", name);
        std::fs::write(&temp_file, domain_xml).map_err(|e| {
            log_error!("Failed to write domain XML: {}", e);
            NovaError::SystemCommandFailed
        })?;

        // Define the domain
        let output = Command::new("virsh")
            .args(&["define", &temp_file])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to define domain: {}", error);
            std::fs::remove_file(&temp_file).ok();
            return Err(NovaError::SystemCommandFailed);
        }

        // Clean up temp file
        std::fs::remove_file(&temp_file).ok();

        log_info!("Libvirt domain '{}' created successfully", name);
        Ok(())
    }

    fn generate_libvirt_domain_xml(&self, name: &str, config: &VmConfig) -> Result<String> {
        let memory_kb = self.parse_memory_mb(&config.memory)? * 1024;

        let mut xml = String::new();
        xml.push_str("<?xml version='1.0' encoding='UTF-8'?>\n");
        xml.push_str("<domain type='kvm'>\n");
        xml.push_str(&format!("  <name>{}</name>\n", name));
        xml.push_str(&format!("  <memory unit='KiB'>{}</memory>\n", memory_kb));
        xml.push_str(&format!("  <currentMemory unit='KiB'>{}</currentMemory>\n", memory_kb));
        xml.push_str(&format!("  <vcpu placement='static'>{}</vcpu>\n", config.cpu));

        xml.push_str("  <os>\n");
        xml.push_str("    <type arch='x86_64' machine='pc-q35-4.2'>hvm</type>\n");
        xml.push_str("    <boot dev='hd'/>\n");
        xml.push_str("  </os>\n");

        xml.push_str("  <features>\n");
        xml.push_str("    <acpi/>\n");
        xml.push_str("    <apic/>\n");
        xml.push_str("  </features>\n");

        xml.push_str("  <cpu mode='host-passthrough' check='none'/>\n");

        xml.push_str("  <clock offset='utc'>\n");
        xml.push_str("    <timer name='rtc' tickpolicy='catchup'/>\n");
        xml.push_str("    <timer name='pit' tickpolicy='delay'/>\n");
        xml.push_str("    <timer name='hpet' present='no'/>\n");
        xml.push_str("  </clock>\n");

        xml.push_str("  <on_poweroff>destroy</on_poweroff>\n");
        xml.push_str("  <on_reboot>restart</on_reboot>\n");
        xml.push_str("  <on_crash>destroy</on_crash>\n");

        xml.push_str("  <devices>\n");
        xml.push_str("    <emulator>/usr/bin/qemu-system-x86_64</emulator>\n");

        // Disk configuration
        if let Some(image_path) = &config.image {
            xml.push_str("    <disk type='file' device='disk'>\n");
            xml.push_str("      <driver name='qemu' type='qcow2'/>\n");
            xml.push_str(&format!("      <source file='{}'/>\n", image_path));
            xml.push_str("      <target dev='vda' bus='virtio'/>\n");
            xml.push_str("    </disk>\n");
        } else {
            // Create temporary disk if no image specified
            let temp_disk = format!("/tmp/nova_{}.qcow2", name);
            xml.push_str("    <disk type='file' device='disk'>\n");
            xml.push_str("      <driver name='qemu' type='qcow2'/>\n");
            xml.push_str(&format!("      <source file='{}'/>\n", temp_disk));
            xml.push_str("      <target dev='vda' bus='virtio'/>\n");
            xml.push_str("    </disk>\n");
        }

        // Network configuration
        if let Some(network) = &config.network {
            xml.push_str("    <interface type='network'>\n");
            xml.push_str(&format!("      <source network='{}'/>\n", network));
            xml.push_str("      <model type='virtio'/>\n");
            xml.push_str("    </interface>\n");
        } else {
            xml.push_str("    <interface type='network'>\n");
            xml.push_str("      <source network='default'/>\n");
            xml.push_str("      <model type='virtio'/>\n");
            xml.push_str("    </interface>\n");
        }

        // Graphics and input
        xml.push_str("    <graphics type='vnc' port='-1' autoport='yes'/>\n");
        xml.push_str("    <input type='tablet' bus='usb'/>\n");
        xml.push_str("    <input type='mouse' bus='ps2'/>\n");
        xml.push_str("    <input type='keyboard' bus='ps2'/>\n");

        // GPU passthrough if enabled
        if config.gpu_passthrough {
            xml.push_str("    <hostdev mode='subsystem' type='pci' managed='yes'>\n");
            xml.push_str("      <source>\n");
            xml.push_str("        <address domain='0x0000' bus='0x01' slot='0x00' function='0x0'/>\n");
            xml.push_str("      </source>\n");
            xml.push_str("    </hostdev>\n");
        }

        xml.push_str("  </devices>\n");
        xml.push_str("</domain>\n");

        Ok(xml)
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

    // Advanced VM operations
    pub async fn pause_vm(&self, name: &str) -> Result<()> {
        if self.use_libvirt {
            let output = Command::new("virsh")
                .args(&["suspend", name])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                return Err(NovaError::SystemCommandFailed);
            }

            log_info!("VM '{}' paused successfully", name);
        } else {
            log_warn!("Pause operation requires libvirt");
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    pub async fn resume_vm(&self, name: &str) -> Result<()> {
        if self.use_libvirt {
            let output = Command::new("virsh")
                .args(&["resume", name])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                return Err(NovaError::SystemCommandFailed);
            }

            log_info!("VM '{}' resumed successfully", name);
        } else {
            log_warn!("Resume operation requires libvirt");
            return Err(NovaError::SystemCommandFailed);
        }

        Ok(())
    }

    pub async fn restart_vm(&self, name: &str) -> Result<()> {
        if self.use_libvirt {
            let output = Command::new("virsh")
                .args(&["reboot", name])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                return Err(NovaError::SystemCommandFailed);
            }

            log_info!("VM '{}' restarted successfully", name);
        } else {
            // For QEMU, we need to stop and start
            self.stop_vm(name).await?;
            tokio::time::sleep(Duration::from_secs(2)).await;
            let config = None; // Would need to store config for restart
            self.start_vm(name, config).await?;
        }

        Ok(())
    }

    pub async fn get_vm_console_url(&self, name: &str) -> Result<String> {
        if self.use_libvirt {
            let output = Command::new("virsh")
                .args(&["domdisplay", name])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if output.status.success() {
                let console_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !console_url.is_empty() {
                    return Ok(console_url);
                }
            }
        }

        // Fallback: assume VNC on localhost
        Ok("vnc://localhost:5900".to_string())
    }

    // Network manager integration
    pub fn get_network_manager(&self) -> Arc<Mutex<NetworkManager>> {
        self.network_manager.clone()
    }

    pub fn get_libvirt_manager(&self) -> Arc<Mutex<LibvirtManager>> {
        self.libvirt_manager.clone()
    }

    pub fn is_using_libvirt(&self) -> bool {
        self.use_libvirt
    }
}

impl Default for EnhancedVmManager {
    fn default() -> Self {
        Self::new()
    }
}