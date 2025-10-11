use eframe::egui;
use nova::{
    config::NovaConfig,
    console_enhanced::{
        ActiveProtocol, EnhancedConsoleConfig, EnhancedConsoleManager, UnifiedConsoleSession,
    },
    container::ContainerManager,
    gpu_passthrough::GpuManager,
    gui_gpu::GpuManagerGui,
    instance::{Instance, InstanceStatus, InstanceType},
    logger,
    migration::{MigrationConfig, MigrationManager},
    network::{
        InterfaceState, NetworkInterface, NetworkManager, NetworkSummary, SwitchOrigin,
        SwitchProfile, SwitchStatus, SwitchType, VirtualSwitch,
    },
    pci_passthrough::PciPassthroughManager,
    spice_console::SpiceManager,
    sriov::SriovManager,
    templates_snapshots::TemplateManager,
    theme,
    usb_passthrough::UsbManager,
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

    // Detect desktop environment for optimizations
    let desktop_env = detect_desktop_environment();
    info!("Detected desktop environment: {:?}", desktop_env);

    // Configure Wayland-optimized settings
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([840.0, 620.0])
            .with_title("Nova Manager")
            .with_icon(eframe::icon_data::from_png_bytes(&[]).unwrap_or_default())
            // Wayland-specific optimizations
            .with_decorations(should_use_decorations(&desktop_env))
            .with_transparent(false) // Solid backgrounds perform better
            .with_resizable(true)
            .with_maximize_button(true)
            .with_minimize_button(true)
            .with_close_button(true),

        // Hardware acceleration settings (eframe will use wgpu on Wayland by default)
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,

        // High DPI support (Wayland handles this well)
        ..Default::default()
    };

    info!("Starting Nova Manager with Wayland optimizations");
    eframe::run_native(
        "Nova Manager",
        options,
        Box::new(|cc| Box::new(NovaApp::new(cc))),
    )
}

/// Desktop environment variants for optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopEnvironment {
    KdePlasma,
    Gnome,
    Cosmic,
    Other,
}

/// Detect the current desktop environment
fn detect_desktop_environment() -> DesktopEnvironment {
    // Check XDG_CURRENT_DESKTOP environment variable
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        let desktop_lower = desktop.to_lowercase();

        if desktop_lower.contains("kde") || desktop_lower.contains("plasma") {
            return DesktopEnvironment::KdePlasma;
        }
        if desktop_lower.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if desktop_lower.contains("cosmic") {
            return DesktopEnvironment::Cosmic;
        }
    }

    // Check XDG_SESSION_DESKTOP as fallback
    if let Ok(session) = std::env::var("XDG_SESSION_DESKTOP") {
        let session_lower = session.to_lowercase();

        if session_lower.contains("plasma") {
            return DesktopEnvironment::KdePlasma;
        }
        if session_lower.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if session_lower.contains("cosmic") {
            return DesktopEnvironment::Cosmic;
        }
    }

    DesktopEnvironment::Other
}

/// Determine whether to use window decorations based on desktop environment
fn should_use_decorations(env: &DesktopEnvironment) -> bool {
    match env {
        DesktopEnvironment::KdePlasma => {
            // KDE Plasma: Use server-side decorations (KWin handles them beautifully)
            true
        }
        DesktopEnvironment::Gnome => {
            // GNOME: Use client-side decorations (GTK style)
            true
        }
        DesktopEnvironment::Cosmic => {
            // Cosmic: Use decorations (Cosmic compositor handles them)
            true
        }
        DesktopEnvironment::Other => {
            // Default: enable decorations
            true
        }
    }
}

/// Apply Wayland-specific rendering optimizations
fn apply_wayland_optimizations(ctx: &egui::Context) {
    // Check if we're running on Wayland
    let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|t| t.to_lowercase() == "wayland")
            .unwrap_or(false);

    if !is_wayland {
        info!("Not running on Wayland, skipping Wayland-specific optimizations");
        return;
    }

    info!("Applying Wayland-specific rendering optimizations");

    // Configure optimal frame rate for Wayland
    // Wayland compositors handle vsync and frame pacing better than X11
    ctx.request_repaint_after(Duration::from_millis(16)); // Target 60 FPS

    // Enable tessellation options for smooth rendering
    let mut tessellation_options = egui::epaint::TessellationOptions::default();
    tessellation_options.feathering_size_in_pixels = 1.0; // Smooth edges on Wayland
    tessellation_options.coarse_tessellation_culling = true; // Better performance
    ctx.tessellation_options_mut(|opts| *opts = tessellation_options);

    // Configure pixel rounding for sharp rendering on Wayland
    // Wayland handles fractional scaling better than X11
    ctx.set_pixels_per_point(ctx.pixels_per_point()); // Use compositor's scale

    info!("Wayland optimizations applied successfully");
}

/// Check if running on Wayland
// Helper function for future use
#[allow(dead_code)]
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|t| t.to_lowercase() == "wayland")
            .unwrap_or(false)
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

#[derive(Debug)]
enum SessionEvent {
    Launched(UnifiedConsoleSession),
    Error { vm: String, message: String },
    Closed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainView {
    Dashboard,
    VirtualMachines,
    Networking,
    Tools,
}

impl Default for MainView {
    fn default() -> Self {
        MainView::VirtualMachines
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Overview,
    Snapshots,
    Networking,
    Sessions,
    Performance,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolWindow {
    None,
    GpuManager,
    UsbPassthrough,
    PciPassthrough,
    SriovManager,
    MigrationManager,
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
    gpu_manager: Arc<Mutex<GpuManager>>,
    usb_manager: Arc<Mutex<UsbManager>>,
    pci_manager: Arc<Mutex<PciPassthroughManager>>,
    sriov_manager: Arc<Mutex<SriovManager>>,
    spice_manager: Arc<Mutex<SpiceManager>>,
    migration_manager: Option<Arc<Mutex<MigrationManager>>>,
    _config: NovaConfig,
    runtime: Runtime,

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
    main_view: MainView,
    detail_tab: DetailTab,
    open_tool_window: ToolWindow,

    // Tool window GUI instances
    gpu_manager_gui: Option<GpuManagerGui>,

    // Theme settings
    theme_variant: theme::TokyoNightVariant,

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
        // Configure Tokyo Night theme
        theme::configure_tokyo_night_theme(&cc.egui_ctx, theme::TokyoNightVariant::Night);

        // Apply Wayland-specific rendering optimizations
        apply_wayland_optimizations(&cc.egui_ctx);

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

        // Initialize new managers
        let gpu_manager = Arc::new(Mutex::new(GpuManager::new()));
        let usb_manager = Arc::new(Mutex::new(UsbManager::new()));
        let pci_manager = Arc::new(Mutex::new(PciPassthroughManager::new()));
        let sriov_manager = Arc::new(Mutex::new(SriovManager::new()));
        let spice_manager = Arc::new(Mutex::new(SpiceManager::new()));

        // Initialize migration manager with default config
        let migration_config = MigrationConfig::default();
        let migration_manager = Some(Arc::new(Mutex::new(
            MigrationManager::new(migration_config, None)
        )));

        let mut app = Self {
            vm_manager,
            container_manager,
            network_manager,
            enhanced_console,
            template_manager,
            session_events,
            gpu_manager,
            usb_manager,
            pci_manager,
            sriov_manager,
            spice_manager,
            migration_manager,
            _config: config,
            runtime,
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
            main_view: MainView::default(),
            detail_tab: DetailTab::Overview,
            open_tool_window: ToolWindow::None,
            gpu_manager_gui: None,
            theme_variant: theme::TokyoNightVariant::Night,
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
                ui.heading("Network Profile");
                ui.small("Choose how VMs on this switch connect to networks:");
                ui.add_space(6.0);

                // External (Bridged) - VMs on LAN
                let external_frame = if matches!(self.new_switch_profile_mode, SwitchProfileMode::External) {
                    egui::Frame::default()
                        .fill(theme::BG_ELEVATED)
                        .stroke(egui::Stroke::new(2.0, theme::TN_NIGHT_BLUE))
                        .rounding(4.0)
                        .inner_margin(10.0)
                } else {
                    egui::Frame::default()
                        .fill(theme::BG_SECONDARY)
                        .stroke(egui::Stroke::new(1.0, theme::BG_HOVER))
                        .rounding(4.0)
                        .inner_margin(10.0)
                };

                let ext_response = external_frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.new_switch_profile_mode,
                            SwitchProfileMode::External,
                            "",
                        );
                        ui.vertical(|ui| {
                            ui.strong("External (Bridged to LAN)");
                            ui.small("VMs appear directly on your physical network");
                            ui.small("✓ VMs get IP from your router (DHCP)");
                            ui.small("✓ Accessible from other devices on LAN");
                        });
                    });
                });
                if ext_response.response.interact(egui::Sense::click()).clicked() {
                    self.new_switch_profile_mode = SwitchProfileMode::External;
                }

                ui.add_space(4.0);

                // NAT - VMs behind NAT
                let nat_frame = if matches!(self.new_switch_profile_mode, SwitchProfileMode::Nat) {
                    egui::Frame::default()
                        .fill(theme::BG_ELEVATED)
                        .stroke(egui::Stroke::new(2.0, theme::TN_NIGHT_BLUE))
                        .rounding(4.0)
                        .inner_margin(10.0)
                } else {
                    egui::Frame::default()
                        .fill(theme::BG_SECONDARY)
                        .stroke(egui::Stroke::new(1.0, theme::BG_HOVER))
                        .rounding(4.0)
                        .inner_margin(10.0)
                };

                let nat_response = nat_frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.new_switch_profile_mode,
                            SwitchProfileMode::Nat,
                            "",
                        );
                        ui.vertical(|ui| {
                            ui.strong("NAT (Shared with Host)");
                            ui.small("VMs share host's IP via NAT (like home router)");
                            ui.small("✓ VMs can access internet through host");
                            ui.small("✓ Includes built-in DHCP server");
                        });
                    });
                });
                if nat_response.response.interact(egui::Sense::click()).clicked() {
                    self.new_switch_profile_mode = SwitchProfileMode::Nat;
                }

                ui.add_space(4.0);

                // Internal (Host-only)
                let internal_frame = if matches!(self.new_switch_profile_mode, SwitchProfileMode::Internal) {
                    egui::Frame::default()
                        .fill(theme::BG_ELEVATED)
                        .stroke(egui::Stroke::new(2.0, theme::TN_NIGHT_BLUE))
                        .rounding(4.0)
                        .inner_margin(10.0)
                } else {
                    egui::Frame::default()
                        .fill(theme::BG_SECONDARY)
                        .stroke(egui::Stroke::new(1.0, theme::BG_HOVER))
                        .rounding(4.0)
                        .inner_margin(10.0)
                };

                let int_response = internal_frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.new_switch_profile_mode,
                            SwitchProfileMode::Internal,
                            "",
                        );
                        ui.vertical(|ui| {
                            ui.strong("Internal (Host-Only Network)");
                            ui.small("Isolated network for VMs and host only");
                            ui.small("✓ VMs can talk to each other and host");
                            ui.small("✗ No internet or LAN access");
                        });
                    });
                });
                if int_response.response.interact(egui::Sense::click()).clicked() {
                    self.new_switch_profile_mode = SwitchProfileMode::Internal;
                }

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

    fn request_session_launch_selected(&mut self) {
        if let Some(instance) = self.selected_instance_owned() {
            self.request_session_launch(&instance);
        }
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

    fn draw_tab_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("nova.tab_bar")
            .frame(egui::Frame::default().fill(theme::BG_CONSOLE).inner_margin(egui::Margin {
                left: 12.0,
                right: 12.0,
                top: 8.0,
                bottom: 8.0,
            }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                    // Dashboard tab
                    let dashboard_selected = self.main_view == MainView::Dashboard;
                    let dashboard_button = if dashboard_selected {
                        egui::Button::new("🏠 Dashboard")
                            .fill(theme::BG_ELEVATED)
                            .stroke(egui::Stroke::new(1.0, theme::TN_NIGHT_BLUE))
                    } else {
                        egui::Button::new("🏠 Dashboard")
                            .fill(theme::BG_PANEL)
                    };
                    if ui.add_sized([120.0, 32.0], dashboard_button).clicked() {
                        self.main_view = MainView::Dashboard;
                        self.log_console("Switched to Dashboard view");
                    }

                    // Virtual Machines tab
                    let vms_selected = self.main_view == MainView::VirtualMachines;
                    let vms_button = if vms_selected {
                        egui::Button::new("💻 Virtual Machines")
                            .fill(theme::BG_ELEVATED)
                            .stroke(egui::Stroke::new(1.0, theme::TN_NIGHT_BLUE))
                    } else {
                        egui::Button::new("💻 Virtual Machines")
                            .fill(theme::BG_PANEL)
                    };
                    if ui.add_sized([160.0, 32.0], vms_button).clicked() {
                        self.main_view = MainView::VirtualMachines;
                        self.log_console("Switched to Virtual Machines view");
                    }

                    // Networking tab
                    let networking_selected = self.main_view == MainView::Networking;
                    let networking_button = if networking_selected {
                        egui::Button::new("🌐 Networking")
                            .fill(theme::BG_ELEVATED)
                            .stroke(egui::Stroke::new(1.0, theme::TN_NIGHT_BLUE))
                    } else {
                        egui::Button::new("🌐 Networking")
                            .fill(theme::BG_PANEL)
                    };
                    if ui.add_sized([130.0, 32.0], networking_button).clicked() {
                        self.main_view = MainView::Networking;
                        self.log_console("Switched to Networking view");
                    }

                    // Tools tab
                    let tools_selected = self.main_view == MainView::Tools;
                    let tools_button = if tools_selected {
                        egui::Button::new("🔧 Tools")
                            .fill(theme::BG_ELEVATED)
                            .stroke(egui::Stroke::new(1.0, theme::TN_NIGHT_BLUE))
                    } else {
                        egui::Button::new("🔧 Tools")
                            .fill(theme::BG_PANEL)
                    };
                    if ui.add_sized([100.0, 32.0], tools_button).clicked() {
                        self.main_view = MainView::Tools;
                        self.log_console("Switched to Tools view");
                    }
                });
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

    fn draw_dashboard_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.heading("📊 System Dashboard");
        });
        ui.add_space(16.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            // System Overview Card
            egui::Frame::default()
                .fill(theme::BG_ELEVATED)
                .rounding(8.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.heading("System Overview");
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("💻 Virtual Machines:");
                        ui.label(format!("{} total", self.instances_cache.iter()
                            .filter(|i| i.instance_type == InstanceType::Vm).count()));
                        ui.label("•");
                        ui.colored_label(theme::STATUS_RUNNING, format!("{} running", self.summary.running));
                        ui.label("•");
                        ui.colored_label(theme::STATUS_STOPPED, format!("{} stopped", self.summary.stopped));
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("📦 Containers:");
                        ui.label(format!("{}", self.instances_cache.iter()
                            .filter(|i| i.instance_type == InstanceType::Container).count()));
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("🌐 Network Switches:");
                        let active_switches = self.network_summary.as_ref()
                            .map(|s| s.active_switches).unwrap_or(0);
                        ui.colored_label(theme::STATUS_WARNING, format!("{} active", active_switches));
                    });
                });

            ui.add_space(16.0);

            // Quick Actions Card
            egui::Frame::default()
                .fill(theme::BG_ELEVATED)
                .rounding(8.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.heading("Quick Actions");
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if ui.button("➕ Create New VM").clicked() {
                            self.log_console("VM creation wizard coming soon");
                        }
                        if ui.button("📦 Create Container").clicked() {
                            self.log_console("Container creation wizard coming soon");
                        }
                        if ui.button("🌐 Create Virtual Switch").clicked() {
                            self.show_create_switch_modal = true;
                        }
                    });
                });

            ui.add_space(16.0);

            // Recent Activity Card
            egui::Frame::default()
                .fill(theme::BG_ELEVATED)
                .rounding(8.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.heading("Recent Activity");
                    ui.add_space(8.0);

                    for (i, line) in self.console_output.iter().rev().take(8).enumerate() {
                        ui.monospace(line);
                        if i < 7 {
                            ui.add_space(2.0);
                        }
                    }
                });
        });
    }

    fn draw_networking_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.heading("🌐 Network Management");
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Network Summary Card
            egui::Frame::default()
                .fill(theme::BG_ELEVATED)
                .rounding(8.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.heading("Network Summary");
                    ui.add_space(8.0);

                    if let Some(summary) = &self.network_summary {
                        ui.horizontal(|ui| {
                            ui.label("Active Switches:");
                            ui.colored_label(theme::STATUS_WARNING, summary.active_switches.to_string());
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Total Interfaces:");
                            ui.label(summary.total_interfaces.to_string());
                        });
                    } else {
                        ui.label("Loading network information...");
                    }
                });

            ui.add_space(16.0);

            // Virtual Switches Card
            egui::Frame::default()
                .fill(theme::BG_ELEVATED)
                .rounding(8.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Virtual Switches");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("➕ Create Switch").clicked() {
                                self.show_create_switch_modal = true;
                            }
                        });
                    });
                    ui.add_space(8.0);

                    if self.network_switches.is_empty() {
                        ui.label("No virtual switches configured.");
                    } else {
                        for switch in &self.network_switches {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    let status_color = match &switch.status {
                                        SwitchStatus::Active => theme::STATUS_RUNNING,
                                        SwitchStatus::Inactive => theme::STATUS_STOPPED,
                                        SwitchStatus::Error(_) => theme::STATUS_STOPPED,
                                    };
                                    ui.colored_label(status_color, "●");
                                    ui.strong(&switch.name);
                                    ui.label(format!("({:?})", switch.switch_type));
                                });
                            });
                        }
                    }
                });

            ui.add_space(16.0);

            // Host Interfaces Card
            egui::Frame::default()
                .fill(theme::BG_ELEVATED)
                .rounding(8.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.heading("Host Network Interfaces");
                    ui.add_space(8.0);

                    if self.network_interfaces.is_empty() {
                        ui.label("No network interfaces detected.");
                    } else {
                        for interface in &self.network_interfaces {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    let status_color = match interface.state {
                                        InterfaceState::Up => theme::STATUS_RUNNING,
                                        InterfaceState::Down => theme::STATUS_STOPPED,
                                        InterfaceState::Unknown => theme::STATUS_SUSPENDED,
                                    };
                                    ui.colored_label(status_color, "●");
                                    ui.strong(&interface.name);
                                });
                            });
                        }
                    }
                });
        });
    }

    fn draw_tools_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.heading("🔧 Tools & Utilities");
        });
        ui.add_space(16.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // GPU Manager Tool
                egui::Frame::default()
                    .fill(theme::BG_ELEVATED)
                    .rounding(8.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_min_width(250.0);
                        ui.vertical(|ui| {
                            ui.heading("🎮 GPU Passthrough Manager");
                            ui.add_space(8.0);
                            ui.label("Manage GPU passthrough for virtual machines");
                            ui.add_space(8.0);
                            if ui.button("Open GPU Manager [Ctrl+G]").clicked() {
                                self.open_tool_window = ToolWindow::GpuManager;
                            }
                        });
                    });

                ui.add_space(8.0);

                // USB Passthrough Tool
                egui::Frame::default()
                    .fill(theme::BG_ELEVATED)
                    .rounding(8.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_min_width(250.0);
                        ui.vertical(|ui| {
                            ui.heading("🔌 USB Passthrough");
                            ui.add_space(8.0);
                            ui.label("Configure USB device passthrough");
                            ui.add_space(8.0);
                            if ui.button("Open USB Manager [Ctrl+U]").clicked() {
                                self.open_tool_window = ToolWindow::UsbPassthrough;
                            }
                        });
                    });
            });

            ui.add_space(16.0);

            ui.horizontal_wrapped(|ui| {
                // PCI Passthrough Tool
                egui::Frame::default()
                    .fill(theme::BG_ELEVATED)
                    .rounding(8.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_min_width(250.0);
                        ui.vertical(|ui| {
                            ui.heading("💾 PCI Passthrough");
                            ui.add_space(8.0);
                            ui.label("Configure PCI device passthrough");
                            ui.add_space(8.0);
                            if ui.button("Open PCI Manager [Ctrl+P]").clicked() {
                                self.open_tool_window = ToolWindow::PciPassthrough;
                            }
                        });
                    });

                ui.add_space(8.0);

                // SR-IOV Manager Tool
                egui::Frame::default()
                    .fill(theme::BG_ELEVATED)
                    .rounding(8.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_min_width(250.0);
                        ui.vertical(|ui| {
                            ui.heading("🔀 SR-IOV Manager");
                            ui.add_space(8.0);
                            ui.label("Manage SR-IOV virtual functions");
                            ui.add_space(8.0);
                            if ui.button("Open SR-IOV Manager [Ctrl+R]").clicked() {
                                self.open_tool_window = ToolWindow::SriovManager;
                            }
                        });
                    });
            });

            ui.add_space(16.0);

            ui.horizontal_wrapped(|ui| {
                // Migration Manager Tool
                egui::Frame::default()
                    .fill(theme::BG_ELEVATED)
                    .rounding(8.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_min_width(250.0);
                        ui.vertical(|ui| {
                            ui.heading("📦 Migration Manager");
                            ui.add_space(8.0);
                            ui.label("Migrate VMs between hosts");
                            ui.add_space(8.0);
                            if ui.button("Open Migration Manager [Ctrl+M]").clicked() {
                                self.open_tool_window = ToolWindow::MigrationManager;
                            }
                        });
                    });
            });
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

        // Prominent toolbar with modern styling
        egui::Frame::default()
            .fill(theme::BG_ELEVATED)
            .stroke(egui::Stroke::new(1.0, theme::BG_HOVER))
            .rounding(6.0)
            .inner_margin(egui::Margin::symmetric(16.0, 12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Creation actions - prominent primary buttons
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Create").strong());
                        if ui.add(egui::Button::new("➕ New VM")
                            .fill(theme::TN_NIGHT_BLUE)
                            .min_size(egui::vec2(100.0, 32.0)))
                            .clicked() {
                            self.log_console("VM creation wizard coming soon");
                        }
                        if ui.add(egui::Button::new("📦 Container")
                            .fill(theme::TN_NIGHT_PURPLE)
                            .min_size(egui::vec2(100.0, 32.0)))
                            .clicked() {
                            self.log_console("Container creation wizard coming soon");
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Power management - color-coded buttons
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Power").strong());
                        if ui
                            .add_enabled(can_start, egui::Button::new("▶ Start")
                                .fill(theme::TN_NIGHT_GREEN)
                                .min_size(egui::vec2(80.0, 32.0)))
                            .clicked()
                        {
                            self.handle_action(InstanceAction::Start);
                        }
                        if ui
                            .add_enabled(can_stop, egui::Button::new("⏹ Stop")
                                .fill(theme::TN_NIGHT_RED)
                                .min_size(egui::vec2(80.0, 32.0)))
                            .clicked()
                        {
                            self.handle_action(InstanceAction::Stop);
                        }
                        if ui
                            .add_enabled(can_restart, egui::Button::new("🔁 Restart")
                                .fill(theme::TN_NIGHT_ORANGE)
                                .min_size(egui::vec2(80.0, 32.0)))
                            .clicked()
                        {
                            self.handle_action(InstanceAction::Restart);
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Management actions
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Manage").strong());
                        if ui
                            .add_enabled(has_selection, egui::Button::new("🖥 Console")
                                .min_size(egui::vec2(90.0, 32.0)))
                            .clicked()
                        {
                            self.show_console = true;
                            self.log_console("Opening interactive console view");
                        }
                        if ui
                            .add_enabled(has_selection, egui::Button::new("🚀 Session")
                                .min_size(egui::vec2(90.0, 32.0)))
                            .clicked()
                        {
                            if let Some(instance) = self.selected_instance_owned() {
                                self.request_session_launch(&instance);
                            }
                        }
                        if ui
                            .add_enabled(ready_session.is_some(), egui::Button::new("🪟 Viewer")
                                .min_size(egui::vec2(80.0, 32.0)))
                            .clicked()
                        {
                            if let Some(session_id) = ready_session.clone() {
                                self.request_session_launch_client(session_id);
                            }
                        }
                        if ui
                            .add_enabled(has_selection, egui::Button::new("🛡 Checkpoint")
                                .min_size(egui::vec2(110.0, 32.0)))
                            .clicked()
                        {
                            self.log_console("Checkpoint workflow coming soon");
                        }
                        if ui
                            .add_enabled(has_selection, egui::Button::new("⚙ Settings")
                                .min_size(egui::vec2(90.0, 32.0)))
                            .clicked()
                        {
                            self.log_console("Settings panel under construction");
                        }
                    });
                });
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
                for instance in instances.iter() {
                    let is_selected = self
                        .selected_instance
                        .as_ref()
                        .map(|name| name == &instance.name)
                        .unwrap_or(false);

                    // Modern card design
                    let frame = if is_selected {
                        egui::Frame::default()
                            .fill(theme::BG_ELEVATED)
                            .stroke(egui::Stroke::new(2.0, theme::TN_NIGHT_CYAN))
                            .rounding(6.0)
                            .inner_margin(12.0)
                    } else {
                        egui::Frame::default()
                            .fill(theme::BG_SECONDARY)
                            .stroke(egui::Stroke::new(1.0, theme::BG_ELEVATED))
                            .rounding(6.0)
                            .inner_margin(12.0)
                    };

                    let response = frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Status dot (prominent)
                            let status_color = theme::get_status_color(&instance.status);
                            let status_icon = theme::get_status_icon(&instance.status);
                            ui.colored_label(status_color,
                                egui::RichText::new(status_icon).size(20.0));

                            ui.vertical(|ui| {
                                // Name and type
                                ui.horizontal(|ui| {
                                    ui.heading(&instance.name);
                                    ui.small(match instance.instance_type {
                                        InstanceType::Vm => "VM",
                                        InstanceType::Container => "Container",
                                    });
                                });

                                // Status and resource info
                                ui.horizontal(|ui| {
                                    ui.colored_label(status_color, format!("{:?}", instance.status));
                                    ui.label("•");
                                    ui.label(format!("{} cores", instance.cpu_cores));
                                    ui.label("•");
                                    ui.label(format!("{} MB RAM", instance.memory_mb));

                                    if let Some(network) = &instance.network {
                                        ui.label("•");
                                        ui.label(network);
                                    }

                                    if let Some(ip) = &instance.ip_address {
                                        ui.label("•");
                                        ui.small(ip);
                                    }
                                });
                            });

                            // Right-aligned actions
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("Details →").clicked() {
                                    self.selected_instance = Some(instance.name.clone());
                                    self.detail_tab = DetailTab::Overview;
                                }
                            });
                        });
                    });

                    // Make entire card clickable
                    if response.response.interact(egui::Sense::click()).clicked() {
                        self.selected_instance = Some(instance.name.clone());
                        self.detail_tab = DetailTab::Overview;
                    }

                    ui.add_space(6.0);
                }
            });
    }

    fn draw_instance_detail(&mut self, ui: &mut egui::Ui, instance: &Instance) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.detail_tab, DetailTab::Overview, "Overview");
                ui.selectable_value(&mut self.detail_tab, DetailTab::Snapshots, "Snapshots");
                ui.selectable_value(&mut self.detail_tab, DetailTab::Networking, "Networking");
                ui.selectable_value(&mut self.detail_tab, DetailTab::Sessions, "Sessions");
                ui.selectable_value(&mut self.detail_tab, DetailTab::Performance, "Performance");
            });

            ui.separator();

            match self.detail_tab {
                DetailTab::Overview => self.draw_overview(ui, instance),
                DetailTab::Snapshots => self.draw_snapshots(ui),
                DetailTab::Networking => self.draw_networking(ui, instance),
                DetailTab::Sessions => self.draw_sessions(ui, instance),
                DetailTab::Performance => self.draw_performance(ui, instance),
            }
        });
    }

    fn draw_performance(&self, ui: &mut egui::Ui, instance: &Instance) {
        ui.heading("Performance metrics");
        ui.separator();
        ui.label("Performance monitoring and graphs are on the roadmap.");
        ui.label("Planned capabilities:");
        ui.small("• CPU utilization graphs over time");
        ui.small("• Memory usage and pressure metrics");
        ui.small("• Disk I/O throughput and latency");
        ui.small("• Network bandwidth utilization");

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new(&instance.name).strong());
            ui.separator();
            ui.label(format!("vCPUs: {}", instance.cpu_cores));
            ui.label(format!("Memory: {} MB", instance.memory_mb));
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label("Performance graphs will appear here.");
                ui.small("Chart rendering with egui_plot is coming soon.");
            });
        });
    }

    fn draw_usb_passthrough_window(&mut self, ui: &mut egui::Ui) {
        ui.heading("USB Device Passthrough");
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh Devices").clicked() {
                let result = if let Ok(mut manager) = self.usb_manager.lock() {
                    manager.discover_devices().map(|_| ())
                } else {
                    Ok(())
                };

                if let Err(e) = result {
                    self.log_console(format!("Failed to refresh USB devices: {:?}", e));
                } else {
                    self.log_console("USB devices refreshed");
                }
            }
        });

        ui.add_space(8.0);

        if let Ok(manager) = self.usb_manager.lock() {
            let devices = manager.list_devices();

            if devices.is_empty() {
                ui.group(|ui| {
                    ui.label("No USB devices detected");
                    ui.small("Click 'Refresh Devices' to scan for USB devices");
                });
            } else {
                ui.label(format!("Found {} USB device(s)", devices.len()));
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        for device in devices.iter() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&device.product_name).strong());
                                    ui.label(format!("by {}", device.vendor_name));
                                });
                                ui.label(format!("Vendor:Product = {}:{}", device.vendor_id, device.product_id));
                                ui.label(format!("Bus {} Device {}", device.bus, device.device));
                                if let Some(serial) = &device.serial {
                                    ui.small(format!("Serial: {}", serial));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            }
        } else {
            ui.label("USB manager is currently busy");
        }
    }

    fn draw_pci_passthrough_window(&mut self, ui: &mut egui::Ui) {
        ui.heading("PCI Device Passthrough");
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh Devices").clicked() {
                let result = if let Ok(mut manager) = self.pci_manager.lock() {
                    manager.discover_devices().map(|_| ())
                } else {
                    Ok(())
                };

                if let Err(e) = result {
                    self.log_console(format!("Failed to discover PCI devices: {:?}", e));
                } else {
                    self.log_console("PCI devices discovered");
                }
            }
        });

        ui.add_space(8.0);

        if let Ok(manager) = self.pci_manager.lock() {
            let devices = manager.list_devices();

            if devices.is_empty() {
                ui.group(|ui| {
                    ui.label("No PCI devices detected");
                    ui.small("Click 'Refresh Devices' to scan for PCI devices");
                });
            } else {
                ui.label(format!("Found {} PCI device(s)", devices.len()));
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        for device in devices.iter() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&device.device_name).strong());
                                    ui.label(&device.vendor_name);
                                });
                                ui.monospace(format!("PCI Address: {}", device.address));
                                ui.label(format!("Vendor:Device = {}:{}", device.vendor_id, device.device_id));
                                if let Some(driver) = &device.driver {
                                    ui.label(format!("Driver: {}", driver));
                                }
                                if let Some(iommu) = device.iommu_group {
                                    ui.label(format!("IOMMU Group: {}", iommu));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            }
        } else {
            ui.label("PCI manager is currently busy");
        }
    }

    fn draw_sriov_manager_window(&mut self, ui: &mut egui::Ui) {
        ui.heading("SR-IOV Virtual Function Manager");
        ui.separator();

        ui.label("SR-IOV allows creating virtual functions (VFs) from physical network adapters.");
        ui.small("Virtual functions can be assigned directly to VMs for near-native performance.");

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh Devices").clicked() {
                let result = if let Ok(mut manager) = self.sriov_manager.lock() {
                    manager.discover_sriov_devices().map(|_| ())
                } else {
                    Ok(())
                };

                if let Err(e) = result {
                    self.log_console(format!("Failed to discover SR-IOV devices: {:?}", e));
                } else {
                    self.log_console("SR-IOV capable devices discovered");
                }
            }
        });

        ui.add_space(8.0);

        if let Ok(manager) = self.sriov_manager.lock() {
            let devices = manager.list_devices();

            if devices.is_empty() {
                ui.group(|ui| {
                    ui.label("No SR-IOV capable devices detected");
                    ui.small("SR-IOV requires compatible hardware and may need to be enabled in BIOS/UEFI");
                });
            } else {
                ui.label(format!("Found {} SR-IOV capable device(s)", devices.len()));
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        for device in devices.iter() {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new(&device.device_name).strong());
                                ui.monospace(format!("PCI: {}", device.pf_address));
                                ui.label(format!("Current VFs: {} / Max VFs: {}", device.current_vfs, device.max_vfs));

                                if device.current_vfs > 0 {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(102, 220, 144),
                                        format!("{} virtual functions active", device.current_vfs)
                                    );
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            }
        } else {
            ui.label("SR-IOV manager is currently busy");
        }
    }

    fn draw_migration_manager_window(&mut self, ui: &mut egui::Ui) {
        ui.heading("Virtual Machine Migration");
        ui.separator();

        ui.label("Live migration allows moving running VMs between hosts with minimal downtime.");
        ui.small("Supports offline, live, and hybrid migration modes.");

        ui.add_space(8.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("Migration Options").strong());
            ui.separator();
            ui.label("• Live Migration: Move running VMs with minimal downtime");
            ui.label("• Offline Migration: Migrate stopped VMs");
            ui.label("• Hybrid Migration: Combine live and offline approaches");
        });

        ui.add_space(8.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("Recent Migrations").strong());
            ui.separator();

            if let Some(ref manager_arc) = self.migration_manager {
                if let Ok(manager) = manager_arc.lock() {
                    let active = manager.list_active_migrations();

                    if active.is_empty() {
                        ui.label("No active migrations");
                    } else {
                        for migration in active.iter() {
                            ui.horizontal(|ui| {
                                ui.label(&migration.vm_name);
                                ui.label(format!("→ {}", migration.destination_host));
                                ui.label(format!("{:?}", migration.status));
                            });
                        }
                    }
                } else {
                    ui.label("Migration manager is currently busy");
                }
            } else {
                ui.label("Migration manager not initialized");
            }
        });

        ui.add_space(8.0);
        ui.small("Use the CLI for initiating VM migrations");
        ui.small("GUI-based migration workflow coming soon");
    }
}

impl eframe::App for NovaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::configure_tokyo_night_theme(ctx, self.theme_variant);

        self.refresh_instances(false);
        self.refresh_network_summary(false);
        self.drain_session_events();

        // Handle keyboard shortcuts
        ctx.input(|i| {
            if i.modifiers.ctrl {
                if i.key_pressed(egui::Key::G) {
                    self.open_tool_window = ToolWindow::GpuManager;
                } else if i.key_pressed(egui::Key::U) {
                    self.open_tool_window = ToolWindow::UsbPassthrough;
                } else if i.key_pressed(egui::Key::P) {
                    self.open_tool_window = ToolWindow::PciPassthrough;
                } else if i.key_pressed(egui::Key::R) {
                    self.open_tool_window = ToolWindow::SriovManager;
                } else if i.key_pressed(egui::Key::M) {
                    self.open_tool_window = ToolWindow::MigrationManager;
                }
            }
        });

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

                    ui.label("Theme:");
                    if ui.radio_value(&mut self.theme_variant, theme::TokyoNightVariant::Night, "🌙 Tokyo Night").clicked() {
                        self.log_console("Theme changed to Tokyo Night");
                    }
                    if ui.radio_value(&mut self.theme_variant, theme::TokyoNightVariant::Storm, "⛈️ Tokyo Storm").clicked() {
                        self.log_console("Theme changed to Tokyo Storm");
                    }
                    if ui.radio_value(&mut self.theme_variant, theme::TokyoNightVariant::Moon, "🌕 Tokyo Moon").clicked() {
                        self.log_console("Theme changed to Tokyo Moon");
                    }
                    ui.separator();

                    ui.label("Detail Tab:");
                    ui.radio_value(&mut self.detail_tab, DetailTab::Overview, "Overview");
                    ui.radio_value(&mut self.detail_tab, DetailTab::Snapshots, "Snapshots");
                    ui.radio_value(
                        &mut self.detail_tab,
                        DetailTab::Networking,
                        "Networking",
                    );
                    ui.radio_value(&mut self.detail_tab, DetailTab::Sessions, "Sessions");
                    ui.radio_value(&mut self.detail_tab, DetailTab::Performance, "Performance");
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("GPU Manager [Ctrl+G]").clicked() {
                        self.open_tool_window = ToolWindow::GpuManager;
                        ui.close_menu();
                    }
                    if ui.button("USB Passthrough [Ctrl+U]").clicked() {
                        self.open_tool_window = ToolWindow::UsbPassthrough;
                        ui.close_menu();
                    }
                    if ui.button("PCI Passthrough [Ctrl+P]").clicked() {
                        self.open_tool_window = ToolWindow::PciPassthrough;
                        ui.close_menu();
                    }
                    if ui.button("SR-IOV Manager [Ctrl+R]").clicked() {
                        self.open_tool_window = ToolWindow::SriovManager;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Migration Manager [Ctrl+M]").clicked() {
                        self.open_tool_window = ToolWindow::MigrationManager;
                        ui.close_menu();
                    }
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
        self.draw_tab_bar(ctx);

        // Only show navigation panel for VMs view
        if self.main_view == MainView::VirtualMachines {
            self.draw_navigation_panel(ctx, &filter);
        }

        if self.show_insights && self.main_view == MainView::VirtualMachines {
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
                match self.main_view {
                    MainView::Dashboard => {
                        self.draw_dashboard_view(ui);
                    }
                    MainView::VirtualMachines => {
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
                    }
                    MainView::Networking => {
                        self.draw_networking_view(ui);
                    }
                    MainView::Tools => {
                        self.draw_tools_view(ui);
                    }
                }
            });

        // Render tool windows
        match self.open_tool_window {
            ToolWindow::None => {}
            ToolWindow::GpuManager => {
                // Lazy-initialize GPU Manager GUI
                if self.gpu_manager_gui.is_none() {
                    let mut gui = GpuManagerGui::new(Arc::clone(&self.gpu_manager));
                    gui.refresh();
                    self.gpu_manager_gui = Some(gui);
                }

                let mut open = true;
                egui::Window::new("GPU Passthrough Manager")
                    .open(&mut open)
                    .default_size([900.0, 600.0])
                    .resizable(true)
                    .show(ctx, |ui| {
                        if let Some(gui) = &mut self.gpu_manager_gui {
                            gui.draw(ui);
                        }
                    });

                if !open {
                    self.open_tool_window = ToolWindow::None;
                    self.gpu_manager_gui = None;
                }
            }
            ToolWindow::UsbPassthrough => {
                let mut open = true;
                egui::Window::new("USB Passthrough Manager")
                    .open(&mut open)
                    .default_size([800.0, 500.0])
                    .resizable(true)
                    .show(ctx, |ui| {
                        self.draw_usb_passthrough_window(ui);
                    });

                if !open {
                    self.open_tool_window = ToolWindow::None;
                }
            }
            ToolWindow::PciPassthrough => {
                let mut open = true;
                egui::Window::new("PCI Passthrough Manager")
                    .open(&mut open)
                    .default_size([800.0, 500.0])
                    .resizable(true)
                    .show(ctx, |ui| {
                        self.draw_pci_passthrough_window(ui);
                    });

                if !open {
                    self.open_tool_window = ToolWindow::None;
                }
            }
            ToolWindow::SriovManager => {
                let mut open = true;
                egui::Window::new("SR-IOV Manager")
                    .open(&mut open)
                    .default_size([800.0, 500.0])
                    .resizable(true)
                    .show(ctx, |ui| {
                        self.draw_sriov_manager_window(ui);
                    });

                if !open {
                    self.open_tool_window = ToolWindow::None;
                }
            }
            ToolWindow::MigrationManager => {
                let mut open = true;
                egui::Window::new("VM Migration Manager")
                    .open(&mut open)
                    .default_size([800.0, 500.0])
                    .resizable(true)
                    .show(ctx, |ui| {
                        self.draw_migration_manager_window(ui);
                    });

                if !open {
                    self.open_tool_window = ToolWindow::None;
                }
            }
        }

        ctx.request_repaint_after(self.refresh_interval.min(self.network_refresh_interval));
    }
}
