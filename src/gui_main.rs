use eframe::egui;
use nova::{
    config::NovaConfig,
    console_enhanced::{
        ActiveProtocol, EnhancedConsoleConfig, EnhancedConsoleManager, UnifiedConsoleSession,
    },
    container::ContainerManager,
    instance::{Instance, InstanceStatus, InstanceType},
    logger,
    network::{
        InterfaceState, NetworkInterface, NetworkManager, NetworkSummary, SwitchOrigin,
        SwitchProfile, SwitchStatus, SwitchType, VirtualSwitch,
    },
    templates_snapshots::{OperatingSystem, TemplateManager, VmTemplate},
    theme,
    vm::VmManager,
};

use chrono::{DateTime, Local, Utc};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::Mutex as AsyncMutex;
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

#[derive(Default, Clone)]
struct TemplateCatalogSummary {
    total: usize,
    linux: usize,
    windows: usize,
    other: usize,
    recent: Vec<String>,
    last_refresh: Option<DateTime<Utc>>,
}

impl TemplateCatalogSummary {
    fn from_manager(manager: &TemplateManager) -> Self {
        let mut templates: Vec<&VmTemplate> = manager.list_templates();
        templates.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let mut summary = TemplateCatalogSummary {
            total: templates.len(),
            ..TemplateCatalogSummary::default()
        };

        for template in &templates {
            match template.os_type {
                OperatingSystem::Linux { .. } => summary.linux += 1,
                OperatingSystem::Windows { .. } => summary.windows += 1,
                OperatingSystem::Other { .. } => summary.other += 1,
            }

            if summary.recent.len() < 3 {
                summary.recent.push(template.name.clone());
            }
        }

        summary.last_refresh = Some(Utc::now());
        summary
    }
}

#[derive(Clone, Copy)]
enum InstanceAction {
    Start,
    Stop,
    Restart,
}

#[derive(Debug)]
enum SessionEvent {
    Launched(UnifiedConsoleSession),
    Error { vm: String, message: String },
    Closed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Overview,
    Snapshots,
    Networking,
    Sessions,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SwitchProfileMode {
    Internal,
    External,
    Nat,
}

struct NovaApp {
    vm_manager: Arc<VmManager>,
    container_manager: Arc<ContainerManager>,
    network_manager: Arc<Mutex<NetworkManager>>,
    enhanced_console: Arc<AsyncMutex<EnhancedConsoleManager>>,
    template_manager: Arc<AsyncMutex<TemplateManager>>,
    session_events: Arc<Mutex<Vec<SessionEvent>>>,
    _config: NovaConfig,
    runtime: Runtime,
    template_summary: TemplateCatalogSummary,

    selected_instance: Option<String>,
    show_console: bool,
    console_output: Vec<String>,
    active_sessions: Vec<UnifiedConsoleSession>,
    last_session_error: Option<String>,

    instances_cache: Vec<Instance>,
    summary: InstanceSummary,
    filter_text: String,
    only_running: bool,

    show_insights: bool,
    detail_tab: DetailTab,

    last_refresh: Option<Instant>,
    last_refresh_at: Option<DateTime<Utc>>,
    refresh_interval: Duration,

    last_network_refresh: Option<Instant>,
    network_refresh_interval: Duration,
    network_summary: Option<NetworkSummary>,
    network_switches: Vec<VirtualSwitch>,
    network_interfaces: Vec<NetworkInterface>,
    network_attach_selection: HashMap<String, String>,
    show_create_switch_modal: bool,
    new_switch_name: String,
    new_switch_type: SwitchType,
    new_switch_profile_mode: SwitchProfileMode,
    new_switch_uplink: String,
    new_switch_subnet: String,
    new_switch_dhcp_start: String,
    new_switch_dhcp_end: String,
    network_last_error: Option<String>,
    network_last_info: Option<String>,
}

impl NovaApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::configure_ocean_theme(&cc.egui_ctx);

        let vm_manager = Arc::new(VmManager::new());
        let container_manager = Arc::new(ContainerManager::new());
        let network_manager = Arc::new(Mutex::new(NetworkManager::new()));
        let enhanced_console = Arc::new(AsyncMutex::new(EnhancedConsoleManager::new(
            EnhancedConsoleConfig::default(),
        )));

        let templates_root = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/var/lib/nova"))
            .join("nova")
            .join("templates");
        let template_manager = match TemplateManager::new(templates_root.clone()) {
            Ok(manager) => manager,
            Err(err) => {
                error!(
                    "Failed to initialize template manager at {:?}: {:?}. Falling back to temporary directory",
                    templates_root, err
                );
                let fallback_dir = std::env::temp_dir().join("nova-templates");
                TemplateManager::new(fallback_dir).unwrap_or_else(|fallback_err| {
                    panic!(
                        "Unable to initialize template manager: {:?}, fallback error: {:?}",
                        err, fallback_err
                    )
                })
            }
        };
        let template_manager = Arc::new(AsyncMutex::new(template_manager));
        let session_events = Arc::new(Mutex::new(Vec::new()));

        let config = NovaConfig::from_file("NovaFile").unwrap_or_default();

        let runtime = Runtime::new().expect("failed to initialize Tokio runtime");

        let mut app = Self {
            vm_manager,
            container_manager,
            network_manager,
            enhanced_console,
            template_manager,
            session_events,
            _config: config,
            runtime,
            template_summary: TemplateCatalogSummary::default(),
            selected_instance: None,
            show_console: false,
            console_output: Vec::new(),
            active_sessions: Vec::new(),
            last_session_error: None,
            instances_cache: Vec::new(),
            summary: InstanceSummary::default(),
            filter_text: String::new(),
            only_running: false,
            show_insights: true,
            detail_tab: DetailTab::Overview,
            last_refresh: None,
            last_refresh_at: None,
            refresh_interval: Duration::from_secs(INSTANCE_REFRESH_SECONDS),
            last_network_refresh: None,
            network_refresh_interval: Duration::from_secs(NETWORK_REFRESH_SECONDS),
            network_summary: None,
            network_switches: Vec::new(),
            network_interfaces: Vec::new(),
            network_attach_selection: HashMap::new(),
            show_create_switch_modal: false,
            new_switch_name: String::new(),
            new_switch_type: SwitchType::LinuxBridge,
            new_switch_profile_mode: SwitchProfileMode::Internal,
            new_switch_uplink: String::new(),
            new_switch_subnet: String::new(),
            new_switch_dhcp_start: String::new(),
            new_switch_dhcp_end: String::new(),
            network_last_error: None,
            network_last_info: None,
        };

        app.reset_new_switch_form();
        app.log_console("Nova Manager v0.1.0 initialized");
        app.log_console("Ready for virtualization management");
        app.refresh_instances(true);
        app.refresh_network_summary(true);
        app.refresh_template_summary();

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

        self.populate_instance_ips();
        self.refresh_template_summary();
    }

    fn populate_instance_ips(&mut self) {
        for instance in self.instances_cache.iter_mut() {
            if instance.instance_type == InstanceType::Vm && instance.ip_address.is_none() {
                if let Some(ip) = Self::probe_vm_ip(&instance.name) {
                    instance.ip_address = Some(ip);
                }
            }
        }
    }

    fn refresh_template_summary(&mut self) {
        let summary = self.runtime.block_on(async {
            let manager = self.template_manager.lock().await;
            TemplateCatalogSummary::from_manager(&manager)
        });
        self.template_summary = summary;
    }

    fn probe_vm_ip(vm_name: &str) -> Option<String> {
        let output = Command::new("virsh")
            .args(["domifaddr", vm_name, "--source", "agent"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("Name") || trimmed.starts_with('-') {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 4 {
                let ip = parts[3].split('/').next().unwrap_or(parts[3]).to_string();
                if !ip.is_empty() {
                    return Some(ip);
                }
            }
        }

        None
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
        let mut should_reconcile = false;

        if let Ok(mut manager) = self.network_manager.lock() {
            match self
                .runtime
                .block_on(async { manager.ensure_fresh_state().await })
            {
                Ok(_) => {
                    self.network_summary = Some(manager.summary());
                    self.network_switches = manager
                        .list_switches()
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.network_switches
                        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                    self.network_interfaces = manager
                        .list_interfaces()
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.network_interfaces
                        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                    self.network_attach_selection.retain(|_, iface| {
                        self.network_interfaces
                            .iter()
                            .any(|candidate| candidate.name == *iface)
                    });

                    self.last_network_refresh = Some(Instant::now());
                    should_reconcile = true;
                }
                Err(err) => {
                    error_msg = Some(format!("Network refresh failed: {}", err));
                }
            }
        }

        if should_reconcile {
            self.reconcile_uplink_selection();
        }

        if let Some(msg) = error_msg {
            self.log_console(msg.clone());
            error!("{}", msg);
        }
    }

    fn reconcile_uplink_selection(&mut self) {
        if self.network_interfaces.is_empty() {
            self.new_switch_uplink.clear();
            return;
        }

        let current_is_valid = self
            .network_interfaces
            .iter()
            .any(|iface| iface.name == self.new_switch_uplink);

        if current_is_valid {
            return;
        }

        if let Some(candidate) = self
            .network_interfaces
            .iter()
            .filter(|iface| !matches!(iface.state, InterfaceState::Down))
            .map(|iface| iface.name.clone())
            .next()
        {
            self.new_switch_uplink = candidate;
        } else {
            self.new_switch_uplink = self.network_interfaces[0].name.clone();
        }
    }

    fn reset_new_switch_form(&mut self) {
        self.new_switch_name.clear();
        self.new_switch_type = SwitchType::LinuxBridge;
        self.new_switch_profile_mode = SwitchProfileMode::Internal;
        self.new_switch_subnet = "192.168.120.1/24".to_string();
        self.new_switch_dhcp_start = "192.168.120.50".to_string();
        self.new_switch_dhcp_end = "192.168.120.200".to_string();
        self.reconcile_uplink_selection();
    }

    fn push_network_feedback(&mut self, message: impl Into<String>, is_error: bool) {
        let payload = message.into();

        if is_error {
            self.network_last_error = Some(payload.clone());
            self.network_last_info = None;
        } else {
            self.network_last_info = Some(payload.clone());
            self.network_last_error = None;
        }

        self.log_console(payload);
    }

    fn pending_switch_profile(&self) -> std::result::Result<Option<SwitchProfile>, String> {
        match self.new_switch_profile_mode {
            SwitchProfileMode::Internal => Ok(Some(SwitchProfile::Internal)),
            SwitchProfileMode::External => {
                let uplink = self.new_switch_uplink.trim();
                if uplink.is_empty() {
                    return Err("Select an uplink interface for the external profile".to_string());
                }
                Ok(Some(SwitchProfile::External {
                    uplink: uplink.to_string(),
                }))
            }
            SwitchProfileMode::Nat => {
                let uplink = self.new_switch_uplink.trim();
                if uplink.is_empty() {
                    return Err("Select an uplink interface for the NAT profile".to_string());
                }

                let subnet = self.new_switch_subnet.trim();
                if subnet.is_empty() {
                    return Err("Provide a subnet CIDR (e.g. 192.168.120.1/24)".to_string());
                }

                let start_raw = self.new_switch_dhcp_start.trim();
                let end_raw = self.new_switch_dhcp_end.trim();

                let start = if start_raw.is_empty() {
                    None
                } else {
                    Some(
                        start_raw
                            .parse::<Ipv4Addr>()
                            .map_err(|_| format!("Invalid DHCP range start: {}", start_raw))?,
                    )
                };

                let end = if end_raw.is_empty() {
                    None
                } else {
                    Some(
                        end_raw
                            .parse::<Ipv4Addr>()
                            .map_err(|_| format!("Invalid DHCP range end: {}", end_raw))?,
                    )
                };

                if start.is_some() ^ end.is_some() {
                    return Err("DHCP range requires both start and end addresses".to_string());
                }

                Ok(Some(SwitchProfile::Nat {
                    uplink: uplink.to_string(),
                    subnet_cidr: subnet.to_string(),
                    dhcp_range_start: start,
                    dhcp_range_end: end,
                }))
            }
        }
    }

    fn handle_create_switch(&mut self) {
        let name = self.new_switch_name.trim();
        if name.is_empty() {
            self.push_network_feedback("Switch name cannot be empty", true);
            return;
        }

        let sanitized = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
        if !sanitized {
            self.push_network_feedback(
                "Switch name must be alphanumeric and may include '-' or '_'",
                true,
            );
            return;
        }

        if self
            .network_switches
            .iter()
            .any(|switch| switch.name.eq_ignore_ascii_case(name))
        {
            self.push_network_feedback(format!("A switch named '{}' already exists", name), true);
            return;
        }

        let profile = match self.pending_switch_profile() {
            Ok(profile) => profile,
            Err(err) => {
                self.push_network_feedback(err, true);
                return;
            }
        };

        let switch_name = name.to_string();
        let switch_type = self.new_switch_type.clone();

        let manager_arc = Arc::clone(&self.network_manager);
        let create_result = match manager_arc.lock() {
            Ok(mut manager) => self.runtime.block_on(manager.create_virtual_switch(
                &switch_name,
                switch_type,
                profile,
            )),
            Err(_) => {
                self.push_network_feedback("Network manager is currently busy", true);
                return;
            }
        };

        match create_result {
            Ok(_) => {
                self.push_network_feedback(
                    format!("Created virtual switch '{}'", switch_name),
                    false,
                );
                self.show_create_switch_modal = false;
                self.reset_new_switch_form();
                self.refresh_network_summary(true);
            }
            Err(err) => {
                self.push_network_feedback(
                    format!("Failed to create switch '{}': {}", switch_name, err),
                    true,
                );
            }
        }
    }

    fn handle_delete_switch(&mut self, name: &str) {
        let manager_arc = Arc::clone(&self.network_manager);
        let mut manager = match manager_arc.lock() {
            Ok(manager) => manager,
            Err(_) => {
                self.push_network_feedback("Network manager is currently busy", true);
                return;
            }
        };

        let delete_result = self.runtime.block_on(manager.delete_virtual_switch(name));

        match delete_result {
            Ok(_) => {
                self.push_network_feedback(format!("Deleted virtual switch '{}'", name), false);
                self.refresh_network_summary(true);
            }
            Err(err) => {
                self.push_network_feedback(
                    format!("Failed to delete switch '{}': {}", name, err),
                    true,
                );
            }
        }
    }

    fn handle_attach_interface(&mut self, switch_name: &str) {
        let selected = self
            .network_attach_selection
            .get(switch_name)
            .cloned()
            .unwrap_or_default();
        let interface = selected.trim();

        if interface.is_empty() {
            self.push_network_feedback("Select an interface to attach", true);
            return;
        }

        if self
            .network_switches
            .iter()
            .find(|switch| switch.name == switch_name)
            .map(|switch| switch.interfaces.iter().any(|iface| iface == interface))
            .unwrap_or(false)
        {
            self.push_network_feedback(
                format!(
                    "Interface '{}' is already attached to '{}'",
                    interface, switch_name
                ),
                true,
            );
            return;
        }

        let manager_arc = Arc::clone(&self.network_manager);
        let attach_result = match manager_arc.lock() {
            Ok(mut manager) => self
                .runtime
                .block_on(manager.add_interface_to_switch(switch_name, interface)),
            Err(_) => {
                self.push_network_feedback("Network manager is currently busy", true);
                return;
            }
        };

        match attach_result {
            Ok(_) => {
                self.push_network_feedback(
                    format!("Attached interface '{}' to '{}'", interface, switch_name),
                    false,
                );
                self.network_attach_selection
                    .insert(switch_name.to_string(), String::new());
                self.refresh_network_summary(true);
            }
            Err(err) => {
                self.push_network_feedback(
                    format!(
                        "Failed to attach interface '{}' to '{}': {}",
                        interface, switch_name, err
                    ),
                    true,
                );
            }
        }
    }

    fn handle_detach_interface(&mut self, switch_name: &str, interface: &str) {
        let manager_arc = Arc::clone(&self.network_manager);
        let mut manager = match manager_arc.lock() {
            Ok(manager) => manager,
            Err(_) => {
                self.push_network_feedback("Network manager is currently busy", true);
                return;
            }
        };

        let detach_result = self
            .runtime
            .block_on(manager.remove_interface_from_switch(switch_name, interface));

        match detach_result {
            Ok(_) => {
                self.push_network_feedback(
                    format!("Detached interface '{}' from '{}'", interface, switch_name),
                    false,
                );
                self.refresh_network_summary(true);
            }
            Err(err) => {
                self.push_network_feedback(
                    format!(
                        "Failed to detach interface '{}' from '{}': {}",
                        interface, switch_name, err
                    ),
                    true,
                );
            }
        }
    }

    fn render_switch_creation_modal(&mut self, ctx: &egui::Context) {
        if !self.show_create_switch_modal {
            return;
        }

        let mut open = self.show_create_switch_modal;
        egui::Window::new("Create virtual switch")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.new_switch_name);

                ui.add_space(6.0);
                ui.label("Switch type");
                egui::ComboBox::from_id_source("nova.new_switch.type")
                    .selected_text(match self.new_switch_type {
                        SwitchType::LinuxBridge => "Linux bridge",
                        SwitchType::OpenVSwitch => "Open vSwitch",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.new_switch_type,
                            SwitchType::LinuxBridge,
                            "Linux bridge",
                        );
                        ui.selectable_value(
                            &mut self.new_switch_type,
                            SwitchType::OpenVSwitch,
                            "Open vSwitch",
                        );
                    });

                ui.add_space(8.0);
                ui.label("Switch profile");
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.new_switch_profile_mode,
                        SwitchProfileMode::Internal,
                        "Internal",
                    );
                    ui.selectable_value(
                        &mut self.new_switch_profile_mode,
                        SwitchProfileMode::External,
                        "External uplink",
                    );
                    ui.selectable_value(
                        &mut self.new_switch_profile_mode,
                        SwitchProfileMode::Nat,
                        "NAT + DHCP",
                    );
                });

                if matches!(
                    self.new_switch_profile_mode,
                    SwitchProfileMode::External | SwitchProfileMode::Nat
                ) {
                    ui.add_space(8.0);
                    ui.label("Uplink interface");
                    self.reconcile_uplink_selection();

                    let mut available: Vec<String> = self
                        .network_interfaces
                        .iter()
                        .map(|iface| iface.name.clone())
                        .collect();
                    available.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

                    if available.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 120, 80),
                            "No host interfaces detected. Refresh topology first.",
                        );
                    } else {
                        if self.new_switch_uplink.is_empty() {
                            self.new_switch_uplink = available[0].clone();
                        }

                        let mut selection = self.new_switch_uplink.clone();
                        egui::ComboBox::from_id_source("nova.new_switch.uplink")
                            .selected_text(selection.clone())
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                for iface in available.iter() {
                                    ui.selectable_value(&mut selection, iface.clone(), iface);
                                }
                            });

                        if selection != self.new_switch_uplink {
                            self.new_switch_uplink = selection;
                        }
                    }
                }

                if matches!(self.new_switch_profile_mode, SwitchProfileMode::Nat) {
                    ui.add_space(8.0);
                    ui.label("Gateway / CIDR");
                    ui.text_edit_singleline(&mut self.new_switch_subnet);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("DHCP range");
                        ui.text_edit_singleline(&mut self.new_switch_dhcp_start);
                        ui.label("to");
                        ui.text_edit_singleline(&mut self.new_switch_dhcp_end);
                    });
                    ui.small("Leave DHCP fields blank to auto-calculate a safe range.");
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Create switch").clicked() {
                        self.handle_create_switch();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_create_switch_modal = false;
                    }
                });
            });

        self.show_create_switch_modal = open;

        if !self.show_create_switch_modal {
            self.reset_new_switch_form();
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

    fn request_session_launch(&mut self, instance: &Instance) {
        if instance.instance_type != InstanceType::Vm {
            let msg = format!(
                "Session manager is currently available for virtual machines only ({}).",
                instance.name
            );
            self.last_session_error = Some(msg.clone());
            self.log_console(msg);
            return;
        }

        let vm_name = instance.name.clone();
        let mut ip_hint = instance.ip_address.clone();
        if ip_hint.is_none() {
            ip_hint = Self::probe_vm_ip(&vm_name);
        }

        let console = self.enhanced_console.clone();
        let events = self.session_events.clone();

        self.log_console(format!(
            "Provisioning high-performance session for '{}'",
            vm_name
        ));

        self.runtime.spawn(async move {
            let result = {
                let mut manager = console.lock().await;
                manager
                    .create_optimal_console(&vm_name, ip_hint.as_deref())
                    .await
            };

            match result {
                Ok(session) => {
                    events.lock().unwrap().push(SessionEvent::Launched(session));
                }
                Err(err) => {
                    events.lock().unwrap().push(SessionEvent::Error {
                        vm: vm_name.clone(),
                        message: format!("{err:?}"),
                    });
                }
            }
        });
    }

    fn request_session_close(&mut self, session_id: String) {
        let console = self.enhanced_console.clone();
        let events = self.session_events.clone();
        self.runtime.spawn(async move {
            let result = {
                let mut manager = console.lock().await;
                manager.close_session(&session_id).await
            };

            match result {
                Ok(_) => {
                    events
                        .lock()
                        .unwrap()
                        .push(SessionEvent::Closed(session_id.clone()));
                }
                Err(err) => {
                    events.lock().unwrap().push(SessionEvent::Error {
                        vm: session_id.clone(),
                        message: format!("{err:?}"),
                    });
                }
            }
        });
    }

    fn request_session_launch_client(&mut self, session_id: String) {
        let console = self.enhanced_console.clone();
        let events = self.session_events.clone();
        self.runtime.spawn(async move {
            let result = {
                let mut manager = console.lock().await;
                manager.launch_session_client(&session_id).await
            };

            if let Err(err) = result {
                events.lock().unwrap().push(SessionEvent::Error {
                    vm: session_id.clone(),
                    message: format!("{err:?}"),
                });
            }
        });
    }

    fn drain_session_events(&mut self) {
        let mut events = self.session_events.lock().unwrap();
        if events.is_empty() {
            return;
        }
        let drained: Vec<SessionEvent> = events.drain(..).collect();
        drop(events);

        for event in drained {
            match event {
                SessionEvent::Launched(session) => {
                    self.active_sessions
                        .retain(|existing| existing.session_id != session.session_id);
                    self.active_sessions.push(session.clone());
                    self.last_session_error = None;
                    self.log_console(format!(
                        "Session '{}' ready ({})",
                        session.session_id, session.vm_name
                    ));
                }
                SessionEvent::Error { vm, message } => {
                    self.last_session_error = Some(message.clone());
                    self.log_console(format!("Session error for '{}': {}", vm, message));
                }
                SessionEvent::Closed(session_id) => {
                    let before = self.active_sessions.len();
                    self.active_sessions
                        .retain(|session| session.session_id != session_id);
                    if self.active_sessions.len() < before {
                        self.log_console(format!("Session '{}' closed", session_id));
                    }
                }
            }
        }
    }

    fn refresh_session_cache(&mut self) {
        let console = self.enhanced_console.clone();
        let sessions = self.runtime.block_on(async {
            let manager = console.lock().await;
            manager.list_active_sessions()
        });
        self.active_sessions = sessions;
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
            self.detail_tab = DetailTab::Overview;
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

    fn draw_insights_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Insights");
        ui.separator();

        if let Some(updated_at) = self.last_refresh_at {
            ui.small(format!(
                "Inventory refreshed at {}",
                updated_at.with_timezone(&Local).format("%H:%M:%S")
            ));
        }

        ui.add_space(6.0);

        if let Some(instance) = self.selected_instance() {
            let status_color = theme::get_status_color(&instance.status);
            ui.group(|ui| {
                ui.label(egui::RichText::new("Selected instance").strong());
                ui.add_space(4.0);
                ui.label(format!("Name: {}", instance.name));
                ui.label(format!("Kind: {:?}", instance.instance_type));
                ui.colored_label(status_color, format!("Status: {:?}", instance.status));
                ui.label(format!("CPU: {} cores", instance.cpu_cores));
                ui.label(format!("Memory: {} MB", instance.memory_mb));
                if let Some(pid) = instance.pid {
                    ui.label(format!("Hypervisor PID: {}", pid));
                }
                if let Some(network) = &instance.network {
                    ui.label(format!("Attached network: {}", network));
                }
                ui.small(format!(
                    "Created {}",
                    instance.created_at.format("%Y-%m-%d %H:%M")
                ));
            });
        } else {
            ui.group(|ui| {
                ui.label(egui::RichText::new("No selection").strong());
                ui.label("Pick a VM or container to drill into runtime metrics.");
            });
        }

        ui.add_space(12.0);
        ui.heading("Network overview");
        ui.separator();

        if let Some(summary) = &self.network_summary {
            ui.label(format!(
                "Virtual switches: {} total ({} Nova · {} system)",
                summary.total_switches, summary.nova_managed_switches, summary.system_switches
            ));
            ui.label(format!(
                "Active interfaces: {} up / {} down / {} unknown",
                summary.interfaces_up, summary.interfaces_down, summary.interfaces_unknown
            ));
            if let Some(scan) = summary.last_refresh_at {
                ui.small(format!(
                    "Topology refreshed {}",
                    scan.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
                ));
            }
            ui.add_space(6.0);
            ui.small("Persistent switch profiles are automatically hydrated after restart.");
        } else {
            ui.label("Network telemetry pending…");
        }

        ui.add_space(12.0);
        ui.heading("Template catalog");
        ui.separator();

        if self.template_summary.total == 0 {
            ui.label("No VM templates have been catalogued yet.");
        } else {
            ui.label(format!(
                "Templates available: {}",
                self.template_summary.total
            ));

            let mut breakdown = Vec::new();
            if self.template_summary.linux > 0 {
                breakdown.push(format!("{} Linux", self.template_summary.linux));
            }
            if self.template_summary.windows > 0 {
                breakdown.push(format!("{} Windows", self.template_summary.windows));
            }
            if self.template_summary.other > 0 {
                breakdown.push(format!("{} Other", self.template_summary.other));
            }

            if !breakdown.is_empty() {
                ui.small(format!("Breakdown: {}", breakdown.join(" • ")));
            }

            if !self.template_summary.recent.is_empty() {
                ui.small(format!(
                    "Recent templates: {}",
                    self.template_summary.recent.join(", ")
                ));
            }

            if let Some(scan) = self.template_summary.last_refresh {
                ui.small(format!(
                    "Catalog refreshed {}",
                    scan.with_timezone(&Local).format("%Y-%m-%d %H:%M")
                ));
            }
        }

        ui.add_space(12.0);
        ui.heading("Next actions");
        ui.separator();
        ui.small("• Create checkpoints for long-running guests");
        ui.small("• Review upcoming resource graphs (coming soon)");
        ui.small("• Explore the polished network topology from the Networking tab");
    }

    fn draw_overview(&self, ui: &mut egui::Ui, instance: &Instance) {
        let status_color = theme::get_status_color(&instance.status);
        let uptime = Utc::now().signed_duration_since(instance.created_at);
        let time_since_update = Utc::now().signed_duration_since(instance.last_updated);

        let uptime_str = if uptime.num_days() > 0 {
            format!("{}d {}h", uptime.num_days(), uptime.num_hours() % 24)
        } else if uptime.num_hours() > 0 {
            format!("{}h {}m", uptime.num_hours(), uptime.num_minutes() % 60)
        } else {
            format!("{}m", uptime.num_minutes().max(1))
        };

        let update_str = if time_since_update.num_minutes() < 1 {
            "moments ago".to_string()
        } else if time_since_update.num_hours() < 1 {
            format!("{} minutes ago", time_since_update.num_minutes())
        } else {
            format!("{}h ago", time_since_update.num_hours())
        };

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(&instance.name);
                ui.colored_label(status_color, format!("{:?}", instance.status));
                ui.label(match instance.instance_type {
                    InstanceType::Vm => "Virtual machine",
                    InstanceType::Container => "Container",
                });
                if let Some(pid) = instance.pid {
                    ui.monospace(format!("PID {pid}"));
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(format!("Uptime {}", uptime_str));
                ui.separator();
                ui.label(format!(
                    "Created {}",
                    instance
                        .created_at
                        .with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M")
                ));
                ui.separator();
                ui.label(format!("Last update {update_str}"));
            });
        });

        ui.add_space(12.0);
        ui.columns(2, |columns| {
            columns[0].group(|ui| {
                ui.label(egui::RichText::new("Resource allocation").strong());
                ui.separator();
                ui.label(format!("vCPUs: {}", instance.cpu_cores));
                ui.label(format!("Memory: {} MB", instance.memory_mb));
                ui.label("Storage: template managed (coming soon)");
                ui.label("Graphics: emulated (QXL)");
            });

            columns[1].group(|ui| {
                ui.label(egui::RichText::new("Networking").strong());
                ui.separator();
                if let Some(network) = &instance.network {
                    ui.label(format!("Attached to {network}"));
                } else {
                    ui.label("Not attached to any virtual switch");
                }
                if let Some(summary) = &self.network_summary {
                    ui.small(format!(
                        "Host has {} virtual switches ({} active)",
                        summary.total_switches, summary.active_switches
                    ));
                    ui.small(format!(
                        "Interfaces up/down: {} / {}",
                        summary.interfaces_up, summary.interfaces_down
                    ));
                } else {
                    ui.small("Network telemetry loading…");
                }
            });
        });

        ui.add_space(12.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Operations").strong());
            ui.separator();
            ui.label("• Live migration and backup orchestration are planned additions.");
            ui.label("• Resource utilisation charts will surface here shortly.");
            if self.detail_tab == DetailTab::Snapshots {
                ui.label("• Snapshot orchestration is active in the Snapshots tab.");
            } else {
                ui.label("• Switch to the Snapshots tab to review restore points.");
            }
        });
    }

    fn draw_snapshots(&self, ui: &mut egui::Ui) {
        ui.heading("Recovery checkpoints");
        ui.separator();
        ui.label("Snapshot orchestration is on the roadmap.");
        ui.label("Planned capabilities:");
        ui.small("• Create crash-consistent checkpoints");
        ui.small("• Schedule nightly restore points");
        ui.small("• Replicate snapshots to remote hosts");

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label("Existing snapshots");
            ui.separator();
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label("No snapshots captured yet.");
                ui.small("Kick off automation once the workflow lands.");
                ui.add_space(12.0);
                let _ = ui.button("Create snapshot (preview)");
            });
        });
    }

    fn draw_networking(&mut self, ui: &mut egui::Ui, instance: &Instance) {
        ui.heading("Network orchestration");
        ui.separator();

        ui.group(|ui| {
            ui.label(egui::RichText::new("Instance linkage").strong());
            ui.separator();
            if let Some(network) = &instance.network {
                ui.label(format!("Attached to virtual switch: {network}"));
            } else {
                ui.label("No virtual switch assigned to this instance");
            }

            if let Some(ip) = &instance.ip_address {
                ui.small(format!("Active IP address: {}", ip));
            } else {
                ui.small("No IP discovered for this workload yet");
            }
        });

        ui.add_space(8.0);
        if let Some(summary) = &self.network_summary {
            ui.horizontal(|ui| {
                Self::summary_chip(
                    ui,
                    "Switches",
                    summary.total_switches,
                    egui::Color32::from_rgb(96, 170, 255),
                );
                Self::summary_chip(
                    ui,
                    "Active",
                    summary.active_switches,
                    egui::Color32::from_rgb(102, 220, 144),
                );
                Self::summary_chip(
                    ui,
                    "Managed",
                    summary.nova_managed_switches,
                    egui::Color32::from_rgb(204, 156, 255),
                );
            });
            ui.small(format!(
                "Interfaces up/down/unknown: {} / {} / {}",
                summary.interfaces_up, summary.interfaces_down, summary.interfaces_unknown
            ));
            if let Some(refresh) = summary.last_refresh_at {
                ui.small(format!(
                    "Topology updated {}",
                    refresh.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
                ));
            }
        } else {
            ui.label("Network telemetry is still loading — refresh shortly.");
        }

        ui.add_space(6.0);
        if let Some(err) = &self.network_last_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), format!("⚠ {}", err));
        } else if let Some(msg) = &self.network_last_info {
            ui.colored_label(egui::Color32::from_rgb(96, 200, 140), format!("✔ {}", msg));
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Refresh topology").clicked() {
                self.refresh_network_summary(true);
            }

            if ui.button("Create virtual switch").clicked() {
                self.show_create_switch_modal = true;
                self.reconcile_uplink_selection();
            }
        });

        self.render_switch_creation_modal(ui.ctx());

        ui.add_space(10.0);
        ui.label(egui::RichText::new("Virtual switches").strong());

        if self.network_switches.is_empty() {
            ui.group(|ui| {
                ui.label("No virtual switches detected on this host.");
                ui.small("Use the Create virtual switch action to bring a bridge online.");
            });
        } else {
            let switches = self.network_switches.clone();
            egui::ScrollArea::vertical()
                .id_source("nova.network.switches")
                .max_height(260.0)
                .show(ui, |ui| {
                    for switch in switches.iter() {
                        egui::CollapsingHeader::new(format!(
                            "{} ({:?})",
                            switch.name, switch.switch_type
                        ))
                        .id_source(format!("nova.switch.{}", switch.name))
                        .default_open(true)
                        .show(ui, |ui| {
                            let (status_icon, status_color, status_text) = match &switch.status {
                                SwitchStatus::Active => (
                                    "●",
                                    egui::Color32::from_rgb(88, 200, 120),
                                    "Active".to_string(),
                                ),
                                SwitchStatus::Inactive => {
                                    ("○", egui::Color32::from_gray(160), "Inactive".to_string())
                                }
                                SwitchStatus::Error(reason) => (
                                    "⚠",
                                    egui::Color32::from_rgb(220, 120, 80),
                                    format!("Error: {}", reason),
                                ),
                            };

                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    status_color,
                                    format!("{} {}", status_icon, status_text),
                                );
                                ui.label(format!("Origin: {:?}", switch.origin));
                                ui.small(format!(
                                    "Created {}",
                                    switch
                                        .created_at
                                        .with_timezone(&Local)
                                        .format("%Y-%m-%d %H:%M")
                                ));
                            });

                            ui.horizontal(|ui| {
                                if let Some(vlan) = switch.vlan_id {
                                    ui.small(format!("VLAN {}", vlan));
                                }
                                ui.small(format!(
                                    "STP {}",
                                    if switch.stp_enabled {
                                        "enabled"
                                    } else {
                                        "disabled"
                                    }
                                ));
                            });

                            match &switch.profile {
                                Some(SwitchProfile::Internal) => {
                                    ui.small("Profile: Internal (isolated host-only bridge)");
                                }
                                Some(SwitchProfile::External { uplink }) => {
                                    ui.small(format!("Profile: External uplink via {}", uplink));
                                }
                                Some(SwitchProfile::Nat {
                                    uplink,
                                    subnet_cidr,
                                    dhcp_range_start,
                                    dhcp_range_end,
                                }) => {
                                    ui.small(format!(
                                        "Profile: NAT via {}, subnet {}",
                                        uplink, subnet_cidr
                                    ));
                                    if let (Some(start), Some(end)) =
                                        (dhcp_range_start, dhcp_range_end)
                                    {
                                        ui.small(format!("DHCP range: {} - {}", start, end));
                                    } else {
                                        ui.small("DHCP range: auto-managed");
                                    }
                                }
                                None => {
                                    if switch.origin == SwitchOrigin::System {
                                        ui.small("Profile: System-managed bridge");
                                    } else {
                                        ui.small("Profile: Unspecified");
                                    }
                                }
                            }

                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let available: Vec<String> = self
                                    .network_interfaces
                                    .iter()
                                    .filter(|iface| iface.bridge.as_deref() != Some(&switch.name))
                                    .map(|iface| iface.name.clone())
                                    .collect();

                                let entry = self
                                    .network_attach_selection
                                    .entry(switch.name.clone())
                                    .or_insert_with(String::new);

                                if entry.is_empty() {
                                    if let Some(first) = available.first() {
                                        *entry = first.clone();
                                    }
                                } else if !available.iter().any(|iface| iface == entry) {
                                    *entry = available.first().cloned().unwrap_or_default();
                                }

                                let mut selection = entry.clone();
                                egui::ComboBox::from_id_source(format!(
                                    "nova.switch.attach.{}",
                                    switch.name
                                ))
                                .selected_text(if selection.is_empty() {
                                    "Select host interface".to_string()
                                } else {
                                    selection.clone()
                                })
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    for iface in available.iter() {
                                        ui.selectable_value(&mut selection, iface.clone(), iface);
                                    }
                                });

                                if *entry != selection {
                                    *entry = selection;
                                }

                                let can_attach = !entry.is_empty();
                                if ui
                                    .add_enabled(can_attach, egui::Button::new("Attach interface"))
                                    .clicked()
                                {
                                    self.handle_attach_interface(&switch.name);
                                }

                                ui.add_space(12.0);
                                if ui.button("Delete switch").clicked() {
                                    self.handle_delete_switch(&switch.name);
                                }
                            });

                            ui.add_space(6.0);
                            if switch.interfaces.is_empty() {
                                ui.small("No host interfaces attached yet.");
                            } else {
                                ui.label("Attached interfaces:");
                                for iface in switch.interfaces.iter() {
                                    let details = self
                                        .network_interfaces
                                        .iter()
                                        .find(|candidate| candidate.name == *iface)
                                        .cloned();
                                    let iface_name = iface.clone();

                                    ui.horizontal(|ui| {
                                        ui.label(format!("• {}", iface_name));
                                        if let Some(meta) = details.as_ref() {
                                            let state_color = match meta.state {
                                                InterfaceState::Up => {
                                                    egui::Color32::from_rgb(96, 200, 140)
                                                }
                                                InterfaceState::Down => {
                                                    egui::Color32::from_rgb(220, 120, 80)
                                                }
                                                InterfaceState::Unknown => {
                                                    egui::Color32::from_gray(160)
                                                }
                                            };
                                            ui.colored_label(
                                                state_color,
                                                format!("{:?}", meta.state),
                                            );
                                            if let Some(ip) = meta.ip_address {
                                                ui.small(format!("IP {}", ip));
                                            }
                                        }

                                        if ui.button("Detach").clicked() {
                                            self.handle_detach_interface(
                                                &switch.name,
                                                iface_name.as_str(),
                                            );
                                        }
                                    });
                                }
                            }
                        });
                        ui.add_space(6.0);
                    }
                });
        }

        ui.add_space(10.0);
        egui::CollapsingHeader::new("Host interfaces")
            .id_source("nova.network.interfaces")
            .default_open(false)
            .show(ui, |ui| {
                if self.network_interfaces.is_empty() {
                    ui.label("No interfaces discovered. Refresh to rescan the host.");
                    return;
                }

                egui::Grid::new("nova.network.interfaces.grid")
                    .striped(true)
                    .min_col_width(110.0)
                    .show(ui, |ui| {
                        ui.strong("Interface");
                        ui.strong("State");
                        ui.strong("Bridge");
                        ui.strong("IPv4");
                        ui.end_row();

                        for iface in self.network_interfaces.iter() {
                            ui.label(&iface.name);
                            let color = match iface.state {
                                InterfaceState::Up => egui::Color32::from_rgb(96, 200, 140),
                                InterfaceState::Down => egui::Color32::from_rgb(220, 120, 80),
                                InterfaceState::Unknown => egui::Color32::from_gray(160),
                            };
                            ui.colored_label(color, format!("{:?}", iface.state));
                            ui.label(
                                iface
                                    .bridge
                                    .as_ref()
                                    .map(|b| b.to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                            );
                            ui.label(
                                iface
                                    .ip_address
                                    .map(|ip| ip.to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                            );
                            ui.end_row();
                        }
                    });
            });
    }

    fn draw_sessions(&mut self, ui: &mut egui::Ui, instance: &Instance) {
        ui.heading("Session manager");
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("🚀 Launch session").clicked() {
                self.request_session_launch(instance);
            }
            if ui.button("🔄 Refresh list").clicked() {
                self.refresh_session_cache();
            }
        });

        if let Some(err) = &self.last_session_error {
            ui.colored_label(theme::STATUS_STOPPED, format!("Last error: {err}"));
            ui.add_space(6.0);
        }

        let sessions: Vec<UnifiedConsoleSession> = self
            .active_sessions
            .iter()
            .filter(|session| session.vm_name == instance.name)
            .cloned()
            .collect();

        if sessions.is_empty() {
            ui.label("No active sessions for this instance yet.");
            ui.small("Use Launch session to provision a RustDesk or SPICE viewer.");
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                for session in sessions {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&session.session_id);
                            ui.add_space(6.0);
                            let status_text = if session.active {
                                egui::RichText::new("Active").color(theme::STATUS_RUNNING)
                            } else {
                                egui::RichText::new("Inactive").color(theme::STATUS_WARNING)
                            };
                            ui.label(status_text);
                            ui.separator();
                            ui.small(format!("Score {:.0}", session.performance_score));
                        });

                        ui.label(format!(
                            "Created {}",
                            session
                                .created_at
                                .with_timezone(&Local)
                                .format("%Y-%m-%d %H:%M:%S")
                        ));
                        ui.small(format!(
                            "Last access {}",
                            session
                                .last_accessed
                                .with_timezone(&Local)
                                .format("%Y-%m-%d %H:%M:%S")
                        ));

                        match &session.protocol_used {
                            ActiveProtocol::RustDesk(rd_session) => {
                                ui.label("Protocol: RustDesk");
                                ui.small(format!("Relay: {}", rd_session.relay_server));
                                ui.small(format!("Profile: {:?}", rd_session.performance_profile));
                                ui.monospace(&rd_session.connection_url);
                            }
                            ActiveProtocol::Standard(console_session) => {
                                ui.label(format!("Protocol: {:?}", console_session.console_type));
                                ui.small(format!(
                                    "Endpoint: {}:{} ({})",
                                    console_session.connection_info.host,
                                    console_session.connection_info.port,
                                    console_session.connection_info.protocol
                                ));
                            }
                        }

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("🪟 Open viewer").clicked() {
                                self.request_session_launch_client(session.session_id.clone());
                            }
                            if ui.button("⏹ Close session").clicked() {
                                self.request_session_close(session.session_id.clone());
                            }
                        });
                    });
                    ui.add_space(8.0);
                }
            });
    }

    fn draw_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("nova.header")
            .frame(egui::Frame::default().fill(theme::BG_ELEVATED))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.heading("Nova Hypervisor Manager");
                    if let Some(updated) = self.last_refresh_at {
                        ui.label(format!(
                            "Inventory synced {}",
                            updated.with_timezone(&Local).format("%H:%M:%S")
                        ));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🔄 Refresh all").clicked() {
                            self.refresh_instances(true);
                            self.refresh_network_summary(true);
                            self.log_console("Manual refresh triggered from header");
                        }
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    Self::summary_chip(ui, "Running", self.summary.running, theme::STATUS_RUNNING);
                    Self::summary_chip(ui, "Stopped", self.summary.stopped, theme::STATUS_STOPPED);
                    let container_count = self
                        .instances_cache
                        .iter()
                        .filter(|inst| inst.instance_type == InstanceType::Container)
                        .count();
                    Self::summary_chip(ui, "Containers", container_count, theme::STATUS_SUSPENDED);

                    let active_switches = self
                        .network_summary
                        .as_ref()
                        .map(|summary| summary.active_switches)
                        .unwrap_or(0);
                    Self::summary_chip(
                        ui,
                        "Active switches",
                        active_switches,
                        theme::STATUS_WARNING,
                    );
                });
                ui.add_space(4.0);
            });
    }

    fn draw_navigation_panel(&mut self, ctx: &egui::Context, filter: &str) {
        egui::SidePanel::left("nova.navigation")
            .default_width(260.0)
            .min_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Inventory");
                ui.separator();
                ui.label("Local host");
                ui.small("Hyper-V style navigation");
                ui.add_space(8.0);
                self.draw_instance_tree(ui, filter);
            });
    }

    fn draw_event_log_panel(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("nova.event_log")
            .default_height(170.0)
            .min_height(120.0)
            .show(ctx, |ui| {
                ui.heading("Event log");
                ui.separator();
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.console_output {
                            ui.monospace(line);
                        }
                    });
            });
    }

    fn draw_action_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        can_start: bool,
        can_stop: bool,
        can_restart: bool,
    ) {
        let has_selection = self.selected_instance.is_some();
        let ready_session = self.selected_instance().and_then(|instance| {
            self.active_sessions
                .iter()
                .filter(|session| session.vm_name == instance.name && session.active)
                .last()
                .map(|session| session.session_id.clone())
        });
        ui.horizontal(|ui| {
            if ui.button("➕ New VM").clicked() {
                self.log_console("VM creation wizard coming soon");
            }
            if ui.button("📦 New Container").clicked() {
                self.log_console("Container creation wizard coming soon");
            }

            ui.separator();

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

            ui.separator();

            if ui
                .add_enabled(has_selection, egui::Button::new("🖥 Console"))
                .clicked()
            {
                self.show_console = true;
                self.log_console("Opening interactive console view");
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("🚀 Session"))
                .clicked()
            {
                if let Some(instance) = self.selected_instance_owned() {
                    self.request_session_launch(&instance);
                }
            }
            if ui
                .add_enabled(ready_session.is_some(), egui::Button::new("🪟 Viewer"))
                .clicked()
            {
                if let Some(session_id) = ready_session.clone() {
                    self.request_session_launch_client(session_id);
                }
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("🛡 Checkpoint"))
                .clicked()
            {
                self.log_console("Checkpoint workflow coming soon");
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("⚙ Settings"))
                .clicked()
            {
                self.log_console("Settings panel under construction");
            }
        });
    }

    fn draw_filter_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.add(
                egui::TextEdit::singleline(&mut self.filter_text)
                    .hint_text("Search by name, network or status…")
                    .desired_width(220.0),
            );
            ui.checkbox(&mut self.only_running, "Running only");
            if ui.button("Clear").clicked() {
                self.filter_text.clear();
                self.only_running = false;
            }
        });
    }

    fn filtered_instances(&self, filter: &str) -> Vec<Instance> {
        self.instances_cache
            .iter()
            .filter(|instance| self.should_display(instance, filter))
            .cloned()
            .collect()
    }

    fn draw_instance_table(&mut self, ui: &mut egui::Ui, filter: &str) {
        let instances = self.filtered_instances(filter);

        if instances.is_empty() {
            ui.group(|ui| {
                ui.label("No instances match the current filter.");
                ui.small("Try adjusting the search or toggle Running only off.");
            });
            return;
        }

        egui::ScrollArea::vertical()
            .id_source("nova.instance_table")
            .show(ui, |ui| {
                egui::Grid::new("instance_grid")
                    .striped(true)
                    .min_col_width(110.0)
                    .show(ui, |ui| {
                        ui.strong("Name");
                        ui.strong("Type");
                        ui.strong("Status");
                        ui.strong("CPU");
                        ui.strong("Memory");
                        ui.strong("Network");
                        ui.strong("Updated");
                        ui.end_row();

                        for instance in instances.iter() {
                            let is_selected = self
                                .selected_instance
                                .as_ref()
                                .map(|name| name == &instance.name)
                                .unwrap_or(false);

                            if ui.selectable_label(is_selected, &instance.name).clicked() {
                                self.selected_instance = Some(instance.name.clone());
                                self.detail_tab = DetailTab::Overview;
                            }
                            ui.label(match instance.instance_type {
                                InstanceType::Vm => "VM",
                                InstanceType::Container => "Container",
                            });

                            let status_color = theme::get_status_color(&instance.status);
                            ui.colored_label(status_color, format!("{:?}", instance.status));

                            ui.label(format!("{} cores", instance.cpu_cores));
                            ui.label(format!("{} MB", instance.memory_mb));
                            ui.label(instance.network.clone().unwrap_or_else(|| "-".to_string()));
                            ui.label(instance.last_updated.format("%H:%M:%S").to_string());
                            ui.end_row();
                        }
                    });
            });
    }

    fn draw_instance_detail(&mut self, ui: &mut egui::Ui, instance: &Instance) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.detail_tab, DetailTab::Overview, "Overview");
                ui.selectable_value(&mut self.detail_tab, DetailTab::Snapshots, "Snapshots");
                ui.selectable_value(&mut self.detail_tab, DetailTab::Networking, "Networking");
                ui.selectable_value(&mut self.detail_tab, DetailTab::Sessions, "Sessions");
            });

            ui.separator();

            match self.detail_tab {
                DetailTab::Overview => self.draw_overview(ui, instance),
                DetailTab::Snapshots => self.draw_snapshots(ui),
                DetailTab::Networking => self.draw_networking(ui, instance),
                DetailTab::Sessions => self.draw_sessions(ui, instance),
            }
        });
    }
}

impl eframe::App for NovaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::configure_ocean_theme(ctx);

        self.refresh_instances(false);
        self.refresh_network_summary(false);
        self.drain_session_events();

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
                    ui.checkbox(&mut self.show_console, "Event log");
                    ui.checkbox(&mut self.show_insights, "Insights panel");
                    ui.separator();
                    ui.radio_value(&mut self.detail_tab, DetailTab::Overview, "Overview tab");
                    ui.radio_value(&mut self.detail_tab, DetailTab::Snapshots, "Snapshots tab");
                    ui.radio_value(
                        &mut self.detail_tab,
                        DetailTab::Networking,
                        "Networking tab",
                    );
                    ui.radio_value(&mut self.detail_tab, DetailTab::Sessions, "Sessions tab");
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

        self.draw_header(ctx);
        self.draw_navigation_panel(ctx, &filter);

        if self.show_insights {
            egui::SidePanel::right("insights")
                .default_width(320.0)
                .min_width(260.0)
                .show(ctx, |ui| {
                    self.draw_insights_panel(ui);
                });
        }

        if self.show_console {
            self.draw_event_log_panel(ctx);
        }

        let selected_instance = self.selected_instance_owned();

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::BG_PANEL))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                self.draw_action_toolbar(ui, can_start, can_stop, can_restart);

                ui.add_space(6.0);
                ui.separator();
                self.draw_filter_bar(ui);
                ui.add_space(10.0);

                ui.columns(2, |columns| {
                    columns[0].heading("Managed instances");
                    columns[0].small(format!(
                        "{} total • {} running",
                        self.instances_cache.len(),
                        self.summary.running
                    ));
                    columns[0].separator();
                    self.draw_instance_table(&mut columns[0], &filter);

                    columns[1].heading("Details & telemetry");
                    columns[1].separator();
                    if let Some(instance) = selected_instance.as_ref() {
                        self.draw_instance_detail(&mut columns[1], instance);
                    } else {
                        columns[1].vertical_centered(|ui| {
                            ui.add_space(60.0);
                            ui.heading("Select an instance");
                            ui.label("Choose a VM or container from the inventory to drill into metrics.");
                        });
                    }
                });
            });

        ctx.request_repaint_after(self.refresh_interval.min(self.network_refresh_interval));
    }
}
