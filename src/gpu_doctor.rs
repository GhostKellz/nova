use crate::{gpu_passthrough::*, log_info};
use std::fs;
use std::path::Path;
use std::process::Command;

/// GPU Doctor - comprehensive system diagnostics for GPU passthrough
pub struct GpuDoctor {
    gpu_manager: GpuManager,
}

#[derive(Debug)]
pub struct DiagnosticReport {
    pub overall_status: SystemStatus,
    pub checks: Vec<DiagnosticCheck>,
    pub recommendations: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum SystemStatus {
    Ready,
    NeedsConfiguration,
    NotSupported,
}

#[derive(Debug)]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub fix_command: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl GpuDoctor {
    pub fn new() -> Self {
        let mut manager = GpuManager::new();
        let _ = manager.discover();

        Self {
            gpu_manager: manager,
        }
    }

    /// Run comprehensive system diagnostics
    pub fn diagnose(&self) -> DiagnosticReport {
        log_info!("Running GPU passthrough diagnostics...");

        let mut checks = Vec::new();
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut recommendations = Vec::new();

        // Check 1: IOMMU enabled in kernel
        checks.push(self.check_iommu());

        // Check 2: Virtualization enabled (VT-x/AMD-V)
        checks.push(self.check_virtualization());

        // Check 3: VFIO modules loaded
        checks.push(self.check_vfio_modules());

        // Check 4: NVIDIA driver detection
        checks.push(self.check_nvidia_drivers());

        // Check 5: GPU detection
        checks.push(self.check_gpu_detection());

        // Check 6: IOMMU groups analysis
        checks.push(self.check_iommu_groups());

        // Check 7: nvbind availability
        checks.push(self.check_nvbind());

        // Check 8: Kernel parameters
        checks.push(self.check_kernel_parameters());

        // Check 9: Conflicting drivers
        checks.push(self.check_conflicting_drivers());

        // Check 10: PCI resizable BAR
        checks.push(self.check_resizable_bar());

        // Analyze results
        let failures = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
        let warns = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();

        let overall_status = if failures > 0 {
            SystemStatus::NotSupported
        } else if warns > 0 {
            SystemStatus::NeedsConfiguration
        } else {
            SystemStatus::Ready
        };

        // Generate recommendations
        for check in &checks {
            match check.status {
                CheckStatus::Fail => {
                    errors.push(format!("❌ {}: {}", check.name, check.message));
                    if let Some(fix) = &check.fix_command {
                        recommendations.push(format!("Run: {}", fix));
                    }
                }
                CheckStatus::Warn => {
                    warnings.push(format!("⚠️  {}: {}", check.name, check.message));
                    if let Some(fix) = &check.fix_command {
                        recommendations.push(format!("Optional: {}", fix));
                    }
                }
                _ => {}
            }
        }

        DiagnosticReport {
            overall_status,
            checks,
            recommendations,
            warnings,
            errors,
        }
    }

    /// Check if IOMMU is enabled
    fn check_iommu(&self) -> DiagnosticCheck {
        let iommu_path = Path::new("/sys/kernel/iommu_groups");

        if iommu_path.exists() {
            let groups_count = fs::read_dir(iommu_path)
                .map(|entries| entries.count())
                .unwrap_or(0);

            DiagnosticCheck {
                name: "IOMMU".to_string(),
                status: CheckStatus::Pass,
                message: format!("IOMMU enabled ({} groups detected)", groups_count),
                fix_command: None,
            }
        } else {
            DiagnosticCheck {
                name: "IOMMU".to_string(),
                status: CheckStatus::Fail,
                message: "IOMMU not enabled or not detected".to_string(),
                fix_command: Some("Add 'intel_iommu=on' or 'amd_iommu=on' to kernel parameters".to_string()),
            }
        }
    }

    /// Check if virtualization is enabled
    fn check_virtualization(&self) -> DiagnosticCheck {
        let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();

        let has_vmx = cpuinfo.contains("vmx"); // Intel VT-x
        let has_svm = cpuinfo.contains("svm"); // AMD-V

        if has_vmx || has_svm {
            let tech = if has_vmx { "Intel VT-x" } else { "AMD-V" };
            DiagnosticCheck {
                name: "Virtualization".to_string(),
                status: CheckStatus::Pass,
                message: format!("{} enabled in CPU", tech),
                fix_command: None,
            }
        } else {
            DiagnosticCheck {
                name: "Virtualization".to_string(),
                status: CheckStatus::Fail,
                message: "Hardware virtualization not detected".to_string(),
                fix_command: Some("Enable VT-x/AMD-V in BIOS/UEFI settings".to_string()),
            }
        }
    }

    /// Check VFIO kernel modules
    fn check_vfio_modules(&self) -> DiagnosticCheck {
        let required = ["vfio", "vfio_pci", "vfio_iommu_type1"];
        let mut loaded = Vec::new();
        let mut missing = Vec::new();

        for module in required {
            if Path::new(&format!("/sys/module/{}", module)).exists() {
                loaded.push(module);
            } else {
                missing.push(module);
            }
        }

        if missing.is_empty() {
            DiagnosticCheck {
                name: "VFIO Modules".to_string(),
                status: CheckStatus::Pass,
                message: "All VFIO modules loaded".to_string(),
                fix_command: None,
            }
        } else {
            DiagnosticCheck {
                name: "VFIO Modules".to_string(),
                status: CheckStatus::Fail,
                message: format!("Missing modules: {}", missing.join(", ")),
                fix_command: Some(format!("modprobe {}", missing.join(" && modprobe "))),
            }
        }
    }

    /// Check NVIDIA driver status
    fn check_nvidia_drivers(&self) -> DiagnosticCheck {
        // Check for nvidia-open kernel module (used for version detection)
        let nvidia_open_version = self.get_nvidia_open_version();

        // Check for proprietary driver
        let nvidia_proprietary = Command::new("nvidia-smi")
            .arg("--query-gpu=driver_version")
            .arg("--format=csv,noheader")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            });

        if let Some(version) = nvidia_open_version {
            DiagnosticCheck {
                name: "NVIDIA Driver".to_string(),
                status: CheckStatus::Pass,
                message: format!("NVIDIA Open Kernel Module {} detected (recommended)", version),
                fix_command: None,
            }
        } else if let Some(version) = nvidia_proprietary {
            DiagnosticCheck {
                name: "NVIDIA Driver".to_string(),
                status: CheckStatus::Warn,
                message: format!("Proprietary NVIDIA driver {} detected", version),
                fix_command: Some("Consider switching to nvidia-open for better passthrough support: yay -S nvidia-open".to_string()),
            }
        } else {
            DiagnosticCheck {
                name: "NVIDIA Driver".to_string(),
                status: CheckStatus::Warn,
                message: "No NVIDIA driver detected".to_string(),
                fix_command: Some("Install nvidia-open: yay -S nvidia-open nvidia-open-dkms".to_string()),
            }
        }
    }

    /// Get NVIDIA open kernel module version
    fn get_nvidia_open_version(&self) -> Option<String> {
        let version_path = "/sys/module/nvidia/version";
        fs::read_to_string(version_path).ok().map(|v| v.trim().to_string())
    }

    /// Check GPU detection
    fn check_gpu_detection(&self) -> DiagnosticCheck {
        let gpus = self.gpu_manager.list_gpus();

        if gpus.is_empty() {
            DiagnosticCheck {
                name: "GPU Detection".to_string(),
                status: CheckStatus::Fail,
                message: "No GPUs detected".to_string(),
                fix_command: Some("Ensure GPU is properly seated and powered".to_string()),
            }
        } else {
            let gpu_list = gpus.iter()
                .map(|g| format!("{} ({})", g.device_name, g.address))
                .collect::<Vec<_>>()
                .join(", ");

            DiagnosticCheck {
                name: "GPU Detection".to_string(),
                status: CheckStatus::Pass,
                message: format!("Detected {} GPU(s): {}", gpus.len(), gpu_list),
                fix_command: None,
            }
        }
    }

    /// Check IOMMU groups viability
    fn check_iommu_groups(&self) -> DiagnosticCheck {
        let groups = self.gpu_manager.list_iommu_groups();
        let viable_count = groups.iter().filter(|g| g.viable_for_passthrough).count();

        if viable_count == 0 {
            DiagnosticCheck {
                name: "IOMMU Groups".to_string(),
                status: CheckStatus::Fail,
                message: "No viable IOMMU groups for GPU passthrough".to_string(),
                fix_command: Some("Check BIOS settings for ACS override or PCIe configuration".to_string()),
            }
        } else if viable_count < groups.len() {
            DiagnosticCheck {
                name: "IOMMU Groups".to_string(),
                status: CheckStatus::Warn,
                message: format!("{}/{} IOMMU groups are viable for passthrough", viable_count, groups.len()),
                fix_command: Some("Some GPUs share IOMMU groups - consider ACS override patch".to_string()),
            }
        } else {
            DiagnosticCheck {
                name: "IOMMU Groups".to_string(),
                status: CheckStatus::Pass,
                message: format!("All {} IOMMU groups are viable", viable_count),
                fix_command: None,
            }
        }
    }

    /// Check nvbind availability
    fn check_nvbind(&self) -> DiagnosticCheck {
        if self.gpu_manager.nvbind_available {
            DiagnosticCheck {
                name: "nvbind".to_string(),
                status: CheckStatus::Pass,
                message: "nvbind GPU runtime available".to_string(),
                fix_command: None,
            }
        } else {
            DiagnosticCheck {
                name: "nvbind".to_string(),
                status: CheckStatus::Warn,
                message: "nvbind not installed (optional for container GPU passthrough)".to_string(),
                fix_command: Some("Install nvbind: cargo install nvbind".to_string()),
            }
        }
    }

    /// Check kernel parameters
    fn check_kernel_parameters(&self) -> DiagnosticCheck {
        let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();

        let mut issues = Vec::new();
        let mut has_iommu = false;

        if cmdline.contains("intel_iommu=on") || cmdline.contains("amd_iommu=on") {
            has_iommu = true;
        } else {
            issues.push("IOMMU not enabled in kernel parameters");
        }

        if !cmdline.contains("iommu=pt") {
            issues.push("Consider adding iommu=pt for better performance");
        }

        if cmdline.contains("nouveau") && !cmdline.contains("nouveau.modeset=0") {
            issues.push("Nouveau driver may conflict with NVIDIA passthrough");
        }

        if issues.is_empty() && has_iommu {
            DiagnosticCheck {
                name: "Kernel Parameters".to_string(),
                status: CheckStatus::Pass,
                message: "Kernel parameters properly configured".to_string(),
                fix_command: None,
            }
        } else {
            DiagnosticCheck {
                name: "Kernel Parameters".to_string(),
                status: if has_iommu { CheckStatus::Warn } else { CheckStatus::Fail },
                message: issues.join("; "),
                fix_command: Some("Edit /etc/default/grub and add: intel_iommu=on iommu=pt, then run: grub-mkconfig -o /boot/grub/grub.cfg".to_string()),
            }
        }
    }

    /// Check for conflicting drivers
    fn check_conflicting_drivers(&self) -> DiagnosticCheck {
        let gpus = self.gpu_manager.list_gpus();
        let mut conflicts = Vec::new();

        for gpu in gpus {
            if let Some(driver) = &gpu.driver {
                if driver == "nouveau" {
                    conflicts.push(format!("{}: nouveau (conflicts with NVIDIA passthrough)", gpu.address));
                }
            }
        }

        if conflicts.is_empty() {
            DiagnosticCheck {
                name: "Driver Conflicts".to_string(),
                status: CheckStatus::Pass,
                message: "No conflicting drivers detected".to_string(),
                fix_command: None,
            }
        } else {
            DiagnosticCheck {
                name: "Driver Conflicts".to_string(),
                status: CheckStatus::Warn,
                message: format!("Conflicts: {}", conflicts.join(", ")),
                fix_command: Some("Blacklist nouveau: echo 'blacklist nouveau' | sudo tee /etc/modprobe.d/blacklist-nouveau.conf".to_string()),
            }
        }
    }

    /// Check PCI Resizable BAR support
    fn check_resizable_bar(&self) -> DiagnosticCheck {
        let gpus = self.gpu_manager.list_gpus();
        let mut rebar_gpus = Vec::new();

        for gpu in gpus {
            let rebar_path = format!("/sys/bus/pci/devices/{}/resource", gpu.address);
            if Path::new(&rebar_path).exists() {
                // Simplified check - real implementation would parse actual ReBAR capability
                rebar_gpus.push(gpu.address.clone());
            }
        }

        if rebar_gpus.is_empty() {
            DiagnosticCheck {
                name: "Resizable BAR".to_string(),
                status: CheckStatus::Warn,
                message: "Resizable BAR not detected (may impact performance on newer GPUs)".to_string(),
                fix_command: Some("Enable Resizable BAR in BIOS if supported".to_string()),
            }
        } else {
            DiagnosticCheck {
                name: "Resizable BAR".to_string(),
                status: CheckStatus::Pass,
                message: format!("Resizable BAR available for {} GPU(s)", rebar_gpus.len()),
                fix_command: None,
            }
        }
    }

    /// Print a formatted diagnostic report
    pub fn print_report(&self, report: &DiagnosticReport) {
        println!("\n╔══════════════════════════════════════════════════════════════════╗");
        println!("║          Nova GPU Passthrough Diagnostic Report                 ║");
        println!("╚══════════════════════════════════════════════════════════════════╝\n");

        // Overall status
        let status_msg = match report.overall_status {
            SystemStatus::Ready => "✅ READY - System is configured for GPU passthrough",
            SystemStatus::NeedsConfiguration => "⚠️  NEEDS CONFIGURATION - System requires adjustments",
            SystemStatus::NotSupported => "❌ NOT READY - Critical issues detected",
        };
        println!("Status: {}\n", status_msg);

        // Detailed checks
        println!("╔══ Diagnostic Checks ══════════════════════════════════════════╗\n");
        for check in &report.checks {
            let icon = match check.status {
                CheckStatus::Pass => "✅",
                CheckStatus::Warn => "⚠️ ",
                CheckStatus::Fail => "❌",
            };
            println!("{} {}: {}", icon, check.name, check.message);
            if let Some(fix) = &check.fix_command {
                println!("   └─ Fix: {}", fix);
            }
        }

        // Errors
        if !report.errors.is_empty() {
            println!("\n╔══ Critical Issues ════════════════════════════════════════════╗\n");
            for error in &report.errors {
                println!("{}", error);
            }
        }

        // Warnings
        if !report.warnings.is_empty() {
            println!("\n╔══ Warnings ═══════════════════════════════════════════════════╗\n");
            for warning in &report.warnings {
                println!("{}", warning);
            }
        }

        // Recommendations
        if !report.recommendations.is_empty() {
            println!("\n╔══ Recommendations ════════════════════════════════════════════╗\n");
            for (i, rec) in report.recommendations.iter().enumerate() {
                println!("{}. {}", i + 1, rec);
            }
        }

        println!("\n╚═══════════════════════════════════════════════════════════════╝");
        println!("\nFor more information: nova gpu --help");
        println!("Report issues: https://github.com/nova-project/nova/issues\n");
    }

    /// Generate a quick-fix script
    pub fn generate_fix_script(&self, report: &DiagnosticReport) -> String {
        let mut script = String::new();
        script.push_str("#!/bin/bash\n");
        script.push_str("# Nova GPU Passthrough Auto-Fix Script\n");
        script.push_str("# Generated by: nova gpu doctor\n\n");
        script.push_str("set -e\n\n");

        for check in &report.checks {
            if check.status == CheckStatus::Fail || check.status == CheckStatus::Warn {
                if let Some(fix) = &check.fix_command {
                    script.push_str(&format!("# Fix: {}\n", check.name));
                    script.push_str(&format!("{}\n\n", fix));
                }
            }
        }

        script.push_str("echo 'Configuration complete. Please reboot for changes to take effect.'\n");
        script
    }
}

impl Default for GpuDoctor {
    fn default() -> Self {
        Self::new()
    }
}
