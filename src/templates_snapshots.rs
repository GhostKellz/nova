use crate::{log_debug, log_error, log_info, log_warn, NovaError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub os_type: OperatingSystem,
    pub version: String,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_size_gb: u64,
    pub network_config: NetworkTemplate,
    pub disk_path: PathBuf,
    pub config_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub tags: Vec<String>,
    pub size_on_disk: u64,
    pub guest_tools_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperatingSystem {
    Windows { version: WindowsVersion },
    Linux { distro: LinuxDistro },
    Other { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WindowsVersion {
    Windows11,
    Windows10,
    WindowsServer2022,
    WindowsServer2019,
    WindowsServer2016,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LinuxDistro {
    Ubuntu { version: String },
    Arch,
    Fedora { version: String },
    Debian { version: String },
    CentOS { version: String },
    OpenSUSE { version: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTemplate {
    pub interface_type: String, // virtio, e1000, etc.
    pub network_name: Option<String>,
    pub mac_address: Option<String>,
    pub boot_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshot {
    pub id: String,
    pub vm_name: String,
    pub name: String,
    pub description: String,
    pub snapshot_type: SnapshotType,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub vm_state: VmState,
    pub parent_snapshot: Option<String>,
    pub children: Vec<String>,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotType {
    Internal,    // QCOW2 internal snapshot
    External,    // External snapshot with overlay
    Memory,      // Memory + disk state
    DiskOnly,    // Disk state only
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmState {
    Running,
    Paused,
    Shutdown,
    Crashed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotTree {
    pub vm_name: String,
    pub root_snapshots: Vec<String>,
    pub snapshot_map: HashMap<String, VmSnapshot>,
}

pub struct TemplateManager {
    templates_dir: PathBuf,
    templates: HashMap<String, VmTemplate>,
    snapshots: HashMap<String, HashMap<String, VmSnapshot>>, // vm_name -> snapshot_id -> snapshot
}

impl TemplateManager {
    pub fn new(templates_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&templates_dir)?;

        let mut manager = Self {
            templates_dir,
            templates: HashMap::new(),
            snapshots: HashMap::new(),
        };

        manager.load_templates()?;

        Ok(manager)
    }

    /// Create a new VM template from an existing VM
    pub async fn create_template_from_vm(
        &mut self,
        vm_name: &str,
        template_name: &str,
        description: &str,
        tags: Vec<String>
    ) -> Result<String> {
        log_info!("Creating template '{}' from VM '{}'", template_name, vm_name);

        // Ensure VM is shut down
        self.ensure_vm_shutdown(vm_name).await?;

        // Get VM configuration
        let vm_info = self.get_vm_info(vm_name).await?;

        let template_id = uuid::Uuid::new_v4().to_string();
        let template_dir = self.templates_dir.join(&template_id);
        std::fs::create_dir_all(&template_dir)?;

        // Copy and compress VM disk
        let source_disk = self.get_vm_disk_path(vm_name).await?;
        let template_disk = template_dir.join("disk.qcow2");

        log_info!("Compressing VM disk for template...");
        self.compress_vm_disk(&source_disk, &template_disk).await?;

        // Save VM configuration as template
        let config_path = template_dir.join("config.xml");
        self.save_vm_config_as_template(vm_name, &config_path).await?;

        // Detect OS type from VM
        let os_type = self.detect_vm_os_type(vm_name).await;

        let template = VmTemplate {
            id: template_id.clone(),
            name: template_name.to_string(),
            description: description.to_string(),
            os_type,
            version: "1.0".to_string(),
            cpu_cores: vm_info.cpu_cores,
            memory_mb: vm_info.memory_mb,
            disk_size_gb: vm_info.disk_size_gb,
            network_config: NetworkTemplate::default(),
            disk_path: template_disk,
            config_path,
            created_at: Utc::now(),
            created_by: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            tags,
            size_on_disk: self.get_directory_size(&template_dir)?,
            guest_tools_installed: self.check_guest_tools_installed(vm_name).await,
        };

        // Save template metadata
        let metadata_path = template_dir.join("template.json");
        let template_json = serde_json::to_string_pretty(&template)?;
        std::fs::write(metadata_path, template_json)?;

        self.templates.insert(template_id.clone(), template);

        log_info!("Template '{}' created successfully with ID: {}", template_name, template_id);
        Ok(template_id)
    }

    /// Create a snapshot of a VM
    pub async fn create_snapshot(
        &mut self,
        vm_name: &str,
        snapshot_name: &str,
        description: &str,
        include_memory: bool
    ) -> Result<String> {
        log_info!("Creating snapshot '{}' for VM '{}'", snapshot_name, vm_name);

        let snapshot_id = uuid::Uuid::new_v4().to_string();

        // Determine snapshot type based on VM state and user preference
        let vm_state = self.get_vm_state(vm_name).await?;
        let snapshot_type = if include_memory && matches!(vm_state, VmState::Running) {
            SnapshotType::Memory
        } else {
            SnapshotType::DiskOnly
        };

        // Create snapshot using virsh
        let mut cmd = Command::new("virsh");
        cmd.args(&["snapshot-create-as", vm_name]);
        cmd.args(&["--name", snapshot_name]);
        cmd.args(&["--description", description]);

        match snapshot_type {
            SnapshotType::Memory => {
                cmd.arg("--memspec").arg(format!("/var/lib/nova/snapshots/{}/{}.mem", vm_name, snapshot_id));
                cmd.arg("--diskspec").arg(format!("vda,file=/var/lib/nova/snapshots/{}/{}.qcow2", vm_name, snapshot_id));
            }
            SnapshotType::DiskOnly => {
                cmd.arg("--disk-only");
                cmd.arg("--diskspec").arg(format!("vda,file=/var/lib/nova/snapshots/{}/{}.qcow2", vm_name, snapshot_id));
            }
            _ => {}
        }

        // Create snapshot directory
        let snapshot_dir = PathBuf::from("/var/lib/nova/snapshots").join(vm_name);
        std::fs::create_dir_all(&snapshot_dir)?;

        let output = cmd.output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to create snapshot: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        // Calculate snapshot size
        let size_bytes = self.calculate_snapshot_size(vm_name, &snapshot_id).await?;

        // Get parent snapshot
        let parent_snapshot = self.get_current_snapshot(vm_name).await;

        let snapshot = VmSnapshot {
            id: snapshot_id.clone(),
            vm_name: vm_name.to_string(),
            name: snapshot_name.to_string(),
            description: description.to_string(),
            snapshot_type,
            created_at: Utc::now(),
            size_bytes,
            vm_state,
            parent_snapshot,
            children: Vec::new(),
            is_current: true,
        };

        // Store snapshot
        self.snapshots.entry(vm_name.to_string())
            .or_insert_with(HashMap::new)
            .insert(snapshot_id.clone(), snapshot);

        log_info!("Snapshot '{}' created with ID: {}", snapshot_name, snapshot_id);
        Ok(snapshot_id)
    }

    // Template management
    pub fn list_templates(&self) -> Vec<&VmTemplate> {
        self.templates.values().collect()
    }

    pub fn get_template(&self, template_id: &str) -> Option<&VmTemplate> {
        self.templates.get(template_id)
    }

    pub fn search_templates(&self, query: &str) -> Vec<&VmTemplate> {
        self.templates.values()
            .filter(|t| {
                t.name.to_lowercase().contains(&query.to_lowercase()) ||
                t.description.to_lowercase().contains(&query.to_lowercase()) ||
                t.tags.iter().any(|tag| tag.to_lowercase().contains(&query.to_lowercase()))
            })
            .collect()
    }

    // Snapshot management
    pub fn list_snapshots(&self, vm_name: &str) -> Vec<&VmSnapshot> {
        self.snapshots.get(vm_name)
            .map(|snapshots| snapshots.values().collect())
            .unwrap_or_default()
    }

    pub fn get_snapshot(&self, vm_name: &str, snapshot_id: &str) -> Option<&VmSnapshot> {
        self.snapshots.get(vm_name)?
            .get(snapshot_id)
    }

    pub async fn get_current_snapshot(&self, vm_name: &str) -> Option<String> {
        self.snapshots.get(vm_name)?
            .values()
            .find(|s| s.is_current)
            .map(|s| s.id.clone())
    }

    // Helper methods
    async fn ensure_vm_shutdown(&self, vm_name: &str) -> Result<()> {
        let state = self.get_vm_state(vm_name).await?;
        if matches!(state, VmState::Running) {
            log_info!("Shutting down VM '{}' for template creation", vm_name);

            let output = Command::new("virsh")
                .args(&["shutdown", vm_name])
                .output()?;

            if !output.status.success() {
                return Err(NovaError::SystemCommandFailed);
            }

            // Wait for shutdown
            for _ in 0..30 {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                if matches!(self.get_vm_state(vm_name).await?, VmState::Shutdown) {
                    break;
                }
            }
        }
        Ok(())
    }

    async fn get_vm_state(&self, vm_name: &str) -> Result<VmState> {
        let output = Command::new("virsh")
            .args(&["domstate", vm_name])
            .output()?;

        if output.status.success() {
            let state = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
            match state.as_str() {
                "running" => Ok(VmState::Running),
                "paused" => Ok(VmState::Paused),
                "shut off" => Ok(VmState::Shutdown),
                "crashed" => Ok(VmState::Crashed),
                _ => Ok(VmState::Shutdown),
            }
        } else {
            Err(NovaError::VmNotFound(vm_name.to_string()))
        }
    }

    async fn compress_vm_disk(&self, source: &Path, target: &Path) -> Result<()> {
        log_info!("Compressing VM disk: {:?} -> {:?}", source, target);

        let output = Command::new("qemu-img")
            .args(&["convert", "-c", "-O", "qcow2"])
            .arg(source)
            .arg(target)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to compress disk: {}", error);
            return Err(NovaError::SystemCommandFailed);
        }

        log_info!("Disk compression completed");
        Ok(())
    }

    fn load_templates(&mut self) -> Result<()> {
        if !self.templates_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.templates_dir)? {
            let entry = entry?;
            let template_dir = entry.path();

            if template_dir.is_dir() {
                let metadata_file = template_dir.join("template.json");
                if metadata_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(metadata_file) {
                        if let Ok(template) = serde_json::from_str::<VmTemplate>(&content) {
                            self.templates.insert(template.id.clone(), template);
                        }
                    }
                }
            }
        }

        log_info!("Loaded {} templates", self.templates.len());
        Ok(())
    }

    // Placeholder implementations for complex operations
    async fn get_vm_info(&self, _vm_name: &str) -> Result<VmInfo> {
        Ok(VmInfo {
            cpu_cores: 2,
            memory_mb: 2048,
            disk_size_gb: 20,
        })
    }

    async fn get_vm_disk_path(&self, _vm_name: &str) -> Result<PathBuf> {
        Ok(PathBuf::from("/var/lib/libvirt/images/vm.qcow2"))
    }

    async fn save_vm_config_as_template(&self, _vm_name: &str, _config_path: &Path) -> Result<()> {
        Ok(())
    }

    async fn detect_vm_os_type(&self, _vm_name: &str) -> OperatingSystem {
        OperatingSystem::Linux { distro: LinuxDistro::Ubuntu { version: "22.04".to_string() } }
    }

    async fn check_guest_tools_installed(&self, _vm_name: &str) -> bool {
        false
    }

    fn get_directory_size(&self, _dir: &Path) -> Result<u64> {
        Ok(1024 * 1024 * 1024) // 1GB placeholder
    }

    async fn calculate_snapshot_size(&self, _vm_name: &str, _snapshot_id: &str) -> Result<u64> {
        Ok(512 * 1024 * 1024) // 512MB placeholder
    }
}

// Helper structs
#[derive(Debug, Clone)]
struct VmInfo {
    cpu_cores: u32,
    memory_mb: u64,
    disk_size_gb: u64,
}

impl Default for NetworkTemplate {
    fn default() -> Self {
        Self {
            interface_type: "virtio".to_string(),
            network_name: None,
            mac_address: None,
            boot_order: 1,
        }
    }
}