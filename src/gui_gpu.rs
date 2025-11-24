use crate::gpu_doctor::{CheckStatus, DiagnosticReport as DoctorReport, GpuDoctor, SystemStatus};
use crate::gpu_passthrough::{GpuCapabilities, GpuManager, PciDevice};
use eframe::egui;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum GpuTab {
    Manager,
    IommuGroups,
    Diagnostics,
}

pub struct GpuManagerGui {
    gpu_manager: Arc<Mutex<GpuManager>>,

    // UI state
    selected_gpu: Option<String>,
    selected_vm: String,
    active_tab: GpuTab,

    // GPU list cache
    gpus: Vec<PciDevice>,
    iommu_groups: HashMap<u32, Vec<String>>,

    // Reservations (PCI address -> VM name)
    reservations: HashMap<String, String>,

    // Cached GPU capabilities
    capabilities: HashMap<String, GpuCapabilities>,

    // Diagnostics
    diagnostic_text: String,
    diagnostic_report: Option<DoctorReport>,

    // Messages
    last_message: Option<String>,
    last_error: Option<String>,
}

impl GpuManagerGui {
    pub fn new(gpu_manager: Arc<Mutex<GpuManager>>) -> Self {
        Self {
            gpu_manager,
            selected_gpu: None,
            selected_vm: String::new(),
            active_tab: GpuTab::Manager,
            gpus: Vec::new(),
            iommu_groups: HashMap::new(),
            reservations: HashMap::new(),
            capabilities: HashMap::new(),
            diagnostic_text: String::new(),
            diagnostic_report: None,
            last_message: None,
            last_error: None,
        }
    }

    /// Refresh GPU list and IOMMU groups
    pub fn refresh(&mut self) {
        if let Ok(mut manager) = self.gpu_manager.lock() {
            // Discover GPUs
            let _ = manager.discover();

            self.gpus = manager.list_gpus().iter().cloned().collect();

            // Build IOMMU group map
            self.iommu_groups.clear();
            for gpu in &self.gpus {
                if let Some(group) = gpu.iommu_group {
                    self.iommu_groups
                        .entry(group)
                        .or_insert_with(Vec::new)
                        .push(gpu.address.clone());
                }
            }

            // Update reservations
            self.reservations.clear();
            let reservations = manager.get_reservations();
            for (addr, vm) in reservations {
                self.reservations.insert(addr.clone(), vm.clone());
            }

            // Cache capabilities for quick UI access
            self.capabilities.clear();
            for gpu in &self.gpus {
                if let Some(caps) = manager.capabilities_for(&gpu.address).cloned() {
                    self.capabilities.insert(gpu.address.clone(), caps);
                }
            }
        }
    }

    /// Run diagnostics
    pub fn run_diagnostics(&mut self) {
        let doctor = GpuDoctor::new();
        let report = doctor.diagnose();

        self.diagnostic_text = Self::format_report_output(&report);
        self.diagnostic_report = Some(report);
    }

    fn format_report_output(report: &DoctorReport) -> String {
        let mut buffer = String::new();
        buffer.push_str(&format!("Overall Status: {:?}\n\n", report.overall_status));

        for check in &report.checks {
            let symbol = match check.status {
                CheckStatus::Pass => "✓",
                CheckStatus::Warn => "⚠",
                CheckStatus::Fail => "✗",
            };
            buffer.push_str(&format!("{} {}: {}\n", symbol, check.name, check.message));
            if let Some(fix) = &check.fix_command {
                buffer.push_str(&format!("    fix: {}\n", fix));
            }
        }

        if !report.errors.is_empty() {
            buffer.push_str("\nErrors:\n");
            for error in &report.errors {
                buffer.push_str(&format!("  {}\n", error));
            }
        }

        if !report.warnings.is_empty() {
            buffer.push_str("\nWarnings:\n");
            for warning in &report.warnings {
                buffer.push_str(&format!("  {}\n", warning));
            }
        }

        if !report.recommendations.is_empty() {
            buffer.push_str("\nRecommendations:\n");
            for rec in &report.recommendations {
                buffer.push_str(&format!("  {}\n", rec));
            }
        }

        buffer
    }

    /// Assign GPU to VM
    fn assign_gpu(&mut self, pci_address: String, vm_name: String) {
        let result = if let Ok(mut manager) = self.gpu_manager.lock() {
            manager.configure_passthrough(&pci_address, &vm_name)
        } else {
            return;
        };

        match result {
            Ok(_) => {
                self.last_message = Some(format!(
                    "Successfully assigned GPU {} to VM '{}'",
                    pci_address, vm_name
                ));
                self.last_error = None;
                self.refresh();
            }
            Err(e) => {
                self.last_error = Some(format!(
                    "Failed to assign GPU {} to VM '{}': {:?}",
                    pci_address, vm_name, e
                ));
                self.last_message = None;
            }
        }
    }

    /// Release GPU from VM
    fn release_gpu(&mut self, pci_address: String) {
        let result = if let Ok(mut manager) = self.gpu_manager.lock() {
            manager.release_gpu(&pci_address)
        } else {
            return;
        };

        match result {
            Ok(_) => {
                self.last_message = Some(format!("Successfully released GPU {}", pci_address));
                self.last_error = None;
                self.refresh();
            }
            Err(e) => {
                self.last_error = Some(format!("Failed to release GPU {}: {:?}", pci_address, e));
                self.last_message = None;
            }
        }
    }

    /// Draw the main GPU manager panel
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        ui.heading("GPU Passthrough Manager");
        ui.separator();

        // Tab selection
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_tab, GpuTab::Manager, "GPU Manager");
            ui.selectable_value(&mut self.active_tab, GpuTab::IommuGroups, "IOMMU Groups");
            ui.selectable_value(&mut self.active_tab, GpuTab::Diagnostics, "Diagnostics");
        });

        ui.separator();

        // Display messages
        if let Some(msg) = &self.last_message {
            ui.colored_label(egui::Color32::from_rgb(96, 200, 140), format!("✓ {}", msg));
        }
        if let Some(err) = &self.last_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), format!("✗ {}", err));
        }

        ui.add_space(8.0);

        // Refresh button
        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh GPU List").clicked() {
                self.refresh();
            }

            if ui.button("🩺 Run Diagnostics").clicked() {
                self.run_diagnostics();
                self.active_tab = GpuTab::Diagnostics;
            }
        });

        ui.add_space(8.0);

        // Draw active tab
        match self.active_tab {
            GpuTab::Manager => self.draw_manager_tab(ui),
            GpuTab::IommuGroups => self.draw_iommu_tab(ui),
            GpuTab::Diagnostics => self.draw_diagnostics_tab(ui),
        }
    }

    /// Draw GPU manager tab
    fn draw_manager_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Available GPUs");
        ui.separator();

        if self.gpus.is_empty() {
            ui.group(|ui| {
                ui.label("No GPUs detected");
                ui.small("Click 'Refresh GPU List' to scan for GPUs");
            });
            return;
        }

        // GPU assignment section
        ui.group(|ui| {
            ui.label(egui::RichText::new("Quick Assignment").strong());
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("VM Name:");
                ui.text_edit_singleline(&mut self.selected_vm);
            });
        });

        ui.add_space(8.0);

        // GPU list
        egui::ScrollArea::vertical()
            .max_height(400.0)
            .show(ui, |ui| {
                for gpu in self.gpus.clone().iter() {
                    self.draw_gpu_card(ui, gpu);
                    ui.add_space(8.0);
                }
            });
    }

    /// Draw individual GPU card
    fn draw_gpu_card(&mut self, ui: &mut egui::Ui, gpu: &PciDevice) {
        let is_selected = self.selected_gpu.as_ref() == Some(&gpu.address);
        let is_assigned = self.reservations.contains_key(&gpu.address);

        egui::Frame::none()
            .fill(if is_selected {
                egui::Color32::from_rgb(45, 55, 75)
            } else {
                egui::Color32::from_rgb(30, 35, 45)
            })
            .stroke(egui::Stroke::new(
                if is_selected { 2.0 } else { 1.0 },
                if is_selected {
                    egui::Color32::from_rgb(96, 170, 255)
                } else {
                    egui::Color32::from_gray(80)
                },
            ))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                // Header with vendor logo color
                ui.horizontal(|ui| {
                    // Determine vendor color
                    let vendor_color = if gpu.vendor_id == "10de" {
                        egui::Color32::from_rgb(118, 185, 0) // NVIDIA
                    } else if gpu.vendor_id == "1002" {
                        egui::Color32::from_rgb(237, 28, 36) // AMD
                    } else if gpu.vendor_id == "8086" {
                        egui::Color32::from_rgb(0, 113, 197) // Intel
                    } else {
                        egui::Color32::from_gray(160)
                    };

                    ui.colored_label(
                        vendor_color,
                        egui::RichText::new(format!("■ {}", gpu.vendor_name)).strong(),
                    );
                });

                ui.label(egui::RichText::new(&gpu.device_name).strong());
                ui.add_space(4.0);

                // PCI address and IOMMU group
                ui.horizontal(|ui| {
                    ui.monospace(format!("PCI: {}", gpu.address));
                    if let Some(group) = gpu.iommu_group {
                        ui.label(format!("IOMMU Group: {}", group));
                    }
                });

                // Device IDs
                ui.small(format!("{}:{}", gpu.vendor_id, gpu.device_id));
                ui.add_space(4.0);

                if let Some(caps) = self.capabilities.get(&gpu.address) {
                    let generation = caps
                        .generation
                        .as_ref()
                        .map(|g| g.to_string())
                        .unwrap_or_else(|| "Unknown".to_string());
                    let min_driver = caps.minimum_driver.as_deref().unwrap_or("-");
                    let recommended_kernel = caps.recommended_kernel.as_deref().unwrap_or("-");
                    let tcc_status = if caps.tcc_supported { "Yes" } else { "No" };

                    ui.horizontal(|ui| {
                        ui.label(format!("Generation: {}", generation));
                        ui.label(format!("Min Driver: {}", min_driver));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("Kernel: {}", recommended_kernel));
                        ui.label(format!("TCC: {}", tcc_status));
                    });

                    if let Some(vram_mb) = caps.vram_mb {
                        ui.label(format!("VRAM: {} MB", vram_mb));
                    }

                    ui.add_space(4.0);
                }

                // Status indicator
                let (status_text, status_color) =
                    if let Some(vm) = self.reservations.get(&gpu.address) {
                        (
                            format!("Assigned to VM: {}", vm),
                            egui::Color32::from_rgb(102, 220, 144),
                        )
                    } else if gpu.driver.as_deref() == Some("vfio-pci") {
                        (
                            "Ready for passthrough".to_string(),
                            egui::Color32::from_rgb(255, 200, 100),
                        )
                    } else if let Some(driver) = &gpu.driver {
                        (
                            format!("Driver: {}", driver),
                            egui::Color32::from_rgb(160, 160, 160),
                        )
                    } else {
                        (
                            "No driver".to_string(),
                            egui::Color32::from_rgb(160, 160, 160),
                        )
                    };

                ui.colored_label(status_color, status_text);
                ui.add_space(6.0);

                // Action buttons
                let address = gpu.address.clone();
                let vm_name = self.selected_vm.clone();

                ui.horizontal(|ui| {
                    if is_assigned {
                        if ui.button("Release").clicked() {
                            self.release_gpu(address);
                        }
                    } else {
                        if ui.button("Assign to VM").clicked() {
                            if !vm_name.is_empty() {
                                self.assign_gpu(address, vm_name);
                            } else {
                                self.last_error = Some("Please enter a VM name".to_string());
                            }
                        }
                    }

                    if ui.button("Select").clicked() {
                        self.selected_gpu = Some(gpu.address.clone());
                    }
                });
            });
    }

    /// Draw IOMMU groups tab
    fn draw_iommu_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("IOMMU Groups Visualization");
        ui.separator();

        ui.label("IOMMU groups determine which devices can be passed through together.");
        ui.small("Devices in the same IOMMU group must be isolated together for passthrough.");

        ui.add_space(8.0);

        if self.iommu_groups.is_empty() {
            ui.group(|ui| {
                ui.label("No IOMMU groups detected");
                ui.small("IOMMU may not be enabled or no compatible devices found");
            });
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(500.0)
            .show(ui, |ui| {
                let mut groups: Vec<_> = self.iommu_groups.iter().collect();
                groups.sort_by_key(|(group_id, _)| *group_id);

                for (group_id, devices) in groups {
                    egui::CollapsingHeader::new(format!("IOMMU Group {}", group_id))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.group(|ui| {
                                ui.label(format!("{} device(s) in group", devices.len()));

                                for pci_address in devices {
                                    if let Some(gpu) =
                                        self.gpus.iter().find(|g| &g.address == pci_address)
                                    {
                                        ui.horizontal(|ui| {
                                            ui.monospace(pci_address);
                                            ui.label(&gpu.device_name);

                                            let (status, color) = if self
                                                .reservations
                                                .contains_key(pci_address)
                                            {
                                                ("Assigned", egui::Color32::from_rgb(102, 220, 144))
                                            } else if gpu.driver.as_deref() == Some("vfio-pci") {
                                                ("VFIO", egui::Color32::from_rgb(255, 200, 100))
                                            } else {
                                                ("Active", egui::Color32::from_gray(160))
                                            };

                                            ui.colored_label(color, status);
                                        });
                                    } else {
                                        ui.monospace(pci_address);
                                    }
                                }

                                // Check if entire group is ready for passthrough
                                let all_vfio = devices.iter().all(|addr| {
                                    self.gpus
                                        .iter()
                                        .find(|g| &g.address == addr)
                                        .and_then(|g| g.driver.as_ref())
                                        .map(|d| d == "vfio-pci")
                                        .unwrap_or(false)
                                });

                                if all_vfio {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(96, 200, 140),
                                        "✓ Entire group ready for passthrough",
                                    );
                                } else {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(220, 120, 80),
                                        "⚠ Some devices not bound to VFIO",
                                    );
                                }
                            });
                        });

                    ui.add_space(6.0);
                }
            });
    }

    /// Draw diagnostics tab
    fn draw_diagnostics_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("GPU Passthrough Diagnostics");
        ui.separator();

        if self.diagnostic_report.is_none() {
            if self.diagnostic_text.is_empty() {
                ui.group(|ui| {
                    ui.label("No diagnostics run yet");
                    ui.small("Click 'Run Diagnostics' to check GPU passthrough readiness");
                });
                return;
            }

            egui::ScrollArea::vertical()
                .max_height(500.0)
                .show(ui, |ui| {
                    ui.monospace(&self.diagnostic_text);
                });
            ui.group(|ui| {
                ui.small("Structured diagnostics will appear here after the next run");
            });
            return;
        }

        let report = self
            .diagnostic_report
            .as_ref()
            .expect("diagnostic report cached");

        let (status_text, status_color, status_icon) = match report.overall_status {
            SystemStatus::Ready => ("Ready", egui::Color32::from_rgb(96, 200, 140), "✓"),
            SystemStatus::NeedsConfiguration => (
                "Needs Configuration",
                egui::Color32::from_rgb(255, 170, 0),
                "⚠",
            ),
            SystemStatus::NotSupported => {
                ("Not Supported", egui::Color32::from_rgb(220, 80, 80), "✗")
            }
        };

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.colored_label(status_color, format!("{} {}", status_icon, status_text));
                ui.label(format!(
                    "Checks: {} • Warnings: {} • Errors: {}",
                    report.checks.len(),
                    report.warnings.len(),
                    report.errors.len()
                ));
            });
        });

        ui.add_space(8.0);

        egui::Grid::new("gpu_diagnostics_checks")
            .striped(true)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Status").strong());
                ui.label(egui::RichText::new("Check").strong());
                ui.label(egui::RichText::new("Details").strong());
                ui.label(egui::RichText::new("Resolution").strong());
                ui.end_row();

                for check in &report.checks {
                    let (icon, color) = match check.status {
                        CheckStatus::Pass => ("✓", egui::Color32::from_rgb(96, 200, 140)),
                        CheckStatus::Warn => ("⚠", egui::Color32::from_rgb(255, 170, 0)),
                        CheckStatus::Fail => ("✗", egui::Color32::from_rgb(220, 80, 80)),
                    };

                    ui.colored_label(color, icon);
                    ui.label(&check.name);
                    ui.label(&check.message);
                    if let Some(fix) = &check.fix_command {
                        ui.monospace(fix);
                    } else {
                        ui.label("—");
                    }
                    ui.end_row();
                }
            });

        ui.add_space(8.0);

        if !report.recommendations.is_empty() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Recommended Actions").strong());
                ui.add_space(4.0);
                for rec in &report.recommendations {
                    ui.small(format!("• {}", rec));
                }
            });
            ui.add_space(8.0);
        }

        if !report.errors.is_empty() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Errors").strong());
                ui.add_space(4.0);
                for error in &report.errors {
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
                }
            });
            ui.add_space(8.0);
        }

        if !report.warnings.is_empty() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Warnings").strong());
                ui.add_space(4.0);
                for warning in &report.warnings {
                    ui.colored_label(egui::Color32::from_rgb(255, 170, 0), warning);
                }
            });
            ui.add_space(8.0);
        }

        egui::CollapsingHeader::new("Raw Diagnostic Output")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        ui.monospace(&self.diagnostic_text);
                    });
            });
    }
}

/// Standalone window for GPU manager
pub struct GpuManagerWindow {
    gui: GpuManagerGui,
    open: bool,
}

impl GpuManagerWindow {
    pub fn new(gpu_manager: Arc<Mutex<GpuManager>>) -> Self {
        let mut gui = GpuManagerGui::new(gpu_manager);
        gui.refresh();

        Self { gui, open: true }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("GPU Passthrough Manager")
            .open(&mut self.open)
            .default_size([900.0, 600.0])
            .resizable(true)
            .show(ctx, |ui| {
                self.gui.draw(ui);
            });
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_manager_gui_creation() {
        let manager = Arc::new(Mutex::new(GpuManager::new()));
        let _gui = GpuManagerGui::new(manager);
    }
}
