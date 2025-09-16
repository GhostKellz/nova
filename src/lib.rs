pub mod arch_integration;
pub mod config;
pub mod container;
pub mod error;
pub mod gui_main;
pub mod gui_network;
pub mod instance;
pub mod libvirt;
pub mod logger;
pub mod monitoring;
pub mod network;
pub mod templates;
pub mod theme;
pub mod vm;
pub mod vm_enhanced;
pub mod console;
pub mod console_enhanced;
pub mod rustdesk_integration;
pub mod migration;
pub mod templates_snapshots;

pub use error::NovaError;
pub use instance::{Instance, InstanceStatus, InstanceType};

pub type Result<T> = std::result::Result<T, NovaError>;

// Convenience re-exports for networking components
pub use network::{NetworkManager, VirtualSwitch, NetworkInterface};
pub use libvirt::{LibvirtManager, LibvirtNetwork};
pub use monitoring::{NetworkMonitor, NetworkTopology};
pub use arch_integration::ArchNetworkManager;