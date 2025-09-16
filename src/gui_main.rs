use eframe::egui;
use nova::{
    config::NovaConfig,
    container::ContainerManager,
    instance::{Instance, InstanceStatus, InstanceType},
    logger,
    theme,
    vm::VmManager,
};
use std::sync::{Arc, Mutex};

fn main() -> Result<(), eframe::Error> {
    // Initialize logging
    logger::init_logger();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_icon(eframe::icon_data::from_png_bytes(&[]).unwrap_or_default()),
        ..Default::default()
    };

    eframe::run_native(
        "Nova Manager",
        options,
        Box::new(|cc| Box::new(NovaApp::new(cc))),
    )
}

struct NovaApp {
    // Core managers
    vm_manager: Arc<VmManager>,
    container_manager: Arc<ContainerManager>,
    config: NovaConfig,

    // UI State
    selected_instance: Option<String>,
    selected_instance_type: InstanceType,
    show_console: bool,
    console_output: Vec<String>,

    // Instance data cache
    instances_cache: Arc<Mutex<Vec<Instance>>>,

    // UI panels
    show_properties: bool,
    show_snapshots: bool,
    show_networking: bool,
}

impl NovaApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Setup deep ocean theme
        theme::configure_ocean_theme(&cc.egui_ctx);

        let vm_manager = Arc::new(VmManager::new());
        let container_manager = Arc::new(ContainerManager::new());

        // Try to load config
        let config = NovaConfig::from_file("NovaFile").unwrap_or_default();

        Self {
            vm_manager,
            container_manager,
            config,
            selected_instance: None,
            selected_instance_type: InstanceType::Vm,
            show_console: false,
            console_output: vec![
                "Nova Manager v0.1.0 initialized".to_string(),
                "Ready for virtualization management".to_string(),
            ],
            instances_cache: Arc::new(Mutex::new(Vec::new())),
            show_properties: true,
            show_snapshots: false,
            show_networking: false,
        }
    }

    fn refresh_instances(&mut self) {
        let vms = self.vm_manager.list_vms();
        let containers = self.container_manager.list_containers();

        let mut all_instances = Vec::new();
        all_instances.extend(vms);
        all_instances.extend(containers);

        if let Ok(mut cache) = self.instances_cache.lock() {
            *cache = all_instances;
        }
    }
}

impl eframe::App for NovaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply ocean theme
        theme::configure_ocean_theme(ctx);

        // Menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New VM...").clicked() {
                        // TODO: Open new VM dialog
                    }
                    if ui.button("New Container...").clicked() {
                        // TODO: Open new container dialog
                    }
                    ui.separator();
                    if ui.button("Import...").clicked() {
                        // TODO: Import dialog
                    }
                    if ui.button("Export...").clicked() {
                        // TODO: Export dialog
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });

                ui.menu_button("Action", |ui| {
                    let has_selection = self.selected_instance.is_some();

                    if ui.add_enabled(has_selection, egui::Button::new("Start")).clicked() {
                        // TODO: Start selected instance
                    }
                    if ui.add_enabled(has_selection, egui::Button::new("Stop")).clicked() {
                        // TODO: Stop selected instance
                    }
                    if ui.add_enabled(has_selection, egui::Button::new("Restart")).clicked() {
                        // TODO: Restart selected instance
                    }
                    ui.separator();
                    if ui.add_enabled(has_selection, egui::Button::new("Take Snapshot")).clicked() {
                        // TODO: Snapshot dialog
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_console, "Console");
                    ui.checkbox(&mut self.show_properties, "Properties");
                    ui.checkbox(&mut self.show_snapshots, "Snapshots");
                    ui.checkbox(&mut self.show_networking, "Networking");
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About Nova").clicked() {
                        // TODO: About dialog
                    }
                });

                // Refresh button on the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔄 Refresh").clicked() {
                        self.refresh_instances();
                    }
                });
            });
        });

        // Main layout - three panels like Hyper-V Manager
        egui::SidePanel::left("instance_tree")
            .default_width(250.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Instances");
                ui.separator();

                // Instance tree view
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.collapsing("Virtual Machines", |ui| {
                        if let Ok(instances) = self.instances_cache.lock() {
                            for instance in instances.iter().filter(|i| i.instance_type == InstanceType::Vm) {
                                let selected = self.selected_instance.as_ref() == Some(&instance.name);

                                let response = ui.selectable_label(selected, &instance.name);
                                if response.clicked() {
                                    self.selected_instance = Some(instance.name.clone());
                                    self.selected_instance_type = InstanceType::Vm;
                                }

                                // Status indicator with ocean theme colors
                                let status_color = theme::get_status_color(&instance.status);
                                let status_icon = theme::get_status_icon(&instance.status);

                                ui.horizontal(|ui| {
                                    ui.add_space(20.0);
                                    ui.colored_label(status_color, format!("{} {:?}", status_icon, instance.status));
                                });
                            }
                        }
                    });

                    ui.collapsing("Containers", |ui| {
                        if let Ok(instances) = self.instances_cache.lock() {
                            for instance in instances.iter().filter(|i| i.instance_type == InstanceType::Container) {
                                let selected = self.selected_instance.as_ref() == Some(&instance.name);

                                let response = ui.selectable_label(selected, &instance.name);
                                if response.clicked() {
                                    self.selected_instance = Some(instance.name.clone());
                                    self.selected_instance_type = InstanceType::Container;
                                }

                                let status_color = theme::get_status_color(&instance.status);
                                let status_icon = theme::get_status_icon(&instance.status);

                                ui.horizontal(|ui| {
                                    ui.add_space(20.0);
                                    ui.colored_label(status_color, format!("{} {:?}", status_icon, instance.status));
                                });
                            }
                        }
                    });
                });
            });

        // Right panel for properties and details
        if self.show_properties {
            egui::SidePanel::right("properties")
                .default_width(300.0)
                .min_width(250.0)
                .show(ctx, |ui| {
                    ui.heading("Properties");
                    ui.separator();

                    if let Some(instance_name) = &self.selected_instance {
                        if let Ok(instances) = self.instances_cache.lock() {
                            if let Some(instance) = instances.iter().find(|i| &i.name == instance_name) {
                                ui.label(format!("Name: {}", instance.name));
                                ui.label(format!("Type: {:?}", instance.instance_type));
                                ui.label(format!("Status: {:?}", instance.status));
                                ui.label(format!("CPU Cores: {}", instance.cpu_cores));
                                ui.label(format!("Memory: {} MB", instance.memory_mb));

                                if let Some(pid) = instance.pid {
                                    ui.label(format!("PID: {}", pid));
                                }

                                if let Some(network) = &instance.network {
                                    ui.label(format!("Network: {}", network));
                                }

                                ui.label(format!("Created: {}", instance.created_at.format("%Y-%m-%d %H:%M:%S")));
                                ui.label(format!("Updated: {}", instance.last_updated.format("%Y-%m-%d %H:%M:%S")));

                                ui.separator();

                                // Action buttons
                                ui.horizontal(|ui| {
                                    match instance.status {
                                        InstanceStatus::Stopped => {
                                            if ui.button("▶ Start").clicked() {
                                                // TODO: Start instance
                                            }
                                        }
                                        InstanceStatus::Running => {
                                            if ui.button("⏹ Stop").clicked() {
                                                // TODO: Stop instance
                                            }
                                            if ui.button("⏸ Suspend").clicked() {
                                                // TODO: Suspend instance
                                            }
                                        }
                                        _ => {
                                            ui.label("Action pending...");
                                        }
                                    }
                                });
                            }
                        }
                    } else {
                        ui.label("No instance selected");
                        ui.label("Select an instance from the tree to view its properties.");
                    }
                });
        }

        // Bottom panel for console/logs
        if self.show_console {
            egui::TopBottomPanel::bottom("console")
                .default_height(150.0)
                .min_height(100.0)
                .show(ctx, |ui| {
                    ui.heading("Console");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.console_output {
                                ui.label(line);
                            }
                        });
                });
        }

        // Central panel - main workspace
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Nova Manager");

            if let Some(instance_name) = &self.selected_instance {
                ui.horizontal(|ui| {
                    ui.label("Managing:");
                    ui.strong(instance_name);
                    ui.label(format!("({:?})", self.selected_instance_type));
                });

                ui.separator();

                // Tabs for different views
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.show_snapshots && !self.show_networking, "Overview").clicked() {
                        self.show_snapshots = false;
                        self.show_networking = false;
                    }
                    if ui.selectable_label(self.show_snapshots, "Snapshots").clicked() {
                        self.show_snapshots = true;
                        self.show_networking = false;
                    }
                    if ui.selectable_label(self.show_networking, "Networking").clicked() {
                        self.show_snapshots = false;
                        self.show_networking = true;
                    }
                });

                ui.separator();

                if self.show_snapshots {
                    ui.heading("Snapshots");
                    ui.label("Snapshot management will be implemented here.");
                    // TODO: Implement snapshot UI
                } else if self.show_networking {
                    ui.heading("Network Configuration");
                    ui.label("Network configuration will be implemented here.");
                    // TODO: Implement networking UI
                } else {
                    // Overview tab
                    ui.heading("Overview");

                    if let Ok(instances) = self.instances_cache.lock() {
                        if let Some(instance) = instances.iter().find(|i| &i.name == instance_name) {
                            ui.columns(2, |columns| {
                                columns[0].group(|ui| {
                                    ui.label("System Information");
                                    ui.separator();
                                    ui.label(format!("CPU Cores: {}", instance.cpu_cores));
                                    ui.label(format!("Memory: {} MB", instance.memory_mb));
                                    ui.label(format!("Status: {:?}", instance.status));
                                });

                                columns[1].group(|ui| {
                                    ui.label("Configuration");
                                    ui.separator();
                                    ui.label(format!("Created: {}", instance.created_at.format("%Y-%m-%d")));
                                    if let Some(network) = &instance.network {
                                        ui.label(format!("Network: {}", network));
                                    }
                                });
                            });

                            ui.separator();

                            // Resource usage graphs would go here
                            ui.label("📊 Resource monitoring graphs will be implemented here");
                        }
                    }
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("Welcome to Nova Manager");
                    ui.label("Select a virtual machine or container from the left panel to manage it.");
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        if ui.button("Create New VM").clicked() {
                            // TODO: Open new VM wizard
                        }
                        if ui.button("Create New Container").clicked() {
                            // TODO: Open new container wizard
                        }
                    });
                });
            }
        });

        // Auto-refresh instances periodically
        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        self.refresh_instances();
    }
}

