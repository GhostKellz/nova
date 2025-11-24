use std::fmt;

#[derive(Debug)]
pub enum NovaError {
    SystemCommandFailed,
    InvalidConfig,
    ConfigError(String),
    VmNotFound(String),
    ContainerNotFound(String),
    LibvirtError(String),
    NetworkError(String),
    NetworkNotFound(String),
    IoError(std::io::Error),
    SerdeError(String),
    SnapshotNotFound(String),
    SnapshotHasChildren,
}

impl fmt::Display for NovaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NovaError::SystemCommandFailed => write!(f, "System command failed"),
            NovaError::InvalidConfig => write!(f, "Invalid configuration"),
            NovaError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            NovaError::VmNotFound(name) => write!(f, "VM '{}' not found", name),
            NovaError::ContainerNotFound(name) => write!(f, "Container '{}' not found", name),
            NovaError::LibvirtError(msg) => write!(f, "Libvirt error: {}", msg),
            NovaError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            NovaError::NetworkNotFound(name) => write!(f, "Network '{}' not found", name),
            NovaError::IoError(err) => write!(f, "IO error: {}", err),
            NovaError::SerdeError(err) => write!(f, "Configuration parse error: {}", err),
            NovaError::SnapshotNotFound(name) => write!(f, "Snapshot '{}' not found", name),
            NovaError::SnapshotHasChildren => write!(
                f,
                "Snapshot has children and cannot be deleted without --children flag"
            ),
        }
    }
}

impl std::error::Error for NovaError {}

impl From<std::io::Error> for NovaError {
    fn from(err: std::io::Error) -> Self {
        NovaError::IoError(err)
    }
}

impl From<toml::de::Error> for NovaError {
    fn from(err: toml::de::Error) -> Self {
        NovaError::SerdeError(err.to_string())
    }
}

impl From<serde_json::Error> for NovaError {
    fn from(err: serde_json::Error) -> Self {
        NovaError::SerdeError(err.to_string())
    }
}
