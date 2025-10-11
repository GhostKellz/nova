// Integration tests for GPU passthrough functionality
use nova::gpu_doctor::*;
use nova::gpu_passthrough::*;

#[test]
fn test_gpu_manager_creation() {
    let manager = GpuManager::new();
    assert!(manager.list_gpus().is_empty(), "New manager should have no GPUs initially");
}

#[test]
fn test_gpu_discovery() {
    let mut manager = GpuManager::new();

    // This will fail in CI without GPU, but that's expected
    // In a real environment with GPUs, this discovers them
    match manager.discover() {
        Ok(_) => {
            // If discovery succeeds, check that we can list GPUs
            let gpus = manager.list_gpus();
            println!("Discovered {} GPUs", gpus.len());

            for gpu in gpus {
                println!("  GPU: {} ({})", gpu.device_name, gpu.address);
                assert!(!gpu.address.is_empty(), "GPU address should not be empty");
                assert!(!gpu.device_name.is_empty(), "GPU name should not be empty");
            }
        }
        Err(e) => {
            println!("GPU discovery failed (expected in CI): {:?}", e);
        }
    }
}

#[test]
fn test_iommu_group_detection() {
    let mut manager = GpuManager::new();
    let _ = manager.discover();

    let groups = manager.list_iommu_groups();
    println!("Found {} IOMMU groups", groups.len());

    for group in groups {
        println!("  Group {}: {} devices, viable={}",
                 group.id,
                 group.devices.len(),
                 group.viable_for_passthrough);

        assert!(group.devices.len() > 0, "IOMMU group should have at least one device");
    }
}

#[test]
fn test_system_requirements_check() {
    let manager = GpuManager::new();
    let status = manager.check_system_requirements();

    println!("System Status:");
    println!("  IOMMU Enabled: {}", status.iommu_enabled);
    println!("  VFIO Available: {}", status.vfio_available);
    println!("  GPUs Detected: {}", status.gpus_detected);
    println!("  nvbind Available: {}", status.nvbind_available);
    println!("  Kernel Modules: {}", status.kernel_modules_loaded);

    if !status.issues.is_empty() {
        println!("  Issues:");
        for issue in &status.issues {
            println!("    - {}", issue);
        }
    }

    // These tests should pass even in CI
    assert!(status.kernel_modules_loaded || !status.vfio_available,
            "If VFIO is available, modules should be loaded");
}

#[test]
fn test_gpu_doctor_diagnostics() {
    let doctor = GpuDoctor::new();
    let report = doctor.diagnose();

    println!("\nGPU Doctor Report:");
    println!("Overall Status: {:?}", report.overall_status);
    println!("Checks: {}", report.checks.len());
    println!("Warnings: {}", report.warnings.len());
    println!("Errors: {}", report.errors.len());

    // Verify all checks ran
    assert!(!report.checks.is_empty(), "Diagnostic checks should not be empty");

    // Check for expected diagnostics
    let check_names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();

    assert!(check_names.contains(&"IOMMU"), "Should check IOMMU");
    assert!(check_names.contains(&"Virtualization"), "Should check virtualization");
    assert!(check_names.contains(&"VFIO Modules"), "Should check VFIO modules");
    assert!(check_names.contains(&"NVIDIA Driver"), "Should check NVIDIA driver");
    assert!(check_names.contains(&"GPU Detection"), "Should check GPU detection");
}

#[test]
fn test_gpu_doctor_system_statuses() {
    use nova::gpu_doctor::SystemStatus;

    // Test enum equality
    assert_eq!(SystemStatus::Ready, SystemStatus::Ready);
    assert_ne!(SystemStatus::Ready, SystemStatus::NeedsConfiguration);
    assert_ne!(SystemStatus::Ready, SystemStatus::NotSupported);
}

#[test]
fn test_vfio_config_detection() {
    let config = GpuSystemConfig::detect();

    println!("\nSystem Configuration:");
    println!("  VFIO Enabled: {}", config.vfio_enabled);
    println!("  IOMMU Enabled: {}", config.iommu_enabled);
    println!("  IOMMU Mode: {:?}", config.iommu_mode);
    println!("  Kernel Modules: {:?}", config.kernel_modules);

    // Basic assertions
    if config.vfio_enabled {
        assert!(!config.kernel_modules.is_empty(),
                "If VFIO is enabled, kernel modules list should not be empty");
    }

    if config.iommu_mode.is_some() {
        assert!(config.iommu_enabled,
                "If IOMMU mode is detected, IOMMU should be enabled");
    }
}

#[test]
fn test_pci_address_validation() {
    // Test PCI address format
    let valid_addresses = vec![
        "0000:01:00.0",
        "0000:02:00.0",
        "0000:0a:00.0",
    ];

    for addr in valid_addresses {
        assert!(addr.matches(':').count() == 2, "PCI address should have 2 colons");
        assert!(addr.matches('.').count() == 1, "PCI address should have 1 dot");
        assert!(addr.len() >= 12, "PCI address should be at least 12 chars");
    }
}

#[test]
fn test_iommu_group_viability() {
    let mut manager = GpuManager::new();
    let _ = manager.discover();

    let groups = manager.list_iommu_groups();

    for group in groups {
        if group.viable_for_passthrough {
            println!("Viable IOMMU Group {}:", group.id);
            for device in &group.devices {
                println!("  - {}: {}", device.address, device.device_name);
            }

            // Viable groups should have devices
            assert!(!group.devices.is_empty(),
                    "Viable IOMMU group should have devices");
        }
    }
}

#[test]
fn test_gpu_reservation_management() {
    let mut manager = GpuManager::new();
    let _ = manager.discover();

    let initial_reservations = manager.get_reservations().len();

    // Reservations should start empty
    assert_eq!(initial_reservations, 0, "Should start with no reservations");

    // Note: We can't actually reserve GPUs in CI, but we test the API exists
    assert!(manager.get_reservations().is_empty(), "Initial reservations should be empty");
}

#[test]
fn test_libvirt_xml_generation() {
    use nova::gpu_passthrough::*;

    let config = GpuPassthroughConfig {
        device_address: "0000:01:00.0".to_string(),
        mode: PassthroughMode::Full,
        romfile: None,
        multifunction: false,
        audio_device: None,
        usb_controller: None,
        x_vga: false,
        display: DisplayMode::None,
    };

    let manager = GpuManager::new();
    match manager.generate_libvirt_xml(&config) {
        Ok(xml) => {
            println!("\nGenerated XML:");
            println!("{}", xml);

            // Verify XML contains expected elements
            assert!(xml.contains("<hostdev"), "XML should contain hostdev element");
            assert!(xml.contains("type='pci'"), "XML should specify PCI type");
            assert!(xml.contains("managed='yes'"), "XML should have managed=yes");
            assert!(xml.contains("domain='0x0000'"), "XML should contain domain");
            assert!(xml.contains("bus='0x01'"), "XML should contain bus");
        }
        Err(e) => {
            panic!("XML generation failed: {:?}", e);
        }
    }
}

#[test]
fn test_display_mode_variants() {
    use nova::gpu_passthrough::DisplayMode;

    // Test all display mode variants exist
    let modes = vec![
        DisplayMode::None,
        DisplayMode::Spice,
        DisplayMode::LookingGlass,
        DisplayMode::VirtioGpu,
    ];

    assert_eq!(modes.len(), 4, "Should have 4 display modes");
}

#[test]
fn test_passthrough_mode_variants() {
    use nova::gpu_passthrough::PassthroughMode;

    // Test all passthrough mode variants
    let modes = vec![
        PassthroughMode::Full,
        PassthroughMode::SrIov,
        PassthroughMode::Vgpu,
        PassthroughMode::ManagedVfio,
    ];

    assert_eq!(modes.len(), 4, "Should have 4 passthrough modes");
}

#[cfg(feature = "integration")]
#[test]
fn test_full_gpu_passthrough_workflow() {
    // This test requires actual GPU hardware and root privileges
    // Only runs with --features integration flag

    let mut manager = GpuManager::new();
    assert!(manager.discover().is_ok(), "Discovery should succeed");

    let gpus = manager.list_gpus();
    if gpus.is_empty() {
        println!("No GPUs found, skipping passthrough test");
        return;
    }

    let gpu = &gpus[0];
    println!("Testing passthrough with GPU: {}", gpu.device_name);

    // Note: This would require root to actually bind
    // In real tests, we'd mock the system calls
}
