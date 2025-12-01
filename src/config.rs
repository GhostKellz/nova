use crate::{
    NovaError, Result, gpu_passthrough::GpuPassthroughConfig, looking_glass::LookingGlassConfig,
    theme,
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
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub iso: IsoConfig,
    #[serde(default)]
    pub templates: TemplatesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IsoConfig {
    /// Directories to scan for ISO files
    #[serde(default = "default_iso_paths")]
    pub paths: Vec<PathBuf>,
    /// Known ISOs with friendly names
    #[serde(default)]
    pub known: HashMap<String, IsoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoEntry {
    pub path: PathBuf,
    pub name: String,
    pub os_type: String,
    pub version: Option<String>,
}

fn default_iso_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/data/iso"),
        PathBuf::from("/var/lib/libvirt/images"),
        PathBuf::from("/home").join(std::env::var("USER").unwrap_or_default()).join("ISOs"),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatesConfig {
    /// Enable built-in VM templates
    #[serde(default = "default_true")]
    pub enable_builtin: bool,
    /// Custom template definitions
    #[serde(default)]
    pub custom: HashMap<String, VmTemplateConfig>,
}

impl Default for TemplatesConfig {
    fn default() -> Self {
        Self {
            enable_builtin: true,
            custom: HashMap::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmTemplateConfig {
    pub name: String,
    pub description: String,
    pub os_type: String,
    pub cpu: u32,
    pub memory: String,
    pub disk_size: String,
    #[serde(default)]
    pub gpu_passthrough: bool,
    #[serde(default)]
    pub uefi: bool,
    #[serde(default)]
    pub secure_boot: bool,
    #[serde(default)]
    pub tpm: bool,
    pub iso_pattern: Option<String>, // regex to match ISO files
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
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
    #[serde(default)]
    pub firmware: VmFirmwareConfig,
    #[serde(default)]
    pub tpm: VmTpmConfig,
    #[serde(default)]
    pub compliance_profile: Option<VmComplianceProfile>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_ui_theme")]
    pub theme: String,
    #[serde(default = "default_ui_font_family")]
    pub font_family: String,
    #[serde(default = "default_ui_font_size")]
    pub font_size: f32,
    #[serde(default)]
    pub compact_layout: bool,
    #[serde(default = "default_ui_auto_refresh")]
    pub auto_refresh: bool,
    #[serde(default = "default_ui_refresh_interval_seconds")]
    pub refresh_interval_seconds: u64,
    #[serde(default = "default_ui_network_refresh_interval_seconds")]
    pub network_refresh_interval_seconds: u64,
    #[serde(default = "default_ui_show_event_log")]
    pub show_event_log: bool,
    #[serde(default = "default_ui_show_insights")]
    pub show_insights: bool,
    #[serde(default = "default_ui_confirm_instance_actions")]
    pub confirm_instance_actions: bool,
    #[serde(default = "default_ui_container_logs_auto_refresh")]
    pub container_logs_auto_refresh: bool,
    #[serde(default = "default_ui_container_logs_refresh_interval_seconds")]
    pub container_logs_refresh_interval_seconds: u64,
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
pub enum VmBootType {
    Legacy,
    Uefi,
}

impl Default for VmBootType {
    fn default() -> Self {
        VmBootType::Legacy
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmFirmwareConfig {
    #[serde(default = "default_vm_boot_type")]
    pub boot_type: VmBootType,
    #[serde(default)]
    pub secure_boot: bool,
    pub ovmf_code: Option<String>,
    pub ovmf_vars: Option<String>,
}

impl Default for VmFirmwareConfig {
    fn default() -> Self {
        Self {
            boot_type: default_vm_boot_type(),
            secure_boot: false,
            ovmf_code: None,
            ovmf_vars: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VmTpmVersion {
    V1_2,
    V2_0,
}

impl Default for VmTpmVersion {
    fn default() -> Self {
        VmTpmVersion::V2_0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmTpmConfig {
    #[serde(default = "default_tpm_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tpm_version")]
    pub version: VmTpmVersion,
}

impl Default for VmTpmConfig {
    fn default() -> Self {
        Self {
            enabled: default_tpm_enabled(),
            version: default_tpm_version(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VmComplianceProfile {
    Windows11,
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
            ui: UiConfig::default(),
            iso: IsoConfig::default(),
            templates: TemplatesConfig::default(),
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
            firmware: VmFirmwareConfig::default(),
            tpm: VmTpmConfig::default(),
            compliance_profile: None,
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

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_ui_theme(),
            font_family: default_ui_font_family(),
            font_size: default_ui_font_size(),
            compact_layout: false,
            auto_refresh: default_ui_auto_refresh(),
            refresh_interval_seconds: default_ui_refresh_interval_seconds(),
            network_refresh_interval_seconds: default_ui_network_refresh_interval_seconds(),
            show_event_log: default_ui_show_event_log(),
            show_insights: default_ui_show_insights(),
            confirm_instance_actions: default_ui_confirm_instance_actions(),
            container_logs_auto_refresh: default_ui_container_logs_auto_refresh(),
            container_logs_refresh_interval_seconds:
                default_ui_container_logs_refresh_interval_seconds(),
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

fn default_vm_boot_type() -> VmBootType {
    VmBootType::Legacy
}

fn default_tpm_enabled() -> bool {
    false
}

fn default_tpm_version() -> VmTpmVersion {
    VmTpmVersion::V2_0
}

fn default_ui_theme() -> String {
    theme::DEFAULT_THEME_NAME.to_string()
}

pub fn default_ui_font_family() -> String {
    "fira-code-nerd".to_string()
}

pub fn default_ui_font_size() -> f32 {
    15.0
}

fn default_ui_auto_refresh() -> bool {
    true
}

fn default_ui_refresh_interval_seconds() -> u64 {
    5
}

fn default_ui_network_refresh_interval_seconds() -> u64 {
    15
}

fn default_ui_show_event_log() -> bool {
    false
}

fn default_ui_show_insights() -> bool {
    true
}

fn default_ui_confirm_instance_actions() -> bool {
    true
}

fn default_ui_container_logs_auto_refresh() -> bool {
    false
}

fn default_ui_container_logs_refresh_interval_seconds() -> u64 {
    15
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
