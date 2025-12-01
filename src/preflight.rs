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

#[cfg(test)]
mod tests {
    use super::{ModuleStatus, PreflightSummary, ToolStatus};

    #[test]
    fn display_formats_readable_summary() {
        let summary = PreflightSummary {
            kernel_release: Some("6.9.1-arch1-1".into()),
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
