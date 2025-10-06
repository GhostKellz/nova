use clap::{Args, Parser, Subcommand, ValueEnum};
use nova::{
    NovaError, Result,
    config::NovaConfig,
    container::ContainerManager,
    libvirt::LibvirtManager,
    logger,
    network::{
        BridgeConfig, InterfaceState, NetworkManager, SwitchOrigin, SwitchProfile, SwitchStatus,
        SwitchType,
    },
    templates::TemplateManager,
    vm::VmManager,
};
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
    /// Target network bridge
    #[arg(long, default_value = "bridge0")]
    network: String,
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
            let containers = container_manager.list_containers();

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
    }

    Ok(())
}

fn handle_vm_wizard(
    args: WizardVmArgs,
    config: &NovaConfig,
    default_output: &PathBuf,
) -> Result<()> {
    ensure_valid_vm_name(&args.name)?;

    let snippet = build_vm_wizard_snippet(&args);

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

fn build_vm_wizard_snippet(args: &WizardVmArgs) -> String {
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
    snippet.push_str(&format!("network = \"{}\"\n", args.network));
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
