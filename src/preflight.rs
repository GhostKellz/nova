use crate::{Result, log_info};
use serde::Serialize;
use std::fmt;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct ModuleStatus {
    pub name: &'static str,
    pub loaded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: &'static str,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightSummary {
    pub kernel_release: Option<String>,
    pub distribution: Option<String>,
    pub module_status: Vec<ModuleStatus>,
    pub tool_status: Vec<ToolStatus>,
    pub issues: Vec<String>,
}

impl PreflightSummary {
    pub fn is_ready(&self) -> bool {
        self.issues.is_empty()
    }
}

impl fmt::Display for PreflightSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Nova Preflight Summary (kernel: {}, distro: {})",
            self.kernel_release.as_deref().unwrap_or("unknown"),
            self.distribution.as_deref().unwrap_or("unknown")
        )?;
        writeln!(f, "\nKernel Modules:")?;
        for module in &self.module_status {
            writeln!(
                f,
                "  - {}: {}",
                module.name,
                if module.loaded { "loaded" } else { "missing" }
            )?;
        }
        writeln!(f, "\nUserland Tooling:")?;
        for tool in &self.tool_status {
            writeln!(
                f,
                "  - {}: {}",
                tool.name,
                if tool.available {
                    "available"
                } else {
                    "missing"
                }
            )?;
        }
        if self.issues.is_empty() {
            writeln!(f, "\n✅ Ready for Nova workloads")
        } else {
            writeln!(f, "\n⚠ Issues:")?;
            for issue in &self.issues {
                writeln!(f, "  - {}", issue)?;
            }
            Ok(())
        }
    }
}

pub fn run_preflight() -> Result<PreflightSummary> {
    log_info!("Running preflight checks for Arch-based setups");

    let kernel_release = read_command_output("uname", &["-r"]);
    let distribution = read_command_output("lsb_release", &["-ds"]).or_else(|| {
        read_command_output("cat", &["/etc/os-release"]).map(|payload| {
            payload
                .lines()
                .find(|line| line.starts_with("PRETTY_NAME"))
                .and_then(|line| line.split('=').nth(1))
                .map(|value| value.trim_matches('"').to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        })
    });

    let module_status = vec![
        probe_module("kvm"),
        probe_module("kvm_intel"),
        probe_module("kvm_amd"),
        probe_module("vfio_pci"),
        probe_module("vfio_iommu_type1"),
        probe_module("vfio_virqfd"),
    ];

    let tool_status = vec![
        probe_tool("virsh"),
        probe_tool("qemu-system-x86_64"),
        probe_tool("ip"),
        probe_tool("nmcli"),
        probe_tool("virt-install"),
    ];

    let mut issues = Vec::new();
    let kvm_loaded = module_status
        .iter()
        .any(|module| module.name == "kvm" && module.loaded);
    if !kvm_loaded {
        issues.push("/dev/kvm unavailable or kvm module missing".to_string());
    }

    if !module_status
        .iter()
        .any(|module| module.name == "vfio_pci" && module.loaded)
    {
        issues.push("vfio_pci not loaded; GPU passthrough will fail".to_string());
    }

    for tool in &tool_status {
        if !tool.available {
            issues.push(format!("{} command missing from PATH", tool.name));
        }
    }

    Ok(PreflightSummary {
        kernel_release,
        distribution,
        module_status,
        tool_status,
        issues,
    })
}

fn probe_module(name: &'static str) -> ModuleStatus {
    ModuleStatus {
        name,
        loaded: Path::new(&format!("/sys/module/{}", name)).exists(),
    }
}

fn probe_tool(name: &'static str) -> ToolStatus {
    let available = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    ToolStatus { name, available }
}

fn read_command_output(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Result of attempting to fix preflight issues
#[derive(Debug, Clone, Serialize)]
pub struct PreflightFixResult {
    pub fixes_attempted: Vec<FixAttempt>,
    pub requires_reboot: bool,
    pub manual_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixAttempt {
    pub issue: String,
    pub action: String,
    pub success: bool,
    pub message: String,
}

impl fmt::Display for PreflightFixResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Preflight Fix Results ===")?;
        writeln!(f)?;

        for fix in &self.fixes_attempted {
            let status = if fix.success { "SUCCESS" } else { "FAILED" };
            writeln!(f, "[{}] {}", status, fix.issue)?;
            writeln!(f, "    Action: {}", fix.action)?;
            writeln!(f, "    Result: {}", fix.message)?;
            writeln!(f)?;
        }

        if self.requires_reboot {
            writeln!(f, "A system reboot is required to apply all changes.")?;
            writeln!(f)?;
        }

        if !self.manual_steps.is_empty() {
            writeln!(f, "Manual steps required:")?;
            for (i, step) in self.manual_steps.iter().enumerate() {
                writeln!(f, "  {}. {}", i + 1, step)?;
            }
        }

        Ok(())
    }
}

/// Attempt to automatically fix detected preflight issues
pub fn run_preflight_fix(summary: &PreflightSummary) -> Result<PreflightFixResult> {
    log_info!("Attempting to fix preflight issues...");

    let mut result = PreflightFixResult {
        fixes_attempted: Vec::new(),
        requires_reboot: false,
        manual_steps: Vec::new(),
    };

    // Fix missing kernel modules
    for module in &summary.module_status {
        if !module.loaded {
            let fix = fix_missing_module(module.name);
            result.fixes_attempted.push(fix);
        }
    }

    // Fix missing tools
    for tool in &summary.tool_status {
        if !tool.available {
            let fix = fix_missing_tool(tool.name);
            if !fix.success {
                // Add manual step for tools that couldn't be auto-installed
                result.manual_steps.push(format!(
                    "Install {} manually: sudo pacman -S {}",
                    tool.name,
                    get_package_name(tool.name)
                ));
            }
            result.fixes_attempted.push(fix);
        }
    }

    // Check and fix IOMMU configuration
    if !Path::new("/sys/kernel/iommu_groups").exists() {
        let fix = fix_iommu_config(summary);
        if fix.success {
            result.requires_reboot = true;
        }
        result.fixes_attempted.push(fix);
    }

    // Setup TPM access if needed
    let tpm_fix = setup_tpm_permissions();
    if tpm_fix.success {
        result.fixes_attempted.push(tpm_fix);
    }

    // Setup libvirt groups
    let libvirt_fix = setup_libvirt_groups();
    result.fixes_attempted.push(libvirt_fix);

    // Enable required services
    let services_fix = enable_required_services();
    result.fixes_attempted.push(services_fix);

    // Check if reboot is needed
    if result.fixes_attempted.iter().any(|f| {
        f.action.contains("modprobe") || f.action.contains("grub") || f.action.contains("kernel")
    }) {
        result.requires_reboot = true;
    }

    Ok(result)
}

fn fix_missing_module(module_name: &str) -> FixAttempt {
    log_info!("Attempting to load module: {}", module_name);

    // Try to load the module
    let load_result = Command::new("sudo")
        .args(["modprobe", module_name])
        .output();

    match load_result {
        Ok(output) if output.status.success() => {
            // Make it persistent
            let persist_result = Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "echo '{}' | sudo tee -a /etc/modules-load.d/nova.conf",
                    module_name
                ))
                .output();

            let persist_msg = if persist_result.map(|o| o.status.success()).unwrap_or(false) {
                "and made persistent"
            } else {
                "but persistence failed"
            };

            FixAttempt {
                issue: format!("Module {} not loaded", module_name),
                action: format!("modprobe {} + persist to /etc/modules-load.d/", module_name),
                success: true,
                message: format!("Module loaded successfully {}", persist_msg),
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            FixAttempt {
                issue: format!("Module {} not loaded", module_name),
                action: format!("modprobe {}", module_name),
                success: false,
                message: format!("Failed to load module: {}", stderr.trim()),
            }
        }
        Err(e) => FixAttempt {
            issue: format!("Module {} not loaded", module_name),
            action: format!("modprobe {}", module_name),
            success: false,
            message: format!("Failed to execute modprobe: {}", e),
        },
    }
}

fn fix_missing_tool(tool_name: &str) -> FixAttempt {
    let package = get_package_name(tool_name);

    log_info!(
        "Attempting to install package for {}: {}",
        tool_name,
        package
    );

    // Check if pacman is available (Arch-based)
    let is_arch = Command::new("which")
        .arg("pacman")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_arch {
        return FixAttempt {
            issue: format!("Tool {} not found", tool_name),
            action: "Install package".to_string(),
            success: false,
            message: "Not an Arch-based system, cannot auto-install".to_string(),
        };
    }

    // Try to install with pacman
    let install_result = Command::new("sudo")
        .args(["pacman", "-S", "--noconfirm", package])
        .output();

    match install_result {
        Ok(output) if output.status.success() => FixAttempt {
            issue: format!("Tool {} not found", tool_name),
            action: format!("pacman -S {}", package),
            success: true,
            message: format!("Package {} installed successfully", package),
        },
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            FixAttempt {
                issue: format!("Tool {} not found", tool_name),
                action: format!("pacman -S {}", package),
                success: false,
                message: format!("Installation failed: {}", stderr.trim()),
            }
        }
        Err(e) => FixAttempt {
            issue: format!("Tool {} not found", tool_name),
            action: format!("pacman -S {}", package),
            success: false,
            message: format!("Failed to execute pacman: {}", e),
        },
    }
}

fn get_package_name(tool_name: &str) -> &str {
    match tool_name {
        "virsh" => "libvirt",
        "qemu-system-x86_64" => "qemu-full",
        "ip" => "iproute2",
        "nmcli" => "networkmanager",
        "virt-install" => "virt-install",
        "swtpm" => "swtpm",
        "looking-glass-client" => "looking-glass",
        _ => tool_name,
    }
}

fn fix_iommu_config(summary: &PreflightSummary) -> FixAttempt {
    log_info!("Checking IOMMU configuration...");

    // Detect CPU vendor
    let is_intel = summary
        .module_status
        .iter()
        .any(|m| m.name == "kvm_intel" && m.loaded);

    let iommu_param = if is_intel {
        "intel_iommu=on iommu=pt"
    } else {
        "amd_iommu=on iommu=pt"
    };

    // Check if already in grub config
    let grub_content = std::fs::read_to_string("/etc/default/grub").unwrap_or_default();
    if grub_content.contains("iommu=on") || grub_content.contains("iommu=pt") {
        return FixAttempt {
            issue: "IOMMU not enabled".to_string(),
            action: "Check GRUB config".to_string(),
            success: true,
            message: "IOMMU parameters already in GRUB config. Reboot may be required.".to_string(),
        };
    }

    // Try to add IOMMU parameters to GRUB
    let sed_cmd = format!(
        r#"sudo sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="\([^"]*\)"/GRUB_CMDLINE_LINUX_DEFAULT="\1 {}"/' /etc/default/grub"#,
        iommu_param
    );

    let sed_result = Command::new("sh").arg("-c").arg(&sed_cmd).output();

    match sed_result {
        Ok(output) if output.status.success() => {
            // Regenerate GRUB config
            let grub_result = Command::new("sudo")
                .args(["grub-mkconfig", "-o", "/boot/grub/grub.cfg"])
                .output();

            match grub_result {
                Ok(grub_out) if grub_out.status.success() => FixAttempt {
                    issue: "IOMMU not enabled".to_string(),
                    action: format!("Add {} to GRUB + regenerate config", iommu_param),
                    success: true,
                    message: "IOMMU parameters added. REBOOT REQUIRED.".to_string(),
                },
                _ => FixAttempt {
                    issue: "IOMMU not enabled".to_string(),
                    action: format!("Add {} to GRUB", iommu_param),
                    success: false,
                    message: "Added to GRUB but failed to regenerate. Run: sudo grub-mkconfig -o /boot/grub/grub.cfg".to_string(),
                },
            }
        }
        _ => FixAttempt {
            issue: "IOMMU not enabled".to_string(),
            action: "Modify GRUB config".to_string(),
            success: false,
            message: format!("Failed to modify GRUB. Manually add: {}", iommu_param),
        },
    }
}

fn setup_tpm_permissions() -> FixAttempt {
    log_info!("Setting up TPM permissions...");

    // Check if TPM device exists
    if !Path::new("/dev/tpm0").exists() && !Path::new("/dev/tpmrm0").exists() {
        return FixAttempt {
            issue: "TPM permissions".to_string(),
            action: "Setup TPM access".to_string(),
            success: false,
            message: "No TPM device found. Enable TPM in BIOS if available.".to_string(),
        };
    }

    // Create udev rule for TPM access
    let udev_rule = r#"SUBSYSTEM=="tpm", MODE="0660", GROUP="tss"
KERNEL=="tpm[0-9]*", MODE="0660", GROUP="tss"
KERNEL=="tpmrm[0-9]*", MODE="0660", GROUP="tss""#;

    let udev_path = "/etc/udev/rules.d/99-tpm.rules";

    let write_result = Command::new("sh")
        .arg("-c")
        .arg(format!("echo '{}' | sudo tee {}", udev_rule, udev_path))
        .output();

    match write_result {
        Ok(output) if output.status.success() => {
            // Reload udev rules
            let _ = Command::new("sudo")
                .args(["udevadm", "control", "--reload-rules"])
                .output();
            let _ = Command::new("sudo").args(["udevadm", "trigger"]).output();

            // Add current user to tss group
            if let Ok(user) = std::env::var("USER") {
                let _ = Command::new("sudo")
                    .args(["usermod", "-aG", "tss", &user])
                    .output();
            }

            FixAttempt {
                issue: "TPM permissions".to_string(),
                action: "Setup TPM udev rules and group access".to_string(),
                success: true,
                message: "TPM access configured. Re-login may be required for group changes."
                    .to_string(),
            }
        }
        _ => FixAttempt {
            issue: "TPM permissions".to_string(),
            action: "Setup TPM udev rules".to_string(),
            success: false,
            message: "Failed to create udev rules for TPM.".to_string(),
        },
    }
}

fn setup_libvirt_groups() -> FixAttempt {
    log_info!("Setting up libvirt groups...");

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());

    // Add user to libvirt and kvm groups
    let groups = ["libvirt", "kvm", "input"];
    let mut success_count = 0;

    for group in &groups {
        let result = Command::new("sudo")
            .args(["usermod", "-aG", group, &user])
            .output();

        if result.map(|o| o.status.success()).unwrap_or(false) {
            success_count += 1;
        }
    }

    if success_count == groups.len() {
        FixAttempt {
            issue: "User group membership".to_string(),
            action: format!("Add {} to libvirt, kvm, input groups", user),
            success: true,
            message: "User added to required groups. Re-login required for changes to take effect."
                .to_string(),
        }
    } else {
        FixAttempt {
            issue: "User group membership".to_string(),
            action: "Add user to virtualization groups".to_string(),
            success: false,
            message: format!(
                "Only {}/{} groups configured. Check group existence.",
                success_count,
                groups.len()
            ),
        }
    }
}

fn enable_required_services() -> FixAttempt {
    log_info!("Enabling required services...");

    let services = ["libvirtd", "virtlogd"];
    let mut success_count = 0;

    for service in &services {
        // Enable service
        let enable_result = Command::new("sudo")
            .args(["systemctl", "enable", service])
            .output();

        // Start service
        let start_result = Command::new("sudo")
            .args(["systemctl", "start", service])
            .output();

        if enable_result.map(|o| o.status.success()).unwrap_or(false)
            && start_result.map(|o| o.status.success()).unwrap_or(false)
        {
            success_count += 1;
        }
    }

    if success_count == services.len() {
        FixAttempt {
            issue: "Required services".to_string(),
            action: "Enable and start libvirtd, virtlogd".to_string(),
            success: true,
            message: "Services enabled and started successfully.".to_string(),
        }
    } else {
        FixAttempt {
            issue: "Required services".to_string(),
            action: "Enable virtualization services".to_string(),
            success: false,
            message: format!(
                "Only {}/{} services configured.",
                success_count,
                services.len()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ModuleStatus, PreflightSummary, ToolStatus};

    #[test]
    fn display_formats_readable_summary() {
        let summary = PreflightSummary {
            kernel_release: Some("7.0.1-arch1-1".into()),
            distribution: Some("Arch Linux".into()),
            module_status: vec![
                ModuleStatus {
                    name: "kvm",
                    loaded: true,
                },
                ModuleStatus {
                    name: "vfio_pci",
                    loaded: false,
                },
            ],
            tool_status: vec![
                ToolStatus {
                    name: "virsh",
                    available: true,
                },
                ToolStatus {
                    name: "nmcli",
                    available: false,
                },
            ],
            issues: vec!["vfio_pci not loaded".into()],
        };

        let printed = summary.to_string();
        assert!(printed.contains("Arch Linux"));
        assert!(printed.contains("vfio_pci: missing"));
        assert!(printed.contains("nmcli: missing"));
        assert!(printed.contains("vfio_pci not loaded"));
        assert!(printed.contains("Nova Preflight Summary"));
    }
}
