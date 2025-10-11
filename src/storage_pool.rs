use crate::{NovaError, Result, log_debug, log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Storage pool types supported by Nova
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PoolType {
    /// Local directory-based storage
    Directory,
    /// Btrfs filesystem with subvolume support
    Btrfs,
    /// ZFS filesystem (future)
    Zfs,
    /// Network File System
    Nfs,
    /// iSCSI storage
    Iscsi,
    /// Ceph RBD
    Ceph,
    /// LVM volume group
    Lvm,
}

/// Storage pool state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PoolState {
    Active,
    Inactive,
    Building,
    Degraded,
    Error(String),
}

/// Storage pool capacity information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolCapacity {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub allocation_bytes: u64,  // Allocated but not yet used (thin provisioning)
}

impl PoolCapacity {
    pub fn usage_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Storage pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePool {
    pub name: String,
    pub pool_type: PoolType,
    pub path: PathBuf,
    pub state: PoolState,
    pub capacity: Option<PoolCapacity>,
    pub autostart: bool,

    // Type-specific configuration
    pub config: PoolConfig,

    // Metadata
    pub uuid: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Type-specific pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PoolConfig {
    Directory {
        permissions: u32,
    },
    Btrfs {
        mount_point: PathBuf,
        subvolume: Option<String>,
        compression: BtrfsCompression,
        quota_enabled: bool,
    },
    Zfs {
        dataset: String,
        compression: ZfsCompression,
        dedup: bool,
    },
    Nfs {
        server: String,
        export_path: String,
        mount_options: Vec<String>,
    },
    Iscsi {
        target: String,
        portal: String,
        lun: u32,
        auth: Option<IscsiAuth>,
    },
    Ceph {
        monitors: Vec<String>,
        pool_name: String,
        user: String,
        secret_uuid: Option<String>,
    },
    Lvm {
        vg_name: String,
        pv_devices: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BtrfsCompression {
    None,
    Zlib,
    Lzo,
    Zstd { level: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZfsCompression {
    Off,
    Lz4,
    Gzip { level: u8 },
    Zstd { level: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IscsiAuth {
    pub username: String,
    pub password: String,
    pub auth_type: String,  // CHAP, etc.
}

/// Storage volume representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageVolume {
    pub name: String,
    pub pool_name: String,
    pub path: PathBuf,
    pub format: VolumeFormat,
    pub capacity_bytes: u64,
    pub allocation_bytes: u64,
    pub backing_store: Option<PathBuf>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VolumeFormat {
    Raw,
    Qcow2,
    Qed,
    Vmdk,
    Vdi,
}

/// Storage pool manager
pub struct StoragePoolManager {
    pools: HashMap<String, StoragePool>,
    volumes: HashMap<String, Vec<StorageVolume>>,
}

impl StoragePoolManager {
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
            volumes: HashMap::new(),
        }
    }

    /// Discover existing storage pools from libvirt
    pub async fn discover_pools(&mut self) -> Result<()> {
        log_info!("Discovering storage pools...");

        // Get pools from virsh
        let output = Command::new("virsh")
            .args(&["pool-list", "--all", "--name"])
            .output()
            .map_err(|e| {
                log_error!("Failed to list pools: {}", e);
                NovaError::SystemCommandFailed
            })?;

        if !output.status.success() {
            return Err(NovaError::SystemCommandFailed);
        }

        let pool_names = String::from_utf8_lossy(&output.stdout);

        for line in pool_names.lines() {
            let name = line.trim();
            if !name.is_empty() {
                if let Ok(pool) = self.get_pool_info(name).await {
                    self.pools.insert(name.to_string(), pool);
                }
            }
        }

        log_info!("Discovered {} storage pools", self.pools.len());
        Ok(())
    }

    /// Get detailed information about a pool
    async fn get_pool_info(&self, name: &str) -> Result<StoragePool> {
        let output = Command::new("virsh")
            .args(&["pool-dumpxml", name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            return Err(NovaError::ConfigError(format!("Pool {} not found", name)));
        }

        let xml = String::from_utf8_lossy(&output.stdout);
        self.parse_pool_xml(&xml, name)
    }

    /// Parse libvirt pool XML
    fn parse_pool_xml(&self, xml: &str, name: &str) -> Result<StoragePool> {
        // Simplified XML parsing - production would use proper XML parser
        let pool_type = if xml.contains("type='dir'") {
            PoolType::Directory
        } else if xml.contains("type='netfs'") {
            PoolType::Nfs
        } else if xml.contains("type='iscsi'") {
            PoolType::Iscsi
        } else if xml.contains("type='logical'") {
            PoolType::Lvm
        } else if xml.contains("type='rbd'") {
            PoolType::Ceph
        } else {
            PoolType::Directory
        };

        // Extract path
        let path = if let Some(start) = xml.find("<path>") {
            if let Some(end) = xml[start..].find("</path>") {
                PathBuf::from(&xml[start + 6..start + end])
            } else {
                PathBuf::from("/var/lib/nova/storage")
            }
        } else {
            PathBuf::from("/var/lib/nova/storage")
        };

        // Get capacity info
        let capacity = self.get_pool_capacity(&path);

        // Check if pool is active
        let is_active = self.is_pool_active(name);
        let state = if is_active {
            PoolState::Active
        } else {
            PoolState::Inactive
        };

        // Create appropriate config
        let config = match pool_type {
            PoolType::Directory => PoolConfig::Directory { permissions: 0o755 },
            PoolType::Btrfs => self.detect_btrfs_config(&path),
            _ => PoolConfig::Directory { permissions: 0o755 },
        };

        Ok(StoragePool {
            name: name.to_string(),
            pool_type,
            path,
            state,
            capacity: Some(capacity),
            autostart: false,
            config,
            uuid: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    /// Check if a pool is active
    fn is_pool_active(&self, name: &str) -> bool {
        Command::new("virsh")
            .args(&["pool-info", name])
            .output()
            .map(|output| {
                if output.status.success() {
                    let info = String::from_utf8_lossy(&output.stdout);
                    info.contains("State:           running")
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    /// Get pool capacity using df
    fn get_pool_capacity(&self, path: &Path) -> PoolCapacity {
        let output = Command::new("df")
            .args(&["-B1", path.to_str().unwrap_or("/")])
            .output()
            .ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = stdout.lines().nth(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        let total = parts[1].parse::<u64>().unwrap_or(0);
                        let used = parts[2].parse::<u64>().unwrap_or(0);
                        let available = parts[3].parse::<u64>().unwrap_or(0);

                        return PoolCapacity {
                            total_bytes: total,
                            used_bytes: used,
                            available_bytes: available,
                            allocation_bytes: used,
                        };
                    }
                }
            }
        }

        // Default empty capacity
        PoolCapacity {
            total_bytes: 0,
            used_bytes: 0,
            available_bytes: 0,
            allocation_bytes: 0,
        }
    }

    /// Detect Btrfs-specific configuration
    fn detect_btrfs_config(&self, path: &Path) -> PoolConfig {
        // Check if path is on btrfs
        let output = Command::new("stat")
            .args(&["-f", "-c", "%T", path.to_str().unwrap_or("/")])
            .output()
            .ok();

        let is_btrfs = output
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("btrfs"))
            .unwrap_or(false);

        if is_btrfs {
            // Get btrfs mount point
            let mount_point = self.find_btrfs_mount(path);

            PoolConfig::Btrfs {
                mount_point,
                subvolume: None,
                compression: BtrfsCompression::Zstd { level: 3 },
                quota_enabled: false,
            }
        } else {
            PoolConfig::Directory { permissions: 0o755 }
        }
    }

    /// Find Btrfs mount point for a path
    fn find_btrfs_mount(&self, path: &Path) -> PathBuf {
        // Read /proc/mounts to find the mount point
        if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && parts[2] == "btrfs" {
                    let mount = PathBuf::from(parts[1]);
                    if path.starts_with(&mount) {
                        return mount;
                    }
                }
            }
        }

        path.to_path_buf()
    }

    /// Create a new storage pool
    pub async fn create_pool(&mut self, pool: StoragePool) -> Result<()> {
        log_info!("Creating storage pool: {}", pool.name);

        match pool.pool_type {
            PoolType::Directory => self.create_directory_pool(&pool).await?,
            PoolType::Btrfs => self.create_btrfs_pool(&pool).await?,
            PoolType::Nfs => self.create_nfs_pool(&pool).await?,
            _ => {
                return Err(NovaError::ConfigError(
                    format!("Pool type {:?} not yet implemented", pool.pool_type)
                ));
            }
        }

        self.pools.insert(pool.name.clone(), pool);
        Ok(())
    }

    /// Create a directory-based pool
    async fn create_directory_pool(&self, pool: &StoragePool) -> Result<()> {
        // Create directory if it doesn't exist
        if !pool.path.exists() {
            fs::create_dir_all(&pool.path).map_err(|e| {
                log_error!("Failed to create pool directory: {}", e);
                NovaError::SystemCommandFailed
            })?;
        }

        // Generate libvirt XML
        let xml = self.generate_directory_pool_xml(pool)?;

        // Write to temp file
        let temp_file = format!("/tmp/nova-pool-{}.xml", pool.name);
        fs::write(&temp_file, xml).map_err(|_| NovaError::SystemCommandFailed)?;

        // Define pool in libvirt
        let output = Command::new("virsh")
            .args(&["pool-define", &temp_file])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            fs::remove_file(&temp_file).ok();
            return Err(NovaError::SystemCommandFailed);
        }

        // Start the pool
        let _ = Command::new("virsh")
            .args(&["pool-start", &pool.name])
            .output();

        // Autostart if requested
        if pool.autostart {
            let _ = Command::new("virsh")
                .args(&["pool-autostart", &pool.name])
                .output();
        }

        fs::remove_file(&temp_file).ok();
        log_info!("Directory pool {} created successfully", pool.name);
        Ok(())
    }

    /// Create a Btrfs-based pool with subvolumes
    async fn create_btrfs_pool(&self, pool: &StoragePool) -> Result<()> {
        log_info!("Creating Btrfs pool: {}", pool.name);

        if let PoolConfig::Btrfs { mount_point, subvolume, compression, .. } = &pool.config {
            // Create btrfs subvolume if specified
            if let Some(subvol) = subvolume {
                let subvol_path = mount_point.join(subvol);

                let output = Command::new("btrfs")
                    .args(&["subvolume", "create", subvol_path.to_str().unwrap()])
                    .output()
                    .map_err(|e| {
                        log_error!("Failed to create btrfs subvolume: {}", e);
                        NovaError::SystemCommandFailed
                    })?;

                if !output.status.success() {
                    let err = String::from_utf8_lossy(&output.stderr);
                    log_error!("btrfs subvolume create failed: {}", err);
                    return Err(NovaError::SystemCommandFailed);
                }

                // Set compression
                match compression {
                    BtrfsCompression::Zstd { level } => {
                        let _ = Command::new("btrfs")
                            .args(&[
                                "property", "set", subvol_path.to_str().unwrap(),
                                "compression", &format!("zstd:{}", level)
                            ])
                            .output();
                    }
                    BtrfsCompression::Lzo => {
                        let _ = Command::new("btrfs")
                            .args(&[
                                "property", "set", subvol_path.to_str().unwrap(),
                                "compression", "lzo"
                            ])
                            .output();
                    }
                    BtrfsCompression::Zlib => {
                        let _ = Command::new("btrfs")
                            .args(&[
                                "property", "set", subvol_path.to_str().unwrap(),
                                "compression", "zlib"
                            ])
                            .output();
                    }
                    BtrfsCompression::None => {}
                }

                log_info!("Btrfs subvolume created: {}", subvol);
            }

            // Create directory pool at the subvolume path
            let dir_pool = StoragePool {
                config: PoolConfig::Directory { permissions: 0o755 },
                ..pool.clone()
            };

            self.create_directory_pool(&dir_pool).await?;
        }

        Ok(())
    }

    /// Create an NFS pool
    async fn create_nfs_pool(&self, pool: &StoragePool) -> Result<()> {
        log_info!("Creating NFS pool: {}", pool.name);

        if let PoolConfig::Nfs { server, export_path, mount_options } = &pool.config {
            // Generate libvirt XML for NFS
            let xml = self.generate_nfs_pool_xml(pool, server, export_path, mount_options)?;

            let temp_file = format!("/tmp/nova-pool-{}.xml", pool.name);
            fs::write(&temp_file, xml).map_err(|_| NovaError::SystemCommandFailed)?;

            let output = Command::new("virsh")
                .args(&["pool-define", &temp_file])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                fs::remove_file(&temp_file).ok();
                return Err(NovaError::SystemCommandFailed);
            }

            // Start the pool
            let output = Command::new("virsh")
                .args(&["pool-start", &pool.name])
                .output()
                .map_err(|_| NovaError::SystemCommandFailed)?;

            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                log_error!("Failed to start NFS pool: {}", err);
            }

            fs::remove_file(&temp_file).ok();
        }

        Ok(())
    }

    /// Generate libvirt XML for directory pool
    fn generate_directory_pool_xml(&self, pool: &StoragePool) -> Result<String> {
        Ok(format!(
            r#"<pool type='dir'>
  <name>{}</name>
  <target>
    <path>{}</path>
    <permissions>
      <mode>0755</mode>
      <owner>0</owner>
      <group>0</group>
    </permissions>
  </target>
</pool>"#,
            pool.name,
            pool.path.display()
        ))
    }

    /// Generate libvirt XML for NFS pool
    fn generate_nfs_pool_xml(
        &self,
        pool: &StoragePool,
        server: &str,
        export_path: &str,
        _mount_options: &[String],
    ) -> Result<String> {
        Ok(format!(
            r#"<pool type='netfs'>
  <name>{}</name>
  <source>
    <host name='{}'/>
    <dir path='{}'/>
    <format type='nfs'/>
  </source>
  <target>
    <path>{}</path>
  </target>
</pool>"#,
            pool.name,
            server,
            export_path,
            pool.path.display()
        ))
    }

    /// Delete a storage pool
    pub async fn delete_pool(&mut self, name: &str, delete_volumes: bool) -> Result<()> {
        log_info!("Deleting storage pool: {}", name);

        // Stop the pool
        let _ = Command::new("virsh")
            .args(&["pool-destroy", name])
            .output();

        // Undefine the pool
        let mut args = vec!["pool-undefine", name];
        if delete_volumes {
            // Note: virsh doesn't have --delete-volumes, we handle it manually
            if let Some(pool) = self.pools.get(name) {
                if pool.path.exists() {
                    log_warn!("Deleting pool directory: {}", pool.path.display());
                    fs::remove_dir_all(&pool.path).ok();
                }
            }
        }

        let output = Command::new("virsh")
            .args(&args)
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to undefine pool: {}", err);
            return Err(NovaError::SystemCommandFailed);
        }

        self.pools.remove(name);
        log_info!("Pool {} deleted successfully", name);
        Ok(())
    }

    /// Create a volume in a pool
    pub async fn create_volume(
        &mut self,
        pool_name: &str,
        volume_name: &str,
        size_bytes: u64,
        format: VolumeFormat,
    ) -> Result<StorageVolume> {
        log_info!("Creating volume {} in pool {}", volume_name, pool_name);

        let format_str = match format {
            VolumeFormat::Raw => "raw",
            VolumeFormat::Qcow2 => "qcow2",
            VolumeFormat::Qed => "qed",
            VolumeFormat::Vmdk => "vmdk",
            VolumeFormat::Vdi => "vdi",
        };

        let output = Command::new("virsh")
            .args(&[
                "vol-create-as",
                pool_name,
                volume_name,
                &size_bytes.to_string(),
                "--format",
                format_str,
            ])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            log_error!("Failed to create volume: {}", err);
            return Err(NovaError::SystemCommandFailed);
        }

        // Get volume path
        let path = self.get_volume_path(pool_name, volume_name)?;

        let volume = StorageVolume {
            name: volume_name.to_string(),
            pool_name: pool_name.to_string(),
            path,
            format,
            capacity_bytes: size_bytes,
            allocation_bytes: 0,
            backing_store: None,
            created_at: chrono::Utc::now(),
        };

        self.volumes
            .entry(pool_name.to_string())
            .or_insert_with(Vec::new)
            .push(volume.clone());

        log_info!("Volume {} created successfully", volume_name);
        Ok(volume)
    }

    /// Get volume path
    fn get_volume_path(&self, pool_name: &str, volume_name: &str) -> Result<PathBuf> {
        let output = Command::new("virsh")
            .args(&["vol-path", volume_name, "--pool", pool_name])
            .output()
            .map_err(|_| NovaError::SystemCommandFailed)?;

        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout);
            Ok(PathBuf::from(path_str.trim()))
        } else {
            Err(NovaError::SystemCommandFailed)
        }
    }

    /// List all pools
    pub fn list_pools(&self) -> Vec<&StoragePool> {
        self.pools.values().collect()
    }

    /// Get a specific pool
    pub fn get_pool(&self, name: &str) -> Option<&StoragePool> {
        self.pools.get(name)
    }

    /// List volumes in a pool
    pub fn list_volumes(&self, pool_name: &str) -> Vec<&StorageVolume> {
        self.volumes
            .get(pool_name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }
}

impl Default for StoragePoolManager {
    fn default() -> Self {
        Self::new()
    }
}
