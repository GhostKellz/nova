use clap::{Args, Parser, Subcommand, ValueEnum};
use nova::{
    NovaError, Result,
    config::NovaConfig,
    container::ContainerManager,
    gpu_doctor::GpuDoctor,
    gpu_passthrough::GpuManager,
    libvirt::LibvirtManager,
    logger,
    migration::{MigrationConfig, MigrationManager},
    network::{
        BridgeConfig, InterfaceState, NetworkManager, SwitchOrigin, SwitchProfile, SwitchStatus,
        SwitchType,
    },
    pci_passthrough::PciPassthroughManager,
    spice_console::{SpiceConfig, SpiceManager},
    sriov::SriovManager,
    storage_pool::{PoolType, StoragePoolManager, VolumeFormat},
    templates::TemplateManager,
    templates_snapshots::TemplateManager as SnapshotManager,
    usb_passthrough::UsbManager,
    vm::VmManager,
};
use std::io::{self, Write};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "nova")]
#[command(about = "Wayland-Native Virtualization & Container Manager")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to NovaFile configuration
    #[arg(short, long, default_value = "NovaFile")]
    config: PathBuf,

    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a VM or container
    Run {
        /// Type of instance (vm or container)
        #[arg(value_enum)]
        instance_type: InstanceType,
        /// Name of the instance
        name: String,
    },
    /// Stop a VM or container
    Stop {
        /// Type of instance (vm or container)
        #[arg(value_enum)]
        instance_type: InstanceType,
        /// Name of the instance
        name: String,
    },
    /// List all instances
    #[command(alias = "ls")]
    List,
    /// Show version information
    Version,
    /// Show status of a specific instance
    Status {
        /// Type of instance (vm or container)
        #[arg(value_enum)]
        instance_type: InstanceType,
        /// Name of the instance
        name: String,
    },
    /// Container template management
    Template {
        #[command(subcommand)]
        template_command: TemplateCommands,
    },
    /// Guided configuration wizards
    Wizard {
        #[command(subcommand)]
        wizard_command: WizardCommands,
    },
    /// Network and bridge management
    Network {
        #[command(subcommand)]
        network_command: NetworkCommands,
    },
    /// GPU passthrough management
    Gpu {
        #[command(subcommand)]
        gpu_command: GpuCommands,
    },
    /// Storage pool management
    Storage {
        #[command(subcommand)]
        storage_command: StorageCommands,
    },
    /// VM snapshot management
    Snapshot {
        #[command(subcommand)]
        snapshot_command: SnapshotCommands,
    },
    /// VM cloning operations
    Clone {
        /// Source VM name
        source: String,
        /// New VM name
        target: String,
        /// Create linked clone (saves disk space)
        #[arg(long)]
        linked: bool,
    },
    /// Live VM migration
    Migrate {
        /// VM name to migrate
        vm: String,
        /// Destination host
        destination: String,
        /// Force offline migration
        #[arg(long)]
        offline: bool,
    },
    /// USB passthrough management
    Usb {
        #[command(subcommand)]
        usb_command: UsbCommands,
    },
    /// PCI passthrough management
    Pci {
        #[command(subcommand)]
        pci_command: PciCommands,
    },
    /// SR-IOV management
    Sriov {
        #[command(subcommand)]
        sriov_command: SriovCommands,
    },
    /// SPICE console management
    Spice {
        #[command(subcommand)]
        spice_command: SpiceCommands,
    },
}

#[derive(Subcommand)]
enum WizardCommands {
    /// Generate a NovaFile VM entry from guided inputs
    Vm(WizardVmArgs),
}

#[derive(Args, Debug)]
struct WizardVmArgs {
    /// Name of the VM to generate (letters, numbers, '-', '_')
    name: String,
    /// Number of virtual CPUs to allocate
    #[arg(long, default_value_t = 4)]
    cpu: u32,
    /// Memory allocation (e.g. "8Gi")
    #[arg(long, default_value = "8Gi")]
    memory: String,
    /// Target network bridge (omit to choose interactively)
    #[arg(long)]
    network: Option<String>,
    /// Override the disk image path (defaults to /var/lib/nova/images/<name>.qcow2)
    #[arg(long)]
    image: Option<String>,
    /// Enable GPU passthrough
    #[arg(long)]
    gpu: bool,
    /// Start the VM automatically with Nova
    #[arg(long)]
    autostart: bool,
    /// Persist the generated entry to a NovaFile
    #[arg(long)]
    apply: bool,
    /// Alternate output file (defaults to --config/NovaFile)
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum TemplateCommands {
    /// List available container templates
    List {
        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,
    },
    /// Show template details
    Show {
        /// Template name
        name: String,
    },
    /// Deploy a template
    Deploy {
        /// Template name
        template: String,
        /// Project name
        #[arg(short, long, default_value = "nova-project")]
        project: String,
        /// Output file for generated NovaFile
        #[arg(short, long, default_value = "NovaFile")]
        output: String,
    },
}

#[derive(Subcommand)]
enum NetworkCommands {
    /// List known bridges and interfaces
    List,
    /// Show details for a bridge or interface
    Inspect {
        /// Bridge or interface name
        name: String,
    },
    /// Create a new virtual switch/bridge
    Create {
        /// Name of the bridge to create
        name: String,
        /// Switch type backend
        #[arg(value_enum, long = "type", default_value = "bridge")]
        switch_type: NetworkSwitchTypeArg,
        /// Profile to apply after creating the bridge
        #[arg(value_enum, long = "profile")]
        profile: Option<NetworkProfileArg>,
        /// Uplink interface for external or NAT profiles
        #[arg(long = "uplink", value_name = "IFACE")]
        uplink: Option<String>,
        /// Subnet in CIDR form for NAT profile (e.g. 192.168.200.1/24)
        #[arg(long = "subnet", value_name = "CIDR")]
        subnet: Option<String>,
        /// DHCP allocation range for NAT profile (format: start-end)
        #[arg(long = "dhcp-range", value_name = "START-END")]
        dhcp_range: Option<String>,
        /// Interfaces to attach after creation
        #[arg(long = "attach", value_name = "IFACE")]
        attach_interfaces: Vec<String>,
        /// Enable Spanning Tree Protocol after creation
        #[arg(long)]
        stp: bool,
    },
    /// Delete an existing virtual switch/bridge
    Delete {
        /// Name of the bridge to delete
        name: String,
    },
    /// Attach a host interface to a bridge
    Attach {
        /// Bridge name
        switch: String,
        /// Interface to attach
        interface: String,
    },
    /// Detach a host interface from a bridge
    Detach {
        /// Bridge name
        switch: String,
        /// Interface to detach
        interface: String,
    },
    /// Manage libvirt networks
    Libvirt {
        #[command(subcommand)]
        command: LibvirtNetworkCommands,
    },
}

#[derive(Subcommand)]
enum LibvirtNetworkCommands {
    /// List libvirt networks and their state
    List,
    /// Start a libvirt network
    Start {
        /// Network name
        name: String,
    },
    /// Stop a libvirt network
    Stop {
        /// Network name
        name: String,
    },
    /// Toggle autostart flag for a libvirt network
    Autostart {
        /// Network name
        name: String,
        /// Disable autostart instead of enabling
        #[arg(long)]
        disable: bool,
    },
    /// Show detailed XML definition for a network
    DumpXml {
        /// Network name
        name: String,
    },
}

#[derive(ValueEnum, Clone)]
enum NetworkSwitchTypeArg {
    Bridge,
    Ovs,
}

impl From<NetworkSwitchTypeArg> for SwitchType {
    fn from(value: NetworkSwitchTypeArg) -> Self {
        match value {
            NetworkSwitchTypeArg::Bridge => SwitchType::LinuxBridge,
            NetworkSwitchTypeArg::Ovs => SwitchType::OpenVSwitch,
        }
    }
}

#[derive(ValueEnum, Clone)]
enum NetworkProfileArg {
    Internal,
    External,
    Nat,
}

impl NetworkProfileArg {
    fn into_switch_profile(
        self,
        uplink: Option<String>,
        subnet: Option<String>,
        dhcp_range: Option<String>,
    ) -> Result<SwitchProfile> {
        match self {
            NetworkProfileArg::Internal => Ok(SwitchProfile::Internal),
            NetworkProfileArg::External => {
                let uplink = uplink.ok_or_else(|| {
                    NovaError::ConfigError(
                        "--uplink is required for the external profile".to_string(),
                    )
                })?;
                Ok(SwitchProfile::External { uplink })
            }
            NetworkProfileArg::Nat => {
                let uplink = uplink.ok_or_else(|| {
                    NovaError::ConfigError("--uplink is required for the NAT profile".to_string())
                })?;
                let subnet_cidr = subnet.ok_or_else(|| {
                    NovaError::ConfigError("--subnet is required for the NAT profile".to_string())
                })?;

                let (dhcp_range_start, dhcp_range_end) = if let Some(range) = dhcp_range {
                    let (start, end) = parse_cli_dhcp_range(&range)?;
                    (Some(start), Some(end))
                } else {
                    (None, None)
                };

                Ok(SwitchProfile::Nat {
                    uplink,
                    subnet_cidr,
                    dhcp_range_start,
                    dhcp_range_end,
                })
            }
        }
    }
}

#[derive(clap::ValueEnum, Clone)]
enum InstanceType {
    Vm,
    Container,
}

#[derive(Subcommand)]
enum GpuCommands {
    /// Run comprehensive GPU passthrough diagnostics
    Doctor,
    /// List all detected GPUs
    List,
    /// Show detailed GPU information
    Info {
        /// PCI address (e.g., 0000:01:00.0) or "all"
        device: String,
    },
    /// Bind a GPU to vfio-pci driver
    Bind {
        /// PCI address of GPU to bind
        device: String,
    },
    /// Release a GPU from vfio-pci
    Release {
        /// PCI address of GPU to release
        device: String,
    },
    /// Reserve a GPU for a VM
    Reserve {
        /// PCI address of GPU
        device: String,
        /// VM name
        vm_name: String,
    },
}

#[derive(Subcommand)]
enum StorageCommands {
    /// List all storage pools
    #[command(name = "list-pools")]
    ListPools,
    /// Create a new storage pool
    #[command(name = "create-pool")]
    CreatePool {
        /// Pool name
        name: String,
        /// Pool type
        #[arg(value_enum, long)]
        pool_type: StoragePoolTypeArg,
        /// Path for the pool
        #[arg(long)]
        path: PathBuf,
        /// Enable compression (btrfs only)
        #[arg(long)]
        compression: bool,
    },
    /// Delete a storage pool
    #[command(name = "delete-pool")]
    DeletePool {
        /// Pool name
        name: String,
        /// Delete all volumes in the pool
        #[arg(long)]
        delete_volumes: bool,
    },
    /// List volumes in a pool
    #[command(name = "list-volumes")]
    ListVolumes {
        /// Pool name
        pool: String,
    },
    /// Create a new volume
    #[command(name = "create-volume")]
    CreateVolume {
        /// Pool name
        pool: String,
        /// Volume name
        name: String,
        /// Size (e.g., 100G, 1T)
        size: String,
        /// Volume format
        #[arg(value_enum, long, default_value = "qcow2")]
        format: VolumeFormatArg,
    },
}

#[derive(ValueEnum, Clone)]
enum StoragePoolTypeArg {
    Dir,
    Btrfs,
    Nfs,
}

impl From<StoragePoolTypeArg> for PoolType {
    fn from(value: StoragePoolTypeArg) -> Self {
        match value {
            StoragePoolTypeArg::Dir => PoolType::Directory,
            StoragePoolTypeArg::Btrfs => PoolType::Btrfs,
            StoragePoolTypeArg::Nfs => PoolType::Nfs,
        }
    }
}

#[derive(ValueEnum, Clone)]
enum VolumeFormatArg {
    Raw,
    Qcow2,
}

impl From<VolumeFormatArg> for VolumeFormat {
    fn from(value: VolumeFormatArg) -> Self {
        match value {
            VolumeFormatArg::Raw => VolumeFormat::Raw,
            VolumeFormatArg::Qcow2 => VolumeFormat::Qcow2,
        }
    }
}

#[derive(Subcommand)]
enum SnapshotCommands {
    /// Create a new VM snapshot
    Create {
        /// VM name
        vm: String,
        /// Snapshot name
        name: String,
        /// Description
        #[arg(short, long, default_value = "")]
        description: String,
        /// Include memory state
        #[arg(long)]
        memory: bool,
    },
    /// List all snapshots for a VM
    List {
        /// VM name
        vm: String,
    },
    /// Revert VM to a snapshot
    Revert {
        /// VM name
        vm: String,
        /// Snapshot name
        snapshot: String,
    },
    /// Delete a snapshot
    Delete {
        /// VM name
        vm: String,
        /// Snapshot name
        snapshot: String,
        /// Delete child snapshots too
        #[arg(long)]
        children: bool,
    },
}

#[derive(Subcommand)]
enum UsbCommands {
    /// List available USB devices
    List,
    /// Attach USB device to VM
    Attach {
        /// VM name
        vm: String,
        /// Vendor ID (e.g., 046d)
        #[arg(long)]
        vendor: String,
        /// Product ID (e.g., c52b)
        #[arg(long)]
        product: String,
    },
    /// Detach USB device from VM
    Detach {
        /// VM name
        vm: String,
        /// Vendor ID
        #[arg(long)]
        vendor: String,
        /// Product ID
        #[arg(long)]
        product: String,
    },
}

#[derive(Subcommand)]
enum PciCommands {
    /// List all PCI devices
    List {
        /// Filter by device class
        #[arg(long)]
        class: Option<String>,
    },
    /// Show PCI device details
    Info {
        /// PCI address (e.g., 0000:01:00.0)
        device: String,
    },
    /// Attach PCI device to VM
    Attach {
        /// VM name
        vm: String,
        /// PCI address
        device: String,
    },
    /// Detach PCI device from VM
    Detach {
        /// PCI address
        device: String,
    },
    /// Check passthrough viability
    Check {
        /// PCI address
        device: String,
    },
}

#[derive(Subcommand)]
enum SriovCommands {
    /// List SR-IOV capable devices
    List,
    /// Enable SR-IOV on a device
    Enable {
        /// PCI address of Physical Function
        pf: String,
        /// Number of Virtual Functions to create
        #[arg(long)]
        num_vfs: u32,
    },
    /// Disable SR-IOV on a device
    Disable {
        /// PCI address of Physical Function
        pf: String,
    },
    /// Assign Virtual Function to VM
    Assign {
        /// PCI address of Physical Function
        pf: String,
        /// VF index
        #[arg(long)]
        vf: u32,
        /// VM name
        #[arg(long)]
        vm: String,
    },
    /// Release Virtual Function
    Release {
        /// VF PCI address
        vf_address: String,
    },
}

#[derive(Subcommand)]
enum SpiceCommands {
    /// Connect to VM via SPICE
    Connect {
        /// VM name
        vm: String,
    },
    /// Show SPICE connection info
    Info {
        /// VM name
        vm: String,
    },
    /// Configure SPICE for a VM
    Config {
        /// VM name
        vm: String,
        /// Enable/disable feature
        #[arg(long)]
        audio: Option<bool>,
        /// Enable/disable clipboard sharing
        #[arg(long)]
        clipboard: Option<bool>,
        /// Enable/disable USB redirection
        #[arg(long)]
        usb: Option<bool>,
        /// Number of monitors
        #[arg(long)]
        monitors: Option<u32>,
    },
}

fn parse_cli_dhcp_range(range: &str) -> Result<(Ipv4Addr, Ipv4Addr)> {
    let mut parts = range.split('-');
    let start = parts
        .next()
        .ok_or_else(|| NovaError::ConfigError("Invalid DHCP range".to_string()))?
        .trim()
        .parse::<Ipv4Addr>()
        .map_err(|_| NovaError::ConfigError("Invalid DHCP start address".to_string()))?;
    let end = parts
        .next()
        .ok_or_else(|| NovaError::ConfigError("Invalid DHCP range".to_string()))?
        .trim()
        .parse::<Ipv4Addr>()
        .map_err(|_| NovaError::ConfigError("Invalid DHCP end address".to_string()))?;

    if u32::from(start) > u32::from(end) {
        return Err(NovaError::ConfigError(
            "DHCP range start must be <= end".to_string(),
        ));
    }

    Ok((start, end))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    if cli.verbose {
        unsafe {
            std::env::set_var("RUST_LOG", "nova=debug");
        }
    }
    logger::init_logger();

    let config_path = cli.config.clone();

    // Load configuration
    let config = if config_path.exists() {
        NovaConfig::from_file(&config_path)?
    } else {
        logger::warn!(
            "NovaFile not found at {}, using defaults",
            config_path.display()
        );
        NovaConfig::default()
    };

    // Initialize managers
    let vm_manager = VmManager::new();
    let container_manager = ContainerManager::new();
    let template_manager = TemplateManager::new();

    match cli.command {
        Commands::Run {
            instance_type,
            name,
        } => match instance_type {
            InstanceType::Vm => {
                let vm_config = config.get_vm(&name);
                vm_manager.start_vm(&name, vm_config).await?;
                println!("VM '{}' started successfully", name);
            }
            InstanceType::Container => {
                let container_config = config.get_container(&name);
                container_manager
                    .start_container(&name, container_config)
                    .await?;
                println!("Container '{}' started successfully", name);
            }
        },
        Commands::Stop {
            instance_type,
            name,
        } => match instance_type {
            InstanceType::Vm => {
                vm_manager.stop_vm(&name).await?;
                println!("VM '{}' stopped successfully", name);
            }
            InstanceType::Container => {
                container_manager.stop_container(&name).await?;
                println!("Container '{}' stopped successfully", name);
            }
        },
        Commands::List => {
            let vms = vm_manager.list_vms();
            let containers = container_manager.list_containers_async().await;

            println!(
                "{:<20} {:<12} {:<12} {:<8} {:<12}",
                "NAME", "TYPE", "STATUS", "PID", "MEMORY"
            );
            println!("{}", "=".repeat(70));

            for vm in &vms {
                println!(
                    "{:<20} {:<12} {:<12} {:<8} {:<12}",
                    vm.name,
                    "VM",
                    format!("{:?}", vm.status),
                    vm.pid.map(|p| p.to_string()).unwrap_or("-".to_string()),
                    format!("{}MB", vm.memory_mb)
                );
            }

            for container in &containers {
                println!(
                    "{:<20} {:<12} {:<12} {:<8} {:<12}",
                    container.name,
                    "Container",
                    format!("{:?}", container.status),
                    container
                        .pid
                        .map(|p| p.to_string())
                        .unwrap_or("-".to_string()),
                    format!("{}MB", container.memory_mb)
                );
            }

            if vms.is_empty() && containers.is_empty() {
                println!("No instances running");
            }
        }
        Commands::Status {
            instance_type,
            name,
        } => match instance_type {
            InstanceType::Vm => {
                if let Some(vm) = vm_manager.get_vm(&name) {
                    println!("VM: {}", vm.name);
                    println!("Status: {:?}", vm.status);
                    println!("CPU Cores: {}", vm.cpu_cores);
                    println!("Memory: {}MB", vm.memory_mb);
                    println!("Created: {}", vm.created_at.format("%Y-%m-%d %H:%M:%S"));
                    if let Some(pid) = vm.pid {
                        println!("PID: {}", pid);
                    }
                    if let Some(network) = &vm.network {
                        println!("Network: {}", network);
                    }
                } else {
                    println!("VM '{}' not found", name);
                }
            }
            InstanceType::Container => {
                if let Some(container) = container_manager.get_container(&name) {
                    println!("Container: {}", container.name);
                    println!("Status: {:?}", container.status);
                    println!("Memory: {}MB", container.memory_mb);
                    println!(
                        "Created: {}",
                        container.created_at.format("%Y-%m-%d %H:%M:%S")
                    );
                    if let Some(pid) = container.pid {
                        println!("PID: {}", pid);
                    }
                    if let Some(network) = &container.network {
                        println!("Network: {}", network);
                    }
                } else {
                    println!("Container '{}' not found", name);
                }
            }
        },
        Commands::Version => {
            println!("Nova v0.1.0 - Wayland-Native Virtualization & Container Manager");
            println!("Built with Rust (version not available in this build)");

            // Check system capabilities
            println!(
                "
System Capabilities:"
            );
            println!("  KVM Available: {}", check_kvm_available());
            println!("  QEMU Available: {}", check_qemu_available());
            println!("  Libvirt Available: {}", vm_manager.check_libvirt());

            // Check container runtimes
            println!(
                "
Container Runtimes:"
            );
            let runtime = container_manager.check_container_runtime();
            println!("  Primary Runtime: {:?}", runtime);
            println!(
                "  Bolt Available: {}",
                container_manager.check_bolt_available()
            );
            println!(
                "  Docker Available: {}",
                container_manager.check_docker_available()
            );
            println!(
                "  Podman Available: {}",
                container_manager.check_podman_available()
            );

            // Show template availability
            println!(
                "
Container Templates:"
            );
            println!(
                "  Available Templates: {}",
                template_manager.get_templates().len()
            );
        }
        Commands::Wizard { wizard_command } => match wizard_command {
            WizardCommands::Vm(args) => {
                handle_vm_wizard(args, &config, &config_path)?;
            }
        },
        Commands::Template { template_command } => match template_command {
            TemplateCommands::List { category: _ } => {
                println!(
                    "Available Container Templates:
"
                );

                println!(
                    "{:<20} {:<15} {:<15} {:<10} {}",
                    "NAME", "CATEGORY", "DIFFICULTY", "GPU", "DESCRIPTION"
                );
                println!("{}", "=".repeat(100));

                for template in template_manager.get_templates() {
                    println!(
                        "{:<20} {:<15} {:<15} {:<10} {}",
                        template.name,
                        format!("{:?}", template.category),
                        format!("{:?}", template.difficulty),
                        if template.requires_gpu { "✅" } else { "❌" },
                        template.description
                    );
                }
            }
            TemplateCommands::Show { name } => {
                if let Some(template) = template_manager.get_template(&name) {
                    println!("Template: {}", template.name);
                    println!("Description: {}", template.description);
                    println!("Category: {:?}", template.category);
                    println!("Difficulty: {:?}", template.difficulty);
                    println!(
                        "Requires GPU: {}",
                        if template.requires_gpu { "Yes" } else { "No" }
                    );

                    if let Some(runtime) = &template.recommended_runtime {
                        println!("Recommended Runtime: {}", runtime);
                    }

                    println!(
                        "
Containers:"
                    );
                    for container in &template.containers {
                        println!("  - {}: {}", container.name, container.image);
                        if !container.ports.is_empty() {
                            println!("    Ports: {}", container.ports.join(", "));
                        }
                        if container.gpu_access {
                            println!("    GPU Access: Yes");
                        }
                    }

                    println!(
                        "
Networks:"
                    );
                    for network in &template.networks {
                        println!("  - {}: {} network", network.name, network.driver);
                    }

                    println!(
                        "
Volumes:"
                    );
                    for volume in &template.volumes {
                        println!("  - {}: {}", volume.name, volume.description);
                    }
                } else {
                    println!("Template '{}' not found", name);
                }
            }
            TemplateCommands::Deploy {
                template,
                project,
                output,
            } => match template_manager.deploy_template(&template, &project) {
                Ok(nova_file_content) => {
                    std::fs::write(&output, nova_file_content)?;
                    println!("✅ Template '{}' deployed successfully!", template);
                    println!("📄 NovaFile written to: {}", output);
                    println!("🚀 Run 'nova run container <name>' to start containers");
                }
                Err(e) => {
                    println!("❌ Failed to deploy template: {}", e);
                }
            },
        },
        Commands::Network { network_command } => match network_command {
            NetworkCommands::List => {
                let mut network_manager = NetworkManager::new();
                network_manager.refresh_state().await?;

                println!(
                    "{:<16} {:<10} {:<10} {:<10} {}",
                    "BRIDGE", "TYPE", "STATE", "ORIGIN", "MEMBERS"
                );
                println!("{}", "-".repeat(70));

                let mut switches = network_manager.list_switches();
                switches.sort_by(|a, b| a.name.cmp(&b.name));

                if switches.is_empty() {
                    println!("(no bridges detected)");
                } else {
                    for switch in switches {
                        let switch_type = match switch.switch_type {
                            SwitchType::LinuxBridge => "bridge",
                            SwitchType::OpenVSwitch => "ovs",
                        };

                        let status = match &switch.status {
                            SwitchStatus::Active => "active".to_string(),
                            SwitchStatus::Inactive => "inactive".to_string(),
                            SwitchStatus::Error(err) => format!("error: {}", err),
                        };

                        let origin = match switch.origin {
                            SwitchOrigin::Nova => "nova",
                            SwitchOrigin::System => "system",
                        };

                        let members = if switch.interfaces.is_empty() {
                            "-".to_string()
                        } else {
                            switch.interfaces.join(", ")
                        };

                        println!(
                            "{:<16} {:<10} {:<10} {:<10} {}",
                            switch.name, switch_type, status, origin, members
                        );
                    }
                }

                println!(
                    "\n{:<16} {:<8} {:<12} {:<18} {}",
                    "INTERFACE", "STATE", "BRIDGE", "IP", "MAC"
                );
                println!("{}", "-".repeat(80));

                let mut interfaces = network_manager.list_interfaces();
                interfaces.sort_by(|a, b| a.name.cmp(&b.name));

                if interfaces.is_empty() {
                    println!("(no interfaces detected)");
                } else {
                    for iface in interfaces {
                        let state = match iface.state {
                            InterfaceState::Up => "up",
                            InterfaceState::Down => "down",
                            InterfaceState::Unknown => "?",
                        };

                        let bridge = iface.bridge.as_ref().map(|b| b.as_str()).unwrap_or("-");

                        let ip = iface
                            .ip_address
                            .map(|ip| ip.to_string())
                            .unwrap_or_else(|| "-".to_string());

                        println!(
                            "{:<16} {:<8} {:<12} {:<18} {}",
                            iface.name, state, bridge, ip, iface.mac_address
                        );
                    }
                }
            }
            NetworkCommands::Inspect { name } => {
                let mut network_manager = NetworkManager::new();
                network_manager.refresh_state().await?;

                if let Some(switch) = network_manager.get_switch(&name) {
                    println!("Bridge: {}", switch.name);
                    println!("  Type: {:?}", switch.switch_type);
                    println!("  Status: {:?}", switch.status);
                    println!("  Origin: {:?}", switch.origin);
                    println!("  STP Enabled: {}", switch.stp_enabled);
                    match &switch.profile {
                        Some(profile) => println!("  Profile: {:?}", profile),
                        None => println!("  Profile: -"),
                    }
                    println!(
                        "  Interfaces: {}",
                        if switch.interfaces.is_empty() {
                            "-".to_string()
                        } else {
                            switch.interfaces.join(", ")
                        }
                    );
                } else if let Some(iface) = network_manager.get_interface(&name) {
                    println!("Interface: {}", iface.name);
                    println!("  State: {:?}", iface.state);
                    println!("  MAC: {}", iface.mac_address);
                    if let Some(ip) = iface.ip_address {
                        println!("  IPv4: {}", ip);
                    }
                    if let Some(bridge) = &iface.bridge {
                        println!("  Attached Bridge: {}", bridge);
                    }
                    if let Some(speed) = iface.speed {
                        println!("  Speed: {} Mbps", speed);
                    }
                } else {
                    println!("Network object '{}' not found", name);
                }
            }
            NetworkCommands::Create {
                name,
                switch_type,
                profile,
                uplink,
                subnet,
                dhcp_range,
                attach_interfaces,
                stp,
            } => {
                let mut network_manager = NetworkManager::new();
                let switch_type: SwitchType = switch_type.into();
                let profile_config = if let Some(profile_arg) = profile {
                    Some(profile_arg.into_switch_profile(
                        uplink.clone(),
                        subnet.clone(),
                        dhcp_range.clone(),
                    )?)
                } else {
                    None
                };
                let profile_clone = profile_config.clone();
                network_manager
                    .create_virtual_switch(&name, switch_type.clone(), profile_config)
                    .await?;

                if stp {
                    let config = BridgeConfig {
                        name: name.clone(),
                        stp: true,
                        forward_delay: 15,
                        hello_time: 2,
                        max_age: 20,
                        aging_time: 300,
                        multicast_snooping: true,
                    };
                    network_manager.configure_bridge(&config).await?;
                }

                let uplink_to_skip = profile_clone.as_ref().and_then(|profile| match profile {
                    SwitchProfile::External { uplink } => Some(uplink.clone()),
                    SwitchProfile::Internal | SwitchProfile::Nat { .. } => None,
                });

                for iface in attach_interfaces {
                    if uplink_to_skip.as_deref() == Some(iface.as_str()) {
                        continue;
                    }
                    network_manager
                        .add_interface_to_switch(&name, &iface)
                        .await?;
                }

                if let Some(profile) = profile_clone {
                    println!(
                        "Bridge '{}' ({:?}) created successfully with {:?} profile",
                        name, switch_type, profile
                    );
                } else {
                    println!("Bridge '{}' ({:?}) created successfully", name, switch_type);
                }
            }
            NetworkCommands::Delete { name } => {
                let mut network_manager = NetworkManager::new();
                network_manager.delete_virtual_switch(&name).await?;
                println!("Bridge '{}' deleted", name);
            }
            NetworkCommands::Attach { switch, interface } => {
                let mut network_manager = NetworkManager::new();
                network_manager.refresh_state().await?;

                if !network_manager.switch_exists(&switch) {
                    println!("Bridge '{}' not found", switch);
                } else {
                    network_manager
                        .add_interface_to_switch(&switch, &interface)
                        .await?;
                    println!("Attached interface '{}' to '{}'", interface, switch);
                }
            }
            NetworkCommands::Detach { switch, interface } => {
                let mut network_manager = NetworkManager::new();
                network_manager.refresh_state().await?;

                if !network_manager.switch_exists(&switch) {
                    println!("Bridge '{}' not found", switch);
                } else {
                    network_manager
                        .remove_interface_from_switch(&switch, &interface)
                        .await?;
                    println!("Detached interface '{}' from '{}'", interface, switch);
                }
            }
            NetworkCommands::Libvirt { command } => {
                let mut libvirt_manager = LibvirtManager::new();
                match command {
                    LibvirtNetworkCommands::List => {
                        libvirt_manager.discover_networks().await?;

                        println!(
                            "{:<20} {:<8} {:<10} {:<12} {:<16}",
                            "NAME", "ACTIVE", "AUTOSTART", "FORWARD", "SUBNET"
                        );
                        println!("{}", "-".repeat(70));

                        for network in libvirt_manager.list_networks() {
                            let forward = network
                                .forward
                                .as_ref()
                                .map(|f| f.mode.as_str())
                                .unwrap_or("none");
                            let subnet = network
                                .ip
                                .as_ref()
                                .map(|ip| format!("{}/{}", ip.address, ip.netmask))
                                .unwrap_or_else(|| "-".to_string());

                            println!(
                                "{:<20} {:<8} {:<10} {:<12} {:<16}",
                                network.name,
                                if network.active { "yes" } else { "no" },
                                if network.autostart { "yes" } else { "no" },
                                forward,
                                subnet
                            );
                        }
                    }
                    LibvirtNetworkCommands::Start { name } => {
                        libvirt_manager.start_network(&name).await?;
                        println!("Libvirt network '{}' started", name);
                    }
                    LibvirtNetworkCommands::Stop { name } => {
                        libvirt_manager.stop_network(&name).await?;
                        println!("Libvirt network '{}' stopped", name);
                    }
                    LibvirtNetworkCommands::Autostart { name, disable } => {
                        libvirt_manager
                            .set_network_autostart(&name, !disable)
                            .await?;
                        println!(
                            "Libvirt network '{}' autostart {}",
                            name,
                            if disable { "disabled" } else { "enabled" }
                        );
                    }
                    LibvirtNetworkCommands::DumpXml { name } => {
                        let output = Command::new("virsh")
                            .args(["net-dumpxml", &name])
                            .output()
                            .map_err(|_| nova::NovaError::SystemCommandFailed)?;

                        if output.status.success() {
                            println!("{}", String::from_utf8_lossy(&output.stdout));
                        } else {
                            println!(
                                "Failed to dump network XML: {}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                        }
                    }
                }
            }
        },
        Commands::Gpu { gpu_command } => match gpu_command {
            GpuCommands::Doctor => {
                let doctor = GpuDoctor::new();
                let report = doctor.diagnose();
                doctor.print_report(&report);

                // Exit with error code if system not ready
                if report.overall_status != nova::gpu_doctor::SystemStatus::Ready {
                    std::process::exit(1);
                }
            }
            GpuCommands::List => {
                let mut gpu_manager = GpuManager::new();
                gpu_manager.discover()?;

                let gpus = gpu_manager.list_gpus();

                if gpus.is_empty() {
                    println!("No GPUs detected");
                    return Ok(());
                }

                println!("{:<18} {:<30} {:<12} {:<15}", "PCI ADDRESS", "GPU MODEL", "IOMMU GROUP", "DRIVER");
                println!("{}", "=".repeat(80));

                for gpu in gpus {
                    let iommu = gpu.iommu_group.map(|g| g.to_string()).unwrap_or_else(|| "-".to_string());
                    let driver = gpu.driver.as_deref().unwrap_or("-");

                    println!(
                        "{:<18} {:<30} {:<12} {:<15}",
                        gpu.address,
                        gpu.device_name,
                        iommu,
                        driver
                    );
                }

                // Show reservations
                let reservations = gpu_manager.get_reservations();
                if !reservations.is_empty() {
                    println!("\nReserved GPUs:");
                    for (device, vm) in reservations {
                        println!("  {} → {}", device, vm);
                    }
                }
            }
            GpuCommands::Info { device } => {
                let mut gpu_manager = GpuManager::new();
                gpu_manager.discover()?;

                if device == "all" {
                    for gpu in gpu_manager.list_gpus() {
                        print_gpu_info(gpu);
                        println!();
                    }
                } else {
                    if let Some(gpu) = gpu_manager.list_gpus().iter().find(|g| g.address == device) {
                        print_gpu_info(gpu);
                    } else {
                        println!("GPU '{}' not found", device);
                    }
                }
            }
            GpuCommands::Bind { device } => {
                let mut gpu_manager = GpuManager::new();
                gpu_manager.discover()?;

                gpu_manager.configure_passthrough(&device, "manual")?;
                println!("✅ GPU {} bound to vfio-pci", device);
            }
            GpuCommands::Release { device } => {
                let mut gpu_manager = GpuManager::new();
                gpu_manager.release_gpu(&device)?;
                println!("✅ GPU {} released from vfio-pci", device);
            }
            GpuCommands::Reserve { device, vm_name } => {
                let mut gpu_manager = GpuManager::new();
                gpu_manager.discover()?;
                gpu_manager.configure_passthrough(&device, &vm_name)?;
                println!("✅ GPU {} reserved for VM '{}'", device, vm_name);
            }
        },
        Commands::Storage { storage_command } => match storage_command {
            StorageCommands::ListPools => {
                let mut storage_manager = StoragePoolManager::new();
                storage_manager.discover_pools().await?;

                let pools = storage_manager.list_pools();

                if pools.is_empty() {
                    println!("No storage pools found");
                    return Ok(());
                }

                println!("{:<20} {:<12} {:<12} {:<15} {:<15}", "NAME", "TYPE", "STATE", "CAPACITY", "USAGE");
                println!("{}", "=".repeat(80));

                for pool in pools {
                    let pool_type = format!("{:?}", pool.pool_type);
                    let state = format!("{:?}", pool.state);

                    let (capacity, usage) = if let Some(cap) = &pool.capacity {
                        let total_gb = cap.total_bytes as f64 / 1_073_741_824.0;
                        let usage_pct = cap.usage_percent();
                        (format!("{:.1} GB", total_gb), format!("{:.1}%", usage_pct))
                    } else {
                        ("-".to_string(), "-".to_string())
                    };

                    println!(
                        "{:<20} {:<12} {:<12} {:<15} {:<15}",
                        pool.name,
                        pool_type,
                        state,
                        capacity,
                        usage
                    );
                }
            }
            StorageCommands::CreatePool { name, pool_type, path, compression } => {
                use nova::storage_pool::{BtrfsCompression, PoolConfig, StoragePool};

                let pool_type: PoolType = pool_type.into();

                let config = if pool_type == PoolType::Btrfs && compression {
                    PoolConfig::Btrfs {
                        mount_point: path.clone(),
                        subvolume: Some(name.clone()),
                        compression: BtrfsCompression::Zstd { level: 3 },
                        quota_enabled: false,
                    }
                } else {
                    PoolConfig::Directory { permissions: 0o755 }
                };

                let pool = StoragePool {
                    name: name.clone(),
                    pool_type,
                    path,
                    state: nova::storage_pool::PoolState::Building,
                    capacity: None,
                    autostart: true,
                    config,
                    uuid: uuid::Uuid::new_v4().to_string(),
                    created_at: chrono::Utc::now(),
                };

                let mut storage_manager = StoragePoolManager::new();
                storage_manager.create_pool(pool).await?;

                println!("✅ Storage pool '{}' created successfully", name);
            }
            StorageCommands::DeletePool { name, delete_volumes } => {
                let mut storage_manager = StoragePoolManager::new();
                storage_manager.delete_pool(&name, delete_volumes).await?;
                println!("✅ Storage pool '{}' deleted", name);
            }
            StorageCommands::ListVolumes { pool } => {
                let storage_manager = StoragePoolManager::new();
                let volumes = storage_manager.list_volumes(&pool);

                if volumes.is_empty() {
                    println!("No volumes in pool '{}'", pool);
                    return Ok(());
                }

                println!("{:<20} {:<12} {:<15} {:<15}", "NAME", "FORMAT", "CAPACITY", "ALLOCATION");
                println!("{}", "=".repeat(70));

                for volume in volumes {
                    let capacity_gb = volume.capacity_bytes as f64 / 1_073_741_824.0;
                    let alloc_gb = volume.allocation_bytes as f64 / 1_073_741_824.0;

                    println!(
                        "{:<20} {:<12} {:<15} {:<15}",
                        volume.name,
                        format!("{:?}", volume.format),
                        format!("{:.1} GB", capacity_gb),
                        format!("{:.1} GB", alloc_gb)
                    );
                }
            }
            StorageCommands::CreateVolume { pool, name, size, format } => {
                let size_bytes = parse_size(&size)?;
                let format: VolumeFormat = format.into();

                let mut storage_manager = StoragePoolManager::new();
                storage_manager.create_volume(&pool, &name, size_bytes, format).await?;

                println!("✅ Volume '{}' created in pool '{}'", name, pool);
            }
        },
        Commands::Snapshot { snapshot_command } => {
            let templates_dir = PathBuf::from("/var/lib/nova/templates");
            let mut snapshot_manager = SnapshotManager::new(templates_dir)?;

            match snapshot_command {
                SnapshotCommands::Create { vm, name, description, memory } => {
                    let snapshot_id = snapshot_manager.create_snapshot(&vm, &name, &description, memory).await?;
                    println!("✅ Snapshot '{}' created with ID: {}", name, snapshot_id);
                }
                SnapshotCommands::List { vm } => {
                    let snapshots = snapshot_manager.list_snapshots_chronological(&vm);

                    if snapshots.is_empty() {
                        println!("No snapshots found for VM '{}'", vm);
                        return Ok(());
                    }

                    println!("{:<20} {:<30} {:<12} {:<10}", "NAME", "CREATED", "SIZE", "CURRENT");
                    println!("{}", "=".repeat(80));

                    for snapshot in snapshots {
                        let size_mb = snapshot.size_bytes as f64 / 1_048_576.0;
                        let current = if snapshot.is_current { "✓" } else { "" };

                        println!(
                            "{:<20} {:<30} {:<12} {:<10}",
                            snapshot.name,
                            snapshot.created_at.format("%Y-%m-%d %H:%M:%S"),
                            format!("{:.1} MB", size_mb),
                            current
                        );
                    }

                    let total_size = snapshot_manager.get_total_snapshot_size(&vm);
                    println!("\nTotal snapshot storage: {:.1} MB", total_size as f64 / 1_048_576.0);
                }
                SnapshotCommands::Revert { vm, snapshot } => {
                    snapshot_manager.revert_to_snapshot(&vm, &snapshot).await?;
                    println!("✅ VM '{}' reverted to snapshot '{}'", vm, snapshot);
                }
                SnapshotCommands::Delete { vm, snapshot, children } => {
                    snapshot_manager.delete_snapshot(&vm, &snapshot, children).await?;
                    println!("✅ Snapshot '{}' deleted", snapshot);
                }
            }
        },
        Commands::Clone { source, target, linked } => {
            let templates_dir = PathBuf::from("/var/lib/nova/templates");
            let mut snapshot_manager = SnapshotManager::new(templates_dir)?;

            if linked {
                snapshot_manager.create_linked_clone(&source, &target).await?;
                println!("✅ Linked clone '{}' created from '{}'", target, source);
            } else {
                snapshot_manager.clone_vm(&source, &target, true).await?;
                println!("✅ VM '{}' cloned to '{}'", source, target);
            }
        },
        Commands::Migrate { vm, destination, offline } => {
            let config = MigrationConfig::default();
            let mut migration_manager = MigrationManager::new(config, None);

            let migration_type = if offline {
                Some(nova::migration::MigrationType::Offline)
            } else {
                None
            };

            let job_id = migration_manager.migrate_vm(&vm, &destination, migration_type).await?;
            println!("✅ Migration started (Job ID: {})", job_id);
            println!("Monitor progress with: nova migration status {}", job_id);
        },
        Commands::Usb { usb_command } => {
            let mut usb_manager = UsbManager::new();

            match usb_command {
                UsbCommands::List => {
                    let devices = usb_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;

                    if devices.is_empty() {
                        println!("No USB devices detected");
                        return Ok(());
                    }

                    println!("{:<10} {:<20} {:<30} {:<10}", "BUS:DEV", "VENDOR:PRODUCT", "DEVICE", "STATUS");
                    println!("{}", "=".repeat(75));

                    for device in devices {
                        let id = format!("{}:{}", device.bus, device.device);
                        let ids = format!("{}:{}", device.vendor_id, device.product_id);
                        let status = device.attached_to_vm.as_deref().unwrap_or("Available");

                        println!(
                            "{:<10} {:<20} {:<30} {:<10}",
                            id,
                            ids,
                            device.product_name,
                            status
                        );
                    }
                }
                UsbCommands::Attach { vm, vendor, product } => {
                    usb_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;

                    if let Some(device) = usb_manager.find_device(&vendor, &product).cloned() {
                        usb_manager.attach_device(&vm, &device).await.map_err(|e| NovaError::LibvirtError(e))?;
                        println!("✅ USB device attached to VM '{}'", vm);
                    } else {
                        println!("❌ USB device {}:{} not found", vendor, product);
                    }
                }
                UsbCommands::Detach { vm, vendor, product } => {
                    usb_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;

                    if let Some(device) = usb_manager.find_device(&vendor, &product).cloned() {
                        usb_manager.detach_device(&vm, &device).await.map_err(|e| NovaError::LibvirtError(e))?;
                        println!("✅ USB device detached from VM '{}'", vm);
                    } else {
                        println!("❌ USB device {}:{} not found", vendor, product);
                    }
                }
            }
        },
        Commands::Pci { pci_command } => {
            let mut pci_manager = PciPassthroughManager::new();

            match pci_command {
                PciCommands::List { class: _ } => {
                    let devices = pci_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;

                    if devices.is_empty() {
                        println!("No PCI devices detected");
                        return Ok(());
                    }

                    println!("{:<18} {:<30} {:<15} {:<12}", "PCI ADDRESS", "DEVICE", "CLASS", "DRIVER");
                    println!("{}", "=".repeat(80));

                    for device in devices {
                        let driver = device.driver.as_deref().unwrap_or("-");

                        println!(
                            "{:<18} {:<30} {:<15} {:<12}",
                            device.address,
                            device.device_name,
                            format!("{:?}", device.device_class),
                            driver
                        );
                    }
                }
                PciCommands::Info { device } => {
                    pci_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;

                    if let Some(dev) = pci_manager.get_device(&device) {
                        nova::pci_passthrough::PciPassthroughManager::print_device_info(dev);
                    } else {
                        println!("❌ PCI device '{}' not found", device);
                    }
                }
                PciCommands::Attach { vm, device } => {
                    pci_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;
                    pci_manager.assign_to_vm(&device, &vm).map_err(|e| NovaError::LibvirtError(e))?;
                    println!("✅ PCI device {} assigned to VM '{}'", device, vm);
                }
                PciCommands::Detach { device } => {
                    pci_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;
                    pci_manager.release_from_vm(&device).map_err(|e| NovaError::ConfigError(e))?;
                    println!("✅ PCI device {} released", device);
                }
                PciCommands::Check { device } => {
                    pci_manager.discover_devices().map_err(|e| NovaError::ConfigError(e))?;
                    let viability = pci_manager.check_passthrough_viability(&device).map_err(|e| NovaError::ConfigError(e))?;
                    viability.print();
                }
            }
        },
        Commands::Sriov { sriov_command } => {
            let mut sriov_manager = SriovManager::new();

            match sriov_command {
                SriovCommands::List => {
                    let devices = sriov_manager.discover_sriov_devices().map_err(|e| NovaError::ConfigError(e))?;

                    if devices.is_empty() {
                        println!("No SR-IOV capable devices found");
                        return Ok(());
                    }

                    println!("{:<18} {:<30} {:<8} {:<8}", "PCI ADDRESS", "DEVICE", "MAX VFs", "ACTIVE VFs");
                    println!("{}", "=".repeat(70));

                    for device in devices {
                        println!(
                            "{:<18} {:<30} {:<8} {:<8}",
                            device.pf_address,
                            device.device_name,
                            device.max_vfs,
                            device.current_vfs
                        );
                    }
                }
                SriovCommands::Enable { pf, num_vfs } => {
                    sriov_manager.discover_sriov_devices().map_err(|e| NovaError::ConfigError(e))?;
                    sriov_manager.enable_sriov(&pf, num_vfs).map_err(|e| NovaError::ConfigError(e))?;
                    println!("✅ SR-IOV enabled on {} with {} VFs", pf, num_vfs);
                }
                SriovCommands::Disable { pf } => {
                    sriov_manager.disable_sriov(&pf).map_err(|e| NovaError::ConfigError(e))?;
                    println!("✅ SR-IOV disabled on {}", pf);
                }
                SriovCommands::Assign { pf, vf, vm } => {
                    sriov_manager.discover_sriov_devices().map_err(|e| NovaError::ConfigError(e))?;
                    let vf_address = sriov_manager.assign_vf_to_vm(&pf, vf, &vm).map_err(|e| NovaError::ConfigError(e))?;
                    println!("✅ VF {} assigned to VM '{}'", vf_address, vm);
                }
                SriovCommands::Release { vf_address } => {
                    sriov_manager.release_vf(&vf_address).map_err(|e| NovaError::ConfigError(e))?;
                    println!("✅ VF {} released", vf_address);
                }
            }
        },
        Commands::Spice { spice_command } => {
            let mut spice_manager = SpiceManager::new();

            match spice_command {
                SpiceCommands::Connect { vm } => {
                    let _info = spice_manager.get_connection_info(&vm).await.map_err(|e| NovaError::LibvirtError(e))?;
                    spice_manager.launch_client(&vm).await.map_err(|e| NovaError::LibvirtError(e))?;
                    println!("✅ SPICE client launched for VM '{}'", vm);
                }
                SpiceCommands::Info { vm } => {
                    let info = spice_manager.get_connection_info(&vm).await.map_err(|e| NovaError::LibvirtError(e))?;

                    println!("SPICE Connection Info for VM '{}':", vm);
                    println!("  URI: {}", info.uri);
                    println!("  Host: {}", info.host);
                    println!("  Port: {}", info.port);
                    if let Some(tls_port) = info.tls_port {
                        println!("  TLS Port: {}", tls_port);
                    }
                    if info.password.is_some() {
                        println!("  Password: Set");
                    }
                }
                SpiceCommands::Config { vm, audio, clipboard, usb, monitors } => {
                    let mut config = spice_manager.get_config(&vm)
                        .cloned()
                        .unwrap_or_else(|| SpiceConfig::default());

                    if let Some(enabled) = audio {
                        config.audio = enabled;
                    }
                    if let Some(enabled) = clipboard {
                        config.clipboard_sharing = enabled;
                    }
                    if let Some(enabled) = usb {
                        config.usb_redirection = enabled;
                    }
                    if let Some(count) = monitors {
                        config.monitors = count;
                    }

                    spice_manager.set_config(&vm, config);
                    spice_manager.apply_config(&vm).await.map_err(|e| NovaError::LibvirtError(e))?;
                    println!("✅ SPICE configuration updated for VM '{}'", vm);
                }
            }
        },
    }

    Ok(())
}

fn print_gpu_info(gpu: &nova::gpu_passthrough::PciDevice) {
    println!("GPU: {}", gpu.device_name);
    println!("  PCI Address: {}", gpu.address);
    println!("  Vendor ID: {}", gpu.vendor_id);
    println!("  Device ID: {}", gpu.device_id);
    println!("  Vendor: {}", gpu.vendor_name);

    if let Some(group) = gpu.iommu_group {
        println!("  IOMMU Group: {}", group);
    } else {
        println!("  IOMMU Group: Not available");
    }

    if let Some(driver) = &gpu.driver {
        println!("  Current Driver: {}", driver);
    } else {
        println!("  Current Driver: None");
    }

    println!("  In Use: {}", if gpu.in_use { "Yes" } else { "No" });
}

fn parse_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();

    // Extract number and unit
    let (num_str, unit) = if size_str.ends_with("TB") || size_str.ends_with("TIB") {
        (&size_str[..size_str.len() - if size_str.ends_with("TB") { 2 } else { 3 }], 1_099_511_627_776u64)
    } else if size_str.ends_with("GB") || size_str.ends_with("GIB") {
        (&size_str[..size_str.len() - if size_str.ends_with("GB") { 2 } else { 3 }], 1_073_741_824u64)
    } else if size_str.ends_with("MB") || size_str.ends_with("MIB") {
        (&size_str[..size_str.len() - if size_str.ends_with("MB") { 2 } else { 3 }], 1_048_576u64)
    } else if size_str.ends_with('T') {
        (&size_str[..size_str.len() - 1], 1_099_511_627_776u64)
    } else if size_str.ends_with('G') {
        (&size_str[..size_str.len() - 1], 1_073_741_824u64)
    } else if size_str.ends_with('M') {
        (&size_str[..size_str.len() - 1], 1_048_576u64)
    } else {
        (size_str.as_str(), 1u64)
    };

    let num: f64 = num_str.parse().map_err(|_| NovaError::ConfigError(format!("Invalid size: {}", size_str)))?;

    Ok((num * unit as f64) as u64)
}

fn handle_vm_wizard(
    mut args: WizardVmArgs,
    config: &NovaConfig,
    default_output: &PathBuf,
) -> Result<()> {
    ensure_valid_vm_name(&args.name)?;

    let selected_network = resolve_wizard_network(&args.name, args.network.clone(), config)?;
    args.network = Some(selected_network.clone());

    let snippet = build_vm_wizard_snippet(&args, &selected_network);

    if !args.apply {
        println!("# NovaFile snippet (dry-run)\n");
        println!("{}", snippet.trim_end());
        let guidance = match &args.output {
            Some(path) => format!(" --output {}", path.display()),
            None => format!(" (default writes to {})", default_output.display()),
        };
        println!(
            "\nRun again with --apply{} to persist this entry.",
            guidance
        );
        return Ok(());
    }

    let target_path = args
        .output
        .clone()
        .unwrap_or_else(|| default_output.clone());

    let existing_content = if target_path.exists() {
        Some(std::fs::read_to_string(&target_path)?)
    } else {
        None
    };

    if let Some(content) = &existing_content {
        if content.contains(&format!("[vm.{}]", args.name)) {
            println!(
                "❌ VM '{}' already exists in {}. Remove it first or update manually.",
                args.name,
                target_path.display()
            );
            return Ok(());
        }
    } else if target_path == *default_output && config.vm.contains_key(&args.name) {
        println!(
            "❌ VM '{}' already exists in {}. Remove it first or update manually.",
            args.name,
            target_path.display()
        );
        return Ok(());
    }

    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut new_content = existing_content.unwrap_or_default();
    if !new_content.trim_end().is_empty() {
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push('\n');
    }

    new_content.push_str(&snippet);
    if !snippet.ends_with('\n') {
        new_content.push('\n');
    }

    std::fs::write(&target_path, new_content)?;

    println!("✅ Added VM '{}' to {}", args.name, target_path.display());

    Ok(())
}

fn resolve_wizard_network(
    vm_name: &str,
    explicit: Option<String>,
    config: &NovaConfig,
) -> Result<String> {
    if let Some(network) = explicit {
        if config.network.is_empty() {
            return Ok(network);
        }

        if !config.network.contains_key(&network) {
            println!(
                "⚠️  Network '{}' isn't defined in the current NovaFile, continuing with it anyway.",
                network
            );
        }
        return Ok(network);
    }

    let mut networks: Vec<String> = config.network.keys().cloned().collect();
    networks.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    if networks.is_empty() {
        println!(
            "ℹ️  No networks are defined in the NovaFile; defaulting VM '{}' to 'bridge0'.",
            vm_name
        );
        return Ok("bridge0".to_string());
    }

    if networks.len() == 1 {
        println!(
            "ℹ️  Using the only configured network '{}' for VM '{}'.",
            networks[0], vm_name
        );
        return Ok(networks.remove(0));
    }

    println!("Select a network for VM '{}':", vm_name);
    for (idx, name) in networks.iter().enumerate() {
        println!("  {}) {}", idx + 1, name);
    }

    loop {
        print!(
            "Enter choice [1-{}] (press Enter for {} or type a name): ",
            networks.len(),
            networks[0]
        );
        io::stdout().flush().ok();

        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|err| {
            NovaError::ConfigError(format!("Failed to read network selection: {}", err))
        })?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            let choice = networks[0].clone();
            println!("➡️  Using network '{}'.", choice);
            return Ok(choice);
        }

        if let Ok(index) = trimmed.parse::<usize>() {
            if (1..=networks.len()).contains(&index) {
                let choice = networks[index - 1].clone();
                println!("➡️  Using network '{}'.", choice);
                return Ok(choice);
            }
        }

        if let Some(choice) = networks
            .iter()
            .find(|name| name.eq_ignore_ascii_case(trimmed))
        {
            println!("➡️  Using network '{}'.", choice);
            return Ok(choice.clone());
        }

        println!(
            "⚠️  '{}' is not a valid selection. Please enter a number between 1 and {} or a network name.",
            trimmed,
            networks.len()
        );
    }
}

fn build_vm_wizard_snippet(args: &WizardVmArgs, network: &str) -> String {
    let image_path = args
        .image
        .clone()
        .unwrap_or_else(|| format!("/var/lib/nova/images/{}.qcow2", args.name));

    let mut snippet = String::new();
    snippet.push_str("# Generated with `nova wizard vm`\n");
    snippet.push_str(&format!("[vm.{}]\n", args.name));
    snippet.push_str(&format!("image = \"{}\"\n", image_path));
    snippet.push_str(&format!("cpu = {}\n", args.cpu));
    snippet.push_str(&format!("memory = \"{}\"\n", args.memory));
    snippet.push_str(&format!(
        "gpu_passthrough = {}\n",
        if args.gpu { "true" } else { "false" }
    ));
    snippet.push_str(&format!("network = \"{}\"\n", network));
    snippet.push_str(&format!(
        "autostart = {}\n",
        if args.autostart { "true" } else { "false" }
    ));
    snippet.push('\n');
    snippet
}

fn ensure_valid_vm_name(name: &str) -> Result<()> {
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');

    if !valid {
        return Err(NovaError::ConfigError(format!(
            "VM name '{}' contains unsupported characters. Use letters, numbers, '-' or '_'.",
            name
        )));
    }

    Ok(())
}

fn check_kvm_available() -> bool {
    std::path::Path::new("/dev/kvm").exists()
}

fn check_qemu_available() -> bool {
    std::process::Command::new("qemu-system-x86_64")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
