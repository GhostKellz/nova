// Built-in VM Templates for Nova
// GPU Passthrough optimized templates for gaming/workstation VMs

use crate::config::VmTemplateConfig;
use std::collections::HashMap;

/// Get all built-in VM templates
pub fn builtin_templates() -> HashMap<String, VmTemplateConfig> {
    let mut templates = HashMap::new();

    // Windows Templates
    templates.insert(
        "windows11".to_string(),
        VmTemplateConfig {
            name: "Windows 11".to_string(),
            description: "Windows 11 with Secure Boot + TPM 2.0".to_string(),
            os_type: "windows".to_string(),
            cpu: 8,
            memory: "16G".to_string(),
            disk_size: "128G".to_string(),
            gpu_passthrough: false,
            uefi: true,
            secure_boot: true,
            tpm: true,
            iso_pattern: Some(r"(?i)win.*11.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["windows".to_string(), "desktop".to_string()],
        },
    );

    templates.insert(
        "windows11-gaming".to_string(),
        VmTemplateConfig {
            name: "Windows 11 Gaming".to_string(),
            description: "Windows 11 with GPU passthrough for gaming".to_string(),
            os_type: "windows".to_string(),
            cpu: 8,
            memory: "32G".to_string(),
            disk_size: "256G".to_string(),
            gpu_passthrough: true,
            uefi: true,
            secure_boot: true,
            tpm: true,
            iso_pattern: Some(r"(?i)win.*11.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["windows".to_string(), "gaming".to_string(), "gpu".to_string()],
        },
    );

    // NVIDIA GPU Passthrough Linux Templates
    templates.insert(
        "nv-arch".to_string(),
        VmTemplateConfig {
            name: "Arch Linux (NVIDIA)".to_string(),
            description: "Arch Linux with NVIDIA GPU passthrough".to_string(),
            os_type: "linux".to_string(),
            cpu: 8,
            memory: "16G".to_string(),
            disk_size: "64G".to_string(),
            gpu_passthrough: true,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)arch.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "arch".to_string(), "nvidia".to_string(), "gpu".to_string()],
        },
    );

    templates.insert(
        "nv-fedora".to_string(),
        VmTemplateConfig {
            name: "Fedora Workstation (NVIDIA)".to_string(),
            description: "Fedora with NVIDIA GPU passthrough".to_string(),
            os_type: "linux".to_string(),
            cpu: 8,
            memory: "16G".to_string(),
            disk_size: "80G".to_string(),
            gpu_passthrough: true,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)fedora.*workstation.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "fedora".to_string(), "nvidia".to_string(), "gpu".to_string()],
        },
    );

    templates.insert(
        "nv-bazzite".to_string(),
        VmTemplateConfig {
            name: "Bazzite (NVIDIA)".to_string(),
            description: "Bazzite gaming distro with NVIDIA GPU passthrough".to_string(),
            os_type: "linux".to_string(),
            cpu: 8,
            memory: "32G".to_string(),
            disk_size: "128G".to_string(),
            gpu_passthrough: true,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)bazzite.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "bazzite".to_string(), "nvidia".to_string(), "gaming".to_string(), "gpu".to_string()],
        },
    );

    templates.insert(
        "nv-nobara".to_string(),
        VmTemplateConfig {
            name: "Nobara (NVIDIA)".to_string(),
            description: "Nobara gaming distro with NVIDIA GPU passthrough".to_string(),
            os_type: "linux".to_string(),
            cpu: 8,
            memory: "32G".to_string(),
            disk_size: "128G".to_string(),
            gpu_passthrough: true,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)nobara.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "nobara".to_string(), "nvidia".to_string(), "gaming".to_string(), "gpu".to_string()],
        },
    );

    templates.insert(
        "nv-popos".to_string(),
        VmTemplateConfig {
            name: "Pop!_OS (NVIDIA)".to_string(),
            description: "Pop!_OS with NVIDIA GPU passthrough".to_string(),
            os_type: "linux".to_string(),
            cpu: 8,
            memory: "16G".to_string(),
            disk_size: "80G".to_string(),
            gpu_passthrough: true,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)pop.?os.*nvidia.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "popos".to_string(), "nvidia".to_string(), "gpu".to_string()],
        },
    );

    templates.insert(
        "nv-cosmic".to_string(),
        VmTemplateConfig {
            name: "COSMIC Desktop (NVIDIA)".to_string(),
            description: "Pop!_OS COSMIC with NVIDIA GPU passthrough".to_string(),
            os_type: "linux".to_string(),
            cpu: 8,
            memory: "16G".to_string(),
            disk_size: "80G".to_string(),
            gpu_passthrough: true,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)(cosmic|pop.?os.*cosmic).*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "cosmic".to_string(), "nvidia".to_string(), "gpu".to_string()],
        },
    );

    // Server Templates
    templates.insert(
        "ubuntu-server".to_string(),
        VmTemplateConfig {
            name: "Ubuntu Server 24.04 LTS".to_string(),
            description: "Ubuntu Server 24.04 LTS".to_string(),
            os_type: "linux".to_string(),
            cpu: 4,
            memory: "4G".to_string(),
            disk_size: "32G".to_string(),
            gpu_passthrough: false,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)ubuntu.*24\.04.*server.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "ubuntu".to_string(), "server".to_string()],
        },
    );

    templates.insert(
        "debian-server".to_string(),
        VmTemplateConfig {
            name: "Debian 12/13 Server".to_string(),
            description: "Debian stable server".to_string(),
            os_type: "linux".to_string(),
            cpu: 4,
            memory: "4G".to_string(),
            disk_size: "32G".to_string(),
            gpu_passthrough: false,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)debian.*(12|13|bookworm|trixie).*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "debian".to_string(), "server".to_string()],
        },
    );

    // Minimal/Desktop Templates (no GPU passthrough)
    templates.insert(
        "fedora".to_string(),
        VmTemplateConfig {
            name: "Fedora Workstation".to_string(),
            description: "Fedora Workstation (no GPU passthrough)".to_string(),
            os_type: "linux".to_string(),
            cpu: 4,
            memory: "8G".to_string(),
            disk_size: "64G".to_string(),
            gpu_passthrough: false,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)fedora.*workstation.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "fedora".to_string(), "desktop".to_string()],
        },
    );

    templates.insert(
        "arch".to_string(),
        VmTemplateConfig {
            name: "Arch Linux".to_string(),
            description: "Arch Linux (no GPU passthrough)".to_string(),
            os_type: "linux".to_string(),
            cpu: 4,
            memory: "4G".to_string(),
            disk_size: "32G".to_string(),
            gpu_passthrough: false,
            uefi: true,
            secure_boot: false,
            tpm: false,
            iso_pattern: Some(r"(?i)arch.*\.iso".to_string()),
            network: Some("virbr0".to_string()),
            tags: vec!["linux".to_string(), "arch".to_string(), "minimal".to_string()],
        },
    );

    templates
}

/// Scan directories for ISO files
pub fn scan_iso_directories(paths: &[std::path::PathBuf]) -> Vec<IsoFile> {
    let mut isos = Vec::new();

    for path in paths {
        if !path.exists() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.extension().map_or(false, |e| e.eq_ignore_ascii_case("iso")) {
                    let name = file_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let os_type = detect_os_from_filename(&name);

                    isos.push(IsoFile {
                        path: file_path,
                        name,
                        os_type,
                    });
                }
            }
        }
    }

    isos.sort_by(|a, b| a.name.cmp(&b.name));
    isos
}

#[derive(Debug, Clone)]
pub struct IsoFile {
    pub path: std::path::PathBuf,
    pub name: String,
    pub os_type: String,
}

fn detect_os_from_filename(filename: &str) -> String {
    let lower = filename.to_lowercase();

    if lower.contains("win") {
        if lower.contains("11") {
            "Windows 11".to_string()
        } else if lower.contains("10") {
            "Windows 10".to_string()
        } else if lower.contains("server") {
            "Windows Server".to_string()
        } else {
            "Windows".to_string()
        }
    } else if lower.contains("bazzite") {
        "Bazzite".to_string()
    } else if lower.contains("nobara") {
        "Nobara".to_string()
    } else if lower.contains("fedora") {
        "Fedora".to_string()
    } else if lower.contains("arch") {
        "Arch Linux".to_string()
    } else if lower.contains("ubuntu") {
        "Ubuntu".to_string()
    } else if lower.contains("debian") {
        "Debian".to_string()
    } else if lower.contains("pop") || lower.contains("cosmic") {
        "Pop!_OS".to_string()
    } else if lower.contains("manjaro") {
        "Manjaro".to_string()
    } else if lower.contains("mint") {
        "Linux Mint".to_string()
    } else if lower.contains("opensuse") || lower.contains("suse") {
        "openSUSE".to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Match ISOs to templates based on patterns
pub fn match_isos_to_template<'a>(
    template: &VmTemplateConfig,
    isos: &'a [IsoFile],
) -> Vec<&'a IsoFile> {
    let Some(pattern_str) = &template.iso_pattern else {
        return vec![];
    };

    let Ok(regex) = regex::Regex::new(pattern_str) else {
        return vec![];
    };

    isos.iter().filter(|iso| regex.is_match(&iso.name)).collect()
}
