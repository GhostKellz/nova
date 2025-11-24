use crate::{
    NovaError, Result, gpu_passthrough::GpuPassthroughConfig, looking_glass::LookingGlassConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NovaConfig {
    pub project: Option<String>,
    #[serde(default)]
    pub vm: HashMap<String, VmConfig>,
    #[serde(default)]
    pub container: HashMap<String, ContainerConfig>,
    #[serde(default)]
    pub network: HashMap<String, NetworkConfig>,
    #[serde(default)]
    pub storage: HashMap<String, StoragePoolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub image: Option<String>,
    #[serde(default = "default_cpu")]
    pub cpu: u32,
    #[serde(default = "default_memory")]
    pub memory: String,
    #[serde(default)]
    pub gpu_passthrough: bool,
    #[serde(default)]
    pub gpu: Option<GpuPassthroughConfig>,
    pub network: Option<String>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub storage: VmStorageConfig,
    #[serde(default)]
    pub looking_glass: LookingGlassConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePoolConfig {
    #[serde(default = "default_storage_pool_type")]
    pub pool_type: StoragePoolType,
    pub directory: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capacity: Option<String>,
    #[serde(default = "default_disk_format")]
    pub default_format: DiskFormat,
    #[serde(default = "default_create_if_missing")]
    pub auto_create: bool,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StoragePoolType {
    Directory,
    Btrfs,
    Nfs,
}

impl StoragePoolType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StoragePoolType::Directory => "directory",
            StoragePoolType::Btrfs => "btrfs",
            StoragePoolType::Nfs => "nfs",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStorageConfig {
    /// Directory containing the VM disk image. Defaults to `/var/lib/nova/disks`.
    pub directory: Option<String>,
    /// Disk filename; defaults to `<vm_name>.<format>`.
    pub filename: Option<String>,
    #[serde(default = "default_disk_format")]
    pub format: DiskFormat,
    #[serde(default = "default_disk_size")]
    pub size: String,
    #[serde(default = "default_create_if_missing")]
    pub create_if_missing: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiskFormat {
    Qcow2,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub capsule: Option<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    pub network: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub autostart: bool,
    pub runtime: Option<String>, // "bolt", "docker", "podman", or auto-detect
    #[serde(default)]
    pub bolt: BoltConfig, // Bolt-specific configuration
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoltConfig {
    #[serde(default)]
    pub isolation: String, // "strict", "standard", "loose"
    #[serde(default)]
    pub gpu_access: bool,
    #[serde(default)]
    pub gpu_devices: Vec<String>, // ["nvidia0", "nvidia1"]
    pub memory_limit: Option<String>, // "8Gi", "512Mi"
    pub cpu_limit: Option<String>,    // "4", "2.5"
    #[serde(default)]
    pub security_profile: String, // "default", "strict", "production"
    #[serde(default)]
    pub network_mode: String, // "bridge", "quic", "host"
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub no_new_privileges: bool,
    pub quic_streams: Option<u32>, // Max QUIC streams
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(rename = "type")]
    pub network_type: NetworkType,
    #[serde(default)]
    pub interfaces: Vec<String>,
    pub driver: Option<String>,
    #[serde(default)]
    pub dns: bool,
    pub subnet: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkType {
    Bridge,
    Overlay,
    Host,
    None,
}

impl Default for NovaConfig {
    fn default() -> Self {
        Self {
            project: None,
            vm: HashMap::new(),
            container: HashMap::new(),
            network: HashMap::new(),
            storage: HashMap::new(),
        }
    }
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            image: None,
            cpu: default_cpu(),
            memory: default_memory(),
            gpu_passthrough: false,
            gpu: None,
            network: None,
            autostart: false,
            storage: VmStorageConfig::default(),
            looking_glass: LookingGlassConfig::default(),
        }
    }
}

impl Default for StoragePoolConfig {
    fn default() -> Self {
        Self {
            pool_type: default_storage_pool_type(),
            directory: "/var/lib/nova/disks".to_string(),
            description: None,
            capacity: None,
            default_format: default_disk_format(),
            auto_create: default_create_if_missing(),
            labels: Vec::new(),
        }
    }
}

impl Default for VmStorageConfig {
    fn default() -> Self {
        Self {
            directory: None,
            filename: None,
            format: default_disk_format(),
            size: default_disk_size(),
            create_if_missing: default_create_if_missing(),
        }
    }
}

impl Default for DiskFormat {
    fn default() -> Self {
        DiskFormat::Qcow2
    }
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            capsule: None,
            volumes: Vec::new(),
            network: None,
            env: HashMap::new(),
            autostart: false,
            runtime: None, // Auto-detect
            bolt: BoltConfig::default(),
        }
    }
}

impl Default for BoltConfig {
    fn default() -> Self {
        Self {
            isolation: "standard".to_string(),
            gpu_access: false,
            gpu_devices: Vec::new(),
            memory_limit: None,
            cpu_limit: None,
            security_profile: "default".to_string(),
            network_mode: "bridge".to_string(),
            read_only: false,
            no_new_privileges: false,
            quic_streams: None,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            network_type: NetworkType::Bridge,
            interfaces: Vec::new(),
            driver: None,
            dns: false,
            subnet: None,
            gateway: None,
        }
    }
}

fn default_cpu() -> u32 {
    2
}

fn default_memory() -> String {
    "1Gi".to_string()
}

fn default_disk_format() -> DiskFormat {
    DiskFormat::Qcow2
}

fn default_disk_size() -> String {
    "20Gi".to_string()
}

fn default_create_if_missing() -> bool {
    true
}

fn default_storage_pool_type() -> StoragePoolType {
    StoragePoolType::Directory
}

impl VmStorageConfig {
    const DEFAULT_DIRECTORY: &str = "/var/lib/nova/disks";

    pub fn resolve_disk_path(&self, vm_name: &str) -> PathBuf {
        let directory = self
            .directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(Self::DEFAULT_DIRECTORY));

        let filename = self
            .filename
            .clone()
            .unwrap_or_else(|| format!("{}.{}", vm_name, self.format.extension()));

        directory.join(filename)
    }
}

impl DiskFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiskFormat::Qcow2 => "qcow2",
            DiskFormat::Raw => "raw",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            DiskFormat::Qcow2 => "qcow2",
            DiskFormat::Raw => "img",
        }
    }
}

impl NovaConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: NovaConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn from_str(contents: &str) -> Result<Self> {
        let config: NovaConfig = toml::from_str(contents)?;
        Ok(config)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let contents = toml::to_string_pretty(self).map_err(|_e| NovaError::InvalidConfig)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn get_vm(&self, name: &str) -> Option<&VmConfig> {
        self.vm.get(name)
    }

    pub fn get_container(&self, name: &str) -> Option<&ContainerConfig> {
        self.container.get(name)
    }

    pub fn get_network(&self, name: &str) -> Option<&NetworkConfig> {
        self.network.get(name)
    }

    pub fn list_vms(&self) -> Vec<&String> {
        self.vm.keys().collect()
    }

    pub fn list_containers(&self) -> Vec<&String> {
        self.container.keys().collect()
    }

    pub fn list_networks(&self) -> Vec<&String> {
        self.network.keys().collect()
    }

    pub fn get_storage_pool(&self, name: &str) -> Option<&StoragePoolConfig> {
        self.storage.get(name)
    }

    pub fn list_storage_pools(&self) -> Vec<&String> {
        self.storage.keys().collect()
    }
}

// Parse memory string like "1Gi", "512Mi", "2G" to bytes
pub fn parse_memory_to_bytes(memory_str: &str) -> Result<u64> {
    let memory_str = memory_str.trim();

    if memory_str.is_empty() {
        return Err(NovaError::InvalidConfig);
    }

    let (number_part, suffix) = if memory_str.ends_with("Gi") {
        (memory_str.trim_end_matches("Gi"), "Gi")
    } else if memory_str.ends_with("Mi") {
        (memory_str.trim_end_matches("Mi"), "Mi")
    } else if memory_str.ends_with("G") {
        (memory_str.trim_end_matches("G"), "G")
    } else if memory_str.ends_with("M") {
        (memory_str.trim_end_matches("M"), "M")
    } else {
        // Assume bytes if no suffix
        (memory_str, "")
    };

    let number: u64 = number_part.parse().map_err(|_| NovaError::InvalidConfig)?;

    let bytes = match suffix {
        "Gi" => number * 1024 * 1024 * 1024,
        "Mi" => number * 1024 * 1024,
        "G" => number * 1000 * 1000 * 1000,
        "M" => number * 1000 * 1000,
        "" => number,
        _ => return Err(NovaError::InvalidConfig),
    };

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_memory() {
        assert_eq!(parse_memory_to_bytes("1Gi").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_memory_to_bytes("512Mi").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_memory_to_bytes("2G").unwrap(), 2 * 1000 * 1000 * 1000);
        assert_eq!(parse_memory_to_bytes("1024M").unwrap(), 1024 * 1000 * 1000);
        assert_eq!(parse_memory_to_bytes("1073741824").unwrap(), 1073741824);
    }

    #[test]
    fn test_config_parsing() {
        let toml_str = r#"
project = "test-lab"

[vm.win11]
image = "/var/lib/nova/images/win11.qcow2"
cpu = 8
memory = "16Gi"
gpu_passthrough = true
network = "bridge0"

[container.api]
capsule = "ubuntu:22.04"
volumes = ["./api:/srv/api"]
network = "nova-net"

[container.api.env]
API_KEY = "secret"

[network.bridge0]
type = "bridge"
interfaces = ["enp6s0"]

[network.nova-net]
type = "overlay"
driver = "quic"
dns = true
"#;

        let config = NovaConfig::from_str(toml_str).unwrap();
        assert_eq!(config.project, Some("test-lab".to_string()));

        let vm = config.get_vm("win11").unwrap();
        assert_eq!(vm.cpu, 8);
        assert_eq!(vm.memory, "16Gi");
        assert_eq!(vm.gpu_passthrough, true);

        let container = config.get_container("api").unwrap();
        assert_eq!(container.capsule, Some("ubuntu:22.04".to_string()));
        assert_eq!(container.volumes, vec!["./api:/srv/api"]);
    }

    #[test]
    fn vm_storage_defaults() {
        let storage = VmStorageConfig::default();
        let path = storage.resolve_disk_path("demo-vm");
        assert_eq!(path, PathBuf::from("/var/lib/nova/disks/demo-vm.qcow2"));
        assert_eq!(storage.format.as_str(), "qcow2");
    }

    #[test]
    fn storage_pool_defaults() {
        let pool = StoragePoolConfig::default();
        assert_eq!(pool.pool_type, StoragePoolType::Directory);
        assert_eq!(pool.directory, "/var/lib/nova/disks");
        assert_eq!(pool.default_format, DiskFormat::Qcow2);
        assert!(pool.labels.is_empty());
    }
}
