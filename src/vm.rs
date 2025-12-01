use crate::{
    NovaError, Result,
    config::{DiskFormat, VmBootType, VmConfig, VmFirmwareConfig, VmTpmConfig, VmTpmVersion},
    gpu_passthrough::{DisplayMode, GpuManager, GpuPassthroughConfig},
    instance::Instance,
    log_debug, log_error, log_info, log_warn,
    looking_glass::{LookingGlassConfig, LookingGlassManager},
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep};

const DEFAULT_OVMF_CODE: &str = "/usr/share/OVMF/OVMF_CODE.fd";
const DEFAULT_OVMF_VARS: &str = "/usr/share/OVMF/OVMF_VARS.fd";
const DEFAULT_OVMF_CODE_SECURE: &str = "/usr/share/OVMF/OVMF_CODE.secboot.fd";
const DEFAULT_OVMF_VARS_SECURE: &str = "/usr/share/OVMF/OVMF_VARS.ms.fd";
const FIRMWARE_WORK_DIR: &str = "/var/lib/nova/firmware";
const TPM_WORK_DIR: &str = "/var/lib/nova/tpm";

#[derive(Clone)]
struct TpmArtifacts {
    socket_path: PathBuf,
    control_path: PathBuf,
    state_dir: PathBuf,
    root_dir: PathBuf,
}

struct ManagedTpm {
    child: Child,
    artifacts: TpmArtifacts,
}

pub struct VmManager {
    instances: Arc<Mutex<HashMap<String, Instance>>>,
    processes: Arc<Mutex<HashMap<String, Child>>>,
    gpu_manager: Arc<Mutex<GpuManager>>,
    gpu_allocations: Arc<Mutex<HashMap<String, GpuPassthroughConfig>>>,
    looking_glass_configs: Arc<Mutex<HashMap<String, LookingGlassConfig>>>,
    tpm_instances: Arc<Mutex<HashMap<String, ManagedTpm>>>,
}

impl VmManager {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(Mutex::new(HashMap::new())),
            processes: Arc::new(Mutex::new(HashMap::new())),
            gpu_manager: Arc::new(Mutex::new(GpuManager::new())),
            gpu_allocations: Arc::new(Mutex::new(HashMap::new())),
            looking_glass_configs: Arc::new(Mutex::new(HashMap::new())),
            tpm_instances: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn gpu_manager_handle(&self) -> Arc<Mutex<GpuManager>> {
        Arc::clone(&self.gpu_manager)
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
        cmd.arg("-name")
            .arg(name)
            .arg("-m")
            .arg(format!("{}M", self.parse_memory_mb(&vm_config.memory)?))
            .arg("-cpu")
            .arg("host")
            .arg("-enable-kvm")
            .arg("-smp")
            .arg(vm_config.cpu.to_string())
            .arg("-daemonize")
            .arg("-monitor")
            .arg("none")
            .arg("-display")
            .arg("none");

        let (disk_path, disk_format) = prepare_vm_disk(name, &vm_config).await?;
        cmd.arg("-drive").arg(format!(
            "file={},format={},if=virtio",
            disk_path.to_string_lossy(),
            disk_format.as_str()
        ));

        // GPU passthrough and Looking Glass support
        self.apply_gpu_passthrough(name, &vm_config, &mut cmd)
            .await?;

        // Firmware / Secure Boot configuration
        self.configure_firmware(name, &vm_config.firmware, &mut cmd)?;

        // TPM device (for Windows 11 compliance, etc.)
        self.configure_tpm(name, &vm_config.tpm, &mut cmd)?;

        // Network configuration
        if let Some(network) = &vm_config.network {
            cmd.arg("-netdev")
                .arg(format!("bridge,id=net0,br={}", network))
                .arg("-device")
                .arg("virtio-net-pci,netdev=net0");
        } else {
            cmd.arg("-netdev")
                .arg("user,id=net0")
                .arg("-device")
                .arg("virtio-net-pci,netdev=net0");
        }

        log_debug!("QEMU command: {:?}", cmd);

        // Start the VM process
        let child = match cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                log_error!("Failed to start QEMU for VM '{}': {}", name, e);
                self.cleanup_post_stop(name).await;
                return Err(NovaError::SystemCommandFailed);
            }
        };

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

        self.cleanup_post_stop(name).await;

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

    async fn apply_gpu_passthrough(
        &self,
        name: &str,
        vm_config: &VmConfig,
        cmd: &mut Command,
    ) -> Result<()> {
        let needs_gpu = vm_config.gpu_passthrough || vm_config.gpu.is_some();

        let gpu_config = if needs_gpu {
            let mut manager = self.gpu_manager.lock().unwrap();
            manager.ensure_discovered()?;

            let mut config = if let Some(cfg) = &vm_config.gpu {
                cfg.clone()
            } else {
                self.default_gpu_config(vm_config, &manager)?
            };

            if vm_config.looking_glass.enabled
                && !matches!(config.display, DisplayMode::LookingGlass)
            {
                config.display = DisplayMode::LookingGlass;
            }

            if config.device_address.is_empty() {
                return Err(NovaError::ConfigError(
                    "GPU passthrough enabled but no device_address configured".to_string(),
                ));
            }

            log_info!(
                "Enabling GPU passthrough for VM '{}' using {}",
                name,
                config.device_address
            );

            manager.configure_passthrough(&config.device_address, name)?;

            Some(config)
        } else {
            None
        };

        if let Some(config) = gpu_config.clone() {
            cmd.arg("-vga").arg("none");
            for arg in config.qemu_args() {
                cmd.arg(arg);
            }

            if matches!(config.display, DisplayMode::LookingGlass) {
                if let Err(err) = self
                    .prepare_looking_glass(name, &vm_config.looking_glass, cmd)
                    .await
                {
                    let mut manager = self.gpu_manager.lock().unwrap();
                    if let Err(release_err) = manager.release_gpu(&config.device_address) {
                        log_warn!(
                            "Failed to release GPU {} after Looking Glass error: {}",
                            config.device_address,
                            release_err
                        );
                    }
                    return Err(err);
                }
            }

            let mut allocations = self.gpu_allocations.lock().unwrap();
            allocations.insert(name.to_string(), config.clone());
        } else if vm_config.looking_glass.enabled {
            self.prepare_looking_glass(name, &vm_config.looking_glass, cmd)
                .await?;
        }

        Ok(())
    }

    fn default_gpu_config(
        &self,
        vm_config: &VmConfig,
        manager: &GpuManager,
    ) -> Result<GpuPassthroughConfig> {
        let gpu = manager
            .list_gpus()
            .iter()
            .find(|gpu| gpu.iommu_group.is_some())
            .or_else(|| manager.list_gpus().first())
            .ok_or_else(|| NovaError::ConfigError("No GPUs available for passthrough".into()))?;

        let mut config = GpuPassthroughConfig::default();
        config.device_address = gpu.address.clone();
        config.display = if vm_config.looking_glass.enabled {
            DisplayMode::LookingGlass
        } else {
            DisplayMode::None
        };

        Ok(config)
    }

    fn configure_firmware(
        &self,
        name: &str,
        firmware: &VmFirmwareConfig,
        cmd: &mut Command,
    ) -> Result<()> {
        if !matches!(firmware.boot_type, VmBootType::Uefi) {
            return Ok(());
        }

        let code_path = firmware.ovmf_code.clone().unwrap_or_else(|| {
            if firmware.secure_boot {
                DEFAULT_OVMF_CODE_SECURE.to_string()
            } else {
                DEFAULT_OVMF_CODE.to_string()
            }
        });
        let vars_source = firmware.ovmf_vars.clone().unwrap_or_else(|| {
            if firmware.secure_boot {
                DEFAULT_OVMF_VARS_SECURE.to_string()
            } else {
                DEFAULT_OVMF_VARS.to_string()
            }
        });

        if !Path::new(&code_path).exists() {
            return Err(NovaError::ConfigError(format!(
                "OVMF firmware image not found at {}",
                code_path
            )));
        }
        if !Path::new(&vars_source).exists() {
            return Err(NovaError::ConfigError(format!(
                "OVMF vars image not found at {}",
                vars_source
            )));
        }

        fs::create_dir_all(FIRMWARE_WORK_DIR).map_err(|err| {
            log_error!(
                "Failed to prepare firmware directory {}: {}",
                FIRMWARE_WORK_DIR,
                err
            );
            NovaError::ConfigError("Unable to prepare firmware workspace".into())
        })?;
        let vars_dest = Path::new(FIRMWARE_WORK_DIR).join(format!("{}-vars.fd", name));
        if !vars_dest.exists() {
            fs::copy(&vars_source, &vars_dest).map_err(|err| {
                log_error!(
                    "Failed to seed OVMF vars for VM '{}' ({} -> {}): {}",
                    name,
                    vars_source,
                    vars_dest.display(),
                    err
                );
                NovaError::ConfigError("Unable to prepare OVMF vars image".into())
            })?;
        }

        cmd.arg("-machine").arg("q35,smm=on");
        cmd.arg("-drive").arg(format!(
            "if=pflash,format=raw,readonly=on,file={}",
            code_path
        ));
        cmd.arg("-drive")
            .arg(format!("if=pflash,format=raw,file={}", vars_dest.display()));

        Ok(())
    }

    fn configure_tpm(&self, name: &str, tpm: &VmTpmConfig, cmd: &mut Command) -> Result<()> {
        if !tpm.enabled {
            let managed = {
                let mut guard = self.tpm_instances.lock().unwrap();
                guard.remove(name)
            };
            if let Some(mut existing) = managed {
                let _ = existing.child.kill();
                let _ = existing.child.wait();
                Self::cleanup_tpm_artifacts(existing.artifacts);
            }
            return Ok(());
        }

        let managed = self.spawn_tpm(name, tpm)?;
        let artifacts = managed.artifacts.clone();
        {
            let mut guard = self.tpm_instances.lock().unwrap();
            guard.insert(name.to_string(), managed);
        }

        let chardev_id = format!("chrtpm-{}", name);
        cmd.arg("-chardev").arg(format!(
            "socket,id={},path={}",
            chardev_id,
            artifacts.socket_path.display()
        ));
        cmd.arg("-tpmdev")
            .arg(format!("emulator,id=tpm-{},chardev={}", name, chardev_id));
        cmd.arg("-device")
            .arg(format!("tpm-tis,tpmdev=tpm-{}", name));

        Ok(())
    }

    fn spawn_tpm(&self, name: &str, config: &VmTpmConfig) -> Result<ManagedTpm> {
        fs::create_dir_all(TPM_WORK_DIR).map_err(|err| {
            log_error!("Failed to prepare TPM directory {}: {}", TPM_WORK_DIR, err);
            NovaError::ConfigError("Unable to prepare TPM workspace".into())
        })?;

        let root_dir = Path::new(TPM_WORK_DIR).join(name);
        let state_dir = root_dir.join("state");
        fs::create_dir_all(&state_dir).map_err(|err| {
            log_error!("Failed to prepare TPM state dir {:?}: {}", state_dir, err);
            NovaError::ConfigError("Unable to prepare TPM state".into())
        })?;

        let socket_path = root_dir.join("swtpm.sock");
        let control_path = root_dir.join("swtpm.ctrl");
        for path in [&socket_path, &control_path] {
            if path.exists() {
                let _ = fs::remove_file(path);
            }
        }

        let mut command = Command::new("swtpm");
        command.arg("socket");
        match config.version {
            VmTpmVersion::V1_2 => {
                command.arg("--tpm1");
            }
            VmTpmVersion::V2_0 => {
                command.arg("--tpm2");
            }
        }
        command
            .arg("--ctrl")
            .arg(format!("type=unixio,path={}", control_path.display()))
            .arg("--server")
            .arg(format!("type=unixio,path={}", socket_path.display()))
            .arg("--tpmstate")
            .arg(format!("dir={},mode=0600", state_dir.display()))
            .arg("--flags")
            .arg("not-need-init");
        command.stdin(Stdio::null()).stdout(Stdio::null());

        let child = command.spawn().map_err(|err| {
            log_error!("Failed to launch swtpm for VM '{}': {}", name, err);
            NovaError::SystemCommandFailed
        })?;

        // Give swtpm a brief moment to create the socket
        std::thread::sleep(std::time::Duration::from_millis(150));

        Ok(ManagedTpm {
            child,
            artifacts: TpmArtifacts {
                socket_path,
                control_path,
                state_dir,
                root_dir,
            },
        })
    }

    async fn prepare_looking_glass(
        &self,
        name: &str,
        requested_config: &LookingGlassConfig,
        cmd: &mut Command,
    ) -> Result<()> {
        if !requested_config.enabled {
            return Ok(());
        }

        if let Err(err) = requested_config.validate() {
            return Err(NovaError::ConfigError(format!(
                "Looking Glass configuration invalid: {}",
                err
            )));
        }

        let mut config = requested_config.clone();
        if config.framebuffer_size == 0 {
            config.framebuffer_size = config.calculate_framebuffer_size();
        }

        let manager = LookingGlassManager::new();
        for arg in manager.generate_qemu_args(&config) {
            cmd.arg(arg);
        }

        if let Err(err) = manager.setup_shmem(&config, name).await {
            return Err(NovaError::ConfigError(format!(
                "Failed to setup Looking Glass shared memory: {}",
                err
            )));
        }

        log_info!(
            "Looking Glass enabled for VM '{}' (shmem: {})",
            name,
            config.shmem_path.display()
        );

        {
            let mut configs = self.looking_glass_configs.lock().unwrap();
            configs.insert(name.to_string(), config);
        }

        Ok(())
    }

    async fn cleanup_post_stop(&self, name: &str) {
        let gpu_config = {
            let mut allocations = self.gpu_allocations.lock().unwrap();
            allocations.remove(name)
        };

        if let Some(config) = gpu_config {
            let mut manager = self.gpu_manager.lock().unwrap();
            if let Err(err) = manager.release_gpu(&config.device_address) {
                log_warn!(
                    "Failed to release GPU {} for VM '{}': {}",
                    config.device_address,
                    name,
                    err
                );
            }
        }

        let looking_glass_config = {
            let mut configs = self.looking_glass_configs.lock().unwrap();
            configs.remove(name)
        };

        if let Some(config) = looking_glass_config {
            let manager = LookingGlassManager::new();
            if let Err(err) = manager.cleanup_shmem(&config).await {
                log_warn!(
                    "Failed to cleanup Looking Glass shared memory for VM '{}': {}",
                    name,
                    err
                );
            }
        }

        let managed_tpm = {
            let mut instances = self.tpm_instances.lock().unwrap();
            instances.remove(name)
        };

        if let Some(mut tpm) = managed_tpm {
            if let Err(err) = tpm.child.kill() {
                log_warn!("Failed to terminate swtpm for VM '{}': {}", name, err);
            }
            let _ = tpm.child.wait();
            Self::cleanup_tpm_artifacts(tpm.artifacts);
        }
    }

    fn cleanup_tpm_artifacts(artifacts: TpmArtifacts) {
        let _ = fs::remove_file(&artifacts.socket_path);
        let _ = fs::remove_file(&artifacts.control_path);
        if let Err(err) = fs::remove_dir_all(&artifacts.root_dir) {
            log_debug!(
                "Unable to remove TPM workspace {:?}: {} (will be reused)",
                artifacts.root_dir,
                err
            );
        }
    }

    fn parse_memory_mb(&self, memory_str: &str) -> Result<u64> {
        let bytes = crate::config::parse_memory_to_bytes(memory_str)?;
        Ok(bytes / (1024 * 1024)) // Convert to MB
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

pub(crate) async fn prepare_vm_disk(
    vm_name: &str,
    config: &VmConfig,
) -> Result<(PathBuf, DiskFormat)> {
    if let Some(image_path) = &config.image {
        return Ok((PathBuf::from(image_path), config.storage.format));
    }

    let storage_cfg = config.storage.clone();
    let disk_path = storage_cfg.resolve_disk_path(vm_name);

    if tokio::fs::metadata(&disk_path).await.is_ok() {
        return Ok((disk_path, storage_cfg.format));
    }

    if !storage_cfg.create_if_missing {
        return Err(NovaError::ConfigError(format!(
            "Disk image '{}' not found and auto-creation disabled",
            disk_path.display()
        )));
    }

    if let Some(parent) = disk_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    log_info!(
        "Provisioning disk for VM '{}' at {} (format={}, size={})",
        vm_name,
        disk_path.display(),
        storage_cfg.format.as_str(),
        storage_cfg.size
    );

    let output = Command::new("qemu-img")
        .args(&[
            "create",
            "-f",
            storage_cfg.format.as_str(),
            &disk_path.to_string_lossy(),
            &storage_cfg.size,
        ])
        .output()
        .map_err(|_| NovaError::SystemCommandFailed)?;

    if !output.status.success() {
        log_error!(
            "Failed to create disk {}: {}",
            disk_path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(NovaError::SystemCommandFailed);
    }

    log_debug!("Disk created at {}", disk_path.display());
    Ok((disk_path, storage_cfg.format))
}
