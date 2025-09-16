use clap::{Parser, Subcommand};
use nova::{
    config::NovaConfig,
    container::ContainerManager,
    logger,
    templates::TemplateManager,
    vm::VmManager,
    Result,
};
use std::path::PathBuf;

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

#[derive(clap::ValueEnum, Clone)]
enum InstanceType {
    Vm,
    Container,
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

    // Load configuration
    let config = if cli.config.exists() {
        NovaConfig::from_file(&cli.config)?
    } else {
        logger::warn!("NovaFile not found at {}, using defaults", cli.config.display());
        NovaConfig::default()
    };

    // Initialize managers
    let vm_manager = VmManager::new();
    let container_manager = ContainerManager::new();
    let template_manager = TemplateManager::new();

    match cli.command {
        Commands::Run { instance_type, name } => {
            match instance_type {
                InstanceType::Vm => {
                    let vm_config = config.get_vm(&name);
                    vm_manager.start_vm(&name, vm_config).await?;
                    println!("VM '{}' started successfully", name);
                }
                InstanceType::Container => {
                    let container_config = config.get_container(&name);
                    container_manager.start_container(&name, container_config).await?;
                    println!("Container '{}' started successfully", name);
                }
            }
        }
        Commands::Stop { instance_type, name } => {
            match instance_type {
                InstanceType::Vm => {
                    vm_manager.stop_vm(&name).await?;
                    println!("VM '{}' stopped successfully", name);
                }
                InstanceType::Container => {
                    container_manager.stop_container(&name).await?;
                    println!("Container '{}' stopped successfully", name);
                }
            }
        }
        Commands::List => {
            let vms = vm_manager.list_vms();
            let containers = container_manager.list_containers();

            println!("{:<20} {:<12} {:<12} {:<8} {:<12}", "NAME", "TYPE", "STATUS", "PID", "MEMORY");
            println!("{}", "=".repeat(70));

            for vm in &vms {
                println!("{:<20} {:<12} {:<12} {:<8} {:<12}",
                    vm.name,
                    "VM",
                    format!("{:?}", vm.status),
                    vm.pid.map(|p| p.to_string()).unwrap_or("-".to_string()),
                    format!("{}MB", vm.memory_mb)
                );
            }

            for container in &containers {
                println!("{:<20} {:<12} {:<12} {:<8} {:<12}",
                    container.name,
                    "Container",
                    format!("{:?}", container.status),
                    container.pid.map(|p| p.to_string()).unwrap_or("-".to_string()),
                    format!("{}MB", container.memory_mb)
                );
            }

            if vms.is_empty() && containers.is_empty() {
                println!("No instances running");
            }
        }
        Commands::Status { instance_type, name } => {
            match instance_type {
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
                        println!("Created: {}", container.created_at.format("%Y-%m-%d %H:%M:%S"));
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
            }
        }
        Commands::Version => {
            println!("Nova v0.1.0 - Wayland-Native Virtualization & Container Manager");
            println!("Built with Rust (version not available in this build)");

            // Check system capabilities
            println!("\nSystem Capabilities:");
            println!("  KVM Available: {}", check_kvm_available());
            println!("  QEMU Available: {}", check_qemu_available());
            println!("  Libvirt Available: {}", vm_manager.check_libvirt());

            // Check container runtimes
            println!("\nContainer Runtimes:");
            let runtime = container_manager.check_container_runtime();
            println!("  Primary Runtime: {:?}", runtime);
            println!("  Bolt Available: {}", container_manager.check_bolt_available());
            println!("  Docker Available: {}", container_manager.check_docker_available());
            println!("  Podman Available: {}", container_manager.check_podman_available());

            // Show template availability
            println!("\nContainer Templates:");
            println!("  Available Templates: {}", template_manager.get_templates().len());
        }
        Commands::Template { template_command } => {
            match template_command {
                TemplateCommands::List { category: _ } => {
                    println!("Available Container Templates:\n");

                    println!("{:<20} {:<15} {:<15} {:<10} {}", "NAME", "CATEGORY", "DIFFICULTY", "GPU", "DESCRIPTION");
                    println!("{}", "=".repeat(100));

                    for template in template_manager.get_templates() {
                        println!("{:<20} {:<15} {:<15} {:<10} {}",
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
                        println!("Requires GPU: {}", if template.requires_gpu { "Yes" } else { "No" });

                        if let Some(runtime) = &template.recommended_runtime {
                            println!("Recommended Runtime: {}", runtime);
                        }

                        println!("\nContainers:");
                        for container in &template.containers {
                            println!("  - {}: {}", container.name, container.image);
                            if !container.ports.is_empty() {
                                println!("    Ports: {}", container.ports.join(", "));
                            }
                            if container.gpu_access {
                                println!("    GPU Access: Yes");
                            }
                        }

                        println!("\nNetworks:");
                        for network in &template.networks {
                            println!("  - {}: {} network", network.name, network.driver);
                        }

                        println!("\nVolumes:");
                        for volume in &template.volumes {
                            println!("  - {}: {}", volume.name, volume.description);
                        }
                    } else {
                        println!("Template '{}' not found", name);
                    }
                }
                TemplateCommands::Deploy { template, project, output } => {
                    match template_manager.deploy_template(&template, &project) {
                        Ok(nova_file_content) => {
                            std::fs::write(&output, nova_file_content)?;
                            println!("✅ Template '{}' deployed successfully!", template);
                            println!("📄 NovaFile written to: {}", output);
                            println!("🚀 Run 'nova run container <name>' to start containers");
                        }
                        Err(e) => {
                            println!("❌ Failed to deploy template: {}", e);
                        }
                    }
                }
            }
        }
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
