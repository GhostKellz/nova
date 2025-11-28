use eframe::egui;
use nova::{
    config::{NovaConfig, default_ui_font_family, default_ui_font_size},
    console_enhanced::{
        ActiveProtocol, EnhancedConsoleConfig, EnhancedConsoleManager, UnifiedConsoleSession,
    },
    container::ContainerManager,
    container_runtime::{ContainerInfo, ContainerStats},
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
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;
use tracing::{error, info, warn};

const MAX_CONSOLE_LINES: usize = 200;
const INSTANCE_REFRESH_SECONDS: u64 = 5;
const NETWORK_REFRESH_SECONDS: u64 = 15;
const MIN_FONT_SIZE: f32 = 12.0;
const MAX_FONT_SIZE: f32 = 20.0;
const MIN_INSTANCE_REFRESH_SECONDS: i32 = 3;
const MAX_INSTANCE_REFRESH_SECONDS: i32 = 60;
const MIN_NETWORK_REFRESH_SECONDS: i32 = 10;
const MAX_NETWORK_REFRESH_SECONDS: i32 = 180;

#[derive(Clone, Copy)]
struct FontChoice {
    id: &'static str,
    label: &'static str,
    description: &'static str,
}

const FONT_CHOICES: [FontChoice; 2] = [
    FontChoice {
        id: "fira-code-nerd",
        label: "Fira Code Nerd (semi-bold)",
        description: "Requires Nerd Font installation under ~/.local/share/fonts or /usr/share/fonts.",
    },
    FontChoice {
        id: "system-default",
        label: "System default",
        description: "Use the platform's proportional UI fonts.",
    },
];

#[derive(Clone)]
struct UiPreferencesSnapshot {
    theme: theme::GuiTheme,
    font_family: String,
    font_size: f32,
    compact_layout: bool,
    auto_refresh: bool,
    refresh_interval_secs: u64,
    network_refresh_secs: u64,
    show_event_log: bool,
    show_insights: bool,
    confirm_actions: bool,
}

#[derive(Clone)]
struct ContainerDetailCache {
    info: ContainerInfo,
    stats: Option<ContainerStats>,
    fetched_at: Instant,
}

#[derive(Clone)]
struct ContainerDetailError {
    message: String,
    recorded_at: Instant,
}

struct ContainerLogsState {
    name: String,
    lines: Vec<String>,
    error: Option<String>,
    fetched_at: Instant,
}

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum InstanceAction {
    Start,
    Stop,
    Restart,
}

#[derive(Clone)]
struct PendingAction {
    action: InstanceAction,
    instance: Instance,
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
    config_path: PathBuf,
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

    auto_refresh: bool,
    show_insights: bool,
    detail_tab: DetailTab,
    confirm_instance_actions: bool,
    pending_action: Option<PendingAction>,
    show_action_confirmation: bool,

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
    theme: theme::GuiTheme,
    font_family: String,
    font_size: f32,
    compact_layout: bool,
    fonts_dirty: bool,
    font_cache: HashMap<String, Option<Arc<Vec<u8>>>>,
    font_load_error: Option<String>,
    show_preferences: bool,
    preferences_dirty: bool,
    preferences_backup: Option<UiPreferencesSnapshot>,
    container_details: HashMap<String, ContainerDetailCache>,
    container_detail_errors: HashMap<String, ContainerDetailError>,
    container_logs: Option<ContainerLogsState>,
}

impl NovaApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config_path = PathBuf::from("NovaFile");
        let mut config = match NovaConfig::from_file(&config_path) {
            Ok(cfg) => cfg,
            Err(err) => {
                warn!(
                    "Failed to load NovaFile at {} ({}); using defaults",
                    config_path.display(),
                    err
                );
                NovaConfig::default()
            }
        };

        let theme = match theme::GuiTheme::from_name(config.ui.theme.as_str()) {
            Some(theme) => theme,
            None => {
                if !config.ui.theme.is_empty() {
                    warn!(
                        "Unrecognized GUI theme '{}' in NovaFile; defaulting to Tokyo Night (Storm)",
                        config.ui.theme
                    );
                }
                let fallback = theme::GuiTheme::default();
                config.ui.theme = fallback.name().to_string();
                if let Err(err) = config.save_to_file(&config_path) {
                    warn!(
                        "Unable to update GUI theme in {}: {}",
                        config_path.display(),
                        err
                    );
                }
                fallback
            }
        };

        theme::apply_theme(&cc.egui_ctx, theme);

        let mut config_dirty = false;

        let default_font_family = default_ui_font_family();
        let original_font_family = config.ui.font_family.clone();
        let mut font_family = if config.ui.font_family.trim().is_empty() {
            config_dirty = true;
            default_font_family.clone()
        } else {
            config.ui.font_family.trim().to_ascii_lowercase()
        };

        if !FONT_CHOICES.iter().any(|choice| choice.id == font_family) {
            warn!(
                "Unsupported font family '{}' in NovaFile; reverting to {}",
                config.ui.font_family, default_font_family
            );
            font_family = default_font_family.clone();
            config_dirty = true;
        }

        config.ui.font_family = font_family.clone();
        if config.ui.font_family != original_font_family {
            config_dirty = true;
        }

        let default_font_size = default_ui_font_size();
        let mut font_size = if config.ui.font_size.is_finite() {
            config.ui.font_size
        } else {
            default_font_size
        };

        if !(MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&font_size) {
            warn!(
                "Out-of-range font size {:.1} detected; clamping to valid bounds",
                font_size
            );
            font_size = font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
            config_dirty = true;
        }

        if (font_size - config.ui.font_size).abs() > f32::EPSILON {
            config.ui.font_size = font_size;
            config_dirty = true;
        }

        let auto_refresh = config.ui.auto_refresh;
        let min_refresh = MIN_INSTANCE_REFRESH_SECONDS as u64;
        let max_refresh = MAX_INSTANCE_REFRESH_SECONDS as u64;
        let mut refresh_secs = config.ui.refresh_interval_seconds;
        if refresh_secs == 0 {
            refresh_secs = INSTANCE_REFRESH_SECONDS;
        }
        let clamped_refresh = refresh_secs.clamp(min_refresh, max_refresh);
        if clamped_refresh != refresh_secs {
            config.ui.refresh_interval_seconds = clamped_refresh;
            config_dirty = true;
        }
        let refresh_interval = Duration::from_secs(clamped_refresh);

        let min_network = MIN_NETWORK_REFRESH_SECONDS as u64;
        let max_network = MAX_NETWORK_REFRESH_SECONDS as u64;
        let mut network_secs = config.ui.network_refresh_interval_seconds;
        if network_secs == 0 {
            network_secs = NETWORK_REFRESH_SECONDS;
        }
        let clamped_network = network_secs.clamp(min_network, max_network);
        if clamped_network != network_secs {
            config.ui.network_refresh_interval_seconds = clamped_network;
            config_dirty = true;
        }
        let network_refresh_interval = Duration::from_secs(clamped_network);

        let show_console = config.ui.show_event_log;
        let show_insights = config.ui.show_insights;
        let confirm_instance_actions = config.ui.confirm_instance_actions;

        if config_dirty {
            if let Err(err) = config.save_to_file(&config_path) {
                warn!(
                    "Unable to persist updated UI defaults in {}: {}",
                    config_path.display(),
                    err
                );
            }
        }

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

        let runtime = Runtime::new().expect("failed to initialize Tokio runtime");

        let compact_layout = config.ui.compact_layout;

        let mut app = Self {
            vm_manager,
            container_manager,
            network_manager,
            enhanced_console,
            template_manager,
            session_events,
            _config: config,
            config_path,
            runtime,
            template_summary: TemplateCatalogSummary::default(),
            selected_instance: None,
            show_console,
            console_output: Vec::new(),
            active_sessions: Vec::new(),
            last_session_error: None,
            instances_cache: Vec::new(),
            summary: InstanceSummary::default(),
            filter_text: String::new(),
            only_running: false,
            auto_refresh,
            show_insights,
            detail_tab: DetailTab::Overview,
            confirm_instance_actions,
            pending_action: None,
            show_action_confirmation: false,
            last_refresh: None,
            last_refresh_at: None,
            refresh_interval,
            last_network_refresh: None,
            network_refresh_interval,
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
            theme,
            font_family,
            font_size,
            compact_layout,
            fonts_dirty: true,
            font_cache: HashMap::new(),
            font_load_error: None,
            show_preferences: false,
            preferences_dirty: false,
            preferences_backup: None,
            container_details: HashMap::new(),
            container_detail_errors: HashMap::new(),
            container_logs: None,
        };

        app.ensure_font_definitions(&cc.egui_ctx);
        app.apply_text_style_overrides(&cc.egui_ctx);
        app.reset_new_switch_form();
        app.log_console("Nova Manager v0.1.0 initialized");
        app.log_console("Ready for virtualization management");
        app.refresh_instances(true);
        app.refresh_network_summary(true);
        app.refresh_template_summary();

        app
    }

    fn refresh_instances(&mut self, force: bool) {
        if !self.auto_refresh && !force {
            return;
        }

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

        let active_names: HashSet<String> = self
            .instances_cache
            .iter()
            .map(|instance| instance.name.clone())
            .collect();
        self.container_details
            .retain(|name, _| active_names.contains(name));
        self.container_detail_errors
            .retain(|name, _| active_names.contains(name));
        if self
            .container_logs
            .as_ref()
            .map(|state| !active_names.contains(&state.name))
            .unwrap_or(false)
        {
            self.container_logs = None;
        }
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

    fn set_theme(&mut self, theme: theme::GuiTheme) {
        if self.theme == theme {
            return;
        }

        self.theme = theme;
        self._config.ui.theme = theme.name().to_string();
        self.preferences_dirty = true;
    }

    fn theme_menu(&mut self, ui: &mut egui::Ui) {
        for option in theme::ALL_THEMES.iter() {
            let is_selected = self.theme == *option;
            let response = ui.selectable_label(is_selected, option.label());
            if response.clicked() && !is_selected {
                self.set_theme(*option);
                theme::apply_theme(ui.ctx(), self.theme);
                self.apply_text_style_overrides(ui.ctx());
                self.ensure_font_definitions(ui.ctx());
                let _ = self.persist_ui_preferences(Some(format!(
                    "Theme set to {} via menu",
                    option.label()
                )));
                ui.close_menu();
            }
        }
    }

    fn ensure_font_definitions(&mut self, ctx: &egui::Context) {
        if !self.fonts_dirty {
            return;
        }

        let mut fonts = egui::FontDefinitions::default();

        if self.font_family == FONT_CHOICES[0].id {
            match self.resolve_font_bytes(FONT_CHOICES[0].id) {
                Some(bytes) => {
                    fonts.font_data.insert(
                        "nova.pref.fira".to_string(),
                        egui::FontData::from_owned((*bytes).clone()),
                    );
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .insert(0, "nova.pref.fira".to_string());
                    fonts
                        .families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .insert(0, "nova.pref.fira".to_string());
                    self.font_load_error = None;
                }
                None => {
                    self.font_load_error = Some(
                        "Fira Code Nerd Font not discovered. Install it under ~/.local/share/fonts or /usr/share/fonts and press Retry.".to_string(),
                    );
                }
            }
        } else {
            self.font_load_error = None;
        }

        ctx.set_fonts(fonts);
        self.fonts_dirty = false;
    }

    fn apply_text_style_overrides(&mut self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        let font_size = self.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        if (font_size - self.font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self._config.ui.font_size = font_size;
        }

        let heading_family = if self.font_family == FONT_CHOICES[0].id {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        };

        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(font_size + 6.0, heading_family.clone()),
        );
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(font_size, heading_family.clone()),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(font_size, heading_family.clone()),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(
                (font_size - 2.0).max(MIN_FONT_SIZE - 2.0),
                heading_family.clone(),
            ),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(font_size - 1.0, egui::FontFamily::Monospace),
        );

        if self.compact_layout {
            style.spacing.item_spacing = egui::vec2(6.0, 4.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);
            style.spacing.menu_margin = egui::Margin::symmetric(8.0, 6.0);
            style.spacing.indent = 18.0;
            style.spacing.window_margin = egui::Margin::symmetric(10.0, 8.0);
        } else {
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.button_padding = egui::vec2(16.0, 8.0);
            style.spacing.menu_margin = egui::Margin::same(10.0);
            style.spacing.indent = 24.0;
            style.spacing.window_margin = egui::Margin::same(12.0);
        }

        ctx.set_style(style);
    }

    fn resolve_font_bytes(&mut self, font_id: &str) -> Option<Arc<Vec<u8>>> {
        if let Some(cached) = self.font_cache.get(font_id) {
            return cached.clone();
        }

        let result = match font_id {
            "fira-code-nerd" => Self::load_fira_code_font(),
            _ => None,
        };

        if let Some(bytes) = result.clone() {
            self.font_cache.insert(font_id.to_string(), Some(bytes));
        } else {
            self.font_cache.insert(font_id.to_string(), None);
        }

        result
    }

    fn load_fira_code_font() -> Option<Arc<Vec<u8>>> {
        let candidates = Self::fira_code_candidates();
        for path in candidates {
            if let Ok(bytes) = fs::read(&path) {
                return Some(Arc::new(bytes));
            }
        }
        None
    }

    fn fira_code_candidates() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local/share/fonts/NerdFonts/FiraCodeNerdFontMono-SemiBold.ttf"));
            paths.push(home.join(".local/share/fonts/FiraCodeNerdFontMono-SemiBold.ttf"));
            paths.push(home.join(".local/share/fonts/FiraCodeNerdFont-Regular.ttf"));
        }

        paths.push(PathBuf::from(
            "/usr/share/fonts/truetype/nerd-fonts/FiraCodeNerdFontMono-SemiBold.ttf",
        ));
        paths.push(PathBuf::from(
            "/usr/share/fonts/truetype/firacode/FiraCodeNerdFontMono-Regular.ttf",
        ));
        paths.push(PathBuf::from(
            "/usr/share/fonts/truetype/firacode/FiraCode-Regular.ttf",
        ));
        paths.push(PathBuf::from("/usr/share/fonts/OTF/FiraCode-Regular.otf"));

        paths
    }

    fn font_choice_label(&self) -> &'static str {
        FONT_CHOICES
            .iter()
            .find(|choice| choice.id == self.font_family)
            .map(|choice| choice.label)
            .unwrap_or(FONT_CHOICES[1].label)
    }

    fn font_choice_description(&self) -> &'static str {
        FONT_CHOICES
            .iter()
            .find(|choice| choice.id == self.font_family)
            .map(|choice| choice.description)
            .unwrap_or(FONT_CHOICES[1].description)
    }

    fn open_preferences(&mut self) {
        if !self.show_preferences {
            self.preferences_backup = Some(UiPreferencesSnapshot {
                theme: self.theme,
                font_family: self.font_family.clone(),
                font_size: self.font_size,
                compact_layout: self.compact_layout,
                auto_refresh: self.auto_refresh,
                refresh_interval_secs: self.refresh_interval.as_secs(),
                network_refresh_secs: self.network_refresh_interval.as_secs(),
                show_event_log: self.show_console,
                show_insights: self.show_insights,
                confirm_actions: self.confirm_instance_actions,
            });
            self.preferences_dirty = false;
            self.show_preferences = true;
        }
    }

    fn cancel_preferences(&mut self, ctx: &egui::Context) {
        if let Some(snapshot) = self.preferences_backup.take() {
            self.theme = snapshot.theme;
            self._config.ui.theme = snapshot.theme.name().to_string();
            self.font_family = snapshot.font_family;
            self._config.ui.font_family = self.font_family.clone();
            self.font_size = snapshot.font_size;
            self._config.ui.font_size = snapshot.font_size;
            self.compact_layout = snapshot.compact_layout;
            self._config.ui.compact_layout = snapshot.compact_layout;
            self.auto_refresh = snapshot.auto_refresh;
            self._config.ui.auto_refresh = snapshot.auto_refresh;
            let restored_refresh = snapshot.refresh_interval_secs.max(1);
            self.refresh_interval = Duration::from_secs(restored_refresh);
            self._config.ui.refresh_interval_seconds = restored_refresh;
            let restored_network = snapshot.network_refresh_secs.max(5);
            self.network_refresh_interval = Duration::from_secs(restored_network);
            self._config.ui.network_refresh_interval_seconds = restored_network;
            self.show_console = snapshot.show_event_log;
            self._config.ui.show_event_log = snapshot.show_event_log;
            self.show_insights = snapshot.show_insights;
            self._config.ui.show_insights = snapshot.show_insights;
            self.confirm_instance_actions = snapshot.confirm_actions;
            self._config.ui.confirm_instance_actions = snapshot.confirm_actions;
            self.last_refresh = None;
            self.last_network_refresh = None;
            self.fonts_dirty = true;
            self.preferences_dirty = false;
            self.show_preferences = false;
            theme::apply_theme(ctx, self.theme);
            self.ensure_font_definitions(ctx);
            self.apply_text_style_overrides(ctx);
        } else {
            self.show_preferences = false;
        }
    }

    fn persist_ui_preferences(&mut self, message: Option<String>) -> bool {
        match self._config.save_to_file(&self.config_path) {
            Ok(_) => {
                if let Some(msg) = message {
                    self.log_console(msg);
                }
                self.preferences_dirty = false;
                true
            }
            Err(err) => {
                let failure = format!(
                    "Failed to write GUI preferences to {}: {}",
                    self.config_path.display(),
                    err
                );
                self.log_console(failure.clone());
                error!("{}", failure);
                false
            }
        }
    }

    fn draw_preferences_window(&mut self, ctx: &egui::Context) {
        if !self.show_preferences {
            return;
        }

        let mut open = true;
        egui::Window::new("Preferences")
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("Appearance");
                ui.separator();
                for option in theme::ALL_THEMES.iter() {
                    let is_selected = self.theme == *option;
                    if ui.selectable_label(is_selected, option.label()).clicked() && !is_selected {
                        self.set_theme(*option);
                        theme::apply_theme(ui.ctx(), self.theme);
                        self.apply_text_style_overrides(ui.ctx());
                        self.ensure_font_definitions(ui.ctx());
                    }
                }

                ui.add_space(8.0);
                egui::ComboBox::from_id_source("nova.pref.font.family")
                    .selected_text(self.font_choice_label())
                    .show_ui(ui, |ui| {
                        for choice in FONT_CHOICES.iter() {
                            let is_selected = self.font_family == choice.id;
                            if ui.selectable_label(is_selected, choice.label).clicked()
                                && !is_selected
                            {
                                self.font_family = choice.id.to_string();
                                self._config.ui.font_family = self.font_family.clone();
                                self.fonts_dirty = true;
                                self.preferences_dirty = true;
                                if choice.id != "fira-code-nerd" {
                                    self.font_load_error = None;
                                }
                                self.font_cache.remove(choice.id);
                                self.ensure_font_definitions(ui.ctx());
                                self.apply_text_style_overrides(ui.ctx());
                            }
                        }
                    });
                ui.small(self.font_choice_description());

                if self.font_family == FONT_CHOICES[0].id {
                    if let Some(warning) = &self.font_load_error {
                        ui.colored_label(theme::STATUS_WARNING, warning);
                    }
                    if ui.button("Retry font discovery").clicked() {
                        self.font_cache.remove(FONT_CHOICES[0].id);
                        self.fonts_dirty = true;
                        self.ensure_font_definitions(ui.ctx());
                        self.apply_text_style_overrides(ui.ctx());
                    }
                }

                let mut size = self.font_size;
                if ui
                    .add(
                        egui::Slider::new(&mut size, MIN_FONT_SIZE..=MAX_FONT_SIZE)
                            .text("Font size"),
                    )
                    .changed()
                {
                    self.font_size = size;
                    self._config.ui.font_size = size;
                    self.preferences_dirty = true;
                    self.apply_text_style_overrides(ui.ctx());
                }

                if ui
                    .checkbox(&mut self.compact_layout, "Compact layout spacing")
                    .changed()
                {
                    self._config.ui.compact_layout = self.compact_layout;
                    self.preferences_dirty = true;
                    self.apply_text_style_overrides(ui.ctx());
                }

                ui.add_space(12.0);
                ui.heading("Behaviour");
                ui.separator();

                if ui
                    .checkbox(&mut self.auto_refresh, "Auto refresh instance state")
                    .changed()
                {
                    self._config.ui.auto_refresh = self.auto_refresh;
                    self.preferences_dirty = true;
                    if self.auto_refresh {
                        self.last_refresh = None;
                    }
                }

                let mut refresh_secs = self.refresh_interval.as_secs() as i32;
                let refresh_slider = egui::Slider::new(
                    &mut refresh_secs,
                    MIN_INSTANCE_REFRESH_SECONDS..=MAX_INSTANCE_REFRESH_SECONDS,
                )
                .text("Refresh cadence (seconds)");
                if ui.add_enabled(self.auto_refresh, refresh_slider).changed() {
                    let adjusted = refresh_secs.max(MIN_INSTANCE_REFRESH_SECONDS) as u64;
                    self.refresh_interval = Duration::from_secs(adjusted);
                    self._config.ui.refresh_interval_seconds = adjusted;
                    self.preferences_dirty = true;
                    self.last_refresh = None;
                }

                if !self.auto_refresh {
                    ui.small("Manual refresh only – use the toolbar action when needed.");
                }

                if ui
                    .checkbox(&mut self.show_console, "Show event log panel")
                    .changed()
                {
                    self._config.ui.show_event_log = self.show_console;
                    self.preferences_dirty = true;
                }

                if ui
                    .checkbox(&mut self.show_insights, "Show insights panel")
                    .changed()
                {
                    self._config.ui.show_insights = self.show_insights;
                    self.preferences_dirty = true;
                }

                if ui
                    .checkbox(
                        &mut self.confirm_instance_actions,
                        "Confirm before stopping or restarting workloads",
                    )
                    .changed()
                {
                    self._config.ui.confirm_instance_actions = self.confirm_instance_actions;
                    self.preferences_dirty = true;
                }

                ui.add_space(12.0);
                ui.heading("System");
                ui.separator();

                let mut network_secs = self.network_refresh_interval.as_secs() as i32;
                if ui
                    .add(
                        egui::Slider::new(
                            &mut network_secs,
                            MIN_NETWORK_REFRESH_SECONDS..=MAX_NETWORK_REFRESH_SECONDS,
                        )
                        .text("Network telemetry cadence (seconds)"),
                    )
                    .changed()
                {
                    let adjusted = network_secs.max(MIN_NETWORK_REFRESH_SECONDS) as u64;
                    self.network_refresh_interval = Duration::from_secs(adjusted);
                    self._config.ui.network_refresh_interval_seconds = adjusted;
                    self.preferences_dirty = true;
                    self.last_network_refresh = None;
                }

                ui.small(format!("NovaFile: {}", self.config_path.display()));
                ui.small("Network metrics refresh automatically when telemetry is available.");

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.preferences_dirty, egui::Button::new("Save & Close"))
                        .clicked()
                    {
                        if self
                            .persist_ui_preferences(Some("Saved Nova UI preferences".to_string()))
                        {
                            self.preferences_backup = None;
                            self.show_preferences = false;
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        self.cancel_preferences(ui.ctx());
                    }
                });
            });

        if !open {
            if self.preferences_dirty {
                self.cancel_preferences(ctx);
            } else {
                self.preferences_backup = None;
                self.show_preferences = false;
            }
        }
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
        if self.confirm_instance_actions
            && matches!(action, InstanceAction::Stop | InstanceAction::Restart)
        {
            self.pending_action = Some(PendingAction { action, instance });
            self.show_action_confirmation = true;
            return;
        }

        self.execute_instance_action(action, instance);
    }

    fn execute_instance_action(&mut self, action: InstanceAction, instance: Instance) {
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

        self.pending_action = None;
        self.show_action_confirmation = false;
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
        let visuals = ui.visuals().clone();
        egui::Frame::none()
            .fill(visuals.widgets.noninteractive.bg_fill)
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

        let status_color = theme::get_status_color(&instance.status, self.theme);
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
            let status_color = theme::get_status_color(&instance.status, self.theme);
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

    fn container_detail(
        &mut self,
        name: &str,
        force_refresh: bool,
    ) -> Option<ContainerDetailCache> {
        if force_refresh {
            self.container_details.remove(name);
            self.container_detail_errors.remove(name);
        }

        let needs_refresh = force_refresh
            || self
                .container_details
                .get(name)
                .map(|entry| entry.fetched_at.elapsed() > Duration::from_secs(30))
                .unwrap_or(true);

        if needs_refresh {
            let should_attempt = force_refresh
                || self
                    .container_detail_errors
                    .get(name)
                    .map(|error| error.recorded_at.elapsed() > Duration::from_secs(15))
                    .unwrap_or(true);

            if should_attempt {
                match self.fetch_container_detail(name.to_string()) {
                    Ok(detail) => {
                        self.container_details
                            .insert(name.to_string(), detail.clone());
                        self.container_detail_errors.remove(name);
                        return Some(detail);
                    }
                    Err(message) => {
                        self.container_detail_errors.insert(
                            name.to_string(),
                            ContainerDetailError {
                                message,
                                recorded_at: Instant::now(),
                            },
                        );
                    }
                }
            }
        }

        self.container_details.get(name).cloned()
    }

    fn fetch_container_detail(
        &self,
        name: String,
    ) -> std::result::Result<ContainerDetailCache, String> {
        let manager = self.container_manager.clone();
        self.runtime.block_on(async move {
            let info = manager
                .inspect_container(&name)
                .await
                .map_err(|err| err.to_string())?;
            let stats = match manager.container_stats(&name).await {
                Ok(stats) => Some(stats),
                Err(err) => {
                    warn!("Failed to gather container stats for '{}': {}", name, err);
                    None
                }
            };
            Ok(ContainerDetailCache {
                info,
                stats,
                fetched_at: Instant::now(),
            })
        })
    }

    fn fetch_container_logs(&self, name: &str, lines: usize) -> ContainerLogsState {
        let manager = self.container_manager.clone();
        let name_owned = name.to_string();
        let fetch_name = name_owned.clone();
        match self
            .runtime
            .block_on(async move { manager.get_container_logs(&fetch_name, lines).await })
        {
            Ok(lines_vec) => ContainerLogsState {
                name: name_owned,
                lines: lines_vec,
                error: None,
                fetched_at: Instant::now(),
            },
            Err(err) => ContainerLogsState {
                name: name_owned,
                lines: Vec::new(),
                error: Some(err.to_string()),
                fetched_at: Instant::now(),
            },
        }
    }

    fn draw_container_logs_window(&mut self, ctx: &egui::Context) {
        if let Some(mut state) = self.container_logs.take() {
            let mut open = true;
            let mut refresh_requested = false;
            egui::Window::new(format!("Container logs – {}", state.name))
                .resizable(true)
                .default_width(520.0)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Refresh").clicked() {
                            refresh_requested = true;
                        }
                        if ui.button("Copy to clipboard").clicked() {
                            ui.output_mut(|out| out.copied_text = state.lines.join("\n"));
                        }
                        ui.add_space(12.0);
                        ui.small(format!(
                            "Fetched {}",
                            Self::format_elapsed(state.fetched_at.elapsed())
                        ));
                    });

                    if let Some(error) = &state.error {
                        ui.colored_label(theme::STATUS_WARNING, error);
                    }

                    egui::ScrollArea::vertical()
                        .id_source(format!("nova.container.logs.{}", state.name))
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &state.lines {
                                ui.monospace(line);
                            }
                        });
                });

            if refresh_requested {
                state = self.fetch_container_logs(&state.name, 200);
            }

            if open {
                self.container_logs = Some(state);
            }
        }
    }

    fn draw_action_confirmation(&mut self, ctx: &egui::Context) {
        if !self.show_action_confirmation {
            return;
        }

        let Some(pending) = self.pending_action.clone() else {
            self.show_action_confirmation = false;
            return;
        };

        let action_label = match pending.action {
            InstanceAction::Start => "start",
            InstanceAction::Stop => "stop",
            InstanceAction::Restart => "restart",
        };
        let workload_label = match pending.instance.instance_type {
            InstanceType::Vm => "virtual machine",
            InstanceType::Container => "container",
        };

        let mut open = true;
        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("Confirm workload action")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("Hold on a moment");
                ui.label(format!(
                    "You're about to {action_label} the {workload_label} '{}'.",
                    pending.instance.name
                ));
                if pending.action == InstanceAction::Restart {
                    ui.small("Nova will stop the workload before bringing it back online.");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                    let confirm_label = match pending.action {
                        InstanceAction::Start => "Start workload",
                        InstanceAction::Stop => "Stop workload",
                        InstanceAction::Restart => "Restart workload",
                    };
                    let confirm_button = ui.button(confirm_label);
                    if confirm_button.clicked() {
                        confirmed = true;
                    }
                });
            });

        if confirmed {
            self.execute_instance_action(pending.action, pending.instance);
        } else if cancelled || !open {
            self.pending_action = None;
            self.show_action_confirmation = false;
        }
    }

    fn draw_overview(&mut self, ui: &mut egui::Ui, instance: &Instance) {
        match instance.instance_type {
            InstanceType::Vm => self.draw_vm_overview(ui, instance),
            InstanceType::Container => self.draw_container_overview(ui, instance),
        }
    }

    fn draw_vm_overview(&self, ui: &mut egui::Ui, instance: &Instance) {
        let status_color = theme::get_status_color(&instance.status, self.theme);
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

    fn draw_container_overview(&mut self, ui: &mut egui::Ui, instance: &Instance) {
        let status_color = theme::get_status_color(&instance.status, self.theme);
        let uptime = Utc::now().signed_duration_since(instance.created_at);
        let time_since_update = Utc::now().signed_duration_since(instance.last_updated);
        let runtime_name = self.container_manager.get_runtime_name().to_string();

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
                ui.label("Container");
                ui.small(format!("Runtime: {runtime_name}"));
                if let Some(pid) = instance.pid {
                    ui.monospace(format!("PID {pid}"));
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(format!("Uptime {uptime_str}"));
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

        let mut detail_opt = self.container_detail(&instance.name, false);
        let mut error_opt = self.container_detail_errors.get(&instance.name).cloned();

        let mut refresh_requested = false;
        let mut logs_requested = false;
        let mut pull_image: Option<String> = None;

        ui.horizontal(|ui| {
            if ui.button("Refresh detail").clicked() {
                refresh_requested = true;
            }
            if ui
                .add_enabled(
                    instance.status == InstanceStatus::Running,
                    egui::Button::new("View logs"),
                )
                .clicked()
            {
                logs_requested = true;
            }
            if let Some(detail) = detail_opt.as_ref() {
                if ui.button("Pull image").clicked() {
                    pull_image = Some(detail.info.image.clone());
                }
            }
        });

        if refresh_requested {
            detail_opt = self.container_detail(&instance.name, true);
            error_opt = self.container_detail_errors.get(&instance.name).cloned();
        }

        if logs_requested {
            let log_state = self.fetch_container_logs(&instance.name, 200);
            self.container_logs = Some(log_state);
        }

        if let Some(image) = pull_image {
            let manager = self.container_manager.clone();
            let name = instance.name.clone();
            self.log_console(format!("Pulling latest image '{image}' for {name}"));
            self.runtime.spawn(async move {
                if let Err(err) = manager.pull_image(&image).await {
                    error!("Failed to pull image {image}: {err:?}");
                }
            });
        }

        match detail_opt {
            Some(detail) => {
                let info = &detail.info;
                let fetched_label = Self::format_elapsed(detail.fetched_at.elapsed());

                ui.add_space(10.0);
                ui.columns(2, |columns| {
                    columns[0].group(|ui| {
                        ui.label(egui::RichText::new("Image & identity").strong());
                        ui.separator();
                        ui.label(format!("Image: {}", info.image));
                        ui.small(format!("Container ID: {}", info.id));
                        ui.label(format!(
                            "Created {}",
                            info.created.with_timezone(&Local).format("%Y-%m-%d %H:%M")
                        ))
                        .on_hover_text("Timestamp aligns with runtime inspect data");
                        if let Some(pid) = info.pid {
                            ui.monospace(format!("PID {pid}"));
                        }
                    });

                    columns[1].group(|ui| {
                        ui.label(egui::RichText::new("Connectivity").strong());
                        ui.separator();
                        if let Some(ip) = &info.ip_address {
                            ui.label(format!("IP address: {ip}"));
                        } else if let Some(ip) = &instance.ip_address {
                            ui.label(format!("Guest IP (cached): {ip}"));
                        } else {
                            ui.label("No IP discovered yet");
                        }

                        if let Some(network) = &info.network {
                            ui.label(format!("Network: {network}"));
                        } else {
                            ui.label("Network: default bridge");
                        }

                        if info.ports.is_empty() {
                            ui.small("No port mappings declared");
                        } else {
                            ui.label("Ports:");
                            for port in info.ports.iter() {
                                let protocol = match port.protocol {
                                    nova::container_runtime::PortProtocol::Tcp => "tcp",
                                    nova::container_runtime::PortProtocol::Udp => "udp",
                                };
                                ui.monospace(format!(
                                    "{} -> {}/{}",
                                    port.host_port, port.container_port, protocol
                                ));
                            }
                        }
                    });
                });

                if let Some(stats) = detail.stats.as_ref() {
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Runtime metrics").strong());
                        ui.separator();
                        ui.label(format!("CPU usage: {:.1}%", stats.cpu_usage_percent));
                        ui.label(format!(
                            "Memory: {:.1} / {:.1} MiB",
                            stats.memory_usage_mb as f64, stats.memory_limit_mb as f64
                        ));
                        ui.label(format!(
                            "Network: {} ↓ / {} ↑",
                            Self::format_bytes(stats.network_rx_bytes),
                            Self::format_bytes(stats.network_tx_bytes)
                        ));
                        ui.label(format!(
                            "Disk IO: {} read / {} write",
                            Self::format_bytes(stats.disk_read_bytes),
                            Self::format_bytes(stats.disk_write_bytes)
                        ));
                    });
                }

                if let Some(cfg) = self._config.container.get(&instance.name) {
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("NovaFile profile").strong());
                        ui.separator();
                        if let Some(capsule) = &cfg.capsule {
                            ui.label(format!("Capsule: {capsule}"));
                        }
                        if let Some(network) = &cfg.network {
                            ui.small(format!("Preferred network: {network}"));
                        }
                        if cfg.bolt.gpu_access {
                            ui.small("Bolt GPU passthrough: enabled");
                        }
                        if !cfg.volumes.is_empty() {
                            ui.label("Volumes:");
                            for volume in cfg.volumes.iter() {
                                ui.monospace(volume);
                            }
                        }
                        if !cfg.env.is_empty() {
                            ui.label("Environment:");
                            let mut env_pairs: Vec<_> = cfg.env.iter().collect();
                            env_pairs.sort_by(|a, b| a.0.cmp(b.0));
                            for (index, (key, value)) in env_pairs.iter().enumerate() {
                                if index >= 8 {
                                    ui.small(format!("… {} more", env_pairs.len() - index));
                                    break;
                                }
                                ui.monospace(format!("{key}={value}"));
                            }
                        }
                    });
                }

                ui.add_space(6.0);
                ui.small(format!("Inspection cached {fetched_label}"));
            }
            None => {
                if let Some(error) = error_opt {
                    ui.colored_label(
                        theme::STATUS_WARNING,
                        format!("Inspection failed: {}", error.message),
                    );
                    ui.small(format!(
                        "Last attempt {}",
                        Self::format_elapsed(error.recorded_at.elapsed())
                    ));
                } else {
                    ui.label("Collecting container metadata…");
                }
            }
        }
    }

    fn format_bytes(bytes: u64) -> String {
        const KB: f64 = 1024.0;
        let value = bytes as f64;
        if value >= KB * KB * KB {
            format!("{:.1} GiB", value / (KB * KB * KB))
        } else if value >= KB * KB {
            format!("{:.1} MiB", value / (KB * KB))
        } else if value >= KB {
            format!("{:.1} KiB", value / KB)
        } else {
            format!("{bytes} B")
        }
    }

    fn format_elapsed(duration: Duration) -> String {
        if duration.as_secs() < 2 {
            "just now".to_string()
        } else if duration.as_secs() < 60 {
            format!("{}s ago", duration.as_secs())
        } else if duration.as_secs() < 3600 {
            format!(
                "{}m {}s ago",
                duration.as_secs() / 60,
                duration.as_secs() % 60
            )
        } else {
            format!(
                "{}h {}m ago",
                duration.as_secs() / 3600,
                (duration.as_secs() % 3600) / 60
            )
        }
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
        egui::TopBottomPanel::top("nova.header").show(ctx, |ui| {
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
            if ui.button("⚙ Preferences").clicked() {
                self.open_preferences();
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

                            let status_color =
                                theme::get_status_color(&instance.status, self.theme);
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
        theme::apply_theme(ctx, self.theme);
        self.ensure_font_definitions(ctx);
        self.apply_text_style_overrides(ctx);

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
                    ui.separator();
                    ui.menu_button("Theme", |ui| {
                        self.theme_menu(ui);
                    });
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

        egui::CentralPanel::default().show(ctx, |ui| {
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
                        ui.label(
                            "Choose a VM or container from the inventory to drill into metrics.",
                        );
                    });
                }
            });
        });

        self.draw_preferences_window(ctx);
        self.draw_container_logs_window(ctx);
        self.draw_action_confirmation(ctx);

        ctx.request_repaint_after(self.refresh_interval.min(self.network_refresh_interval));
    }
}
