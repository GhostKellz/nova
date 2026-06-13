use crate::arch_integration::ArchNetworkManager;
use crate::libvirt::{
    BridgeConfig, DhcpRange, ForwardMode, IpConfig, LibvirtManager, LibvirtNetwork,
};
use crate::monitoring::{
    self, BandwidthUsage, NetworkMonitor, NetworkTopology, PacketCaptureConfig,
};
use crate::network::{NetworkInterface, NetworkManager, SwitchType, VirtualSwitch};
use crate::theme::{self, ButtonIntent, ButtonRole, GuiTheme};
use crate::{log_info, log_warn};

use chrono::{DateTime, Local};
use eframe::egui;
use egui::{Color32, Id, Pos2, Rect, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::Read;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const NETWORK_STATE_KEY: &str = "nova.networking.state";
const CAPTURE_SCAN_FEEDBACK_WINDOW: Duration = Duration::from_millis(900);
const CAPTURE_PREVIEW_BYTES: usize = 64;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct CapturePersistedState {
    show_dialog: bool,
    interface: String,
    filter: String,
    duration: String,
    packet_count: String,
    output_file: String,
    #[serde(default = "default_capture_auto_scan")]
    auto_scan_enabled: bool,
    #[serde(default = "default_capture_scan_interval")]
    scan_interval_secs: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct NetworkingPersistedState {
    #[serde(default)]
    selected_tab: NetworkTab,
    #[serde(default)]
    capture: CapturePersistedState,
    #[serde(default = "default_monitoring_poll_secs")]
    monitoring_poll_secs: u64,
    #[serde(default = "default_monitoring_offline_threshold_secs")]
    monitoring_offline_threshold_secs: u64,
    #[serde(default = "default_monitoring_notifications")]
    monitoring_notifications_enabled: bool,
}

const fn default_capture_auto_scan() -> bool {
    true
}

const fn default_capture_scan_interval() -> u64 {
    5
}

const fn default_monitoring_poll_secs() -> u64 {
    5
}

const fn default_monitoring_offline_threshold_secs() -> u64 {
    20
}

const fn default_monitoring_notifications() -> bool {
    true
}

#[allow(dead_code)]
pub struct NetworkingGui {
    // Core managers
    network_manager: Arc<Mutex<NetworkManager>>,
    libvirt_manager: Arc<Mutex<LibvirtManager>>,
    network_monitor: Arc<Mutex<NetworkMonitor>>,
    arch_manager: Arc<Mutex<ArchNetworkManager>>,

    // Tokio runtime handle for driving the managers' async APIs from the UI.
    runtime: Option<tokio::runtime::Handle>,

    // GUI state
    selected_tab: NetworkTab,
    switch_creation_dialog: SwitchCreationDialog,
    network_creation_dialog: NetworkCreationDialog,
    monitoring_enabled: bool,
    monitoring_poll_secs: u64,
    monitoring_offline_threshold_secs: u64,
    monitoring_notifications_enabled: bool,
    last_monitoring_poll: Option<Instant>,
    capture_dialog: CaptureDialog,
    topology_view: TopologyView,
    theme: GuiTheme,
    libvirt_selection: HashSet<String>,
    last_refresh_all: Option<Instant>,
    refresh_feedback_until: Option<Instant>,
    arch_task_until: Option<Instant>,
    arch_task_message: Option<String>,
    last_action_message: Option<String>,
    last_action_error: Option<String>,
    pending_delete_networks: Option<Vec<String>>,
    persist_state_loaded: bool,
    toasts: Vec<NetworkToast>,
    offline_interfaces: HashSet<String>,

    // Data
    switches: Vec<VirtualSwitch>,
    libvirt_networks: Vec<LibvirtNetwork>,
    interfaces: Vec<NetworkInterface>,
    topology: Option<NetworkTopology>,
    bandwidth_data: HashMap<String, Vec<BandwidthUsage>>,
}

#[derive(Debug, Clone, Copy)]
enum ToastKind {
    Success,
    Error,
    Info,
}

#[derive(Debug, Clone)]
struct NetworkToast {
    message: String,
    kind: ToastKind,
    expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum NetworkTab {
    #[default]
    Overview,
    VirtualSwitches,
    LibvirtNetworks,
    Monitoring,
    Topology,
    PacketCapture,
    ArchConfig,
}

#[derive(Debug, Clone)]
struct SwitchCreationDialog {
    show: bool,
    name: String,
    switch_type: SwitchType,
    interfaces: Vec<String>,
    selected_interfaces: Vec<bool>,
    enable_stp: bool,
    vlan_id: String,
}

#[derive(Debug, Clone)]
struct NetworkCreationDialog {
    show: bool,
    name: String,
    network_type: String,
    subnet: String,
    gateway: String,
    dhcp_enabled: bool,
    dhcp_start: String,
    dhcp_end: String,
    autostart: bool,
}

#[derive(Debug, Clone)]
struct CaptureDialog {
    show: bool,
    interface: String,
    filter: String,
    duration: String,
    packet_count: String,
    output_file: String,
    active_captures: Vec<String>,
    recent_files: Vec<PathBuf>,
    last_file_scan: Option<Instant>,
    scan_feedback_until: Option<Instant>,
    auto_scan_enabled: bool,
    force_rescan: bool,
    scan_interval_secs: u64,
    preview: Option<CapturePreview>,
    pending_delete: Option<PathBuf>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TopologyView {
    zoom: f32,
    pan_offset: Vec2,
    selected_node: Option<String>,
    node_positions: HashMap<String, Pos2>,
}

#[derive(Debug, Clone)]
struct CapturePreview {
    path: PathBuf,
    size_bytes: u64,
    modified: SystemTime,
    header_hex: String,
    sampled_bytes: usize,
}

impl NetworkingGui {
    pub fn new() -> Self {
        Self::with_managers(
            Arc::new(Mutex::new(NetworkManager::new())),
            Arc::new(Mutex::new(LibvirtManager::new())),
            Arc::new(Mutex::new(NetworkMonitor::new())),
            Arc::new(Mutex::new(ArchNetworkManager::new())),
        )
    }

    pub fn with_managers(
        network_manager: Arc<Mutex<NetworkManager>>,
        libvirt_manager: Arc<Mutex<LibvirtManager>>,
        network_monitor: Arc<Mutex<NetworkMonitor>>,
        arch_manager: Arc<Mutex<ArchNetworkManager>>,
    ) -> Self {
        Self {
            network_manager,
            libvirt_manager,
            network_monitor,
            arch_manager,
            runtime: None,

            selected_tab: NetworkTab::Overview,
            switch_creation_dialog: SwitchCreationDialog {
                show: false,
                name: String::new(),
                switch_type: SwitchType::LinuxBridge,
                interfaces: Vec::new(),
                selected_interfaces: Vec::new(),
                enable_stp: false,
                vlan_id: String::new(),
            },
            network_creation_dialog: NetworkCreationDialog {
                show: false,
                name: String::new(),
                network_type: "NAT".to_string(),
                subnet: "192.168.100.0/24".to_string(),
                gateway: "192.168.100.1".to_string(),
                dhcp_enabled: true,
                dhcp_start: "192.168.100.2".to_string(),
                dhcp_end: "192.168.100.254".to_string(),
                autostart: true,
            },
            monitoring_enabled: false,
            monitoring_poll_secs: default_monitoring_poll_secs(),
            monitoring_offline_threshold_secs: default_monitoring_offline_threshold_secs(),
            monitoring_notifications_enabled: default_monitoring_notifications(),
            last_monitoring_poll: None,
            capture_dialog: CaptureDialog {
                show: false,
                interface: String::new(),
                filter: String::new(),
                duration: "60".to_string(),
                packet_count: "1000".to_string(),
                output_file: "/tmp/nova-capture.pcap".to_string(),
                active_captures: Vec::new(),
                recent_files: Vec::new(),
                last_file_scan: None,
                scan_feedback_until: None,
                auto_scan_enabled: true,
                force_rescan: false,
                scan_interval_secs: default_capture_scan_interval(),
                preview: None,
                pending_delete: None,
            },
            topology_view: TopologyView {
                zoom: 1.0,
                pan_offset: Vec2::ZERO,
                selected_node: None,
                node_positions: HashMap::new(),
            },
            theme: GuiTheme::default(),
            libvirt_selection: HashSet::new(),
            last_refresh_all: None,
            refresh_feedback_until: None,
            arch_task_until: None,
            arch_task_message: None,
            last_action_message: None,
            last_action_error: None,
            pending_delete_networks: None,
            persist_state_loaded: false,
            toasts: Vec::new(),
            offline_interfaces: HashSet::new(),

            switches: Vec::new(),
            libvirt_networks: Vec::new(),
            interfaces: Vec::new(),
            topology: None,
            bandwidth_data: HashMap::new(),
        }
    }

    /// Inject the Tokio runtime handle used to drive the managers' async APIs.
    pub fn set_runtime(&mut self, handle: tokio::runtime::Handle) {
        self.runtime = Some(handle);
    }

    /// Drive a manager future to completion on the shared runtime.
    ///
    /// Returns `None` when no runtime has been injected yet (e.g. a standalone
    /// `NetworkingGui::new()` used in tests), allowing callers to degrade
    /// gracefully instead of panicking.
    ///
    /// # Locking invariant
    ///
    /// Callers typically lock a manager's `std::sync::Mutex` and hold the guard
    /// across the `.await` inside the future they pass here. That is sound (and
    /// the `clippy::await_holding_lock` allowance on those callers is justified)
    /// because this drives the future synchronously on the calling UI thread:
    /// the executor never yields to another task, the guard is released before
    /// control returns, and no other thread contends for the lock.
    fn block_on<F: std::future::Future>(&self, fut: F) -> Option<F::Output> {
        self.runtime.as_ref().map(|handle| handle.block_on(fut))
    }

    /// Standalone-window entry point: renders the networking UI as the whole
    /// viewport. The embedded path ([`Self::show_embedded`]) is canonical under
    /// egui 0.34's `ui`-based model; this top-level variant intentionally wraps
    /// the content in a `CentralPanel` from a bare `Context`, for which there is
    /// no non-deprecated replacement (the new API requires a root `Ui`).
    #[allow(deprecated)]
    pub fn show(&mut self, ctx: &egui::Context) {
        self.begin_frame(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_contents(ui);
        });
        self.end_frame(ctx);
    }

    pub fn show_embedded(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.begin_frame(&ctx);
        self.render_contents(ui);
        self.end_frame(&ctx);
    }

    fn begin_frame(&mut self, ctx: &egui::Context) {
        self.ensure_persisted_state(ctx);
    }

    fn end_frame(&mut self, ctx: &egui::Context) {
        if self.refresh_feedback_active() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        // Show dialogs
        self.show_switch_creation_dialog(ctx);
        self.show_network_creation_dialog(ctx);
        self.show_capture_dialog(ctx);
        self.show_capture_delete_confirmation(ctx);
        self.show_delete_confirmation(ctx);
        self.draw_toasts(ctx);

        self.persist_state(ctx);
    }

    fn render_contents(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.selected_tab, NetworkTab::Overview, "Overview");
            ui.selectable_value(
                &mut self.selected_tab,
                NetworkTab::VirtualSwitches,
                "Virtual Switches",
            );
            ui.selectable_value(
                &mut self.selected_tab,
                NetworkTab::LibvirtNetworks,
                "Libvirt Networks",
            );
            ui.selectable_value(&mut self.selected_tab, NetworkTab::Monitoring, "Monitoring");
            ui.selectable_value(&mut self.selected_tab, NetworkTab::Topology, "Topology");
            ui.selectable_value(
                &mut self.selected_tab,
                NetworkTab::PacketCapture,
                "Packet Capture",
            );
            ui.selectable_value(
                &mut self.selected_tab,
                NetworkTab::ArchConfig,
                "Arch Config",
            );
        });

        ui.separator();

        if self.last_refresh_all.is_some() {
            self.show_refresh_feedback(ui);
            ui.separator();
        }

        match self.selected_tab {
            NetworkTab::Overview => self.show_overview(ui),
            NetworkTab::VirtualSwitches => self.show_virtual_switches(ui),
            NetworkTab::LibvirtNetworks => self.show_libvirt_networks(ui),
            NetworkTab::Monitoring => self.show_monitoring(ui),
            NetworkTab::Topology => self.show_topology(ui),
            NetworkTab::PacketCapture => self.show_packet_capture(ui),
            NetworkTab::ArchConfig => self.show_arch_config(ui),
        }
    }

    pub fn set_theme(&mut self, theme: GuiTheme) {
        self.theme = theme;
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

    fn record_success<S: Into<String>>(&mut self, message: S) {
        let msg = message.into();
        self.last_action_message = Some(msg.clone());
        self.last_action_error = None;
        self.push_toast(msg, ToastKind::Success);
    }

    fn record_info<S: Into<String>>(&mut self, message: S) {
        let msg = message.into();
        self.last_action_message = Some(msg.clone());
        self.last_action_error = None;
        self.push_toast(msg, ToastKind::Info);
    }

    fn record_error<S: Into<String>>(&mut self, message: S) {
        let msg = message.into();
        self.last_action_error = Some(msg.clone());
        self.last_action_message = None;
        self.push_toast(msg, ToastKind::Error);
    }

    fn push_toast(&mut self, message: String, kind: ToastKind) {
        let expires_at = Instant::now() + Duration::from_secs(4);
        self.toasts.push(NetworkToast {
            message,
            kind,
            expires_at,
        });
    }

    fn ensure_persisted_state(&mut self, ctx: &egui::Context) {
        if self.persist_state_loaded {
            return;
        }

        let persisted = ctx.data_mut(|data| {
            data.get_persisted::<NetworkingPersistedState>(Id::new(NETWORK_STATE_KEY))
        });

        if let Some(state) = persisted {
            self.selected_tab = state.selected_tab;
            self.capture_dialog.show = state.capture.show_dialog;
            self.capture_dialog.interface = state.capture.interface;
            self.capture_dialog.filter = state.capture.filter;
            self.capture_dialog.duration = state.capture.duration;
            self.capture_dialog.packet_count = state.capture.packet_count;
            if !state.capture.output_file.is_empty() {
                self.capture_dialog.output_file = state.capture.output_file;
            }
            self.capture_dialog.auto_scan_enabled = state.capture.auto_scan_enabled;
            if state.capture.scan_interval_secs > 0 {
                self.capture_dialog.scan_interval_secs = state.capture.scan_interval_secs;
            }
            if state.monitoring_poll_secs > 0 {
                self.monitoring_poll_secs = state.monitoring_poll_secs;
            }
            if state.monitoring_offline_threshold_secs > 0 {
                self.monitoring_offline_threshold_secs = state.monitoring_offline_threshold_secs;
            }
            self.monitoring_notifications_enabled = state.monitoring_notifications_enabled;
        }

        self.persist_state_loaded = true;
    }

    fn persist_state(&self, ctx: &egui::Context) {
        if !self.persist_state_loaded {
            return;
        }

        let capture = CapturePersistedState {
            show_dialog: self.capture_dialog.show,
            interface: self.capture_dialog.interface.clone(),
            filter: self.capture_dialog.filter.clone(),
            duration: self.capture_dialog.duration.clone(),
            packet_count: self.capture_dialog.packet_count.clone(),
            output_file: self.capture_dialog.output_file.clone(),
            auto_scan_enabled: self.capture_dialog.auto_scan_enabled,
            scan_interval_secs: self.capture_dialog.scan_interval_secs,
        };

        let state = NetworkingPersistedState {
            selected_tab: self.selected_tab.clone(),
            capture,
            monitoring_poll_secs: self.monitoring_poll_secs,
            monitoring_offline_threshold_secs: self.monitoring_offline_threshold_secs,
            monitoring_notifications_enabled: self.monitoring_notifications_enabled,
        };

        ctx.data_mut(|data| {
            data.insert_persisted(Id::new(NETWORK_STATE_KEY), state);
        });
    }

    fn maybe_poll_monitoring(&mut self, ctx: &egui::Context) {
        if !self.monitoring_enabled {
            return;
        }

        let interval = Duration::from_secs(self.monitoring_poll_secs.max(1));
        let needs_poll = match self.last_monitoring_poll {
            None => true,
            Some(last) => last.elapsed() >= interval,
        };

        if needs_poll {
            self.last_monitoring_poll = Some(Instant::now());
            self.synthesize_bandwidth_samples();
            self.evaluate_offline_interfaces();
        }

        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn synthesize_bandwidth_samples(&mut self) {
        if !self.monitoring_enabled {
            return;
        }

        let reference_epoch = Self::epoch_secs();
        let mut interfaces: Vec<String> = if !self.interfaces.is_empty() {
            self.interfaces
                .iter()
                .take(4)
                .map(|iface| iface.name.clone())
                .collect()
        } else if !self.bandwidth_data.is_empty() {
            self.bandwidth_data.keys().cloned().collect()
        } else {
            vec!["br0".to_string(), "virbr0".to_string()]
        };

        interfaces.sort();
        interfaces.dedup();

        for (idx, iface) in interfaces.iter().enumerate() {
            let phase = ((reference_epoch % 60) + idx as u64 * 7) as f64;
            let rx = (phase * 1_500_000.0) % 80_000_000.0;
            let tx = ((phase + 15.0) * 1_200_000.0) % 60_000_000.0;
            let rx_pps = rx / 1024.0;
            let tx_pps = tx / 1200.0;

            let entry = self.bandwidth_data.entry(iface.clone()).or_default();
            entry.push(BandwidthUsage {
                interface: iface.clone(),
                timestamp: reference_epoch,
                rx_bps: rx,
                tx_bps: tx,
                rx_pps,
                tx_pps,
            });

            if entry.len() > 180 {
                entry.drain(0..(entry.len() - 180));
            }
        }
    }

    fn evaluate_offline_interfaces(&mut self) {
        let now = Self::epoch_secs();
        let threshold = self.monitoring_offline_threshold_secs;
        let offline_now =
            monitoring::offline_interfaces_from_history(&self.bandwidth_data, threshold, now);

        if self.monitoring_notifications_enabled {
            let newly_offline: Vec<String> = offline_now
                .difference(&self.offline_interfaces)
                .cloned()
                .collect();
            let back_online: Vec<String> = self
                .offline_interfaces
                .difference(&offline_now)
                .cloned()
                .collect();

            for iface in newly_offline {
                self.record_error(format!("Interface {} marked offline", iface));
            }
            for iface in back_online {
                self.record_success(format!("Interface {} back online", iface));
            }
        }

        self.offline_interfaces = offline_now;
    }

    fn seconds_until_next_poll(&self) -> Option<u64> {
        if !self.monitoring_enabled {
            return None;
        }

        let interval = Duration::from_secs(self.monitoring_poll_secs.max(1));
        match self.last_monitoring_poll {
            None => Some(0),
            Some(last) => {
                if last.elapsed() >= interval {
                    Some(0)
                } else {
                    Some((interval - last.elapsed()).as_secs())
                }
            }
        }
    }

    fn epoch_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs()
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
            format!("{} B", bytes)
        }
    }

    fn format_timestamp(ts: SystemTime) -> String {
        let datetime: DateTime<Local> = ts.into();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    fn capture_display_name(path: &Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    }

    fn format_hex_block(bytes: &[u8]) -> String {
        if bytes.is_empty() {
            return "File is empty".to_string();
        }

        let mut output = String::new();
        for (chunk_idx, chunk) in bytes.chunks(16).enumerate() {
            let offset = chunk_idx * 16;
            let _ = write!(&mut output, "{offset:04X}: ");
            for byte in chunk {
                let _ = write!(&mut output, "{byte:02X} ");
            }
            if chunk.len() < 16 {
                for _ in 0..(16 - chunk.len()) {
                    output.push_str("   ");
                }
            }
            output.push_str(" |");
            for byte in chunk {
                let ch = if byte.is_ascii_graphic() || *byte == b' ' {
                    *byte as char
                } else {
                    '.'
                };
                output.push(ch);
            }
            output.push_str("|\n");
        }
        output
    }

    fn show_action_feedback(&mut self, ui: &mut egui::Ui) {
        if let Some(msg) = &self.last_action_message {
            ui.colored_label(Color32::from_rgb(96, 200, 140), format!("✔ {}", msg));
        }
        if let Some(err) = &self.last_action_error {
            ui.colored_label(Color32::from_rgb(220, 80, 80), format!("⚠ {}", err));
        }
    }

    fn touch_refresh_feedback(&mut self) {
        let now = Instant::now();
        self.last_refresh_all = Some(now);
        self.refresh_feedback_until = Some(now + Duration::from_millis(1200));
    }

    fn refresh_feedback_active(&self) -> bool {
        self.refresh_feedback_until
            .map(|until| Instant::now() <= until)
            .unwrap_or(false)
    }

    fn update_recent_capture_files(&mut self, ctx: &egui::Context, force: bool) -> bool {
        if !self.capture_dialog.auto_scan_enabled && !force {
            return false;
        }
        let interval = Duration::from_secs(self.capture_dialog.scan_interval_secs.max(1));
        let needs_scan = if force {
            true
        } else {
            match self.capture_dialog.last_file_scan {
                None => true,
                Some(ts) => ts.elapsed() >= interval,
            }
        };

        if !needs_scan {
            return false;
        }

        ctx.request_repaint_after(Duration::from_millis(250));
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir("/tmp") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| name.starts_with("nova-capture") && name.ends_with(".pcap"))
                    .is_some()
                {
                    files.push(path);
                }
            }
        }

        files.sort();
        files.reverse();
        self.capture_dialog.recent_files = files;
        if let Some(preview) = &self.capture_dialog.preview {
            let still_present = self
                .capture_dialog
                .recent_files
                .iter()
                .any(|entry| entry == &preview.path);
            if !still_present {
                self.capture_dialog.preview = None;
            }
        }
        self.capture_dialog.last_file_scan = Some(Instant::now());
        self.capture_dialog.scan_feedback_until =
            Some(Instant::now() + CAPTURE_SCAN_FEEDBACK_WINDOW);
        self.capture_dialog.force_rescan = false;
        true
    }

    fn remove_capture_file(&mut self, path: &Path) {
        let display_name = Self::capture_display_name(path);
        match fs::remove_file(path) {
            Ok(_) => {
                self.record_success(format!("Deleted capture '{}'.", display_name));
            }
            Err(err) => {
                let message = format!("Failed to delete capture '{}': {}", display_name, err);
                log_warn!("{}", message);
                self.record_error(message);
            }
        }

        self.capture_dialog
            .recent_files
            .retain(|existing| existing.as_path() != path);

        if self
            .capture_dialog
            .preview
            .as_ref()
            .map(|preview| preview.path.as_path() == path)
            .unwrap_or(false)
        {
            self.capture_dialog.preview = None;
        }

        self.capture_dialog.pending_delete = None;
        self.capture_dialog.force_rescan = true;
    }

    fn preview_capture_file(&mut self, path: &Path) {
        match Self::build_capture_preview(path) {
            Ok(preview) => {
                let label = Self::capture_display_name(path);
                self.capture_dialog.preview = Some(preview);
                self.record_info(format!("Previewing capture '{}'.", label));
            }
            Err(err) => {
                self.record_error(format!(
                    "Failed to build preview for '{}': {}",
                    Self::capture_display_name(path),
                    err
                ));
            }
        }
    }

    fn build_capture_preview(path: &Path) -> Result<CapturePreview, String> {
        let metadata = fs::metadata(path).map_err(|err| err.to_string())?;
        let mut header = vec![0u8; CAPTURE_PREVIEW_BYTES];
        let mut file = fs::File::open(path).map_err(|err| err.to_string())?;
        let bytes_read = file.read(&mut header).map_err(|err| err.to_string())?;
        let header_hex = Self::format_hex_block(&header[..bytes_read]);
        let modified = metadata.modified().unwrap_or_else(|_| SystemTime::now());

        Ok(CapturePreview {
            path: path.to_path_buf(),
            size_bytes: metadata.len(),
            modified,
            header_hex,
            sampled_bytes: bytes_read,
        })
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|toast| toast.expires_at > now);
        if self.toasts.is_empty() {
            return;
        }

        let screen = ctx.content_rect();
        for (index, toast) in self.toasts.iter().enumerate() {
            let pos = Pos2::new(
                screen.right() - 320.0,
                screen.top() + 24.0 + index as f32 * 70.0,
            );
            let (bg, stroke) = match toast.kind {
                ToastKind::Success => (
                    Color32::from_rgb(22, 73, 56),
                    Color32::from_rgb(96, 200, 140),
                ),
                ToastKind::Error => (
                    Color32::from_rgb(73, 28, 34),
                    Color32::from_rgb(220, 80, 80),
                ),
                ToastKind::Info => (
                    Color32::from_rgb(32, 52, 78),
                    Color32::from_rgb(120, 180, 255),
                ),
            };

            egui::Area::new(Id::new(format!("network.toast.{index}")))
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    ui.set_width(280.0);
                    egui::Frame::new()
                        .fill(bg)
                        .stroke(Stroke::new(1.0, stroke))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&toast.message)
                                    .color(Color32::from_rgb(235, 245, 255)),
                            );
                        });
                });
        }
    }

    fn execute_pending_network_deletions(&mut self) {
        if let Some(pending) = self.pending_delete_networks.clone() {
            let mut triggered = false;
            for name in pending {
                self.delete_libvirt_network(&name);
                self.libvirt_selection.remove(&name);
                triggered = true;
            }
            if triggered {
                self.touch_refresh_feedback();
                self.record_success("Requested deletion for selected networks");
            }
        }
    }

    fn show_delete_confirmation(&mut self, ctx: &egui::Context) {
        if let Some(pending) = self.pending_delete_networks.clone() {
            let mut open = true;
            egui::Window::new("Confirm network deletion")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("The following networks will be removed:");
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .show(ui, |ui| {
                            for name in &pending {
                                ui.label(format!("• {}", name));
                            }
                        });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if self
                            .preset_button(ui, ButtonIntent::ConfirmDelete, Some("Networks"), true)
                            .clicked()
                        {
                            self.execute_pending_network_deletions();
                            self.pending_delete_networks = None;
                            ui.close();
                        }
                        if self
                            .preset_button(ui, ButtonIntent::Cancel, None, true)
                            .clicked()
                        {
                            self.pending_delete_networks = None;
                        }
                    });
                });

            if !open {
                self.pending_delete_networks = None;
            }
        }
    }

    fn bulk_start_selected_networks(&mut self) {
        let targets: Vec<String> = self.libvirt_selection.iter().cloned().collect();
        let mut triggered = false;
        for name in targets {
            if let Some(is_active) = self
                .libvirt_networks
                .iter()
                .find(|n| n.name == name)
                .map(|n| n.active)
                && !is_active
            {
                self.toggle_libvirt_network(&name, is_active);
                triggered = true;
            }
        }
        if triggered {
            self.touch_refresh_feedback();
            self.record_success("Requested start for selected networks");
        } else {
            self.record_error("No inactive networks selected to start");
        }
    }

    fn bulk_stop_selected_networks(&mut self) {
        let targets: Vec<String> = self.libvirt_selection.iter().cloned().collect();
        let mut triggered = false;
        for name in targets {
            if let Some(is_active) = self
                .libvirt_networks
                .iter()
                .find(|n| n.name == name)
                .map(|n| n.active)
                && is_active
            {
                self.toggle_libvirt_network(&name, is_active);
                triggered = true;
            }
        }
        if triggered {
            self.touch_refresh_feedback();
            self.record_success("Requested stop for selected networks");
        } else {
            self.record_error("No active networks selected to stop");
        }
    }

    fn bulk_delete_selected_networks(&mut self) {
        let targets: Vec<String> = self.libvirt_selection.iter().cloned().collect();
        if targets.is_empty() {
            self.record_error("Select at least one network to delete");
            return;
        }

        self.pending_delete_networks = Some(targets);
        self.record_success("Review and confirm network deletion");
    }

    fn select_all_libvirt_networks(&mut self) {
        self.libvirt_selection.clear();
        self.libvirt_selection
            .extend(self.libvirt_networks.iter().map(|n| n.name.clone()));
        if self.libvirt_selection.is_empty() {
            self.record_error("No libvirt networks available to select");
        } else {
            self.record_success(format!(
                "Selected {} libvirt network(s)",
                self.libvirt_selection.len()
            ));
        }
    }

    fn metric_chip(&self, ui: &mut egui::Ui, label: &str, value: usize, role: ButtonRole) {
        let colors = theme::button_palette(self.theme, role);
        let fill = colors.fill.linear_multiply(0.25);
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, colors.stroke))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(label).small());
                    ui.heading(value.to_string());
                });
            });
    }

    fn legend_chip(&self, ui: &mut egui::Ui, label: &str, color: Color32) {
        egui::Frame::new()
            .fill(color.linear_multiply(0.18))
            .stroke(egui::Stroke::new(1.0, color))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(color, "⬤");
                    ui.label(label);
                });
            });
    }

    fn show_refresh_feedback(&self, ui: &mut egui::Ui) {
        if let Some(last) = self.last_refresh_all {
            ui.horizontal(|ui| {
                if self.refresh_feedback_active() {
                    ui.spinner();
                    ui.label("Refreshing data…");
                } else {
                    ui.label(format!("Last refreshed {}s ago", last.elapsed().as_secs()));
                }
            });
        }
    }

    fn show_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Network Overview");

        self.show_action_feedback(ui);
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            // Quick stats
            egui::Frame::new()
                .fill(Color32::from_gray(40))
                .corner_radius(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Virtual Switches");
                        ui.heading(self.switches.len().to_string());
                    });
                });

            egui::Frame::new()
                .fill(Color32::from_gray(40))
                .corner_radius(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Libvirt Networks");
                        ui.heading(self.libvirt_networks.len().to_string());
                    });
                });

            egui::Frame::new()
                .fill(Color32::from_gray(40))
                .corner_radius(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Network Interfaces");
                        ui.heading(self.interfaces.len().to_string());
                    });
                });
        });

        let active_networks = self
            .libvirt_networks
            .iter()
            .filter(|network| network.active)
            .count();
        let inactive_networks = self.libvirt_networks.len().saturating_sub(active_networks);
        let active_captures = self.capture_dialog.active_captures.len();

        ui.add_space(8.0);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                self.metric_chip(ui, "Active Libvirt", active_networks, ButtonRole::Start);
                self.metric_chip(ui, "Idle Libvirt", inactive_networks, ButtonRole::Secondary);
                self.metric_chip(ui, "Active Captures", active_captures, ButtonRole::Primary);
            });
        });

        ui.separator();

        // Quick actions
        ui.heading("Quick Actions");
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                if self
                    .preset_button(ui, ButtonIntent::Create, Some("Virtual Switch"), true)
                    .clicked()
                {
                    self.switch_creation_dialog.show = true;
                    self.refresh_interfaces();
                }
                if self
                    .preset_button(ui, ButtonIntent::Create, Some("Libvirt Network"), true)
                    .clicked()
                {
                    self.network_creation_dialog.show = true;
                }
                if self
                    .preset_button(ui, ButtonIntent::Refresh, Some("All"), true)
                    .clicked()
                {
                    self.refresh_all_data();
                }
            });
        });

        ui.add_space(6.0);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                if self
                    .themed_button(ui, "View Virtual Switches", ButtonRole::Secondary, true)
                    .clicked()
                {
                    self.selected_tab = NetworkTab::VirtualSwitches;
                }
                if self
                    .themed_button(ui, "View Libvirt Networks", ButtonRole::Secondary, true)
                    .clicked()
                {
                    self.selected_tab = NetworkTab::LibvirtNetworks;
                }
                if self
                    .themed_button(ui, "Open Monitoring", ButtonRole::Primary, true)
                    .clicked()
                {
                    self.selected_tab = NetworkTab::Monitoring;
                }
                if self
                    .themed_button(ui, "Packet Capture", ButtonRole::Secondary, true)
                    .clicked()
                {
                    self.selected_tab = NetworkTab::PacketCapture;
                }
            });
        });

        ui.separator();

        // Recent activity
        ui.heading("Network Status");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for interface in &self.interfaces {
                ui.horizontal(|ui| {
                    let color = match interface.state {
                        crate::network::InterfaceState::Up => Color32::GREEN,
                        crate::network::InterfaceState::Down => Color32::RED,
                        crate::network::InterfaceState::Unknown => Color32::YELLOW,
                    };

                    ui.colored_label(color, "●");
                    ui.label(&interface.name);
                    ui.label(&interface.mac_address);
                    if let Some(ip) = interface.ip_address {
                        ui.label(ip.to_string());
                    }
                });
            }
        });
    }

    fn show_virtual_switches(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Virtual Switches");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if self
                        .preset_button(ui, ButtonIntent::Create, Some("Switch"), true)
                        .clicked()
                    {
                        self.switch_creation_dialog.show = true;
                        self.refresh_interfaces();
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Refresh, Some("Switches"), true)
                        .clicked()
                    {
                        self.refresh_switches();
                    }
                });
            });
        });

        ui.separator();

        let switches_to_delete = std::cell::RefCell::new(Vec::new());

        egui::ScrollArea::vertical().show(ui, |ui| {
            for switch in &self.switches {
                let switch_name = switch.name.clone();
                egui::Frame::new()
                    .fill(Color32::from_gray(30))
                    .corner_radius(5.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.heading(&switch.name);
                                ui.label(format!("Type: {:?}", switch.switch_type));
                                ui.label(format!("Interfaces: {}", switch.interfaces.len()));
                                ui.label(format!("Status: {:?}", switch.status));
                                if let Some(vlan) = switch.vlan_id {
                                    ui.label(format!("VLAN: {}", vlan));
                                }
                                ui.label(format!(
                                    "STP: {}",
                                    if switch.stp_enabled {
                                        "Enabled"
                                    } else {
                                        "Disabled"
                                    }
                                ));
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if self
                                    .preset_button(ui, ButtonIntent::Delete, Some("Switch"), true)
                                    .clicked()
                                {
                                    switches_to_delete.borrow_mut().push(switch_name.clone());
                                }
                                if self
                                    .preset_button(
                                        ui,
                                        ButtonIntent::Configure,
                                        Some("Switch"),
                                        true,
                                    )
                                    .clicked()
                                {
                                    // Open configuration dialog
                                }
                            });
                        });

                        if !switch.interfaces.is_empty() {
                            ui.separator();
                            ui.label("Attached Interfaces:");
                            for interface in &switch.interfaces {
                                ui.label(format!("  • {}", interface));
                            }
                        }
                    });
                ui.add_space(5.0);
            }
        });

        // Process deletions after the immutable borrow is complete
        for switch_name in switches_to_delete.into_inner() {
            self.delete_switch(&switch_name);
        }
    }

    fn show_libvirt_networks(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Libvirt Networks");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if self
                        .preset_button(ui, ButtonIntent::Create, Some("Network"), true)
                        .clicked()
                    {
                        self.network_creation_dialog.show = true;
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Refresh, Some("Networks"), true)
                        .clicked()
                    {
                        self.refresh_libvirt_networks();
                    }
                });
            });
        });

        ui.separator();

        self.libvirt_selection
            .retain(|name| self.libvirt_networks.iter().any(|n| &n.name == name));

        if !self.libvirt_selection.is_empty() {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.horizontal(|ui| {
                    ui.label(format!("{} selected", self.libvirt_selection.len()));
                    let total_networks = self.libvirt_networks.len();
                    let all_selected =
                        total_networks > 0 && self.libvirt_selection.len() == total_networks;
                    if self
                        .preset_button(
                            ui,
                            ButtonIntent::Select,
                            Some("All"),
                            total_networks > 0 && !all_selected,
                        )
                        .clicked()
                    {
                        self.select_all_libvirt_networks();
                    }
                    let has_active = self
                        .libvirt_networks
                        .iter()
                        .any(|n| self.libvirt_selection.contains(&n.name) && n.active);
                    let has_inactive = self
                        .libvirt_networks
                        .iter()
                        .any(|n| self.libvirt_selection.contains(&n.name) && !n.active);

                    if self
                        .preset_button(ui, ButtonIntent::Start, Some("Networks"), has_inactive)
                        .clicked()
                    {
                        self.bulk_start_selected_networks();
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Stop, Some("Networks"), has_active)
                        .clicked()
                    {
                        self.bulk_stop_selected_networks();
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Delete, Some("Networks"), true)
                        .clicked()
                    {
                        self.bulk_delete_selected_networks();
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Cancel, Some("Selection"), true)
                        .clicked()
                    {
                        self.libvirt_selection.clear();
                        self.record_success("Cleared libvirt network selection");
                    }
                });
            });

            ui.separator();
        }

        let networks_to_toggle = std::cell::RefCell::new(Vec::new());
        let networks_to_delete = std::cell::RefCell::new(Vec::new());

        egui::ScrollArea::vertical().show(ui, |ui| {
            for network in &self.libvirt_networks {
                let network_name = network.name.clone();
                let is_active = network.active;
                let mut selected = self.libvirt_selection.contains(&network_name);
                egui::Frame::new()
                    .fill(Color32::from_gray(30))
                    .corner_radius(5.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut checkbox_selected = selected;
                            ui.vertical_centered(|ui| {
                                if ui.checkbox(&mut checkbox_selected, "").changed() {
                                    selected = checkbox_selected;
                                }
                            });
                            ui.vertical(|ui| {
                                ui.heading(&network.name);
                                if let Some(uuid) = &network.uuid {
                                    ui.label(format!("UUID: {}", uuid));
                                }
                                ui.label(format!("Active: {}", network.active));
                                ui.label(format!("Autostart: {}", network.autostart));

                                if let Some(forward) = &network.forward {
                                    ui.label(format!("Forward Mode: {}", forward.mode));
                                }

                                if let Some(ip) = &network.ip {
                                    ui.label(format!("Network: {}/{}", ip.address, ip.netmask));
                                    if let Some(dhcp) = &ip.dhcp {
                                        ui.label(format!(
                                            "DHCP Range: {} - {}",
                                            dhcp.start, dhcp.end
                                        ));
                                    }
                                }
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                let action_text = if is_active {
                                    ButtonIntent::Stop
                                } else {
                                    ButtonIntent::Start
                                };
                                if self
                                    .preset_button(ui, action_text, Some("Network"), true)
                                    .clicked()
                                {
                                    networks_to_toggle
                                        .borrow_mut()
                                        .push((network_name.clone(), is_active));
                                }
                                if self
                                    .preset_button(
                                        ui,
                                        ButtonIntent::Configure,
                                        Some("Network"),
                                        true,
                                    )
                                    .clicked()
                                {
                                    // Open edit dialog
                                }
                                if self
                                    .preset_button(ui, ButtonIntent::Delete, Some("Network"), true)
                                    .clicked()
                                {
                                    networks_to_delete.borrow_mut().push(network_name.clone());
                                }
                            });
                        });
                    });
                ui.add_space(5.0);

                if selected {
                    self.libvirt_selection.insert(network_name);
                } else {
                    self.libvirt_selection.remove(&network_name);
                }
            }
        });

        // Process actions after the immutable borrow is complete
        for (network_name, is_active) in networks_to_toggle.into_inner() {
            self.toggle_libvirt_network(&network_name, is_active);
        }
        for network_name in networks_to_delete.into_inner() {
            self.delete_libvirt_network(&network_name);
        }
    }

    fn show_monitoring(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Network Monitoring");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let intent = if self.monitoring_enabled {
                    ButtonIntent::Stop
                } else {
                    ButtonIntent::Start
                };
                if self
                    .preset_button(ui, intent, Some("Monitoring"), true)
                    .clicked()
                {
                    self.toggle_monitoring();
                }
            });
        });

        ui.separator();

        ui.horizontal_wrapped(|ui| {
            let poll_resp = ui.add(
                egui::Slider::new(&mut self.monitoring_poll_secs, 1..=60).text("Poll interval (s)"),
            );
            if poll_resp.changed() {
                self.last_monitoring_poll = None;
                if self.monitoring_offline_threshold_secs <= self.monitoring_poll_secs {
                    self.monitoring_offline_threshold_secs = self.monitoring_poll_secs + 5;
                }
                self.record_info(format!(
                    "Monitoring cadence set to {}s",
                    self.monitoring_poll_secs
                ));
            }

            let threshold_resp = ui.add(
                egui::Slider::new(&mut self.monitoring_offline_threshold_secs, 5..=600)
                    .text("Offline after (s)"),
            );
            if threshold_resp.changed() {
                if self.monitoring_offline_threshold_secs <= self.monitoring_poll_secs {
                    self.monitoring_offline_threshold_secs = self.monitoring_poll_secs + 5;
                }
                self.record_info(format!(
                    "Offline threshold set to {}s",
                    self.monitoring_offline_threshold_secs
                ));
                self.evaluate_offline_interfaces();
            }

            let notify_resp = ui.checkbox(
                &mut self.monitoring_notifications_enabled,
                "Notify on state change",
            );
            if notify_resp.changed() {
                if self.monitoring_notifications_enabled {
                    self.record_success("Monitoring notifications enabled");
                } else {
                    self.record_info("Monitoring notifications muted");
                }
            }

            if let Some(next) = self.seconds_until_next_poll() {
                ui.small(format!("Next refresh in {}s", next));
            }
        });

        if self.monitoring_enabled {
            self.maybe_poll_monitoring(ui.ctx());
        }

        if !self.monitoring_enabled {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label("Monitoring is paused.");
                ui.add_space(8.0);
                if self
                    .preset_button(ui, ButtonIntent::Start, Some("Monitoring"), true)
                    .clicked()
                {
                    self.toggle_monitoring();
                }
                ui.add_space(12.0);
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.horizontal(|ui| {
                        if self
                            .preset_button(ui, ButtonIntent::Create, Some("Capture"), true)
                            .clicked()
                        {
                            self.selected_tab = NetworkTab::PacketCapture;
                            self.capture_dialog.show = true;
                        }
                        if self
                            .preset_button(ui, ButtonIntent::Diagnostics, Some("Networking"), true)
                            .clicked()
                        {
                            self.selected_tab = NetworkTab::Topology;
                            self.refresh_topology();
                            self.touch_refresh_feedback();
                        }
                    });
                });
                ui.add_space(8.0);
                ui.label("Use quick links to launch captures or review diagnostics.");
            });
            return;
        }

        ui.add_space(6.0);
        if self.offline_interfaces.is_empty() {
            ui.colored_label(
                Color32::from_rgb(96, 200, 140),
                "All tracked interfaces responding",
            );
        } else {
            let mut offline_list: Vec<_> = self.offline_interfaces.iter().cloned().collect();
            offline_list.sort();
            ui.colored_label(
                Color32::from_rgb(220, 120, 80),
                format!("Offline: {}", offline_list.join(", ")),
            );
        }

        // Bandwidth charts
        let now_epoch = Self::epoch_secs();
        for (interface, bandwidth_history) in &self.bandwidth_data {
            if bandwidth_history.is_empty() {
                continue;
            }

            let is_offline = self.offline_interfaces.contains(interface);
            let stale_secs = bandwidth_history
                .last()
                .map(|sample| now_epoch.saturating_sub(sample.timestamp));

            egui::Frame::new()
                .fill(if is_offline {
                    Color32::from_rgb(50, 25, 25)
                } else {
                    Color32::from_gray(30)
                })
                .corner_radius(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(format!("Interface: {}", interface));
                        ui.add_space(8.0);
                        if is_offline {
                            ui.colored_label(
                                Color32::from_rgb(220, 120, 80),
                                format!(
                                    "Offline • {}s stale",
                                    stale_secs.unwrap_or(self.monitoring_offline_threshold_secs)
                                ),
                            );
                        } else if let Some(stale) = stale_secs {
                            ui.small(format!("Updated {}s ago", stale));
                        }
                    });

                    // Show current bandwidth
                    if let Some(latest) = bandwidth_history.last() {
                        ui.horizontal(|ui| {
                            ui.label(format!("RX: {:.2} MB/s", latest.rx_bps / 1024.0 / 1024.0));
                            ui.label(format!("TX: {:.2} MB/s", latest.tx_bps / 1024.0 / 1024.0));
                        });
                    }

                    // Simple bandwidth chart (would be more sophisticated in real implementation)
                    let available_rect = ui.available_rect_before_wrap();
                    let chart_rect = Rect::from_min_size(
                        available_rect.min,
                        Vec2::new(available_rect.width(), 100.0),
                    );

                    ui.painter()
                        .rect_filled(chart_rect, 2.0, Color32::from_gray(20));

                    // Draw bandwidth lines
                    if bandwidth_history.len() > 1 {
                        let max_bps = bandwidth_history
                            .iter()
                            .map(|b| b.rx_bps.max(b.tx_bps))
                            .fold(0.0, f64::max);

                        if max_bps > 0.0 {
                            let points_rx: Vec<Pos2> = bandwidth_history
                                .iter()
                                .enumerate()
                                .map(|(i, b)| {
                                    let x = chart_rect.min.x
                                        + (i as f32 / bandwidth_history.len() as f32)
                                            * chart_rect.width();
                                    let y = chart_rect.max.y
                                        - (b.rx_bps / max_bps) as f32 * chart_rect.height();
                                    Pos2::new(x, y)
                                })
                                .collect();

                            let points_tx: Vec<Pos2> = bandwidth_history
                                .iter()
                                .enumerate()
                                .map(|(i, b)| {
                                    let x = chart_rect.min.x
                                        + (i as f32 / bandwidth_history.len() as f32)
                                            * chart_rect.width();
                                    let y = chart_rect.max.y
                                        - (b.tx_bps / max_bps) as f32 * chart_rect.height();
                                    Pos2::new(x, y)
                                })
                                .collect();

                            // Draw RX line in green
                            for window in points_rx.windows(2) {
                                ui.painter().line_segment(
                                    [window[0], window[1]],
                                    Stroke::new(2.0, Color32::GREEN),
                                );
                            }

                            // Draw TX line in blue
                            for window in points_tx.windows(2) {
                                ui.painter().line_segment(
                                    [window[0], window[1]],
                                    Stroke::new(2.0, Color32::BLUE),
                                );
                            }
                        }
                    }

                    ui.allocate_space(Vec2::new(chart_rect.width(), chart_rect.height()));
                });
            ui.add_space(10.0);
        }
    }

    fn show_topology(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Network Topology");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self
                    .preset_button(ui, ButtonIntent::Refresh, Some("Topology"), true)
                    .clicked()
                {
                    self.refresh_topology();
                }
            });
        });

        ui.separator();

        if self.topology.is_some() {
            ui.horizontal(|ui| {
                ui.label("Legend:");
                self.legend_chip(ui, "Linux Bridge", Color32::BLUE);
                self.legend_chip(ui, "Open vSwitch", Color32::GREEN);
                self.legend_chip(ui, "Other", Color32::GRAY);
            });
            ui.add_space(6.0);
        }

        if let Some(topology) = &self.topology {
            let available_rect = ui.available_rect_before_wrap();

            ui.painter()
                .rect_filled(available_rect, 2.0, Color32::from_gray(20));

            // Draw bridges
            let mut bridge_positions = HashMap::new();
            for (i, bridge) in topology.bridges.iter().enumerate() {
                let x = available_rect.min.x + 100.0 + (i as f32) * 200.0;
                let y = available_rect.min.y + 100.0;
                let pos = Pos2::new(x, y);
                bridge_positions.insert(bridge.name.clone(), pos);

                // Draw bridge node
                let color = match bridge.bridge_type.as_str() {
                    "linux" => Color32::BLUE,
                    "ovs" => Color32::GREEN,
                    _ => Color32::GRAY,
                };

                ui.painter().circle_filled(pos, 30.0, color);
                ui.painter().text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    &bridge.name,
                    egui::FontId::default(),
                    Color32::WHITE,
                );

                // Draw interfaces connected to bridge
                for (j, interface) in bridge.interfaces.iter().enumerate() {
                    let iface_x = x;
                    let iface_y = y + 80.0 + (j as f32) * 30.0;
                    let iface_pos = Pos2::new(iface_x, iface_y);

                    ui.painter().circle_filled(iface_pos, 15.0, Color32::YELLOW);
                    ui.painter().text(
                        iface_pos + Vec2::new(20.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        interface,
                        egui::FontId::default(),
                        Color32::WHITE,
                    );

                    // Draw connection line
                    ui.painter()
                        .line_segment([pos, iface_pos], Stroke::new(2.0, Color32::WHITE));
                }
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("Click 'Refresh' to discover network topology");
            });
        }
    }

    fn show_packet_capture(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Packet Capture");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self
                    .preset_button(ui, ButtonIntent::Create, Some("Capture"), true)
                    .clicked()
                {
                    self.capture_dialog.show = true;
                }
            });
        });

        ui.separator();

        ui.horizontal_wrapped(|ui| {
            let auto_scan_response = ui.checkbox(
                &mut self.capture_dialog.auto_scan_enabled,
                "Auto-scan capture folder",
            );
            if auto_scan_response.changed() {
                if self.capture_dialog.auto_scan_enabled {
                    self.capture_dialog.force_rescan = true;
                    self.record_success(format!(
                        "Auto-scan enabled (every {}s)",
                        self.capture_dialog.scan_interval_secs
                    ));
                } else {
                    self.record_info("Auto-scan paused");
                }
            }

            if self
                .preset_button(ui, ButtonIntent::Refresh, Some("Capture Folder"), true)
                .clicked()
            {
                self.capture_dialog.force_rescan = true;
                self.record_info("Manual capture rescan requested");
            }

            let slider_response = ui.add(
                egui::Slider::new(&mut self.capture_dialog.scan_interval_secs, 2..=60)
                    .text("Scan every (s)"),
            );
            if slider_response.changed() {
                self.capture_dialog.force_rescan = true;
                if self.capture_dialog.auto_scan_enabled {
                    self.record_success(format!(
                        "Auto-scan cadence set to {}s",
                        self.capture_dialog.scan_interval_secs
                    ));
                } else {
                    self.record_info(format!(
                        "Scan interval set to {}s (auto-scan disabled)",
                        self.capture_dialog.scan_interval_secs
                    ));
                }
            }

            let last_scan_text = if let Some(ts) = self.capture_dialog.last_file_scan {
                let elapsed = ts.elapsed();
                if elapsed.as_secs() == 0 {
                    "Last scan: just now".to_string()
                } else if elapsed.as_secs() < 60 {
                    format!("Last scan: {}s ago", elapsed.as_secs())
                } else {
                    format!("Last scan: {}m ago", elapsed.as_secs() / 60)
                }
            } else {
                "Last scan: never".to_string()
            };
            ui.label(egui::RichText::new(last_scan_text).small());
        });

        if !self.capture_dialog.auto_scan_enabled {
            ui.label(
                egui::RichText::new(
                    "Auto-scan is disabled; use 'Rescan capture folder' to refresh.",
                )
                .small(),
            );
            ui.add_space(6.0);
        }

        let scanning = {
            let ctx = ui.ctx().clone();
            let force = self.capture_dialog.force_rescan;
            self.update_recent_capture_files(&ctx, force)
        };

        let mut show_scan_feedback = scanning;
        let mut scan_progress: Option<f32> = if scanning { Some(0.05) } else { None };
        if let Some(until) = self.capture_dialog.scan_feedback_until
            && let Some(remaining) = until.checked_duration_since(Instant::now())
        {
            let total = CAPTURE_SCAN_FEEDBACK_WINDOW.as_secs_f32().max(0.001);
            let done: f32 = 1.0 - (remaining.as_secs_f32() / total).clamp(0.0, 1.0);
            let current: f32 = scan_progress.unwrap_or(0.0);
            scan_progress = Some(current.max(done));
            show_scan_feedback = true;
        }

        if show_scan_feedback {
            let progress: f32 = scan_progress.unwrap_or(0.0).clamp(0.0, 1.0);
            ui.add(
                egui::ProgressBar::new(progress.max(0.05)).text("Scanning capture directory..."),
            );
            ui.add_space(6.0);
        }

        // Active captures

        if !self.capture_dialog.active_captures.is_empty() {
            ui.heading("Active Captures");
            for capture_id in &self.capture_dialog.active_captures.clone() {
                ui.horizontal(|ui| {
                    ui.label(capture_id);
                    if self
                        .preset_button(ui, ButtonIntent::Stop, Some("Capture"), true)
                        .clicked()
                    {
                        self.stop_capture(capture_id);
                    }
                });
            }
            ui.separator();
        }

        // Capture files
        ui.heading("Capture Files");
        if self.capture_dialog.recent_files.is_empty() {
            ui.label("No capture files found yet. Start a capture to populate this list.");
        } else {
            let recent = self.capture_dialog.recent_files.clone();
            for path in recent {
                let display = Self::capture_display_name(&path);
                let metadata = fs::metadata(&path).ok();
                let size_label = metadata
                    .as_ref()
                    .map(|data| Self::format_bytes(data.len()))
                    .unwrap_or_else(|| "Unknown size".to_string());
                let modified_label = metadata
                    .as_ref()
                    .and_then(|data| data.modified().ok())
                    .map(Self::format_timestamp)
                    .unwrap_or_else(|| "Unknown modified time".to_string());

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&display).strong());
                        ui.small(format!("{} • {}", size_label, modified_label));
                    });
                    ui.small(path.display().to_string());
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if self
                            .preset_button(ui, ButtonIntent::Inspect, Some("Capture"), true)
                            .clicked()
                        {
                            self.preview_capture_file(&path);
                        }
                        if self
                            .preset_button(ui, ButtonIntent::Open, Some("in Wireshark"), true)
                            .clicked()
                            && let Some(as_str) = path.to_str()
                        {
                            self.open_in_wireshark(as_str);
                        }
                        if self
                            .preset_button(ui, ButtonIntent::Delete, Some("Capture"), true)
                            .clicked()
                        {
                            self.capture_dialog.pending_delete = Some(path.clone());
                        }
                    });
                });
                ui.add_space(4.0);
            }
        }

        if let Some(preview) = &self.capture_dialog.preview {
            ui.separator();
            ui.heading("Capture Preview");
            ui.label(egui::RichText::new(Self::capture_display_name(&preview.path)).strong());
            ui.small(format!(
                "{} • {}",
                Self::format_bytes(preview.size_bytes),
                Self::format_timestamp(preview.modified)
            ));

            if preview.sampled_bytes == 0 {
                ui.label("Capture file is empty.");
            } else {
                ui.small(format!(
                    "Showing first {} bytes ({} total)",
                    preview.sampled_bytes,
                    Self::format_bytes(preview.size_bytes)
                ));
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.monospace(&preview.header_hex);
                    });
            }
        }
    }

    fn show_arch_config(&mut self, ui: &mut egui::Ui) {
        ui.heading("Arch Linux Configuration");

        ui.separator();

        // Network manager detection
        ui.heading("Network Management");
        ui.label("Detected network management systems:");

        // Would show actual detection results in real implementation
        ui.horizontal(|ui| {
            ui.label("● systemd-networkd:");
            ui.colored_label(Color32::GREEN, "Active");
        });
        ui.horizontal(|ui| {
            ui.label("● NetworkManager:");
            ui.colored_label(Color32::RED, "Inactive");
        });

        ui.separator();

        // KVM optimization
        ui.heading("Virtualization Optimization");
        if self
            .preset_button(ui, ButtonIntent::Start, Some("Optimizations"), true)
            .clicked()
        {
            self.apply_arch_optimizations();
            self.arch_task_until = Some(Instant::now() + Duration::from_secs(2));
            self.arch_task_message = Some("Arch Linux optimizations applied.".to_string());
        }

        if let Some(until) = self.arch_task_until {
            if Instant::now() < until {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Applying Arch Linux KVM optimizations…");
                });
            } else {
                self.arch_task_until = None;
            }
        }

        if self.arch_task_until.is_none()
            && let Some(message) = &self.arch_task_message
        {
            ui.label(message);
        }

        ui.label("This will:");
        ui.label("• Load required KVM kernel modules");
        ui.label("• Configure systemd for virtualization");
        ui.label("• Set up user groups for KVM access");
        ui.label("• Optimize network settings for bridges");
    }

    // Dialog implementations
    fn show_switch_creation_dialog(&mut self, ctx: &egui::Context) {
        if !self.switch_creation_dialog.show {
            return;
        }

        egui::Window::new("Create Virtual Switch")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.switch_creation_dialog.name);
                });

                ui.horizontal(|ui| {
                    ui.label("Type:");
                    egui::ComboBox::from_label("")
                        .selected_text(format!("{:?}", self.switch_creation_dialog.switch_type))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.switch_creation_dialog.switch_type,
                                SwitchType::LinuxBridge,
                                "Linux Bridge",
                            );
                            ui.selectable_value(
                                &mut self.switch_creation_dialog.switch_type,
                                SwitchType::OpenVSwitch,
                                "Open vSwitch",
                            );
                        });
                });

                ui.checkbox(&mut self.switch_creation_dialog.enable_stp, "Enable STP");

                ui.horizontal(|ui| {
                    ui.label("VLAN ID (optional):");
                    ui.text_edit_singleline(&mut self.switch_creation_dialog.vlan_id);
                });

                ui.label("Select interfaces to add:");
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for (i, interface) in
                            self.switch_creation_dialog.interfaces.iter().enumerate()
                        {
                            if i >= self.switch_creation_dialog.selected_interfaces.len() {
                                self.switch_creation_dialog.selected_interfaces.push(false);
                            }
                            ui.checkbox(
                                &mut self.switch_creation_dialog.selected_interfaces[i],
                                interface,
                            );
                        }
                    });

                ui.separator();

                ui.horizontal(|ui| {
                    if self
                        .preset_button(ui, ButtonIntent::Create, Some("Switch"), true)
                        .clicked()
                    {
                        self.create_switch();
                        self.switch_creation_dialog.show = false;
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Cancel, None, true)
                        .clicked()
                    {
                        self.switch_creation_dialog.show = false;
                    }
                });
            });
    }

    fn show_network_creation_dialog(&mut self, ctx: &egui::Context) {
        if !self.network_creation_dialog.show {
            return;
        }

        egui::Window::new("Create Libvirt Network")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.network_creation_dialog.name);
                });

                ui.horizontal(|ui| {
                    ui.label("Type:");
                    egui::ComboBox::from_label("")
                        .selected_text(&self.network_creation_dialog.network_type)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.network_creation_dialog.network_type,
                                "NAT".to_string(),
                                "NAT",
                            );
                            ui.selectable_value(
                                &mut self.network_creation_dialog.network_type,
                                "Bridge".to_string(),
                                "Bridge",
                            );
                            ui.selectable_value(
                                &mut self.network_creation_dialog.network_type,
                                "Isolated".to_string(),
                                "Isolated",
                            );
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Subnet:");
                    ui.text_edit_singleline(&mut self.network_creation_dialog.subnet);
                });

                ui.horizontal(|ui| {
                    ui.label("Gateway:");
                    ui.text_edit_singleline(&mut self.network_creation_dialog.gateway);
                });

                ui.checkbox(
                    &mut self.network_creation_dialog.dhcp_enabled,
                    "Enable DHCP",
                );

                if self.network_creation_dialog.dhcp_enabled {
                    ui.horizontal(|ui| {
                        ui.label("DHCP Start:");
                        ui.text_edit_singleline(&mut self.network_creation_dialog.dhcp_start);
                    });
                    ui.horizontal(|ui| {
                        ui.label("DHCP End:");
                        ui.text_edit_singleline(&mut self.network_creation_dialog.dhcp_end);
                    });
                }

                ui.checkbox(&mut self.network_creation_dialog.autostart, "Autostart");

                ui.separator();

                ui.horizontal(|ui| {
                    if self
                        .preset_button(ui, ButtonIntent::Create, Some("Network"), true)
                        .clicked()
                    {
                        self.create_libvirt_network();
                        self.network_creation_dialog.show = false;
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Cancel, None, true)
                        .clicked()
                    {
                        self.network_creation_dialog.show = false;
                    }
                });
            });
    }

    fn show_capture_dialog(&mut self, ctx: &egui::Context) {
        if !self.capture_dialog.show {
            return;
        }

        egui::Window::new("Start Packet Capture")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Interface:");
                    ui.text_edit_singleline(&mut self.capture_dialog.interface);
                });

                ui.horizontal(|ui| {
                    ui.label("Filter (BPF):");
                    ui.text_edit_singleline(&mut self.capture_dialog.filter);
                });

                ui.horizontal(|ui| {
                    ui.label("Duration (seconds):");
                    ui.text_edit_singleline(&mut self.capture_dialog.duration);
                });

                ui.horizontal(|ui| {
                    ui.label("Packet Count:");
                    ui.text_edit_singleline(&mut self.capture_dialog.packet_count);
                });

                ui.horizontal(|ui| {
                    ui.label("Output File:");
                    ui.text_edit_singleline(&mut self.capture_dialog.output_file);
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if self
                        .preset_button(ui, ButtonIntent::Start, Some("Capture"), true)
                        .clicked()
                    {
                        self.start_capture();
                        self.capture_dialog.show = false;
                    }
                    if self
                        .preset_button(ui, ButtonIntent::Cancel, None, true)
                        .clicked()
                    {
                        self.capture_dialog.show = false;
                    }
                });
            });
    }

    fn show_capture_delete_confirmation(&mut self, ctx: &egui::Context) {
        if let Some(target) = self.capture_dialog.pending_delete.clone() {
            let mut open = true;
            egui::Window::new("Delete capture file")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("This capture will be permanently removed:");
                    ui.monospace(target.display().to_string());
                    if let Ok(metadata) = fs::metadata(&target) {
                        if let Ok(modified) = metadata.modified() {
                            ui.label(format!(
                                "Size {} • Modified {}",
                                Self::format_bytes(metadata.len()),
                                Self::format_timestamp(modified)
                            ));
                        } else {
                            ui.label(format!(
                                "Size {} • Modified time unavailable",
                                Self::format_bytes(metadata.len())
                            ));
                        }
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if self
                            .preset_button(ui, ButtonIntent::ConfirmDelete, Some("Capture"), true)
                            .clicked()
                        {
                            self.remove_capture_file(&target);
                        }
                        if self
                            .preset_button(ui, ButtonIntent::Cancel, None, true)
                            .clicked()
                        {
                            self.capture_dialog.pending_delete = None;
                        }
                    });
                });

            if !open {
                self.capture_dialog.pending_delete = None;
            }
        }
    }

    // Action implementations backed by the underlying managers. Each routes
    // through the shared Tokio runtime so the GUI stays a thin view over real
    // manager state rather than maintaining its own duplicate shell-outs.
    fn refresh_all_data(&mut self) {
        log_info!("Refreshing all network data");
        self.refresh_switches();
        self.refresh_libvirt_networks();
        self.refresh_topology();
        self.last_refresh_all = Some(Instant::now());
        self.touch_refresh_feedback();
    }

    fn refresh_interfaces(&mut self) {
        let manager = Arc::clone(&self.network_manager);
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mut mgr = manager.lock().unwrap();
            mgr.discover_interfaces().await
        });
        if let Some(Err(err)) = &result {
            log_warn!("Failed to discover interfaces: {:?}", err);
        }
        if result.is_some()
            && let Ok(mgr) = self.network_manager.lock()
        {
            self.interfaces = mgr.list_interfaces().into_iter().cloned().collect();
        }

        // Mirror the live interface names into the switch-creation dialog so the
        // user picks real uplinks instead of hard-coded placeholders.
        let names: Vec<String> = self.interfaces.iter().map(|i| i.name.clone()).collect();
        self.switch_creation_dialog.selected_interfaces = vec![false; names.len()];
        self.switch_creation_dialog.interfaces = names;
    }

    fn refresh_switches(&mut self) {
        log_info!("Refreshing virtual switches");
        let manager = Arc::clone(&self.network_manager);
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mut mgr = manager.lock().unwrap();
            mgr.refresh_state().await
        });
        match result {
            Some(Ok(())) => {
                if let Ok(mgr) = self.network_manager.lock() {
                    self.switches = mgr.list_switches().into_iter().cloned().collect();
                    self.interfaces = mgr.list_interfaces().into_iter().cloned().collect();
                }
            }
            Some(Err(err)) => self.record_error(format!("Failed to refresh switches: {:?}", err)),
            None => {}
        }
        self.touch_refresh_feedback();
    }

    fn refresh_libvirt_networks(&mut self) {
        log_info!("Refreshing libvirt networks");
        let manager = Arc::clone(&self.libvirt_manager);
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mut mgr = manager.lock().unwrap();
            mgr.discover_networks().await
        });
        match result {
            Some(Ok(())) => {
                if let Ok(mgr) = self.libvirt_manager.lock() {
                    self.libvirt_networks = mgr.list_networks().clone();
                }
            }
            Some(Err(err)) => {
                self.record_error(format!("Failed to refresh libvirt networks: {:?}", err))
            }
            None => {}
        }
        self.touch_refresh_feedback();
    }

    fn refresh_topology(&mut self) {
        log_info!("Refreshing network topology");
        let manager = Arc::clone(&self.network_monitor);
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mgr = manager.lock().unwrap();
            mgr.discover_topology().await
        });
        match result {
            Some(Ok(topology)) => self.topology = Some(topology),
            Some(Err(err)) => self.record_error(format!("Failed to refresh topology: {:?}", err)),
            None => {}
        }
        self.touch_refresh_feedback();
    }

    fn create_switch(&mut self) {
        let name = self.switch_creation_dialog.name.trim().to_string();
        if name.is_empty() {
            self.record_error("Switch name cannot be empty");
            return;
        }
        let switch_type = self.switch_creation_dialog.switch_type.clone();
        let selected: Vec<String> = self
            .switch_creation_dialog
            .interfaces
            .iter()
            .zip(self.switch_creation_dialog.selected_interfaces.iter())
            .filter(|&(_, &sel)| sel)
            .map(|(iface, _)| iface.clone())
            .collect();

        log_info!("Creating virtual switch: {}", name);
        let manager = Arc::clone(&self.network_manager);
        let create_name = name.clone();
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mut mgr = manager.lock().unwrap();
            mgr.create_virtual_switch(&create_name, switch_type, None)
                .await?;
            for iface in &selected {
                mgr.add_interface_to_switch(&create_name, iface).await?;
            }
            Ok::<(), crate::NovaError>(())
        });
        match result {
            Some(Ok(())) => {
                self.record_success(format!("Created virtual switch '{}'.", name));
                self.refresh_switches();
            }
            Some(Err(err)) => {
                self.record_error(format!("Failed to create switch '{}': {:?}", name, err))
            }
            None => self.record_error("Runtime unavailable; cannot create switch"),
        }
    }

    fn delete_switch(&mut self, name: &str) {
        log_info!("Deleting virtual switch: {}", name);
        let manager = Arc::clone(&self.network_manager);
        let target = name.to_string();
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mut mgr = manager.lock().unwrap();
            mgr.delete_virtual_switch(&target).await
        });
        match result {
            Some(Ok(())) => {
                self.record_success(format!("Deleted switch '{}'.", name));
                self.refresh_switches();
            }
            Some(Err(err)) => {
                self.record_error(format!("Failed to delete switch '{}': {:?}", name, err))
            }
            None => self.record_error("Runtime unavailable; cannot delete switch"),
        }
    }

    fn create_libvirt_network(&mut self) {
        let network = match self.build_libvirt_network_from_dialog() {
            Ok(network) => network,
            Err(err) => {
                self.record_error(err);
                return;
            }
        };
        let name = network.name.clone();
        log_info!("Creating libvirt network: {}", name);
        let manager = Arc::clone(&self.libvirt_manager);
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mut mgr = manager.lock().unwrap();
            mgr.create_network(&network).await
        });
        match result {
            Some(Ok(())) => {
                self.record_success(format!("Created libvirt network '{}'.", name));
                self.refresh_libvirt_networks();
            }
            Some(Err(err)) => {
                self.record_error(format!("Failed to create network '{}': {:?}", name, err))
            }
            None => self.record_error("Runtime unavailable; cannot create network"),
        }
    }

    /// Translate the network-creation dialog fields into a `LibvirtNetwork`,
    /// validating the user-supplied addresses up front.
    fn build_libvirt_network_from_dialog(&self) -> std::result::Result<LibvirtNetwork, String> {
        let dialog = &self.network_creation_dialog;
        let name = dialog.name.trim();
        if name.is_empty() {
            return Err("Network name cannot be empty".to_string());
        }

        let gateway = Ipv4Addr::from_str(dialog.gateway.trim())
            .map_err(|_| format!("Invalid gateway address '{}'", dialog.gateway))?;

        let dhcp = if dialog.dhcp_enabled {
            let start = Ipv4Addr::from_str(dialog.dhcp_start.trim())
                .map_err(|_| format!("Invalid DHCP start address '{}'", dialog.dhcp_start))?;
            let end = Ipv4Addr::from_str(dialog.dhcp_end.trim())
                .map_err(|_| format!("Invalid DHCP end address '{}'", dialog.dhcp_end))?;
            Some(DhcpRange {
                start,
                end,
                hosts: Vec::new(),
            })
        } else {
            None
        };

        let forward = match dialog.network_type.as_str() {
            "NAT" => Some(ForwardMode {
                mode: "nat".to_string(),
                dev: None,
                interfaces: Vec::new(),
            }),
            "Bridge" => Some(ForwardMode {
                mode: "bridge".to_string(),
                dev: None,
                interfaces: Vec::new(),
            }),
            // "Isolated" (or anything else) -> no forwarding.
            _ => None,
        };

        Ok(LibvirtNetwork {
            name: name.to_string(),
            uuid: None,
            forward,
            bridge: Some(BridgeConfig {
                name: format!("virbr-{}", name),
                stp: Some(true),
                delay: Some(0),
            }),
            ip: Some(IpConfig {
                address: gateway,
                netmask: Ipv4Addr::new(255, 255, 255, 0),
                dhcp,
            }),
            dns: None,
            autostart: dialog.autostart,
            active: false,
        })
    }

    fn delete_libvirt_network(&mut self, name: &str) {
        log_info!("Deleting libvirt network: {}", name);
        let manager = Arc::clone(&self.libvirt_manager);
        let target = name.to_string();
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mut mgr = manager.lock().unwrap();
            mgr.delete_network(&target).await
        });
        match result {
            Some(Ok(())) => {
                self.record_success(format!("Deleted libvirt network '{}'.", name));
                self.refresh_libvirt_networks();
            }
            Some(Err(err)) => {
                self.record_error(format!("Failed to delete network '{}': {:?}", name, err))
            }
            None => self.record_error("Runtime unavailable; cannot delete network"),
        }
    }

    fn toggle_libvirt_network(&mut self, name: &str, currently_active: bool) {
        let manager = Arc::clone(&self.libvirt_manager);
        let target = name.to_string();
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mgr = manager.lock().unwrap();
            if currently_active {
                mgr.stop_network(&target).await
            } else {
                mgr.start_network(&target).await
            }
        });
        match result {
            Some(Ok(())) => {
                let verb = if currently_active {
                    "Stopped"
                } else {
                    "Started"
                };
                self.record_success(format!("{} libvirt network '{}'.", verb, name));
                self.refresh_libvirt_networks();
            }
            Some(Err(err)) => {
                self.record_error(format!("Failed to toggle network '{}': {:?}", name, err))
            }
            None => self.record_error("Runtime unavailable; cannot toggle network"),
        }
    }

    fn toggle_monitoring(&mut self) {
        self.monitoring_enabled = !self.monitoring_enabled;
        if self.monitoring_enabled {
            log_info!("Starting network monitoring");
            let interfaces: Vec<String> = self.interfaces.iter().map(|i| i.name.clone()).collect();
            let interval = self.monitoring_poll_secs;
            let manager = Arc::clone(&self.network_monitor);
            // Lock held across the await: sound here, see `block_on` docs.
            #[allow(clippy::await_holding_lock)]
            let result = self.block_on(async move {
                let mut mgr = manager.lock().unwrap();
                mgr.start_monitoring(interfaces, interval).await
            });
            match result {
                Some(Ok(())) => self.record_success("Network monitoring enabled"),
                Some(Err(err)) => {
                    self.record_error(format!("Failed to start monitoring: {:?}", err))
                }
                None => self.record_error("Runtime unavailable; cannot start monitoring"),
            }
            self.last_monitoring_poll = None;
            self.offline_interfaces.clear();
        } else {
            log_info!("Stopping network monitoring");
            if let Ok(mut mgr) = self.network_monitor.lock() {
                mgr.stop_monitoring();
            }
            self.record_success("Network monitoring disabled");
            self.offline_interfaces.clear();
        }
        self.touch_refresh_feedback();
    }

    fn start_capture(&mut self) {
        let interface = self.capture_dialog.interface.trim().to_string();
        if interface.is_empty() {
            self.record_error("Select an interface for packet capture");
            return;
        }
        let output_file = if self.capture_dialog.output_file.trim().is_empty() {
            format!("/tmp/nova-capture-{}.pcap", interface)
        } else {
            self.capture_dialog.output_file.trim().to_string()
        };
        let filter = {
            let f = self.capture_dialog.filter.trim();
            if f.is_empty() {
                None
            } else {
                Some(f.to_string())
            }
        };
        let duration = self.capture_dialog.duration.trim().parse::<u64>().ok();
        let packet_count = self.capture_dialog.packet_count.trim().parse::<u64>().ok();

        let config = PacketCaptureConfig {
            interface: interface.clone(),
            filter,
            duration,
            packet_count,
            output_file,
        };

        log_info!("Starting packet capture on {}", interface);
        let manager = Arc::clone(&self.network_monitor);
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mgr = manager.lock().unwrap();
            mgr.start_packet_capture(&config).await
        });
        match result {
            Some(Ok(capture_id)) => {
                self.capture_dialog.active_captures.push(capture_id);
                self.capture_dialog.last_file_scan = None;
                self.record_success("Packet capture started");
            }
            Some(Err(err)) => self.record_error(format!("Failed to start capture: {:?}", err)),
            None => self.record_error("Runtime unavailable; cannot start capture"),
        }
    }

    fn stop_capture(&mut self, capture_id: &str) {
        log_info!("Stopping packet capture: {}", capture_id);
        let manager = Arc::clone(&self.network_monitor);
        let id = capture_id.to_string();
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mgr = manager.lock().unwrap();
            mgr.stop_packet_capture(&id).await
        });
        match result {
            Some(Ok(())) => {
                self.capture_dialog
                    .active_captures
                    .retain(|c| c != capture_id);
                self.capture_dialog.last_file_scan = None;
                self.record_success(format!("Packet capture '{}' stopped", capture_id));
            }
            Some(Err(err)) => self.record_error(format!("Failed to stop capture: {:?}", err)),
            None => self.record_error("Runtime unavailable; cannot stop capture"),
        }
    }

    fn open_in_wireshark(&mut self, file_path: &str) {
        log_info!("Opening {} in Wireshark", file_path);
        let manager = Arc::clone(&self.network_monitor);
        let path = file_path.to_string();
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mgr = manager.lock().unwrap();
            mgr.launch_wireshark(&path).await
        });
        match result {
            Some(Ok(())) => self.record_success(format!("Launching Wireshark with {}", file_path)),
            Some(Err(err)) => self.record_error(format!("Failed to launch Wireshark: {:?}", err)),
            None => self.record_error("Runtime unavailable; cannot launch Wireshark"),
        }
    }

    fn apply_arch_optimizations(&mut self) {
        log_info!("Applying Arch Linux optimizations");
        let manager = Arc::clone(&self.arch_manager);
        // Lock held across the await: sound here, see `block_on` docs.
        #[allow(clippy::await_holding_lock)]
        let result = self.block_on(async move {
            let mgr = manager.lock().unwrap();
            mgr.optimize_for_virtualization().await
        });
        match result {
            Some(Ok(())) => self.record_success("Arch Linux virtualization optimizations applied"),
            Some(Err(err)) => {
                self.record_error(format!("Failed to apply optimizations: {:?}", err))
            }
            None => self.record_error("Runtime unavailable; cannot apply optimizations"),
        }
    }
}

impl Default for NetworkingGui {
    fn default() -> Self {
        Self::new()
    }
}
