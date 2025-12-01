use eframe::egui;
use nova::{
    config::{NovaConfig, default_ui_font_family, default_ui_font_size},
    console_enhanced::{
        ActiveProtocol, EnhancedConsoleConfig, EnhancedConsoleManager, UnifiedConsoleSession,
    },
    container::ContainerManager,
    container_runtime::{ContainerInfo, ContainerStats},
    firewall::FirewallManager,
    gui_gpu::GpuManagerWindow,
    gui_network::NetworkingGui,
    instance::{Instance, InstanceStatus, InstanceType},
    logger,
    network::{
        InterfaceState,
        NetworkInterface,
        NetworkManager,
        NetworkSummary,
        SwitchOrigin,
        SwitchProfile,
        SwitchStatus,
        SwitchType,
        VirtualSwitch,
    },
    preflight::PreflightSummary,
    sriov::SriovManager,
    storage_pool::StoragePoolManager,
    templates_snapshots::{OperatingSystem, TemplateManager, VmTemplate},
    theme::{self, ButtonIntent, ButtonRole},
    usb_passthrough::{UsbDevice, UsbManager},
    vm::VmManager,
    ArchNetworkManager, LibvirtManager, NetworkMonitor,
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
const MIN_LOG_REFRESH_SECONDS: i32 = 5;
const MAX_LOG_REFRESH_SECONDS: i32 = 120;
const DEFAULT_LOG_REFRESH_SECONDS: u64 = 15;

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
    container_logs_auto_refresh: bool,
    container_logs_refresh_secs: u64,
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

#[derive(Clone)]
struct SriovDeviceInfo {
    pf_address: String,   // e.g., "0000:06:00.0"
    device_name: String,  // e.g., "Intel X710"
    vendor: String,
    driver: String,
    max_vfs: u32,
    active_vfs: u32,
    device_type: String,  // "GPU", "NIC", "Other"
}

#[derive(Clone)]
struct FirewallRuleInfo {
    chain: String,
    table: String,
    action: String,      // ACCEPT, DROP, REJECT
    protocol: String,    // tcp, udp, icmp, all
    port: String,        // e.g., "22", "80:443", ""
    source: String,      // e.g., "0.0.0.0/0", "192.168.1.0/24"
    destination: String,
    comment: String,
    packets: u64,
    bytes: u64,
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
    libvirt_manager: Arc<Mutex<LibvirtManager>>,
    network_monitor: Arc<Mutex<NetworkMonitor>>,
    arch_network_manager: Arc<Mutex<ArchNetworkManager>>,
    enhanced_console: Arc<AsyncMutex<EnhancedConsoleManager>>,
    template_manager: Arc<AsyncMutex<TemplateManager>>,
    session_events: Arc<Mutex<Vec<SessionEvent>>>,
    usb_manager: Arc<Mutex<UsbManager>>,
    storage_pool_manager: Arc<Mutex<StoragePoolManager>>,
    sriov_manager: Arc<Mutex<SriovManager>>,
    firewall_manager: Arc<Mutex<FirewallManager>>,
    _config: NovaConfig,
    config_path: PathBuf,
    runtime: Runtime,
    template_summary: TemplateCatalogSummary,
    networking_gui: NetworkingGui,

    selected_instance: Option<String>,
    show_console: bool,
    console_output: Vec<String>,
    active_sessions: Vec<UnifiedConsoleSession>,
    last_session_error: Option<String>,
    gpu_window: Option<GpuManagerWindow>,
    show_network_manager: bool,

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
    container_logs_filter: String,
    container_logs_auto_refresh: bool,
    container_logs_refresh_interval: Duration,

    // Dialog states
    show_new_vm_dialog: bool,
    show_new_container_dialog: bool,
    show_about_dialog: bool,
    show_usb_manager: bool,
    show_storage_manager: bool,
    show_sriov_manager: bool,
    show_migration_dialog: bool,
    show_preflight_dialog: bool,
    show_metrics_panel: bool,
    show_support_dialog: bool,
    show_firewall_manager: bool,

    // New VM dialog state
    new_vm_name: String,
    new_vm_cpu: u32,
    new_vm_memory: String,
    new_vm_disk_size: String,
    new_vm_network: String,
    new_vm_iso_path: String,
    new_vm_enable_gpu: bool,
    new_vm_enable_uefi: bool,
    new_vm_enable_secure_boot: bool,
    new_vm_enable_tpm: bool,
    new_vm_autostart: bool,
    new_vm_selected_template: Option<String>,
    available_templates: std::collections::HashMap<String, nova::config::VmTemplateConfig>,
    available_isos: Vec<nova::vm_templates::IsoFile>,

    // New Container dialog state
    new_container_name: String,
    new_container_image: String,
    new_container_ports: String,
    new_container_volumes: String,
    new_container_env_vars: String,
    new_container_network: String,

    // Cached data from managers
    usb_devices_cache: Vec<UsbDevice>,
    storage_pools_cache: Vec<(String, String, String, String, u64, u64, u64)>, // name, type, path, state, capacity, used, available
    sriov_devices_cache: Vec<SriovDeviceInfo>, // SR-IOV capable devices
    firewall_rules_cache: Vec<FirewallRuleInfo>, // Firewall rules from nft/iptables
    firewall_backend: String, // nftables, iptables, firewalld
    preflight_result: Option<PreflightSummary>,
    migration_dest_host: String,
    migration_offline: bool,
    migration_copy_storage: bool,
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

        let container_logs_auto_refresh = config.ui.container_logs_auto_refresh;
        let min_log_refresh = MIN_LOG_REFRESH_SECONDS as u64;
        let max_log_refresh = MAX_LOG_REFRESH_SECONDS as u64;
        let mut log_refresh_secs = config.ui.container_logs_refresh_interval_seconds;
        if log_refresh_secs == 0 {
            log_refresh_secs = DEFAULT_LOG_REFRESH_SECONDS;
        }
        let clamped_log_refresh = log_refresh_secs.clamp(min_log_refresh, max_log_refresh);
        if clamped_log_refresh != log_refresh_secs {
            config.ui.container_logs_refresh_interval_seconds = clamped_log_refresh;
            config_dirty = true;
        }
        let container_logs_refresh_interval = Duration::from_secs(clamped_log_refresh);

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
        let libvirt_manager = Arc::new(Mutex::new(LibvirtManager::new()));
        let network_monitor = Arc::new(Mutex::new(NetworkMonitor::new()));
        let arch_network_manager = Arc::new(Mutex::new(ArchNetworkManager::new()));
        let enhanced_console = Arc::new(AsyncMutex::new(EnhancedConsoleManager::new(
            EnhancedConsoleConfig::default(),
        )));
        let networking_gui = NetworkingGui::with_managers(
            Arc::clone(&network_manager),
            Arc::clone(&libvirt_manager),
            Arc::clone(&network_monitor),
            Arc::clone(&arch_network_manager),
        );

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

        // Additional managers
        let usb_manager = Arc::new(Mutex::new(UsbManager::new()));
        let storage_pool_manager = Arc::new(Mutex::new(StoragePoolManager::new()));
        let sriov_manager = Arc::new(Mutex::new(SriovManager::new()));
        let firewall_manager = Arc::new(Mutex::new(
            FirewallManager::new().unwrap_or_else(|e| {
                warn!("Failed to initialize firewall manager: {:?}", e);
                FirewallManager::default()
            })
        ));

        let runtime = Runtime::new().expect("failed to initialize Tokio runtime");

        let compact_layout = config.ui.compact_layout;
        let iso_paths = config.iso.paths.clone();

        let mut app = Self {
            vm_manager,
            container_manager,
            network_manager,
            libvirt_manager,
            network_monitor,
            arch_network_manager,
            enhanced_console,
            template_manager,
            session_events,
            usb_manager,
            storage_pool_manager,
            sriov_manager,
            firewall_manager,
            _config: config,
            config_path,
            runtime,
            template_summary: TemplateCatalogSummary::default(),
            networking_gui,
            selected_instance: None,
            show_console,
            console_output: Vec::new(),
            active_sessions: Vec::new(),
            last_session_error: None,
            gpu_window: None,
            show_network_manager: false,
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
            container_logs_filter: String::new(),
            container_logs_auto_refresh,
            container_logs_refresh_interval,

            // Dialog states
            show_new_vm_dialog: false,
            show_new_container_dialog: false,
            show_about_dialog: false,
            show_usb_manager: false,
            show_storage_manager: false,
            show_sriov_manager: false,
            show_migration_dialog: false,
            show_preflight_dialog: false,
            show_metrics_panel: false,
            show_support_dialog: false,
            show_firewall_manager: false,

            // New VM dialog state
            new_vm_name: String::new(),
            new_vm_cpu: 4,
            new_vm_memory: "8G".to_string(),
            new_vm_disk_size: "64G".to_string(),
            new_vm_network: "virbr0".to_string(),
            new_vm_iso_path: String::new(),
            new_vm_enable_gpu: false,
            new_vm_enable_uefi: true,
            new_vm_enable_secure_boot: false,
            new_vm_enable_tpm: false,
            new_vm_autostart: false,
            new_vm_selected_template: None,
            available_templates: nova::vm_templates::builtin_templates(),
            available_isos: nova::vm_templates::scan_iso_directories(&iso_paths),

            // New Container dialog state
            new_container_name: String::new(),
            new_container_image: String::new(),
            new_container_ports: String::new(),
            new_container_volumes: String::new(),
            new_container_env_vars: String::new(),
            new_container_network: "bridge".to_string(),

            // Cached data from managers
            usb_devices_cache: Vec::new(),
            storage_pools_cache: Vec::new(),
            sriov_devices_cache: Vec::new(),
            firewall_rules_cache: Vec::new(),
            firewall_backend: String::new(),
            preflight_result: None,
            migration_dest_host: String::new(),
            migration_offline: false,
            migration_copy_storage: false,
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

        if let Some(window) = self.gpu_window.as_mut() {
            window.set_theme(self.theme);
        }
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
                container_logs_auto_refresh: self.container_logs_auto_refresh,
                container_logs_refresh_secs: self.container_logs_refresh_interval.as_secs(),
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
            self.container_logs_auto_refresh = snapshot.container_logs_auto_refresh;
            self._config.ui.container_logs_auto_refresh = snapshot.container_logs_auto_refresh;
            let min_log = MIN_LOG_REFRESH_SECONDS as u64;
            let max_log = MAX_LOG_REFRESH_SECONDS as u64;
            let restored_log_refresh = snapshot.container_logs_refresh_secs.clamp(min_log, max_log);
            self.container_logs_refresh_interval = Duration::from_secs(restored_log_refresh);
            self._config.ui.container_logs_refresh_interval_seconds = restored_log_refresh;
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
                    if self
                        .themed_button(ui, "Retry font discovery", ButtonRole::Secondary, true)
                        .clicked()
                    {
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
                ui.heading("Logs");
                ui.separator();

                if ui
                    .checkbox(
                        &mut self.container_logs_auto_refresh,
                        "Auto refresh container logs",
                    )
                    .changed()
                {
                    self._config.ui.container_logs_auto_refresh = self.container_logs_auto_refresh;
                    self.preferences_dirty = true;
                }

                let mut log_refresh_secs = self.container_logs_refresh_interval.as_secs() as i32;
                let log_slider = egui::Slider::new(
                    &mut log_refresh_secs,
                    MIN_LOG_REFRESH_SECONDS..=MAX_LOG_REFRESH_SECONDS,
                )
                .text("Log refresh cadence (seconds)");
                if ui
                    .add_enabled(self.container_logs_auto_refresh, log_slider)
                    .changed()
                {
                    let adjusted = log_refresh_secs
                        .clamp(MIN_LOG_REFRESH_SECONDS, MAX_LOG_REFRESH_SECONDS)
                        as u64;
                    self.container_logs_refresh_interval = Duration::from_secs(adjusted);
                    self._config.ui.container_logs_refresh_interval_seconds = adjusted;
                    self.preferences_dirty = true;
                }

                if !self.container_logs_auto_refresh {
                    ui.small("Manual refresh only – use the logs window actions when needed.");
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
                    if self
                        .themed_button(
                            ui,
                            "Save & Close",
                            ButtonRole::Primary,
                            self.preferences_dirty,
                        )
                        .clicked()
                    {
                        if self
                            .persist_ui_preferences(Some("Saved Nova UI preferences".to_string()))
                        {
                            self.preferences_backup = None;
                            self.show_preferences = false;
                        }
                    }

                    if self
                        .themed_button(ui, "Cancel", ButtonRole::Secondary, true)
                        .clicked()
                    {
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
                    if self
                        .themed_button(ui, "Create switch", ButtonRole::Primary, true)
                        .clicked()
                    {
                        self.handle_create_switch();
                    }
                    if self
                        .themed_button(ui, "Cancel", ButtonRole::Secondary, true)
                        .clicked()
                    {
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
                let manager = console.lock().await;
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

    fn export_container_logs(&mut self, state: &ContainerLogsState, lines: &[&String]) {
        if lines.is_empty() {
            self.log_console(format!(
                "No log lines available to export for {}",
                state.name
            ));
            return;
        }

        let sanitized_name = Self::sanitize_filename(&state.name);
        let timestamp = Local::now().format("%Y%m%d-%H%M%S");
        let filename = format!("nova-logs-{}-{}.log", sanitized_name, timestamp);
        let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let path = base_dir.join(filename);

        let payload = lines
            .iter()
            .map(|line| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        match fs::write(&path, payload) {
            Ok(_) => self.log_console(format!(
                "Exported logs for {} to {}",
                state.name,
                path.display()
            )),
            Err(err) => {
                self.log_console(format!("Failed to export logs for {}: {}", state.name, err))
            }
        }
    }

    fn sanitize_filename(input: &str) -> String {
        let mut sanitized: String = input
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                _ => '-',
            })
            .collect();

        while sanitized.contains("--") {
            sanitized = sanitized.replace("--", "-");
        }

        let trimmed = sanitized.trim_matches('-').to_string();
        if trimmed.is_empty() {
            "container".to_string()
        } else {
            trimmed
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
                        ui.add(
                            egui::TextEdit::singleline(&mut self.container_logs_filter)
                                .hint_text("Search logs…")
                                .desired_width(220.0),
                        );

                        if !self.container_logs_filter.is_empty() {
                            if self
                                .themed_button(ui, "Clear", ButtonRole::Secondary, true)
                                .clicked()
                            {
                                self.container_logs_filter.clear();
                            }
                        }
                    });

                    let filter_trimmed = self.container_logs_filter.trim();
                    let filter_lower = filter_trimmed.to_lowercase();
                    let filtered_lines: Vec<&String> = if filter_trimmed.is_empty() {
                        state.lines.iter().collect()
                    } else {
                        state
                            .lines
                            .iter()
                            .filter(|line| line.to_lowercase().contains(&filter_lower))
                            .collect()
                    };

                    ui.small(format!(
                        "Showing {} of {} lines",
                        filtered_lines.len(),
                        state.lines.len()
                    ));
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        if self
                            .themed_button(ui, "Refresh", ButtonRole::Secondary, true)
                            .clicked()
                        {
                            refresh_requested = true;
                        }
                        if self
                            .themed_button(ui, "Copy to clipboard", ButtonRole::Secondary, true)
                            .clicked()
                        {
                            let joined: String = filtered_lines
                                .iter()
                                .map(|line| line.as_str())
                                .collect::<Vec<_>>()
                                .join("\n");
                            ui.output_mut(|out| out.copied_text = joined);
                        }
                        if self
                            .themed_button(
                                ui,
                                "Save to file",
                                ButtonRole::Secondary,
                                !filtered_lines.is_empty(),
                            )
                            .clicked()
                        {
                            self.export_container_logs(&state, &filtered_lines);
                        }
                        ui.add_space(12.0);
                        ui.label(format!(
                            "Auto refresh: {} ({}s)",
                            if self.container_logs_auto_refresh {
                                "enabled"
                            } else {
                                "disabled"
                            },
                            self.container_logs_refresh_interval.as_secs()
                        ));
                        if self
                            .themed_button(ui, "Preferences…", ButtonRole::Secondary, true)
                            .clicked()
                        {
                            self.open_preferences();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.small(format!(
                                "Fetched {}",
                                Self::format_elapsed(state.fetched_at.elapsed())
                            ));
                        });
                    });

                    if let Some(error) = &state.error {
                        ui.colored_label(theme::STATUS_WARNING, error);
                    }

                    egui::ScrollArea::vertical()
                        .id_source(format!("nova.container.logs.{}", state.name))
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in filtered_lines {
                                ui.monospace(line);
                            }
                        });
                });

            if !refresh_requested
                && self.container_logs_auto_refresh
                && state.error.is_none()
                && state.fetched_at.elapsed() >= self.container_logs_refresh_interval
            {
                refresh_requested = true;
            }

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
                    if self
                        .themed_button(ui, "Cancel", ButtonRole::Secondary, true)
                        .clicked()
                    {
                        cancelled = true;
                    }
                    let (confirm_label, confirm_role) = match pending.action {
                        InstanceAction::Start => ("Start workload", ButtonRole::Start),
                        InstanceAction::Stop => ("Stop workload", ButtonRole::Stop),
                        InstanceAction::Restart => ("Restart workload", ButtonRole::Restart),
                    };
                    if self
                        .themed_button(ui, confirm_label, confirm_role, true)
                        .clicked()
                    {
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

        let created_local = instance
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M")
            .to_string();

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(&instance.name);
                ui.add_space(6.0);
                ui.colored_label(status_color, format!("{:?}", instance.status));
            });
            ui.small(format!("Container runtime: {runtime_name}"));

            ui.add_space(6.0);
            egui::Grid::new("nova.container.overview.lifecycle")
                .num_columns(2)
                .spacing(egui::vec2(12.0, 4.0))
                .show(ui, |grid| {
                    grid.label(egui::RichText::new("Uptime").strong());
                    grid.label(uptime_str.clone());
                    grid.end_row();

                    grid.label(egui::RichText::new("Created").strong());
                    grid.label(created_local.clone());
                    grid.end_row();

                    grid.label(egui::RichText::new("Last update").strong());
                    grid.label(update_str.clone());
                    grid.end_row();

                    grid.label(egui::RichText::new("Type").strong());
                    grid.label("Container");
                    grid.end_row();

                    grid.label(egui::RichText::new("PID").strong());
                    if let Some(pid) = instance.pid {
                        grid.monospace(format!("{pid}"));
                    } else {
                        grid.small("N/A");
                    }
                    grid.end_row();
                });
        });

        ui.add_space(12.0);

        let mut detail_opt = self.container_detail(&instance.name, false);
        let mut error_opt = self.container_detail_errors.get(&instance.name).cloned();

        let mut refresh_requested = false;
        let mut logs_requested = false;
        let mut pull_image: Option<String> = None;

        ui.horizontal(|ui| {
            if self
                .themed_button(ui, "Refresh detail", ButtonRole::Secondary, true)
                .clicked()
            {
                refresh_requested = true;
            }
            if self
                .themed_button(
                    ui,
                    "View logs",
                    ButtonRole::Secondary,
                    instance.status == InstanceStatus::Running,
                )
                .clicked()
            {
                logs_requested = true;
            }
            if let Some(detail) = detail_opt.as_ref() {
                if self
                    .themed_button(ui, "Pull image", ButtonRole::Secondary, true)
                    .clicked()
                {
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
                let _ = self.themed_button(
                    ui,
                    "Create snapshot (preview)",
                    ButtonRole::Secondary,
                    false,
                );
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
            if self
                .themed_button(ui, "Refresh topology", ButtonRole::Secondary, true)
                .clicked()
            {
                self.refresh_network_summary(true);
            }

            if self
                .themed_button(ui, "Create virtual switch", ButtonRole::Primary, true)
                .clicked()
            {
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
                                if self
                                    .themed_button(
                                        ui,
                                        "Attach interface",
                                        ButtonRole::Secondary,
                                        can_attach,
                                    )
                                    .clicked()
                                {
                                    self.handle_attach_interface(&switch.name);
                                }

                                ui.add_space(12.0);
                                if self
                                    .themed_button(ui, "Delete switch", ButtonRole::Stop, true)
                                    .clicked()
                                {
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

                                        if self
                                            .themed_button(ui, "Detach", ButtonRole::Stop, true)
                                            .clicked()
                                        {
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
            if self
                .themed_button(ui, "🚀 Launch session", ButtonRole::Primary, true)
                .clicked()
            {
                self.request_session_launch(instance);
            }
            if self
                .themed_button(ui, "🔄 Refresh list", ButtonRole::Secondary, true)
                .clicked()
            {
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
            ui.small("Use Launch session to open SPICE, VNC, or Looking Glass viewer.");
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

                        // Display protocol info
                        let protocol_name = match &session.protocol_used {
                            ActiveProtocol::LookingGlass => "Looking Glass",
                            ActiveProtocol::SPICE => "SPICE",
                            ActiveProtocol::VNC => "VNC",
                            ActiveProtocol::Serial => "Serial",
                        };
                        ui.label(format!("Protocol: {}", protocol_name));

                        let conn = &session.connection_info;
                        if conn.port > 0 {
                            ui.small(format!("Endpoint: {}:{}", conn.host, conn.port));
                        } else if let Some(shmem) = &conn.shmem_path {
                            ui.small(format!("Shared memory: {}", shmem));
                        }
                        ui.monospace(&conn.viewer_command);

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if self
                                .themed_button(ui, "🪟 Open viewer", ButtonRole::Secondary, true)
                                .clicked()
                            {
                                self.request_session_launch_client(session.session_id.clone());
                            }
                            if self
                                .themed_button(ui, "⏹ Close session", ButtonRole::Stop, true)
                                .clicked()
                            {
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
                    if self
                        .themed_button(ui, "🔄 Refresh all", ButtonRole::Secondary, true)
                        .clicked()
                    {
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
                ui.heading("Instances");
                ui.separator();
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
            if self
                .themed_button(ui, "➕ New VM", ButtonRole::Primary, true)
                .clicked()
            {
                self.log_console("VM creation wizard coming soon");
            }
            if self
                .themed_button(ui, "📦 New Container", ButtonRole::Primary, true)
                .clicked()
            {
                self.log_console("Container creation wizard coming soon");
            }

            ui.separator();

            if self
                .themed_button(ui, "▶ Start", ButtonRole::Start, can_start)
                .clicked()
            {
                self.handle_action(InstanceAction::Start);
            }
            if self
                .themed_button(ui, "⏹ Stop", ButtonRole::Stop, can_stop)
                .clicked()
            {
                self.handle_action(InstanceAction::Stop);
            }
            if self
                .themed_button(ui, "🔁 Restart", ButtonRole::Restart, can_restart)
                .clicked()
            {
                self.handle_action(InstanceAction::Restart);
            }

            ui.separator();

            if self
                .themed_button(ui, "🖥 Console", ButtonRole::Secondary, has_selection)
                .clicked()
            {
                self.show_console = true;
                self.log_console("Opening interactive console view");
            }
            if self
                .themed_button(ui, "🚀 Session", ButtonRole::Secondary, has_selection)
                .clicked()
            {
                if let Some(instance) = self.selected_instance_owned() {
                    self.request_session_launch(&instance);
                }
            }
            if self
                .themed_button(
                    ui,
                    "🪟 Viewer",
                    ButtonRole::Secondary,
                    ready_session.is_some(),
                )
                .clicked()
            {
                if let Some(session_id) = ready_session.clone() {
                    self.request_session_launch_client(session_id);
                }
            }
            if self
                .themed_button(ui, "🛡 Checkpoint", ButtonRole::Secondary, has_selection)
                .clicked()
            {
                self.log_console("Checkpoint workflow coming soon");
            }
            if self
                .themed_button(ui, "⚙ Preferences", ButtonRole::Secondary, true)
                .clicked()
            {
                self.open_preferences();
            }
            if self
                .preset_button(ui, ButtonIntent::Launch, Some("GPU Manager"), true)
                .clicked()
            {
                self.open_gpu_manager();
            }
        });
    }

    fn themed_button(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        role: ButtonRole,
        enabled: bool,
    ) -> egui::Response {
        theme::themed_button(ui, label, self.theme, role, enabled)
    }

    fn preset_button(
        &self,
        ui: &mut egui::Ui,
        intent: ButtonIntent,
        subject: Option<&str>,
        enabled: bool,
    ) -> egui::Response {
        theme::themed_button_preset(ui, self.theme, intent, subject, enabled)
    }

    fn open_gpu_manager(&mut self) {
        if let Some(window) = self.gpu_window.as_mut() {
            window.reopen();
            window.set_theme(self.theme);
            return;
        }

        let gpu_manager = self.vm_manager.gpu_manager_handle();
        let mut window = GpuManagerWindow::new(gpu_manager);
        window.set_theme(self.theme);
        self.gpu_window = Some(window);
    }

    fn draw_gpu_window(&mut self, ctx: &egui::Context) {
        if let Some(window) = self.gpu_window.as_mut() {
            window.show(ctx);
            if !window.is_open() {
                self.gpu_window = None;
            }
        }
    }

    fn draw_network_manager(&mut self, ctx: &egui::Context) {
        if !self.show_network_manager {
            return;
        }

        egui::Window::new("Network Manager")
            .id(egui::Id::new("nova.network_manager"))
            .default_size([900.0, 600.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                self.networking_gui.show_embedded(ui);
            });
    }

    fn draw_new_vm_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_new_vm_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Create New Virtual Machine")
            .id(egui::Id::new("nova.new_vm"))
            .default_size([650.0, 600.0])
            .resizable(true)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                // Template Selection Section
                ui.heading("Quick Start Templates");
                ui.add_space(4.0);

                // Get sorted template keys
                let mut template_keys: Vec<_> = self.available_templates.keys().cloned().collect();
                template_keys.sort();

                // Group templates by category
                let gpu_templates: Vec<_> = template_keys.iter()
                    .filter(|k| k.starts_with("nv-") || k.contains("gaming"))
                    .cloned()
                    .collect();
                let server_templates: Vec<_> = template_keys.iter()
                    .filter(|k| k.contains("server"))
                    .cloned()
                    .collect();
                let desktop_templates: Vec<_> = template_keys.iter()
                    .filter(|k| !k.starts_with("nv-") && !k.contains("gaming") && !k.contains("server"))
                    .cloned()
                    .collect();

                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    // GPU Passthrough Templates
                    if !gpu_templates.is_empty() {
                        ui.collapsing("🎮 GPU Passthrough", |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for key in &gpu_templates {
                                    if let Some(template) = self.available_templates.get(key) {
                                        let selected = self.new_vm_selected_template.as_ref() == Some(key);
                                        if ui.selectable_label(selected, &template.name)
                                            .on_hover_text(&template.description)
                                            .clicked()
                                        {
                                            self.apply_template(key.clone());
                                        }
                                    }
                                }
                            });
                        });
                    }

                    // Server Templates
                    if !server_templates.is_empty() {
                        ui.collapsing("🖥 Servers", |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for key in &server_templates {
                                    if let Some(template) = self.available_templates.get(key) {
                                        let selected = self.new_vm_selected_template.as_ref() == Some(key);
                                        if ui.selectable_label(selected, &template.name)
                                            .on_hover_text(&template.description)
                                            .clicked()
                                        {
                                            self.apply_template(key.clone());
                                        }
                                    }
                                }
                            });
                        });
                    }

                    // Desktop Templates
                    if !desktop_templates.is_empty() {
                        ui.collapsing("🖵 Desktop/General", |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for key in &desktop_templates {
                                    if let Some(template) = self.available_templates.get(key) {
                                        let selected = self.new_vm_selected_template.as_ref() == Some(key);
                                        if ui.selectable_label(selected, &template.name)
                                            .on_hover_text(&template.description)
                                            .clicked()
                                        {
                                            self.apply_template(key.clone());
                                        }
                                    }
                                }
                            });
                        });
                    }
                });

                if self.new_vm_selected_template.is_some() {
                    if ui.small_button("Clear template").clicked() {
                        self.new_vm_selected_template = None;
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // VM Configuration
                ui.heading("VM Configuration");
                ui.add_space(4.0);

                egui::Grid::new("new_vm_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_vm_name).hint_text("my-vm"));
                        ui.end_row();

                        ui.label("CPUs:");
                        ui.add(egui::DragValue::new(&mut self.new_vm_cpu).clamp_range(1..=64));
                        ui.end_row();

                        ui.label("Memory:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_vm_memory).hint_text("8G"));
                        ui.end_row();

                        ui.label("Disk Size:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_vm_disk_size).hint_text("64G"));
                        ui.end_row();

                        ui.label("ISO:");
                        ui.vertical(|ui| {
                            // Dropdown for scanned ISOs
                            let current_iso = if self.new_vm_iso_path.is_empty() {
                                "Select ISO...".to_string()
                            } else {
                                std::path::Path::new(&self.new_vm_iso_path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| self.new_vm_iso_path.clone())
                            };

                            egui::ComboBox::from_id_source("iso_select")
                                .selected_text(&current_iso)
                                .width(350.0)
                                .show_ui(ui, |ui| {
                                    if !self.available_isos.is_empty() {
                                        for iso in &self.available_isos {
                                            let label = format!("{} ({})", iso.name, iso.os_type);
                                            if ui.selectable_label(
                                                self.new_vm_iso_path == iso.path.to_string_lossy(),
                                                &label
                                            ).clicked() {
                                                self.new_vm_iso_path = iso.path.to_string_lossy().to_string();
                                            }
                                        }
                                        ui.separator();
                                    }
                                    if ui.selectable_label(false, "📁 Custom path...").clicked() {
                                        // Clear to allow manual entry
                                        self.new_vm_iso_path.clear();
                                    }
                                });

                            // Manual path input
                            ui.add(egui::TextEdit::singleline(&mut self.new_vm_iso_path)
                                .hint_text("/path/to/installer.iso")
                                .desired_width(350.0));
                        });
                        ui.end_row();

                        ui.label("Network:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_vm_network).hint_text("virbr0"));
                        ui.end_row();

                        ui.label("Firmware:");
                        ui.vertical(|ui| {
                            ui.checkbox(&mut self.new_vm_enable_uefi, "UEFI Boot");
                            ui.checkbox(&mut self.new_vm_enable_secure_boot, "Secure Boot");
                            ui.checkbox(&mut self.new_vm_enable_tpm, "TPM 2.0");
                        });
                        ui.end_row();

                        ui.label("Features:");
                        ui.vertical(|ui| {
                            ui.checkbox(&mut self.new_vm_enable_gpu, "GPU Passthrough");
                            ui.checkbox(&mut self.new_vm_autostart, "Start on boot");
                        });
                        ui.end_row();
                    });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if self.themed_button(ui, "Create VM", ButtonRole::Primary, !self.new_vm_name.is_empty()).clicked() {
                        self.create_new_vm();
                        self.show_new_vm_dialog = false;
                    }
                    if self.themed_button(ui, "Cancel", ButtonRole::Secondary, true).clicked() {
                        self.show_new_vm_dialog = false;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("🔄 Rescan ISOs").clicked() {
                            self.available_isos = nova::vm_templates::scan_iso_directories(
                                &self._config.iso.paths
                            );
                            self.log_console(format!("Found {} ISOs", self.available_isos.len()));
                        }
                    });
                });
            });

        if !open {
            self.show_new_vm_dialog = false;
        }
    }

    fn apply_template(&mut self, template_key: String) {
        if let Some(template) = self.available_templates.get(&template_key) {
            self.new_vm_cpu = template.cpu;
            self.new_vm_memory = template.memory.clone();
            self.new_vm_disk_size = template.disk_size.clone();
            self.new_vm_enable_gpu = template.gpu_passthrough;
            self.new_vm_enable_uefi = template.uefi;
            self.new_vm_enable_secure_boot = template.secure_boot;
            self.new_vm_enable_tpm = template.tpm;
            if let Some(net) = &template.network {
                self.new_vm_network = net.clone();
            }

            // Try to auto-select matching ISO
            if let Some(pattern) = &template.iso_pattern {
                if let Ok(regex) = regex::Regex::new(pattern) {
                    for iso in &self.available_isos {
                        if regex.is_match(&iso.name) {
                            self.new_vm_iso_path = iso.path.to_string_lossy().to_string();
                            break;
                        }
                    }
                }
            }

            self.new_vm_selected_template = Some(template_key);
            self.log_console(format!("Applied template: {}", template.name));
        }
    }

    fn create_new_vm(&mut self) {
        let name = self.new_vm_name.clone();
        if name.is_empty() {
            self.log_console("VM name cannot be empty");
            return;
        }

        // Log configuration details
        let mut features = Vec::new();
        if self.new_vm_enable_gpu { features.push("GPU passthrough"); }
        if self.new_vm_enable_uefi { features.push("UEFI"); }
        if self.new_vm_enable_secure_boot { features.push("Secure Boot"); }
        if self.new_vm_enable_tpm { features.push("TPM 2.0"); }

        self.log_console(format!(
            "Creating VM '{}': {} CPUs, {} RAM, {} disk{}",
            name, self.new_vm_cpu, self.new_vm_memory, self.new_vm_disk_size,
            if features.is_empty() { String::new() } else { format!(" [{}]", features.join(", ")) }
        ));

        if !self.new_vm_iso_path.is_empty() {
            self.log_console(format!("  ISO: {}", self.new_vm_iso_path));
        }

        // Build virt-install command
        let mut cmd = std::process::Command::new("virt-install");

        cmd.arg("--name").arg(&name)
            .arg("--vcpus").arg(self.new_vm_cpu.to_string())
            .arg("--memory").arg(self.parse_memory_for_virt_install(&self.new_vm_memory))
            .arg("--disk").arg(format!("size={}", self.parse_disk_size_gb(&self.new_vm_disk_size)))
            .arg("--os-variant").arg(self.detect_os_variant());

        // Network
        if !self.new_vm_network.is_empty() {
            cmd.arg("--network").arg(format!("bridge={}", self.new_vm_network));
        } else {
            cmd.arg("--network").arg("default");
        }

        // ISO/CDROM
        if !self.new_vm_iso_path.is_empty() {
            cmd.arg("--cdrom").arg(&self.new_vm_iso_path);
        } else {
            cmd.arg("--import");
        }

        // UEFI/BIOS
        if self.new_vm_enable_uefi {
            if self.new_vm_enable_secure_boot {
                cmd.arg("--boot").arg("uefi,loader=/usr/share/OVMF/OVMF_CODE.secboot.fd,loader.readonly=yes,loader.type=pflash,nvram.template=/usr/share/OVMF/OVMF_VARS.ms.fd,loader.secure=yes");
            } else {
                cmd.arg("--boot").arg("uefi");
            }
        }

        // TPM
        if self.new_vm_enable_tpm {
            cmd.arg("--tpm").arg("backend.type=emulator,backend.version=2.0,model=tpm-crb");
        }

        // Graphics
        cmd.arg("--graphics").arg("spice,listen=none");
        cmd.arg("--video").arg("qxl");

        // Don't start immediately (define only)
        cmd.arg("--noautoconsole");

        // Autostart if requested
        if self.new_vm_autostart {
            cmd.arg("--autostart");
        }

        self.log_console(format!("Running: virt-install --name {} ...", name));

        // Execute virt-install
        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    self.log_console(format!("VM '{}' created successfully!", name));
                    self.log_console("Note: VM is defined but not started. Use Start to boot.");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    self.log_console(format!("Failed to create VM: {}", stderr));
                }
            }
            Err(e) => {
                self.log_console(format!("Failed to run virt-install: {}", e));
                self.log_console("Make sure virt-install is installed (libvirt package)");
            }
        }

        // Reset form
        self.new_vm_name.clear();
        self.new_vm_cpu = 4;
        self.new_vm_memory = "8G".to_string();
        self.new_vm_disk_size = "64G".to_string();
        self.new_vm_iso_path.clear();
        self.new_vm_network = "virbr0".to_string();
        self.new_vm_enable_gpu = false;
        self.new_vm_enable_uefi = true;
        self.new_vm_enable_secure_boot = false;
        self.new_vm_enable_tpm = false;
        self.new_vm_autostart = false;
        self.new_vm_selected_template = None;

        self.refresh_instances(true);
    }

    fn parse_memory_for_virt_install(&self, memory: &str) -> String {
        // Convert memory string like "8G" or "16384M" to MB for virt-install
        let memory = memory.trim().to_uppercase();
        if memory.ends_with('G') {
            let gb: u32 = memory[..memory.len()-1].parse().unwrap_or(4);
            (gb * 1024).to_string()
        } else if memory.ends_with('M') {
            memory[..memory.len()-1].to_string()
        } else {
            // Assume GB if no unit
            let gb: u32 = memory.parse().unwrap_or(4);
            (gb * 1024).to_string()
        }
    }

    fn parse_disk_size_gb(&self, size: &str) -> String {
        // Convert disk size to GB number for virt-install
        let size = size.trim().to_uppercase();
        if size.ends_with('G') {
            size[..size.len()-1].to_string()
        } else if size.ends_with('T') {
            let tb: u32 = size[..size.len()-1].parse().unwrap_or(1);
            (tb * 1024).to_string()
        } else if size.ends_with('M') {
            let mb: u32 = size[..size.len()-1].parse().unwrap_or(65536);
            (mb / 1024).to_string()
        } else {
            // Assume GB
            size.to_string()
        }
    }

    fn detect_os_variant(&self) -> String {
        // Try to detect OS variant from selected template or ISO path
        if let Some(ref template_key) = self.new_vm_selected_template {
            if let Some(template) = self.available_templates.get(template_key) {
                return match template.os_type.as_str() {
                    "windows" => {
                        if template.name.contains("11") {
                            "win11".to_string()
                        } else {
                            "win10".to_string()
                        }
                    }
                    "linux" => {
                        let name_lower = template.name.to_lowercase();
                        if name_lower.contains("ubuntu") {
                            "ubuntu24.04".to_string()
                        } else if name_lower.contains("debian") {
                            "debian12".to_string()
                        } else if name_lower.contains("fedora") || name_lower.contains("nobara") || name_lower.contains("bazzite") {
                            "fedora40".to_string()
                        } else if name_lower.contains("arch") {
                            "archlinux".to_string()
                        } else {
                            "linux2022".to_string()
                        }
                    }
                    _ => "linux2022".to_string(),
                };
            }
        }

        // Fallback: try to detect from ISO path
        let iso_lower = self.new_vm_iso_path.to_lowercase();
        if iso_lower.contains("win") {
            if iso_lower.contains("11") { "win11".to_string() }
            else { "win10".to_string() }
        } else if iso_lower.contains("ubuntu") {
            "ubuntu24.04".to_string()
        } else if iso_lower.contains("debian") {
            "debian12".to_string()
        } else if iso_lower.contains("fedora") || iso_lower.contains("nobara") || iso_lower.contains("bazzite") {
            "fedora40".to_string()
        } else if iso_lower.contains("arch") {
            "archlinux".to_string()
        } else {
            "linux2022".to_string()
        }
    }

    fn draw_new_container_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_new_container_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Create New Container")
            .id(egui::Id::new("nova.new_container"))
            .default_size([500.0, 400.0])
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("new_container_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_container_name).hint_text("my-container"));
                        ui.end_row();

                        ui.label("Image:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_container_image).hint_text("nginx:latest"));
                        ui.end_row();

                        ui.label("Ports:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_container_ports).hint_text("8080:80, 443:443"));
                        ui.end_row();

                        ui.label("Volumes:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_container_volumes).hint_text("/host/path:/container/path"));
                        ui.end_row();

                        ui.label("Environment:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_container_env_vars).hint_text("KEY=value, FOO=bar"));
                        ui.end_row();

                        ui.label("Network:");
                        ui.add(egui::TextEdit::singleline(&mut self.new_container_network).hint_text("bridge"));
                        ui.end_row();
                    });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let can_create = !self.new_container_name.is_empty() && !self.new_container_image.is_empty();
                    if self.themed_button(ui, "Create Container", ButtonRole::Primary, can_create).clicked() {
                        self.create_new_container();
                        self.show_new_container_dialog = false;
                    }
                    if self.themed_button(ui, "Cancel", ButtonRole::Secondary, true).clicked() {
                        self.show_new_container_dialog = false;
                    }
                });
            });

        if !open {
            self.show_new_container_dialog = false;
        }
    }

    fn create_new_container(&mut self) {
        let name = self.new_container_name.clone();
        let image = self.new_container_image.clone();

        self.log_console(format!("Creating container '{}' from image '{}'", name, image));

        // TODO: Actually create container via ContainerManager
        self.log_console(format!("Container '{}' created", name));

        // Reset form
        self.new_container_name.clear();
        self.new_container_image.clear();
        self.new_container_ports.clear();
        self.new_container_volumes.clear();
        self.new_container_env_vars.clear();
        self.new_container_network = "bridge".to_string();

        self.refresh_instances(true);
    }

    fn draw_about_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_about_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("About Nova")
            .id(egui::Id::new("nova.about"))
            .default_size([400.0, 300.0])
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.heading("Nova Hypervisor Manager");
                    ui.add_space(8.0);
                    ui.label("Version 0.1.0");
                    ui.add_space(16.0);
                    ui.label("A modern, Wayland-native virtualization");
                    ui.label("and container management platform.");
                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label("Built with:");
                    ui.small("libvirt, QEMU/KVM, Podman, egui");
                    ui.add_space(16.0);
                    ui.label("Supports:");
                    ui.small("GPU Passthrough, SR-IOV, Looking Glass");
                    ui.small("SPICE, VNC, USB Passthrough, Live Migration");
                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.hyperlink_to("GitHub Repository", "https://github.com/nova-hypervisor/nova");
                    ui.add_space(16.0);
                });
            });

        if !open {
            self.show_about_dialog = false;
        }
    }

    fn draw_usb_manager(&mut self, ctx: &egui::Context) {
        if !self.show_usb_manager {
            return;
        }

        egui::Window::new("USB Passthrough Manager")
            .id(egui::Id::new("nova.usb_manager"))
            .default_size([700.0, 500.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("USB Device Passthrough");
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if self.themed_button(ui, "Refresh Devices", ButtonRole::Secondary, true).clicked() {
                        self.refresh_usb_devices();
                    }
                });

                ui.add_space(8.0);
                ui.label(format!("Available USB Devices ({})", self.usb_devices_cache.len()));

                let devices = self.usb_devices_cache.clone();
                let selected_vm = self.selected_instance.clone();

                egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                    if devices.is_empty() {
                        ui.label("No USB devices found. Click Refresh to scan.");
                    }
                    for device in &devices {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                let label = format!(
                                    "Bus {:03} Device {:03}: {} {}",
                                    device.bus, device.device,
                                    device.vendor_name, device.product_name
                                );
                                ui.label(&label);

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if let Some(vm) = &device.attached_to_vm {
                                        ui.colored_label(theme::STATUS_RUNNING, format!("→ {}", vm));
                                        if self.themed_button(ui, "Detach", ButtonRole::Stop, true).clicked() {
                                            self.detach_usb_device(device.bus, device.device);
                                        }
                                    } else {
                                        let can_attach = selected_vm.is_some();
                                        if self.themed_button(ui, "Attach", ButtonRole::Primary, can_attach).clicked() {
                                            if let Some(vm) = &selected_vm {
                                                self.attach_usb_device(vm, device.bus, device.device);
                                            }
                                        }
                                    }
                                });
                            });
                            ui.small(format!(
                                "ID {:04x}:{:04x} | {:?} | {:?}",
                                u16::from_str_radix(&device.vendor_id, 16).unwrap_or(0),
                                u16::from_str_radix(&device.product_id, 16).unwrap_or(0),
                                device.device_class,
                                device.speed
                            ));
                        });
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                if selected_vm.is_some() {
                    ui.small(format!("Target VM: {}", selected_vm.as_deref().unwrap_or("")));
                } else {
                    ui.colored_label(theme::STATUS_WARNING, "Select a VM first to attach USB devices");
                }
            });
    }

    fn refresh_usb_devices(&mut self) {
        self.log_console("Scanning USB devices...");
        let devices = if let Ok(mut manager) = self.usb_manager.lock() {
            manager.discover_devices().ok()
        } else {
            None
        };
        if let Some(devs) = devices {
            let count = devs.len();
            self.usb_devices_cache = devs;
            self.log_console(format!("Found {} USB devices", count));
        } else {
            self.log_console("USB scan failed");
        }
    }

    fn attach_usb_device(&mut self, vm_name: &str, bus: u8, device: u8) {
        self.log_console(format!("Attaching USB {}:{} to VM '{}'", bus, device, vm_name));

        // Find the device in cache
        let device_opt = self.usb_devices_cache.iter()
            .find(|d| d.bus == bus && d.device == device)
            .cloned();

        if let Some(usb_device) = device_opt {
            // Generate USB XML for virsh attach
            let usb_xml = format!(
                r#"<hostdev mode='subsystem' type='usb' managed='yes'>
                    <source>
                        <vendor id='0x{}'/>
                        <product id='0x{}'/>
                    </source>
                </hostdev>"#,
                usb_device.vendor_id, usb_device.product_id
            );

            // Write to temp file and attach
            let tmp_path = format!("/tmp/nova-usb-{}-{}.xml", bus, device);
            if fs::write(&tmp_path, &usb_xml).is_ok() {
                match Command::new("virsh")
                    .args(["attach-device", vm_name, &tmp_path, "--live"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        self.log_console(format!("USB device attached to {}", vm_name));
                    }
                    Ok(output) => {
                        let err = String::from_utf8_lossy(&output.stderr);
                        self.log_console(format!("Failed to attach: {}", err.trim()));
                    }
                    Err(e) => self.log_console(format!("Failed to run virsh: {}", e)),
                }
                let _ = fs::remove_file(&tmp_path);
            }
        }
    }

    fn detach_usb_device(&mut self, bus: u8, device: u8) {
        self.log_console(format!("Detaching USB {}:{}", bus, device));

        // Find the device and its VM
        let device_opt = self.usb_devices_cache.iter()
            .find(|d| d.bus == bus && d.device == device)
            .cloned();

        if let Some(usb_device) = device_opt {
            if let Some(vm_name) = &usb_device.attached_to_vm {
                // Generate USB XML for virsh detach
                let usb_xml = format!(
                    r#"<hostdev mode='subsystem' type='usb' managed='yes'>
                        <source>
                            <vendor id='0x{}'/>
                            <product id='0x{}'/>
                        </source>
                    </hostdev>"#,
                    usb_device.vendor_id, usb_device.product_id
                );

                let tmp_path = format!("/tmp/nova-usb-detach-{}-{}.xml", bus, device);
                if fs::write(&tmp_path, &usb_xml).is_ok() {
                    match Command::new("virsh")
                        .args(["detach-device", vm_name, &tmp_path, "--live"])
                        .output()
                    {
                        Ok(output) if output.status.success() => {
                            self.log_console("USB device detached");
                        }
                        Ok(output) => {
                            let err = String::from_utf8_lossy(&output.stderr);
                            self.log_console(format!("Failed to detach: {}", err.trim()));
                        }
                        Err(e) => self.log_console(format!("Failed to run virsh: {}", e)),
                    }
                    let _ = fs::remove_file(&tmp_path);
                }
            }
        }
    }

    fn draw_storage_manager(&mut self, ctx: &egui::Context) {
        if !self.show_storage_manager {
            return;
        }

        egui::Window::new("Storage Pool Manager")
            .id(egui::Id::new("nova.storage_manager"))
            .default_size([850.0, 550.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Storage Pools");
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if self.themed_button(ui, "Create Pool", ButtonRole::Primary, true).clicked() {
                        self.log_console("Opening pool creation dialog...");
                    }
                    if self.themed_button(ui, "Refresh", ButtonRole::Secondary, true).clicked() {
                        self.refresh_storage_pools();
                    }
                });

                ui.add_space(8.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.storage_pools_cache.is_empty() {
                        ui.label("No storage pools found. Click Refresh to scan.");
                    } else {
                        for (name, pool_type, path, state, capacity, used, available) in &self.storage_pools_cache.clone() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(name);
                                    ui.label(format!("| Type: {} | Path: {}", pool_type, path));
                                });

                                // Capacity bar
                                if *capacity > 0 {
                                    let usage_ratio = *used as f32 / *capacity as f32;
                                    let cap_gb = *capacity as f64 / 1_073_741_824.0;
                                    let used_gb = *used as f64 / 1_073_741_824.0;
                                    let avail_gb = *available as f64 / 1_073_741_824.0;

                                    ui.horizontal(|ui| {
                                        ui.label(format!(
                                            "Capacity: {:.1} GB | Used: {:.1} GB | Available: {:.1} GB ({:.1}%)",
                                            cap_gb, used_gb, avail_gb, usage_ratio * 100.0
                                        ));
                                    });

                                    // Progress bar for usage
                                    let bar_color = if usage_ratio > 0.9 {
                                        theme::STATUS_STOPPED  // Red for critical
                                    } else if usage_ratio > 0.75 {
                                        theme::STATUS_WARNING
                                    } else {
                                        theme::STATUS_RUNNING
                                    };
                                    ui.horizontal(|ui| {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(300.0, 8.0),
                                            egui::Sense::hover()
                                        );
                                        ui.painter().rect_filled(rect, 2.0, egui::Color32::DARK_GRAY);
                                        let filled_rect = egui::Rect::from_min_size(
                                            rect.min,
                                            egui::vec2(rect.width() * usage_ratio, rect.height())
                                        );
                                        ui.painter().rect_filled(filled_rect, 2.0, bar_color);
                                    });
                                }

                                ui.horizontal(|ui| {
                                    if state == "running" {
                                        ui.colored_label(theme::STATUS_RUNNING, "● Active");
                                    } else {
                                        ui.colored_label(theme::STATUS_STOPPED, "○ Inactive");
                                    }
                                    ui.separator();

                                    if state != "running" {
                                        if self.themed_button(ui, "Start", ButtonRole::Start, true).clicked() {
                                            self.start_storage_pool(name);
                                        }
                                    } else {
                                        if self.themed_button(ui, "Stop", ButtonRole::Stop, true).clicked() {
                                            self.stop_storage_pool(name);
                                        }
                                    }

                                    if self.themed_button(ui, "Browse", ButtonRole::Secondary, true).clicked() {
                                        if let Err(e) = std::process::Command::new("xdg-open")
                                            .arg(path)
                                            .spawn()
                                        {
                                            self.log_console(format!("Failed to open: {}", e));
                                        }
                                    }
                                });
                            });
                            ui.add_space(4.0);
                        }
                    }
                });
            });
    }

    fn refresh_storage_pools(&mut self) {
        self.log_console("Refreshing storage pools...");
        self.storage_pools_cache.clear();

        // Get pool list from virsh
        let output = std::process::Command::new("virsh")
            .args(["pool-list", "--all"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(2) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let name = parts[0].to_string();
                        let state = parts[1].to_string();

                        // Get pool details
                        if let Some(pool_info) = self.get_pool_details(&name) {
                            self.storage_pools_cache.push(pool_info);
                        } else {
                            self.storage_pools_cache.push((
                                name,
                                "unknown".to_string(),
                                "unknown".to_string(),
                                state,
                                0, 0, 0
                            ));
                        }
                    }
                }
                self.log_console(format!("Found {} storage pools", self.storage_pools_cache.len()));
            }
        }
    }

    fn get_pool_details(&self, name: &str) -> Option<(String, String, String, String, u64, u64, u64)> {
        // Get pool info
        let info_output = std::process::Command::new("virsh")
            .args(["pool-info", name])
            .output()
            .ok()?;

        if !info_output.status.success() {
            return None;
        }

        let info_str = String::from_utf8_lossy(&info_output.stdout);
        let mut state = "unknown".to_string();
        let mut capacity: u64 = 0;
        let mut allocation: u64 = 0;
        let mut available: u64 = 0;

        for line in info_str.lines() {
            if line.starts_with("State:") {
                state = line.split_whitespace().last().unwrap_or("unknown").to_string();
            } else if line.starts_with("Capacity:") {
                capacity = self.parse_virsh_size(line);
            } else if line.starts_with("Allocation:") {
                allocation = self.parse_virsh_size(line);
            } else if line.starts_with("Available:") {
                available = self.parse_virsh_size(line);
            }
        }

        // Get pool XML for type and path
        let xml_output = std::process::Command::new("virsh")
            .args(["pool-dumpxml", name])
            .output()
            .ok()?;

        let xml_str = String::from_utf8_lossy(&xml_output.stdout);
        let pool_type = if xml_str.contains("type='dir'") {
            "dir"
        } else if xml_str.contains("type='netfs'") {
            "nfs"
        } else if xml_str.contains("type='logical'") {
            "lvm"
        } else if xml_str.contains("type='iscsi'") {
            "iscsi"
        } else if xml_str.contains("type='rbd'") {
            "ceph"
        } else {
            "unknown"
        };

        // Extract path
        let path = if let Some(start) = xml_str.find("<path>") {
            if let Some(end) = xml_str[start..].find("</path>") {
                xml_str[start + 6..start + end].to_string()
            } else {
                "N/A".to_string()
            }
        } else {
            "N/A".to_string()
        };

        Some((
            name.to_string(),
            pool_type.to_string(),
            path,
            state,
            capacity,
            allocation,
            available
        ))
    }

    fn parse_virsh_size(&self, line: &str) -> u64 {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let value: f64 = parts[1].parse().unwrap_or(0.0);
            let unit = parts.get(2).unwrap_or(&"B");
            match *unit {
                "TiB" => (value * 1_099_511_627_776.0) as u64,
                "GiB" => (value * 1_073_741_824.0) as u64,
                "MiB" => (value * 1_048_576.0) as u64,
                "KiB" => (value * 1024.0) as u64,
                _ => value as u64,
            }
        } else {
            0
        }
    }

    fn start_storage_pool(&mut self, name: &str) {
        self.log_console(format!("Starting pool '{}'...", name));
        let output = std::process::Command::new("virsh")
            .args(["pool-start", name])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                self.log_console(format!("Pool '{}' started", name));
                self.refresh_storage_pools();
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                self.log_console(format!("Failed to start pool: {}", err));
            }
            Err(e) => {
                self.log_console(format!("Failed to start pool: {}", e));
            }
        }
    }

    fn stop_storage_pool(&mut self, name: &str) {
        self.log_console(format!("Stopping pool '{}'...", name));
        let output = std::process::Command::new("virsh")
            .args(["pool-destroy", name])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                self.log_console(format!("Pool '{}' stopped", name));
                self.refresh_storage_pools();
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                self.log_console(format!("Failed to stop pool: {}", err));
            }
            Err(e) => {
                self.log_console(format!("Failed to stop pool: {}", e));
            }
        }
    }

    fn draw_sriov_manager(&mut self, ctx: &egui::Context) {
        if !self.show_sriov_manager {
            return;
        }

        egui::Window::new("SR-IOV Manager")
            .id(egui::Id::new("nova.sriov_manager"))
            .default_size([850.0, 550.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("SR-IOV Virtual Functions");
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if self.themed_button(ui, "Scan Devices", ButtonRole::Primary, true).clicked() {
                        self.scan_sriov_devices();
                    }
                    if self.themed_button(ui, "Refresh", ButtonRole::Secondary, true).clicked() {
                        self.scan_sriov_devices();
                    }
                });

                ui.add_space(8.0);

                // Check IOMMU status
                let iommu_enabled = std::path::Path::new("/sys/kernel/iommu_groups").exists()
                    && std::fs::read_dir("/sys/kernel/iommu_groups")
                        .map(|d| d.count() > 0)
                        .unwrap_or(false);

                if !iommu_enabled {
                    ui.horizontal(|ui| {
                        ui.colored_label(theme::STATUS_WARNING, "⚠");
                        ui.label("IOMMU not detected. SR-IOV requires IOMMU enabled in BIOS and kernel.");
                    });
                    ui.add_space(4.0);
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.sriov_devices_cache.is_empty() {
                        ui.label("No SR-IOV capable devices found. Click 'Scan Devices' to search.");
                        ui.add_space(8.0);
                        ui.collapsing("Setup Instructions", |ui| {
                            ui.label("1. Enable IOMMU in BIOS (Intel VT-d / AMD-Vi)");
                            ui.label("2. Add kernel parameters: intel_iommu=on iommu=pt");
                            ui.label("3. Reboot the system");
                            ui.label("4. Load vfio-pci module: modprobe vfio-pci");
                        });
                    } else {
                        for device in &self.sriov_devices_cache.clone() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    let type_icon = match device.device_type.as_str() {
                                        "GPU" => "🎮",
                                        "NIC" => "🌐",
                                        _ => "🔌",
                                    };
                                    ui.strong(format!("{} {}", type_icon, device.device_name));
                                });

                                ui.horizontal(|ui| {
                                    ui.label(format!("PCI: {} | Vendor: {} | Driver: {}",
                                        device.pf_address, device.vendor, device.driver));
                                });

                                ui.horizontal(|ui| {
                                    ui.label(format!("Max VFs: {} | Active VFs: {}",
                                        device.max_vfs, device.active_vfs));

                                    if device.active_vfs > 0 {
                                        ui.colored_label(theme::STATUS_RUNNING, "● VFs Active");
                                    } else {
                                        ui.colored_label(theme::STATUS_STOPPED, "○ No VFs");
                                    }
                                });

                                ui.horizontal(|ui| {
                                    // Enable VFs button
                                    if device.active_vfs == 0 {
                                        if self.themed_button(ui, "Enable VFs", ButtonRole::Start, true).clicked() {
                                            self.enable_sriov_vfs(&device.pf_address, 4); // Default to 4 VFs
                                        }
                                    } else {
                                        if self.themed_button(ui, "Disable VFs", ButtonRole::Stop, true).clicked() {
                                            self.disable_sriov_vfs(&device.pf_address);
                                        }
                                    }

                                    if self.themed_button(ui, "View IOMMU Group", ButtonRole::Secondary, true).clicked() {
                                        self.show_iommu_group(&device.pf_address);
                                    }
                                });
                            });
                            ui.add_space(4.0);
                        }
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.small("SR-IOV allows sharing PCIe devices (GPUs, NICs) across multiple VMs");
            });
    }

    fn scan_sriov_devices(&mut self) {
        self.log_console("Scanning for SR-IOV capable devices...");
        self.sriov_devices_cache.clear();

        let pci_path = std::path::Path::new("/sys/bus/pci/devices");
        if !pci_path.exists() {
            self.log_console("PCI sysfs not available");
            return;
        }

        if let Ok(entries) = std::fs::read_dir(pci_path) {
            for entry in entries.flatten() {
                let device_path = entry.path();
                let address = entry.file_name().to_string_lossy().to_string();

                // Check for SR-IOV capability
                let sriov_totalvfs = device_path.join("sriov_totalvfs");
                if sriov_totalvfs.exists() {
                    if let Ok(max_vfs_str) = std::fs::read_to_string(&sriov_totalvfs) {
                        if let Ok(max_vfs) = max_vfs_str.trim().parse::<u32>() {
                            if max_vfs > 0 {
                                // Get device info
                                let vendor_id = std::fs::read_to_string(device_path.join("vendor"))
                                    .map(|s| s.trim().to_string())
                                    .unwrap_or_default();
                                let device_id = std::fs::read_to_string(device_path.join("device"))
                                    .map(|s| s.trim().to_string())
                                    .unwrap_or_default();
                                let active_vfs = std::fs::read_to_string(device_path.join("sriov_numvfs"))
                                    .ok()
                                    .and_then(|s| s.trim().parse().ok())
                                    .unwrap_or(0);
                                let driver = device_path.join("driver")
                                    .read_link()
                                    .ok()
                                    .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                                    .unwrap_or_else(|| "none".to_string());

                                let (vendor_name, device_name, device_type) =
                                    self.lookup_pci_device(&vendor_id, &device_id);

                                self.sriov_devices_cache.push(SriovDeviceInfo {
                                    pf_address: address,
                                    device_name,
                                    vendor: vendor_name,
                                    driver,
                                    max_vfs,
                                    active_vfs,
                                    device_type,
                                });
                            }
                        }
                    }
                }
            }
        }

        self.log_console(format!("Found {} SR-IOV capable devices", self.sriov_devices_cache.len()));
    }

    fn lookup_pci_device(&self, vendor_id: &str, device_id: &str) -> (String, String, String) {
        let vendor = vendor_id.trim_start_matches("0x");
        let device = device_id.trim_start_matches("0x");

        let (vendor_name, device_type) = match vendor {
            "10de" => ("NVIDIA", "GPU"),
            "1002" => ("AMD", "GPU"),
            "8086" => ("Intel", if device.starts_with("15") { "NIC" } else { "Other" }),
            "14e4" => ("Broadcom", "NIC"),
            "15b3" => ("Mellanox", "NIC"),
            "1924" => ("Solarflare", "NIC"),
            "177d" => ("Cavium", "NIC"),
            _ => ("Unknown", "Other"),
        };

        // Try to get actual device name from lspci
        let device_name = std::process::Command::new("lspci")
            .args(["-s", &format!("{}:", vendor)])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .next()
                        .map(|l| l.split(':').last().unwrap_or("Unknown").trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| format!("{} Device", vendor_name));

        (vendor_name.to_string(), device_name, device_type.to_string())
    }

    fn enable_sriov_vfs(&mut self, pf_address: &str, num_vfs: u32) {
        self.log_console(format!("Enabling {} VFs on {}...", num_vfs, pf_address));

        let sysfs_path = format!("/sys/bus/pci/devices/{}/sriov_numvfs", pf_address);

        // First disable existing VFs
        if let Err(e) = std::fs::write(&sysfs_path, "0") {
            self.log_console(format!("Warning: Failed to reset VFs: {}", e));
        }

        // Enable new VFs
        match std::fs::write(&sysfs_path, num_vfs.to_string()) {
            Ok(_) => {
                self.log_console(format!("Enabled {} VFs on {}", num_vfs, pf_address));
                std::thread::sleep(std::time::Duration::from_millis(500));
                self.scan_sriov_devices();
            }
            Err(e) => {
                self.log_console(format!("Failed to enable VFs: {}. Try running with sudo.", e));
            }
        }
    }

    fn disable_sriov_vfs(&mut self, pf_address: &str) {
        self.log_console(format!("Disabling VFs on {}...", pf_address));

        let sysfs_path = format!("/sys/bus/pci/devices/{}/sriov_numvfs", pf_address);

        match std::fs::write(&sysfs_path, "0") {
            Ok(_) => {
                self.log_console(format!("Disabled VFs on {}", pf_address));
                self.scan_sriov_devices();
            }
            Err(e) => {
                self.log_console(format!("Failed to disable VFs: {}", e));
            }
        }
    }

    fn show_iommu_group(&mut self, pf_address: &str) {
        let iommu_link = format!("/sys/bus/pci/devices/{}/iommu_group", pf_address);

        if let Ok(target) = std::fs::read_link(&iommu_link) {
            let group = target.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            self.log_console(format!("Device {} is in IOMMU group {}", pf_address, group));

            // List other devices in the same group
            let group_path = format!("/sys/kernel/iommu_groups/{}/devices", group);
            if let Ok(entries) = std::fs::read_dir(&group_path) {
                for entry in entries.flatten() {
                    let dev = entry.file_name().to_string_lossy().to_string();
                    self.log_console(format!("  Group {}: {}", group, dev));
                }
            }
        } else {
            self.log_console(format!("Device {} not in any IOMMU group", pf_address));
        }
    }

    fn draw_migration_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_migration_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Migrate Virtual Machine")
            .id(egui::Id::new("nova.migration"))
            .default_size([450.0, 300.0])
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("Live Migration");
                ui.separator();
                ui.add_space(8.0);

                if let Some(vm_name) = &self.selected_instance {
                    ui.label(format!("Migrate VM: {}", vm_name));
                } else {
                    ui.colored_label(theme::STATUS_WARNING, "No VM selected");
                }

                ui.add_space(8.0);
                ui.label("Destination Host:");
                // TODO: Add destination host input
                ui.text_edit_singleline(&mut String::new());

                ui.add_space(8.0);
                ui.checkbox(&mut false, "Offline migration (stop VM first)");
                ui.checkbox(&mut false, "Copy storage");

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let can_migrate = self.selected_instance.is_some();
                    if self.themed_button(ui, "Start Migration", ButtonRole::Primary, can_migrate).clicked() {
                        self.log_console("Starting VM migration...");
                        self.show_migration_dialog = false;
                    }
                    if self.themed_button(ui, "Cancel", ButtonRole::Secondary, true).clicked() {
                        self.show_migration_dialog = false;
                    }
                });
            });

        if !open {
            self.show_migration_dialog = false;
        }
    }

    fn draw_preflight_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_preflight_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("System Preflight Check")
            .id(egui::Id::new("nova.preflight"))
            .default_size([700.0, 550.0])
            .resizable(true)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("System Readiness");
                ui.separator();
                ui.add_space(8.0);

                if self.themed_button(ui, "Run Checks", ButtonRole::Primary, true).clicked() {
                    self.run_preflight_checks();
                }

                ui.add_space(8.0);

                if let Some(ref result) = self.preflight_result {
                    // System Info
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong("System:");
                            if let Some(ref distro) = result.distribution {
                                ui.label(distro);
                            }
                            if let Some(ref kernel) = result.kernel_release {
                                ui.label(format!("({})", kernel));
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // Overall status
                    if result.is_ready() {
                        ui.colored_label(theme::STATUS_RUNNING, "✓ System is ready for Nova workloads");
                    } else {
                        ui.colored_label(theme::STATUS_WARNING, format!("⚠ {} issues found", result.issues.len()));
                    }

                    ui.add_space(8.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Kernel Modules
                        ui.collapsing("Kernel Modules", |ui| {
                            for module in &result.module_status {
                                Self::preflight_item(
                                    ui,
                                    module.name,
                                    module.loaded,
                                    if module.loaded { "loaded" } else { "not loaded" }
                                );
                            }
                        });

                        // Additional checks not in preflight module
                        ui.collapsing("Hardware Features", |ui| {
                            // IOMMU check
                            let iommu_enabled = std::path::Path::new("/sys/kernel/iommu_groups").exists()
                                && std::fs::read_dir("/sys/kernel/iommu_groups")
                                    .map(|d| d.count() > 0)
                                    .unwrap_or(false);
                            Self::preflight_item(ui, "IOMMU", iommu_enabled,
                                if iommu_enabled { "IOMMU groups detected" } else { "No IOMMU groups (check BIOS)" });

                            // Hugepages
                            let hugepages = std::fs::read_to_string("/proc/sys/vm/nr_hugepages")
                                .ok()
                                .and_then(|s| s.trim().parse::<u64>().ok())
                                .unwrap_or(0);
                            Self::preflight_item(ui, "Hugepages", hugepages > 0,
                                &format!("{} pages configured", hugepages));

                            // Nested virtualization
                            let nested = std::fs::read_to_string("/sys/module/kvm_intel/parameters/nested")
                                .or_else(|_| std::fs::read_to_string("/sys/module/kvm_amd/parameters/nested"))
                                .map(|s| s.trim() == "Y" || s.trim() == "1")
                                .unwrap_or(false);
                            Self::preflight_item(ui, "Nested Virt", nested,
                                if nested { "enabled" } else { "disabled (optional)" });
                        });

                        // Tools
                        ui.collapsing("Userland Tools", |ui| {
                            for tool in &result.tool_status {
                                Self::preflight_item(
                                    ui,
                                    tool.name,
                                    tool.available,
                                    if tool.available { "available" } else { "not found in PATH" }
                                );
                            }

                            // Additional tools
                            let looking_glass = std::process::Command::new("which")
                                .arg("looking-glass-client")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            Self::preflight_item(ui, "Looking Glass", looking_glass,
                                if looking_glass { "client installed" } else { "not installed (optional)" });

                            let podman = std::process::Command::new("which")
                                .arg("podman")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);
                            Self::preflight_item(ui, "Podman", podman,
                                if podman { "available" } else { "not installed" });
                        });

                        // Services
                        ui.collapsing("Services", |ui| {
                            let libvirtd = std::process::Command::new("systemctl")
                                .args(["is-active", "libvirtd"])
                                .output()
                                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
                                .unwrap_or(false);
                            Self::preflight_item(ui, "libvirtd", libvirtd,
                                if libvirtd { "service running" } else { "not running" });

                            let virtlogd = std::process::Command::new("systemctl")
                                .args(["is-active", "virtlogd"])
                                .output()
                                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
                                .unwrap_or(false);
                            Self::preflight_item(ui, "virtlogd", virtlogd,
                                if virtlogd { "service running" } else { "not running" });
                        });

                        // Issues
                        if !result.issues.is_empty() {
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(4.0);
                            ui.heading("Issues");
                            for issue in &result.issues {
                                ui.horizontal(|ui| {
                                    ui.colored_label(theme::STATUS_WARNING, "⚠");
                                    ui.label(issue);
                                });
                            }
                        }
                    });
                } else {
                    ui.label("Click 'Run Checks' to scan your system.");
                }
            });

        if !open {
            self.show_preflight_dialog = false;
        }
    }

    fn run_preflight_checks(&mut self) {
        self.log_console("Running preflight checks...");
        match nova::preflight::run_preflight() {
            Ok(result) => {
                if result.is_ready() {
                    self.log_console("Preflight: System ready for Nova workloads");
                } else {
                    self.log_console(format!("Preflight: {} issues found", result.issues.len()));
                }
                self.preflight_result = Some(result);
            }
            Err(e) => {
                self.log_console(format!("Preflight check failed: {:?}", e));
            }
        }
    }

    fn preflight_item(ui: &mut egui::Ui, name: &str, ok: bool, detail: &str) {
        ui.horizontal(|ui| {
            if ok {
                ui.colored_label(theme::STATUS_RUNNING, "✓");
            } else {
                ui.colored_label(theme::STATUS_WARNING, "✗");
            }
            ui.strong(name);
            ui.separator();
            ui.label(detail);
        });
    }

    fn draw_metrics_panel(&mut self, ctx: &egui::Context) {
        if !self.show_metrics_panel {
            return;
        }

        egui::Window::new("Metrics Dashboard")
            .id(egui::Id::new("nova.metrics"))
            .default_size([800.0, 500.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("System Metrics");
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Prometheus endpoint:");
                    ui.monospace("http://localhost:9090/metrics");
                    if self.themed_button(ui, "Copy", ButtonRole::Secondary, true).clicked() {
                        ui.ctx().copy_text("http://localhost:9090/metrics".to_string());
                    }
                });

                ui.add_space(8.0);
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("VM Metrics");
                    ui.group(|ui| {
                        ui.label("Total VMs: 5");
                        ui.label("Running: 3 | Stopped: 2");
                        ui.label("Total vCPUs allocated: 24");
                        ui.label("Total RAM allocated: 64 GB");
                    });

                    ui.add_space(8.0);
                    ui.heading("Container Metrics");
                    ui.group(|ui| {
                        ui.label("Total Containers: 12");
                        ui.label("Running: 10 | Stopped: 2");
                    });

                    ui.add_space(8.0);
                    ui.heading("Host Resources");
                    ui.group(|ui| {
                        ui.label("CPU Usage: 45%");
                        ui.label("Memory: 32 GB / 128 GB");
                        ui.label("Storage: 500 GB / 2 TB");
                    });
                });
            });
    }

    fn draw_support_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_support_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Generate Support Bundle")
            .id(egui::Id::new("nova.support"))
            .default_size([500.0, 350.0])
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("Support Bundle");
                ui.separator();
                ui.add_space(8.0);

                ui.label("Generate a diagnostic bundle for troubleshooting.");
                ui.add_space(8.0);

                ui.label("Include:");
                ui.checkbox(&mut true, "System information");
                ui.checkbox(&mut true, "Nova configuration");
                ui.checkbox(&mut true, "libvirt logs");
                ui.checkbox(&mut true, "VM definitions");
                ui.checkbox(&mut false, "Full logs (large)");

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if self.themed_button(ui, "Generate Bundle", ButtonRole::Primary, true).clicked() {
                        self.log_console("Generating support bundle...");
                        self.log_console("Bundle saved to /tmp/nova-support-bundle.tar.gz");
                        self.show_support_dialog = false;
                    }
                    if self.themed_button(ui, "Cancel", ButtonRole::Secondary, true).clicked() {
                        self.show_support_dialog = false;
                    }
                });
            });

        if !open {
            self.show_support_dialog = false;
        }
    }

    fn draw_firewall_manager(&mut self, ctx: &egui::Context) {
        if !self.show_firewall_manager {
            return;
        }

        egui::Window::new("Firewall Manager")
            .id(egui::Id::new("nova.firewall"))
            .default_size([900.0, 600.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Firewall Rules");
                ui.separator();
                ui.add_space(8.0);

                // Show detected backend
                ui.horizontal(|ui| {
                    ui.label("Backend:");
                    if self.firewall_backend.is_empty() {
                        ui.colored_label(theme::STATUS_WARNING, "Not detected");
                    } else {
                        ui.strong(&self.firewall_backend);
                    }
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if self.themed_button(ui, "Refresh Rules", ButtonRole::Primary, true).clicked() {
                        self.refresh_firewall_rules();
                    }
                    if self.themed_button(ui, "Add Rule", ButtonRole::Secondary, true).clicked() {
                        self.log_console("Rule creation dialog coming soon...");
                    }
                });

                ui.add_space(8.0);

                // Tabs for different tables/chains
                ui.horizontal(|ui| {
                    ui.selectable_label(true, "INPUT");
                    ui.selectable_label(false, "OUTPUT");
                    ui.selectable_label(false, "FORWARD");
                    ui.selectable_label(false, "NAT");
                });

                ui.add_space(4.0);

                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    if self.firewall_rules_cache.is_empty() {
                        ui.label("No rules loaded. Click 'Refresh Rules' to scan.");
                    } else {
                        // Table header
                        egui::Grid::new("firewall_rules_header")
                            .num_columns(7)
                            .spacing([8.0, 4.0])
                            .striped(false)
                            .show(ui, |ui| {
                                ui.strong("Action");
                                ui.strong("Protocol");
                                ui.strong("Port");
                                ui.strong("Source");
                                ui.strong("Destination");
                                ui.strong("Packets");
                                ui.strong("Chain");
                                ui.end_row();
                            });

                        ui.separator();

                        // Rules
                        egui::Grid::new("firewall_rules_grid")
                            .num_columns(7)
                            .spacing([8.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for rule in &self.firewall_rules_cache.clone() {
                                    // Action with color
                                    let action_color = match rule.action.as_str() {
                                        "ACCEPT" => theme::STATUS_RUNNING,
                                        "DROP" => theme::STATUS_STOPPED,
                                        "REJECT" => theme::STATUS_WARNING,
                                        _ => theme::STATUS_UNKNOWN,
                                    };
                                    ui.colored_label(action_color, &rule.action);

                                    ui.label(&rule.protocol);
                                    ui.label(if rule.port.is_empty() { "any" } else { &rule.port });
                                    ui.label(if rule.source.is_empty() { "any" } else { &rule.source });
                                    ui.label(if rule.destination.is_empty() { "any" } else { &rule.destination });
                                    ui.label(format!("{}", rule.packets));
                                    ui.label(&rule.chain);
                                    ui.end_row();
                                }
                            });
                    }
                });

                ui.add_space(8.0);
                ui.separator();

                // Quick actions
                ui.horizontal(|ui| {
                    ui.label("Quick Actions:");
                    if self.themed_button(ui, "Allow SSH (22)", ButtonRole::Secondary, true).clicked() {
                        self.add_firewall_rule("ACCEPT", "tcp", "22", "", "");
                    }
                    if self.themed_button(ui, "Allow HTTP (80)", ButtonRole::Secondary, true).clicked() {
                        self.add_firewall_rule("ACCEPT", "tcp", "80", "", "");
                    }
                    if self.themed_button(ui, "Allow HTTPS (443)", ButtonRole::Secondary, true).clicked() {
                        self.add_firewall_rule("ACCEPT", "tcp", "443", "", "");
                    }
                    if self.themed_button(ui, "Allow libvirt (16509)", ButtonRole::Secondary, true).clicked() {
                        self.add_firewall_rule("ACCEPT", "tcp", "16509", "", "");
                    }
                });

                ui.add_space(4.0);
                ui.small(format!("Managing firewall via {}",
                    if self.firewall_backend.is_empty() { "auto-detect" } else { &self.firewall_backend }));
            });
    }

    fn refresh_firewall_rules(&mut self) {
        self.log_console("Scanning firewall rules...");
        self.firewall_rules_cache.clear();

        // Detect backend
        self.firewall_backend = self.detect_firewall_backend();
        self.log_console(format!("Detected firewall backend: {}", self.firewall_backend));

        match self.firewall_backend.as_str() {
            "nftables" => self.load_nftables_rules(),
            "iptables" => self.load_iptables_rules(),
            "firewalld" => self.load_firewalld_rules(),
            _ => {
                self.log_console("No supported firewall backend found");
            }
        }

        self.log_console(format!("Loaded {} firewall rules", self.firewall_rules_cache.len()));
    }

    fn detect_firewall_backend(&self) -> String {
        // Check for nft first (modern)
        if std::process::Command::new("nft")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return "nftables".to_string();
        }

        // Check for firewall-cmd (firewalld)
        if std::process::Command::new("firewall-cmd")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return "firewalld".to_string();
        }

        // Fall back to iptables
        if std::process::Command::new("iptables")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return "iptables".to_string();
        }

        "none".to_string()
    }

    fn load_nftables_rules(&mut self) {
        // Get nft rules in JSON format for easier parsing
        let output = std::process::Command::new("nft")
            .args(["-a", "list", "ruleset"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.parse_nft_output(&stdout);
            } else {
                // Try without sudo message
                self.log_console("Note: Run as root to see all nftables rules");
            }
        }
    }

    fn parse_nft_output(&mut self, output: &str) {
        let mut current_table = String::new();
        let mut current_chain = String::new();

        for line in output.lines() {
            let line = line.trim();

            if line.starts_with("table") {
                // table inet filter {
                current_table = line.split_whitespace().nth(2).unwrap_or("").to_string();
            } else if line.starts_with("chain") {
                // chain input {
                current_chain = line.split_whitespace().nth(1).unwrap_or("").to_string();
            } else if line.contains("accept") || line.contains("drop") || line.contains("reject") {
                // Parse rule line
                let action = if line.contains("accept") {
                    "ACCEPT"
                } else if line.contains("drop") {
                    "DROP"
                } else {
                    "REJECT"
                };

                let protocol = if line.contains("tcp") {
                    "tcp"
                } else if line.contains("udp") {
                    "udp"
                } else if line.contains("icmp") {
                    "icmp"
                } else {
                    "all"
                };

                // Extract port if present
                let port = if let Some(pos) = line.find("dport") {
                    line[pos..].split_whitespace().nth(1).unwrap_or("").to_string()
                } else {
                    String::new()
                };

                // Extract source if present
                let source = if let Some(pos) = line.find("saddr") {
                    line[pos..].split_whitespace().nth(1).unwrap_or("").to_string()
                } else {
                    String::new()
                };

                // Extract packets/bytes counter if present
                let (packets, bytes) = if let Some(pos) = line.find("counter") {
                    let counter_part = &line[pos..];
                    let p = counter_part.split("packets").nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let b = counter_part.split("bytes").nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    (p, b)
                } else {
                    (0, 0)
                };

                self.firewall_rules_cache.push(FirewallRuleInfo {
                    chain: current_chain.clone(),
                    table: current_table.clone(),
                    action: action.to_string(),
                    protocol: protocol.to_string(),
                    port,
                    source,
                    destination: String::new(),
                    comment: String::new(),
                    packets,
                    bytes,
                });
            }
        }
    }

    fn load_iptables_rules(&mut self) {
        // Load INPUT chain
        self.load_iptables_chain("filter", "INPUT");
        self.load_iptables_chain("filter", "OUTPUT");
        self.load_iptables_chain("filter", "FORWARD");
        self.load_iptables_chain("nat", "PREROUTING");
        self.load_iptables_chain("nat", "POSTROUTING");
    }

    fn load_iptables_chain(&mut self, table: &str, chain: &str) {
        let output = std::process::Command::new("iptables")
            .args(["-t", table, "-L", chain, "-n", "-v", "--line-numbers"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(2) {
                    // Skip header lines
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 8 {
                        let packets = parts[1].parse().unwrap_or(0);
                        let bytes = parts[2].parse().unwrap_or(0);
                        let action = parts[3].to_string();
                        let protocol = parts[4].to_string();
                        let source = parts[8].to_string();
                        let destination = parts[9].to_string();

                        // Extract port from remaining parts
                        let port = parts.iter()
                            .find(|p| p.starts_with("dpt:"))
                            .map(|p| p.trim_start_matches("dpt:").to_string())
                            .unwrap_or_default();

                        self.firewall_rules_cache.push(FirewallRuleInfo {
                            chain: chain.to_string(),
                            table: table.to_string(),
                            action,
                            protocol,
                            port,
                            source,
                            destination,
                            comment: String::new(),
                            packets,
                            bytes,
                        });
                    }
                }
            }
        }
    }

    fn load_firewalld_rules(&mut self) {
        // Get active zone
        let zone_output = std::process::Command::new("firewall-cmd")
            .arg("--get-active-zones")
            .output();

        if let Ok(output) = zone_output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let zone = stdout.lines().next().unwrap_or("public");

                // Get services in zone
                let services_output = std::process::Command::new("firewall-cmd")
                    .args(["--zone", zone, "--list-services"])
                    .output();

                if let Ok(output) = services_output {
                    if output.status.success() {
                        let services = String::from_utf8_lossy(&output.stdout);
                        for service in services.split_whitespace() {
                            self.firewall_rules_cache.push(FirewallRuleInfo {
                                chain: "INPUT".to_string(),
                                table: zone.to_string(),
                                action: "ACCEPT".to_string(),
                                protocol: "tcp".to_string(),
                                port: service.to_string(),
                                source: String::new(),
                                destination: String::new(),
                                comment: format!("firewalld service: {}", service),
                                packets: 0,
                                bytes: 0,
                            });
                        }
                    }
                }

                // Get ports in zone
                let ports_output = std::process::Command::new("firewall-cmd")
                    .args(["--zone", zone, "--list-ports"])
                    .output();

                if let Ok(output) = ports_output {
                    if output.status.success() {
                        let ports = String::from_utf8_lossy(&output.stdout);
                        for port_proto in ports.split_whitespace() {
                            let parts: Vec<&str> = port_proto.split('/').collect();
                            if parts.len() == 2 {
                                self.firewall_rules_cache.push(FirewallRuleInfo {
                                    chain: "INPUT".to_string(),
                                    table: zone.to_string(),
                                    action: "ACCEPT".to_string(),
                                    protocol: parts[1].to_string(),
                                    port: parts[0].to_string(),
                                    source: String::new(),
                                    destination: String::new(),
                                    comment: String::new(),
                                    packets: 0,
                                    bytes: 0,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn add_firewall_rule(&mut self, action: &str, protocol: &str, port: &str, source: &str, dest: &str) {
        self.log_console(format!("Adding rule: {} {} port {}", action, protocol, port));

        let result = match self.firewall_backend.as_str() {
            "nftables" => {
                let chain = "input";
                let rule = if source.is_empty() {
                    format!("{} dport {} accept", protocol, port)
                } else {
                    format!("ip saddr {} {} dport {} accept", source, protocol, port)
                };
                std::process::Command::new("nft")
                    .args(["add", "rule", "inet", "filter", chain, &rule])
                    .output()
            }
            "iptables" => {
                let mut args = vec!["-A", "INPUT", "-p", protocol, "--dport", port];
                if !source.is_empty() {
                    args.extend(["-s", source]);
                }
                args.extend(["-j", action]);
                std::process::Command::new("iptables")
                    .args(&args)
                    .output()
            }
            "firewalld" => {
                std::process::Command::new("firewall-cmd")
                    .args(["--add-port", &format!("{}/{}", port, protocol)])
                    .output()
            }
            _ => {
                self.log_console("No firewall backend available");
                return;
            }
        };

        match result {
            Ok(output) => {
                if output.status.success() {
                    self.log_console(format!("Rule added successfully"));
                    self.refresh_firewall_rules();
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    self.log_console(format!("Failed to add rule: {}", stderr));
                    self.log_console("Note: May require root/sudo privileges");
                }
            }
            Err(e) => {
                self.log_console(format!("Failed to execute command: {}", e));
            }
        }
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
            if self
                .themed_button(ui, "Clear", ButtonRole::Secondary, true)
                .clicked()
            {
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

        let (refresh_shortcut, open_gpu_shortcut, open_prefs_shortcut) = ctx.input(|input| {
            let ctrl = input.modifiers.ctrl;
            let shift = input.modifiers.shift;
            let refresh =
                input.key_pressed(egui::Key::F5) || (ctrl && input.key_pressed(egui::Key::R));
            let open_gpu = ctrl && shift && input.key_pressed(egui::Key::G);
            let open_prefs = ctrl && input.key_pressed(egui::Key::P);
            (refresh, open_gpu, open_prefs)
        });

        if refresh_shortcut {
            self.refresh_instances(true);
            self.refresh_network_summary(true);
            self.log_console("Manual refresh triggered via shortcut");
        }
        if open_gpu_shortcut {
            self.open_gpu_manager();
        }
        if open_prefs_shortcut {
            self.open_preferences();
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New VM...").clicked() {
                        self.show_new_vm_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("New Container...").clicked() {
                        self.show_new_container_dialog = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Import...").clicked() {
                        self.log_console("Import functionality coming soon");
                        ui.close_menu();
                    }
                    if ui.button("Export...").clicked() {
                        self.log_console("Export functionality coming soon");
                        ui.close_menu();
                    }
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
                    ui.menu_button("Managers", |ui| {
                        if ui.button("GPU Passthrough").clicked() {
                            self.open_gpu_manager();
                            ui.close_menu();
                        }
                        if ui.button("Network").clicked() {
                            self.show_network_manager = !self.show_network_manager;
                            ui.close_menu();
                        }
                        if ui.button("USB Passthrough").clicked() {
                            self.show_usb_manager = !self.show_usb_manager;
                            ui.close_menu();
                        }
                        if ui.button("Storage Pools").clicked() {
                            self.show_storage_manager = !self.show_storage_manager;
                            ui.close_menu();
                        }
                        if ui.button("SR-IOV").clicked() {
                            self.show_sriov_manager = !self.show_sriov_manager;
                            ui.close_menu();
                        }
                        if ui.button("Firewall").clicked() {
                            self.show_firewall_manager = !self.show_firewall_manager;
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    ui.menu_button("Theme", |ui| {
                        self.theme_menu(ui);
                    });
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("Preflight Check").clicked() {
                        self.show_preflight_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("Metrics Dashboard").clicked() {
                        self.show_metrics_panel = !self.show_metrics_panel;
                        ui.close_menu();
                    }
                    if ui.button("Support Bundle...").clicked() {
                        self.show_support_dialog = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Migrate VM...").clicked() {
                        self.show_migration_dialog = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About Nova").clicked() {
                        self.show_about_dialog = true;
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self
                        .themed_button(ui, "🔄 Refresh", ButtonRole::Secondary, true)
                        .clicked()
                    {
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
        self.draw_gpu_window(ctx);
        self.draw_network_manager(ctx);

        // Dialogs
        self.draw_new_vm_dialog(ctx);
        self.draw_new_container_dialog(ctx);
        self.draw_about_dialog(ctx);
        self.draw_migration_dialog(ctx);
        self.draw_preflight_dialog(ctx);
        self.draw_support_dialog(ctx);

        // Manager panels
        self.draw_usb_manager(ctx);
        self.draw_storage_manager(ctx);
        self.draw_sriov_manager(ctx);
        self.draw_metrics_panel(ctx);
        self.draw_firewall_manager(ctx);

        ctx.request_repaint_after(self.refresh_interval.min(self.network_refresh_interval));
    }
}
