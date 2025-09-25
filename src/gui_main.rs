use eframe::egui;
extern crate nova;

use chrono::{DateTime, Local, Utc};
use nova::{
    config::NovaConfig,
    container::ContainerManager,
    instance::{Instance, InstanceStatus, InstanceType},
    logger,
    network::{NetworkManager, NetworkSummary},
    theme,
    vm::VmManager,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::time::sleep;
use tracing::{error, info};

const MAX_CONSOLE_LINES: usize = 200;
const INSTANCE_REFRESH_SECONDS: u64 = 5;
const NETWORK_REFRESH_SECONDS: u64 = 15;

fn main() -> Result<(), eframe::Error> {
    logger::init_logger();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([840.0, 620.0])
            .with_icon(eframe::icon_data::from_png_bytes(&[]).unwrap_or_default()),
        ..Default::default()
    };

    eframe::run_native(
        "Nova Manager",
        options,
        Box::new(|cc| Box::new(NovaApp::new(cc))),
    )
}

#[derive(Default, Clone)]
struct InstanceSummary {
    total: usize,
    running: usize,
    stopped: usize,
    suspended: usize,
    errors: usize,
    pending: usize,
}

impl InstanceSummary {
    fn from_instances(instances: &[Instance]) -> Self {
        let mut summary = InstanceSummary::default();
        for instance in instances {
            summary.total += 1;
            match instance.status {
                InstanceStatus::Running => summary.running += 1,
                InstanceStatus::Stopped => summary.stopped += 1,
                InstanceStatus::Suspended => summary.suspended += 1,
                InstanceStatus::Error => summary.errors += 1,
                InstanceStatus::Starting | InstanceStatus::Stopping => summary.pending += 1,
            }
        }
        summary
    }
}

#[derive(Clone, Copy)]
enum InstanceAction {
    Start,
    Stop,
    Restart,
}

struct NovaApp {
    vm_manager: Arc<VmManager>,
    container_manager: Arc<ContainerManager>,
    network_manager: Arc<Mutex<NetworkManager>>,
    config: NovaConfig,
    runtime: Runtime,

    selected_instance: Option<String>,
    selected_instance_type: InstanceType,
    show_console: bool,
    console_output: Vec<String>,

    instances_cache: Vec<Instance>,
    summary: InstanceSummary,
    filter_text: String,
    only_running: bool,

    show_properties: bool,
    show_snapshots: bool,
    show_networking: bool,

    last_refresh: Option<Instant>,
    last_refresh_at: Option<DateTime<Utc>>,
    refresh_interval: Duration,

    last_network_refresh: Option<Instant>,
    network_refresh_interval: Duration,
    network_summary: Option<NetworkSummary>,
}

impl NovaApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::configure_ocean_theme(&cc.egui_ctx);

        let vm_manager = Arc::new(VmManager::new());
        let container_manager = Arc::new(ContainerManager::new());
        let network_manager = Arc::new(Mutex::new(NetworkManager::new()));
        let config = NovaConfig::from_file("NovaFile").unwrap_or_default();

        let runtime = Runtime::new().expect("failed to initialize Tokio runtime");

        let mut app = Self {
            vm_manager,
            container_manager,
            network_manager,
            config,
            runtime,
            selected_instance: None,
            selected_instance_type: InstanceType::Vm,
            show_console: false,
            console_output: Vec::new(),
            instances_cache: Vec::new(),
            summary: InstanceSummary::default(),
            filter_text: String::new(),
            only_running: false,
            show_properties: true,
            show_snapshots: false,
            show_networking: false,
            last_refresh: None,
            last_refresh_at: None,
            refresh_interval: Duration::from_secs(INSTANCE_REFRESH_SECONDS),
            last_network_refresh: None,
            network_refresh_interval: Duration::from_secs(NETWORK_REFRESH_SECONDS),
            network_summary: None,
        };

        app.log_console("Nova Manager v0.1.0 initialized");
        app.log_console("Ready for virtualization management");
        app.refresh_instances(true);
        app.refresh_network_summary(true);

        app
    }

    fn refresh_instances(&mut self, force: bool) {
        let should_refresh = force
            || self
                .last_refresh
                .map(|ts| ts.elapsed() >= self.refresh_interval)
                .unwrap_or(true);

        if !should_refresh {
            return;
        }

        let mut all_instances = self.vm_manager.list_vms();
        all_instances.extend(self.container_manager.list_containers());
        all_instances.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        self.instances_cache = all_instances;
        self.summary = InstanceSummary::from_instances(&self.instances_cache);
        self.last_refresh = Some(Instant::now());
        self.last_refresh_at = Some(Utc::now());

        if self
            .selected_instance
            .as_ref()
            .map(|name| !self.instances_cache.iter().any(|i| &i.name == name))
            .unwrap_or(false)
        {
            self.selected_instance = None;
        }
    }

    fn refresh_network_summary(&mut self, force: bool) {
        let should_refresh = force
            || self
                .last_network_refresh
                .map(|ts| ts.elapsed() >= self.network_refresh_interval)
                .unwrap_or(true);

        if !should_refresh {
            return;
        }

        let mut error_msg: Option<String> = None;

        if let Ok(mut manager) = self.network_manager.lock() {
            match self
                .runtime
                .block_on(async { manager.ensure_fresh_state().await })
            {
                Ok(_) => {
                    self.network_summary = Some(manager.summary());
                    self.last_network_refresh = Some(Instant::now());
                }
                Err(err) => {
                    error_msg = Some(format!("Network refresh failed: {}", err));
                }
            }
        }

        if let Some(msg) = error_msg {
            self.log_console(msg.clone());
            error!("{}", msg);
        }
    }

    fn log_console(&mut self, message: impl Into<String>) {
        let timestamp = Local::now().format("%H:%M:%S");
        let line = format!("[{timestamp}] {}", message.into());
        self.console_output.push(line);
        if self.console_output.len() > MAX_CONSOLE_LINES {
            let overflow = self.console_output.len() - MAX_CONSOLE_LINES;
            self.console_output.drain(0..overflow);
        }
    }

    fn selected_instance(&self) -> Option<&Instance> {
        let name = self.selected_instance.as_ref()?;
        self.instances_cache
            .iter()
            .find(|instance| &instance.name == name)
    }

    fn selected_instance_owned(&self) -> Option<Instance> {
        self.selected_instance().cloned()
    }

    fn should_display(&self, instance: &Instance, filter: &str) -> bool {
        if self.only_running && instance.status != InstanceStatus::Running {
            return false;
        }

        if filter.is_empty() {
            return true;
        }

        let filter = filter.to_lowercase();
        instance.name.to_lowercase().contains(&filter)
            || instance
                .network
                .as_ref()
                .map(|net| net.to_lowercase().contains(&filter))
                .unwrap_or(false)
    }

    fn compute_action_state(&self) -> (bool, bool, bool) {
        if let Some(instance) = self.selected_instance() {
            match instance.status {
                InstanceStatus::Running => (false, true, true),
                InstanceStatus::Stopped => (true, false, false),
                InstanceStatus::Suspended => (true, true, true),
                InstanceStatus::Error => (true, false, true),
                InstanceStatus::Starting | InstanceStatus::Stopping => (false, false, false),
            }
        } else {
            (false, false, false)
        }
    }

    fn handle_action(&mut self, action: InstanceAction) {
        let Some(instance) = self.selected_instance_owned() else {
            return;
        };
        let name = instance.name.clone();
        let instance_type = instance.instance_type;

        match (instance_type, action) {
            (InstanceType::Vm, InstanceAction::Start) => {
                let vm_manager = self.vm_manager.clone();
                self.log_console(format!("Starting VM '{}'", name));
                info!("Starting VM {name}");
                self.runtime.spawn(async move {
                    if let Err(err) = vm_manager.start_vm(&name, None).await {
                        error!("Failed to start VM {name}: {err:?}");
                    }
                });
            }
            (InstanceType::Vm, InstanceAction::Stop) => {
                let vm_manager = self.vm_manager.clone();
                self.log_console(format!("Stopping VM '{}'", name));
                info!("Stopping VM {name}");
                self.runtime.spawn(async move {
                    if let Err(err) = vm_manager.stop_vm(&name).await {
                        error!("Failed to stop VM {name}: {err:?}");
                    }
                });
            }
            (InstanceType::Vm, InstanceAction::Restart) => {
                let vm_manager = self.vm_manager.clone();
                self.log_console(format!("Restarting VM '{}'", name));
                info!("Restarting VM {name}");
                self.runtime.spawn(async move {
                    if let Err(err) = vm_manager.stop_vm(&name).await {
                        error!("Failed to stop VM {name}: {err:?}");
                        return;
                    }
                    sleep(Duration::from_millis(800)).await;
                    if let Err(err) = vm_manager.start_vm(&name, None).await {
                        error!("Failed to start VM {name}: {err:?}");
                    }
                });
            }
            (InstanceType::Container, InstanceAction::Start) => {
                let container_manager = self.container_manager.clone();
                self.log_console(format!("Starting container '{}'", name));
                info!("Starting container {name}");
                self.runtime.spawn(async move {
                    if let Err(err) = container_manager.start_container(&name, None).await {
                        error!("Failed to start container {name}: {err:?}");
                    }
                });
            }
            (InstanceType::Container, InstanceAction::Stop) => {
                let container_manager = self.container_manager.clone();
                self.log_console(format!("Stopping container '{}'", name));
                info!("Stopping container {name}");
                self.runtime.spawn(async move {
                    if let Err(err) = container_manager.stop_container(&name).await {
                        error!("Failed to stop container {name}: {err:?}");
                    }
                });
            }
            (InstanceType::Container, InstanceAction::Restart) => {
                let container_manager = self.container_manager.clone();
                self.log_console(format!("Restarting container '{}'", name));
                info!("Restarting container {name}");
                self.runtime.spawn(async move {
                    if let Err(err) = container_manager.stop_container(&name).await {
                        error!("Failed to stop container {name}: {err:?}");
                        return;
                    }
                    sleep(Duration::from_millis(600)).await;
                    if let Err(err) = container_manager.start_container(&name, None).await {
                        error!("Failed to start container {name}: {err:?}");
                    }
                });
            }
        }

        // Ensure UI picks up the latest state soon after actions
        self.refresh_instances(true);
        self.refresh_network_summary(true);
    }

    fn summary_chip(ui: &mut egui::Ui, label: &str, value: usize, color: egui::Color32) {
        egui::Frame::none()
            .fill(theme::BG_ELEVATED)
            .rounding(egui::Rounding::same(6.0))
            .stroke(egui::Stroke::new(1.0, color))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(label).color(color));
                    ui.heading(value.to_string());
                });
            });
    }

    fn draw_instance_tree(&mut self, ui: &mut egui::Ui, filter: &str) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.collapsing("Virtual Machines", |ui| {
                let vms: Vec<Instance> = self
                    .instances_cache
                    .iter()
                    .filter(|i| i.instance_type == InstanceType::Vm)
                    .filter(|instance| self.should_display(instance, filter))
                    .cloned()
                    .collect();

                for instance in vms.iter() {
                    self.draw_instance_entry(ui, instance);
                }
            });

            ui.collapsing("Containers", |ui| {
                let containers: Vec<Instance> = self
                    .instances_cache
                    .iter()
                    .filter(|i| i.instance_type == InstanceType::Container)
                    .filter(|instance| self.should_display(instance, filter))
                    .cloned()
                    .collect();

                for instance in containers.iter() {
                    self.draw_instance_entry(ui, instance);
                }
            });
        });
    }

    fn draw_instance_entry(&mut self, ui: &mut egui::Ui, instance: &Instance) {
        let selected = self
            .selected_instance
            .as_ref()
            .map(|name| name == &instance.name)
            .unwrap_or(false);

        let status_color = theme::get_status_color(&instance.status);
        let status_icon = theme::get_status_icon(&instance.status);

        let label = egui::RichText::new(format!("{} {}", status_icon, instance.name))
            .color(theme::TEXT_PRIMARY);

        let response = ui.selectable_label(selected, label);
        if response.clicked() {
            self.selected_instance = Some(instance.name.clone());
            self.selected_instance_type = instance.instance_type;
        }

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(18.0);
            ui.colored_label(status_color, format!("{:?}", instance.status));
            if let Some(network) = &instance.network {
                ui.small(format!("Network: {}", network));
            }
        });
        ui.add_space(6.0);
    }

    fn draw_properties_panel(
        &mut self,
        ui: &mut egui::Ui,
        can_start: bool,
        can_stop: bool,
        can_restart: bool,
    ) {
        ui.heading("Properties");
        ui.separator();

        if let Some(instance) = self.selected_instance() {
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

            ui.label(format!(
                "Created: {}",
                instance.created_at.format("%Y-%m-%d %H:%M:%S")
            ));
            ui.label(format!(
                "Updated: {}",
                instance.last_updated.format("%Y-%m-%d %H:%M:%S")
            ));

            ui.separator();

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_start, egui::Button::new("▶ Start"))
                    .clicked()
                {
                    self.handle_action(InstanceAction::Start);
                }
                if ui
                    .add_enabled(can_stop, egui::Button::new("⏹ Stop"))
                    .clicked()
                {
                    self.handle_action(InstanceAction::Stop);
                }
                if ui
                    .add_enabled(can_restart, egui::Button::new("🔁 Restart"))
                    .clicked()
                {
                    self.handle_action(InstanceAction::Restart);
                }
            });
        } else {
            ui.label("No instance selected");
            ui.label("Select an instance from the tree to view its properties.");
        }
    }

    fn draw_overview(&self, ui: &mut egui::Ui) {
        ui.heading("Overview");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            Self::summary_chip(ui, "Running", self.summary.running, theme::STATUS_RUNNING);
            Self::summary_chip(ui, "Stopped", self.summary.stopped, theme::STATUS_STOPPED);
            Self::summary_chip(ui, "Pending", self.summary.pending, theme::STATUS_WARNING);
            Self::summary_chip(ui, "Errors", self.summary.errors, theme::STATUS_STOPPED);
        });

        if let Some(updated_at) = self.last_refresh_at {
            ui.label(format!(
                "Last refresh: {}",
                updated_at.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
            ));
        }

        ui.separator();
        ui.heading("Network Health");

        if let Some(summary) = &self.network_summary {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Switches: {} total · {} active",
                    summary.total_switches, summary.active_switches
                ));
                ui.separator();
                ui.label(format!(
                    "Interfaces: {} up / {} down",
                    summary.interfaces_up, summary.interfaces_down
                ));
            });

            if let Some(last_refresh) = summary.last_refresh_at {
                ui.label(format!(
                    "Network scan at {}",
                    last_refresh
                        .with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M:%S")
                ));
            }
        } else {
            ui.label("Refreshing network summary…");
        }

        ui.separator();
        ui.label("Tip: Toggle the Networking tab to inspect bridge membership and host topology.");
    }

    fn draw_snapshots(&self, ui: &mut egui::Ui) {
        ui.heading("Snapshots");
        ui.label("Snapshot management will be implemented here.");
    }

    fn draw_networking(&self, ui: &mut egui::Ui, instance: &Instance) {
        ui.heading("Network Configuration");

        if let Some(network) = &instance.network {
            ui.label(format!("Attached to: {}", network));
        } else {
            ui.label("No network configured for this instance.");
        }

        ui.separator();

        if let Some(summary) = &self.network_summary {
            ui.label(format!(
                "Host switches: {} total ({} Nova-managed, {} system)",
                summary.total_switches, summary.nova_managed_switches, summary.system_switches
            ));
            ui.label(format!(
                "Interfaces up/down: {} / {}",
                summary.interfaces_up, summary.interfaces_down
            ));
        } else {
            ui.label("Network telemetry is still loading. Try refreshing in a moment.");
        }
    }

    fn draw_console(&self, ui: &mut egui::Ui) {
        ui.heading("Console");
        ui.separator();

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.console_output {
                    ui.monospace(line);
                }
            });
    }
}

impl eframe::App for NovaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::configure_ocean_theme(ctx);

        self.refresh_instances(false);
        self.refresh_network_summary(false);

        let filter = self.filter_text.trim().to_lowercase();
        let (can_start, can_stop, can_restart) = self.compute_action_state();

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New VM...").clicked() {}
                    if ui.button("New Container...").clicked() {}
                    ui.separator();
                    if ui.button("Import...").clicked() {}
                    if ui.button("Export...").clicked() {}
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });

                ui.menu_button("Action", |ui| {
                    if ui
                        .add_enabled(can_start, egui::Button::new("Start"))
                        .clicked()
                    {
                        self.handle_action(InstanceAction::Start);
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(can_stop, egui::Button::new("Stop"))
                        .clicked()
                    {
                        self.handle_action(InstanceAction::Stop);
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(can_restart, egui::Button::new("Restart"))
                        .clicked()
                    {
                        self.handle_action(InstanceAction::Restart);
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_console, "Console");
                    ui.checkbox(&mut self.show_properties, "Properties");
                    ui.checkbox(&mut self.show_snapshots, "Snapshots");
                    ui.checkbox(&mut self.show_networking, "Networking");
                });

                ui.menu_button("Help", |ui| if ui.button("About Nova").clicked() {});

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔄 Refresh").clicked() {
                        self.refresh_instances(true);
                        self.refresh_network_summary(true);
                        self.log_console("Manual refresh triggered");
                    }
                });
            });
        });

        egui::SidePanel::left("instance_tree")
            .default_width(260.0)
            .min_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Instances");
                if let Some(updated_at) = self.last_refresh_at {
                    ui.small(format!(
                        "Last sync: {}",
                        updated_at.with_timezone(&Local).format("%H:%M:%S")
                    ));
                }
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    Self::summary_chip(ui, "Running", self.summary.running, theme::STATUS_RUNNING);
                    Self::summary_chip(ui, "Stopped", self.summary.stopped, theme::STATUS_STOPPED);
                });

                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter_text)
                        .hint_text("Search instances or networks…"),
                );
                ui.checkbox(&mut self.only_running, "Running only");
                ui.separator();

                self.draw_instance_tree(ui, &filter);
            });

        if self.show_properties {
            egui::SidePanel::right("properties")
                .default_width(320.0)
                .min_width(260.0)
                .show(ctx, |ui| {
                    self.draw_properties_panel(ui, can_start, can_stop, can_restart);
                });
        }

        if self.show_console {
            egui::TopBottomPanel::bottom("console")
                .default_height(170.0)
                .min_height(120.0)
                .show(ctx, |ui| self.draw_console(ui));
        }

        let selected_instance = self.selected_instance_owned();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Nova Manager");
            ui.separator();

            if let Some(instance) = selected_instance.as_ref() {
                ui.horizontal(|ui| {
                    ui.label("Managing:");
                    ui.strong(&instance.name);
                    ui.label(format!("({:?})", instance.instance_type));
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(!self.show_snapshots && !self.show_networking, "Overview")
                        .clicked()
                    {
                        self.show_snapshots = false;
                        self.show_networking = false;
                    }
                    if ui
                        .selectable_label(self.show_snapshots, "Snapshots")
                        .clicked()
                    {
                        self.show_snapshots = true;
                        self.show_networking = false;
                    }
                    if ui
                        .selectable_label(self.show_networking, "Networking")
                        .clicked()
                    {
                        self.show_snapshots = false;
                        self.show_networking = true;
                    }
                });

                ui.separator();

                if self.show_snapshots {
                    self.draw_snapshots(ui);
                } else if self.show_networking {
                    self.draw_networking(ui, instance);
                } else {
                    self.draw_overview(ui);

                    ui.separator();
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
                            ui.label(format!(
                                "Created: {}",
                                instance.created_at.format("%Y-%m-%d")
                            ));
                            if let Some(network) = &instance.network {
                                ui.label(format!("Network: {}", network));
                            }
                        });
                    });

                    ui.separator();
                    ui.label("📊 Resource monitoring graphs will be implemented here");
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("Welcome to Nova Manager");
                    ui.label(
                        "Select a virtual machine or container from the left panel to manage it.",
                    );
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        if ui.button("Create New VM").clicked() {}
                        if ui.button("Create New Container").clicked() {}
                    });
                });
            }
        });

        ctx.request_repaint_after(self.refresh_interval.min(self.network_refresh_interval));
    }
}
