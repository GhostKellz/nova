// Integration tests for VM template validation
use std::fs;
use std::path::PathBuf;

#[test]
fn test_ml_pytorch_template_validation() {
    let template_path = PathBuf::from("examples/vm-templates/ml-pytorch.toml");

    assert!(template_path.exists(), "ML PyTorch template should exist");

    let content = fs::read_to_string(&template_path)
        .expect("Should be able to read template");

    // Validate TOML structure
    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok(), "Template should be valid TOML");

    let value = parsed.unwrap();

    // Check required sections
    assert!(value.get("project").is_some(), "Should have project name");
    assert!(value.get("vm").is_some(), "Should have VM configuration");

    // Check VM configuration
    if let Some(vm) = value.get("vm").and_then(|v| v.as_table()) {
        if let Some(ml_pytorch) = vm.get("ml-pytorch").and_then(|v| v.as_table()) {
            assert!(ml_pytorch.get("image").is_some(), "VM should have image path");
            assert!(ml_pytorch.get("cpu").is_some(), "VM should have CPU config");
            assert!(ml_pytorch.get("memory").is_some(), "VM should have memory config");
            assert!(ml_pytorch.get("gpu_passthrough").is_some(), "VM should have GPU config");
            assert!(ml_pytorch.get("network").is_some(), "VM should have network config");
        }
    }

    // Check for CUDA configuration
    assert!(content.contains("cuda"), "Template should mention CUDA");
    assert!(content.contains("pytorch") || content.contains("PyTorch"), "Should mention PyTorch");
}

#[test]
fn test_ml_tensorflow_template_validation() {
    let template_path = PathBuf::from("examples/vm-templates/ml-tensorflow.toml");

    assert!(template_path.exists(), "ML TensorFlow template should exist");

    let content = fs::read_to_string(&template_path)
        .expect("Should be able to read template");

    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok(), "Template should be valid TOML");

    // Check for TensorFlow-specific content
    assert!(content.contains("tensorflow") || content.contains("TensorFlow"), "Should mention TensorFlow");
    assert!(content.contains("tensorboard") || content.contains("TensorBoard"), "Should mention TensorBoard");
}

#[test]
fn test_stable_diffusion_template_validation() {
    let template_path = PathBuf::from("examples/vm-templates/stable-diffusion.toml");

    assert!(template_path.exists(), "Stable Diffusion template should exist");

    let content = fs::read_to_string(&template_path)
        .expect("Should be able to read template");

    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok(), "Template should be valid TOML");

    // Check for SD-specific features
    assert!(content.contains("automatic1111") || content.contains("Automatic1111"), "Should mention Automatic1111");
    assert!(content.contains("comfyui") || content.contains("ComfyUI"), "Should mention ComfyUI");
    assert!(content.contains("looking-glass") || content.contains("LookingGlass"), "Should configure Looking Glass");

    // Check storage requirements (SD needs lots of storage)
    assert!(content.contains("500G") || content.contains("models"), "Should have large model storage");
}

#[test]
fn test_arch_nvidia_dev_template_validation() {
    let template_path = PathBuf::from("examples/vm-templates/arch-nvidia-dev.toml");

    assert!(template_path.exists(), "Arch NVIDIA dev template should exist");

    let content = fs::read_to_string(&template_path)
        .expect("Should be able to read template");

    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok(), "Template should be valid TOML");

    // Check for Arch-specific features
    assert!(content.contains("nvidia-open"), "Should use nvidia-open driver");
    assert!(content.contains("KDE") || content.contains("kde"), "Should mention KDE");
    assert!(content.contains("AUR") || content.contains("yay") || content.contains("paru"), "Should have AUR support");
    assert!(content.contains("wayland") || content.contains("Wayland"), "Should support Wayland");
}

#[test]
fn test_arch_gnome_nvidia_template_validation() {
    let template_path = PathBuf::from("examples/vm-templates/arch-gnome-nvidia.toml");

    assert!(template_path.exists(), "Arch GNOME NVIDIA template should exist");

    let content = fs::read_to_string(&template_path)
        .expect("Should be able to read template");

    let parsed: Result<toml::Value, _> = toml::from_str(&content);
    assert!(parsed.is_ok(), "Template should be valid TOML");

    // Check for GNOME-specific features
    assert!(content.contains("gnome") || content.contains("GNOME"), "Should mention GNOME");
    assert!(content.contains("wayland") || content.contains("Wayland"), "Should support Wayland");
    assert!(content.contains("nvidia-open"), "Should use nvidia-open driver");
    assert!(content.contains("gtk4") || content.contains("GTK"), "Should have GTK development tools");
}

#[test]
fn test_all_templates_have_gpu_passthrough() {
    let template_dir = PathBuf::from("examples/vm-templates");

    if !template_dir.exists() {
        println!("Template directory not found, skipping test");
        return;
    }

    let entries = fs::read_dir(&template_dir)
        .expect("Should be able to read template directory");

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = fs::read_to_string(&path)
                    .expect("Should be able to read template file");

                assert!(content.contains("gpu_passthrough") || content.contains("gpu"),
                        "Template {} should have GPU configuration", path.display());
            }
        }
    }
}

#[test]
fn test_all_templates_have_network_config() {
    let template_dir = PathBuf::from("examples/vm-templates");

    if !template_dir.exists() {
        println!("Template directory not found, skipping test");
        return;
    }

    let entries = fs::read_dir(&template_dir)
        .expect("Should be able to read template directory");

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = fs::read_to_string(&path)
                    .expect("Should be able to read template file");

                assert!(content.contains("network"),
                        "Template {} should have network configuration", path.display());
            }
        }
    }
}

#[test]
fn test_templates_have_valid_memory_sizes() {
    let template_dir = PathBuf::from("examples/vm-templates");

    if !template_dir.exists() {
        println!("Template directory not found, skipping test");
        return;
    }

    let entries = fs::read_dir(&template_dir)
        .expect("Should be able to read template directory");

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = fs::read_to_string(&path)
                    .expect("Should be able to read template file");

                // Check for memory specification
                if let Some(parsed) = toml::from_str::<toml::Value>(&content).ok() {
                    if let Some(vm) = parsed.get("vm").and_then(|v| v.as_table()) {
                        for (_, vm_config) in vm {
                            if let Some(memory) = vm_config.get("memory").and_then(|m| m.as_str()) {
                                // Validate memory format (should be like "16Gi", "32G", etc.)
                                assert!(memory.contains('G') || memory.contains('M'),
                                        "Memory size should have unit (G/M) in {}", path.display());
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn test_templates_have_valid_cpu_counts() {
    let template_dir = PathBuf::from("examples/vm-templates");

    if !template_dir.exists() {
        println!("Template directory not found, skipping test");
        return;
    }

    let entries = fs::read_dir(&template_dir)
        .expect("Should be able to read template directory");

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = fs::read_to_string(&path)
                    .expect("Should be able to read template file");

                if let Some(parsed) = toml::from_str::<toml::Value>(&content).ok() {
                    if let Some(vm) = parsed.get("vm").and_then(|v| v.as_table()) {
                        for (_, vm_config) in vm {
                            if let Some(cpu) = vm_config.get("cpu").and_then(|c| c.as_integer()) {
                                assert!(cpu > 0 && cpu <= 128,
                                        "CPU count should be reasonable (1-128) in {}", path.display());
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn test_ml_templates_have_cloud_init() {
    let ml_templates = vec![
        "examples/vm-templates/ml-pytorch.toml",
        "examples/vm-templates/ml-tensorflow.toml",
        "examples/vm-templates/stable-diffusion.toml",
    ];

    for template_path in ml_templates {
        let path = PathBuf::from(template_path);

        if !path.exists() {
            println!("Template {} not found, skipping", template_path);
            continue;
        }

        let content = fs::read_to_string(&path)
            .expect("Should be able to read template");

        assert!(content.contains("cloud_init") || content.contains("cloud-config"),
                "ML template {} should have cloud-init configuration", template_path);
    }
}

#[test]
fn test_arch_templates_have_post_install_scripts() {
    let arch_templates = vec![
        "examples/vm-templates/arch-nvidia-dev.toml",
        "examples/vm-templates/arch-gnome-nvidia.toml",
    ];

    for template_path in arch_templates {
        let path = PathBuf::from(template_path);

        if !path.exists() {
            println!("Template {} not found, skipping", template_path);
            continue;
        }

        let content = fs::read_to_string(&path)
            .expect("Should be able to read template");

        assert!(content.contains("post_install") || content.contains("script"),
                "Arch template {} should have post-install script", template_path);
    }
}

#[test]
fn test_templates_dont_have_hardcoded_secrets() {
    let template_dir = PathBuf::from("examples/vm-templates");

    if !template_dir.exists() {
        println!("Template directory not found, skipping test");
        return;
    }

    let dangerous_patterns = vec![
        "password = \"",
        "api_key = \"[A-Za-z0-9]",
        "secret = \"[A-Za-z0-9]",
        "token = \"[A-Za-z0-9]",
    ];

    let entries = fs::read_dir(&template_dir)
        .expect("Should be able to read template directory");

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = fs::read_to_string(&path)
                    .expect("Should be able to read template file");

                // Check content doesn't have hardcoded secrets
                // Allow empty strings for placeholders
                assert!(!content.contains("password = \"secret\""),
                        "Template {} should not have hardcoded passwords", path.display());
                assert!(!content.contains("api_key = \"sk-"),
                        "Template {} should not have hardcoded API keys", path.display());
            }
        }
    }
}

#[test]
fn test_stable_diffusion_has_model_storage_config() {
    let template_path = PathBuf::from("examples/vm-templates/stable-diffusion.toml");

    if !template_path.exists() {
        println!("Template not found, skipping test");
        return;
    }

    let content = fs::read_to_string(&template_path)
        .expect("Should be able to read template");

    // SD needs lots of storage for models
    assert!(content.contains("models"), "Should have model storage configuration");
    assert!(content.contains("output") || content.contains("outputs"), "Should have output storage");

    // Check for large storage sizes (SD models are big!)
    assert!(content.contains("500G") || content.contains("200G"),
            "Should have large storage allocations for models");
}

#[test]
fn test_templates_use_consistent_naming() {
    let template_dir = PathBuf::from("examples/vm-templates");

    if !template_dir.exists() {
        println!("Template directory not found, skipping test");
        return;
    }

    let entries = fs::read_dir(&template_dir)
        .expect("Should be able to read template directory");

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let filename = path.file_stem().unwrap().to_string_lossy();

                // Template filenames should use kebab-case
                assert!(filename.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                        "Template filename {} should use kebab-case", filename);
            }
        }
    }
}
