pub mod arch_integration;
pub mod bolt_runtime;
pub mod config;
pub mod console;
pub mod console_enhanced;
pub mod container;
pub mod container_runtime;
pub mod docker_runtime;
pub mod error;
pub mod firewall;
pub mod gpu_doctor;
pub mod gpu_passthrough;
pub mod gui_gpu;
pub mod gui_network;
pub mod instance;
pub mod libvirt;
pub mod logger;
pub mod looking_glass;
pub mod migration;
pub mod monitoring;
pub mod network;
pub mod pci_passthrough;
pub mod performance_monitor;
pub mod port_monitor;
pub mod prometheus;
pub mod rustdesk_integration;
pub mod spice_console;
pub mod sriov;
pub mod storage;
pub mod storage_pool;
pub mod support;
pub mod templates;
pub mod templates_snapshots;
pub mod theme;
pub mod usb_passthrough;
pub mod vm;
pub mod vm_enhanced;

pub use error::NovaError;
pub use instance::{Instance, InstanceStatus, InstanceType};

pub type Result<T> = std::result::Result<T, NovaError>;

// Convenience re-exports for networking components
pub use arch_integration::ArchNetworkManager;
pub use libvirt::{LibvirtManager, LibvirtNetwork};
pub use monitoring::{NetworkMonitor, NetworkTopology};
pub use network::{NetworkInterface, NetworkManager, NetworkSummary, VirtualSwitch};
pub use storage::StorageManager;
