//! Container Runtime Abstraction Layer
//!
//! Provides a unified interface for multiple container runtimes:
//! - Bolt (primary): High-performance Rust container runtime
//! - Docker (fallback): Industry standard
//! - Podman (alternative): Daemonless alternative
//! - Unshare (basic): Simple namespace isolation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result type for container runtime operations
pub type Result<T> = std::result::Result<T, ContainerRuntimeError>;

/// Unified interface for container runtimes
#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    /// Check if this runtime is available on the system
    fn is_available(&self) -> bool;

    /// Get runtime name
    fn name(&self) -> &str;

    /// Get runtime version
    async fn version(&self) -> Result<String>;

    /// Run a container
    async fn run_container(
        &self,
        image: &str,
        name: Option<&str>,
        config: &ContainerConfig,
    ) -> Result<String>;

    /// Stop a container
    async fn stop_container(&self, id_or_name: &str) -> Result<()>;

    /// Remove a container
    async fn remove_container(&self, id_or_name: &str, force: bool) -> Result<()>;

    /// List containers
    async fn list_containers(&self, all: bool) -> Result<Vec<ContainerInfo>>;

    /// Inspect container details
    async fn inspect_container(&self, id_or_name: &str) -> Result<ContainerInfo>;

    /// Pull an image
    async fn pull_image(&self, image: &str) -> Result<()>;

    /// List images
    async fn list_images(&self) -> Result<Vec<ImageInfo>>;

    /// Get container logs
    async fn get_logs(&self, id_or_name: &str, lines: usize) -> Result<Vec<String>>;

    /// Get container stats/metrics
    async fn get_stats(&self, id_or_name: &str) -> Result<ContainerStats>;
}

/// Container configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container image/capsule
    pub capsule: String,

    /// Port mappings (host:container)
    pub ports: Vec<String>,

    /// Volume mounts (host:container)
    pub volumes: Vec<String>,

    /// Environment variables
    pub env: HashMap<String, String>,

    /// Network to connect to
    pub network: Option<String>,

    /// Enable GPU passthrough
    pub gpu_passthrough: bool,

    /// Memory limit in MB
    pub memory_mb: Option<u64>,

    /// CPU limit (number of cores)
    pub cpus: Option<u32>,

    /// Restart policy
    pub restart_policy: RestartPolicy,

    /// Run in detached mode
    pub detach: bool,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            capsule: String::new(),
            ports: Vec::new(),
            volumes: Vec::new(),
            env: HashMap::new(),
            network: None,
            gpu_passthrough: false,
            memory_mb: None,
            cpus: None,
            restart_policy: RestartPolicy::No,
            detach: true,
        }
    }
}

/// Restart policy for containers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RestartPolicy {
    No,
    Always,
    OnFailure,
    UnlessStopped,
}

/// Container information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: ContainerStatus,
    pub created: chrono::DateTime<chrono::Utc>,
    pub ports: Vec<PortMapping>,
    pub network: Option<String>,
    pub pid: Option<u32>,
    pub ip_address: Option<String>,
}

/// Container status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContainerStatus {
    Running,
    Stopped,
    Paused,
    Restarting,
    Starting,
    Dead,
    Unknown,
}

/// Port mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: PortProtocol,
}

/// Port protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PortProtocol {
    Tcp,
    Udp,
}

/// Image information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub tags: Vec<String>,
    pub size: u64,
    pub created: chrono::DateTime<chrono::Utc>,
}

/// Container resource statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStats {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub memory_limit_mb: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
}

/// Container runtime errors
#[derive(Debug, thiserror::Error)]
pub enum ContainerRuntimeError {
    #[error("Runtime not available: {0}")]
    RuntimeNotAvailable(String),

    #[error("Container not found: {0}")]
    ContainerNotFound(String),

    #[error("Image not found: {0}")]
    ImageNotFound(String),

    #[error("Container already exists: {0}")]
    ContainerAlreadyExists(String),

    #[error("Failed to start container: {0}")]
    StartFailed(String),

    #[error("Failed to stop container: {0}")]
    StopFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("GPU configuration error: {0}")]
    GpuError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}
