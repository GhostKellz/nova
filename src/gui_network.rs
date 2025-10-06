use crate::arch_integration::ArchNetworkManager;
use crate::libvirt::{LibvirtManager, LibvirtNetwork};
use crate::log_info;
use crate::monitoring::{BandwidthUsage, NetworkMonitor, NetworkTopology};
use crate::network::{NetworkInterface, NetworkManager, SwitchType, VirtualSwitch};

use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
pub struct NetworkingGui {
    // Core managers
    network_manager: Arc<Mutex<NetworkManager>>,
    libvirt_manager: Arc<Mutex<LibvirtManager>>,
    network_monitor: Arc<Mutex<NetworkMonitor>>,
    arch_manager: Arc<Mutex<ArchNetworkManager>>,

    // GUI state
    selected_tab: NetworkTab,
    switch_creation_dialog: SwitchCreationDialog,
    network_creation_dialog: NetworkCreationDialog,
    monitoring_enabled: bool,
    capture_dialog: CaptureDialog,
    topology_view: TopologyView,

    // Data
    switches: Vec<VirtualSwitch>,
    libvirt_networks: Vec<LibvirtNetwork>,
    interfaces: Vec<NetworkInterface>,
    topology: Option<NetworkTopology>,
    bandwidth_data: HashMap<String, Vec<BandwidthUsage>>,
}

#[derive(Debug, Clone, PartialEq)]
enum NetworkTab {
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
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TopologyView {
    zoom: f32,
    pan_offset: Vec2,
    selected_node: Option<String>,
    node_positions: HashMap<String, Pos2>,
}

impl NetworkingGui {
    pub fn new() -> Self {
        Self {
            network_manager: Arc::new(Mutex::new(NetworkManager::new())),
            libvirt_manager: Arc::new(Mutex::new(LibvirtManager::new())),
            network_monitor: Arc::new(Mutex::new(NetworkMonitor::new())),
            arch_manager: Arc::new(Mutex::new(ArchNetworkManager::new())),

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
            capture_dialog: CaptureDialog {
                show: false,
                interface: String::new(),
                filter: String::new(),
                duration: "60".to_string(),
                packet_count: "1000".to_string(),
                output_file: "/tmp/nova-capture.pcap".to_string(),
                active_captures: Vec::new(),
            },
            topology_view: TopologyView {
                zoom: 1.0,
                pan_offset: Vec2::ZERO,
                selected_node: None,
                node_positions: HashMap::new(),
            },

            switches: Vec::new(),
            libvirt_networks: Vec::new(),
            interfaces: Vec::new(),
            topology: None,
            bandwidth_data: HashMap::new(),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
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

            match self.selected_tab {
                NetworkTab::Overview => self.show_overview(ui),
                NetworkTab::VirtualSwitches => self.show_virtual_switches(ui),
                NetworkTab::LibvirtNetworks => self.show_libvirt_networks(ui),
                NetworkTab::Monitoring => self.show_monitoring(ui),
                NetworkTab::Topology => self.show_topology(ui),
                NetworkTab::PacketCapture => self.show_packet_capture(ui),
                NetworkTab::ArchConfig => self.show_arch_config(ui),
            }
        });

        // Show dialogs
        self.show_switch_creation_dialog(ctx);
        self.show_network_creation_dialog(ctx);
        self.show_capture_dialog(ctx);
    }

    fn show_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Network Overview");

        ui.horizontal(|ui| {
            // Quick stats
            egui::Frame::none()
                .fill(Color32::from_gray(40))
                .rounding(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Virtual Switches");
                        ui.heading(self.switches.len().to_string());
                    });
                });

            egui::Frame::none()
                .fill(Color32::from_gray(40))
                .rounding(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Libvirt Networks");
                        ui.heading(self.libvirt_networks.len().to_string());
                    });
                });

            egui::Frame::none()
                .fill(Color32::from_gray(40))
                .rounding(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Network Interfaces");
                        ui.heading(self.interfaces.len().to_string());
                    });
                });
        });

        ui.separator();

        // Quick actions
        ui.heading("Quick Actions");
        ui.horizontal(|ui| {
            if ui.button("Create Virtual Switch").clicked() {
                self.switch_creation_dialog.show = true;
                self.refresh_interfaces();
            }
            if ui.button("Create Libvirt Network").clicked() {
                self.network_creation_dialog.show = true;
            }
            if ui.button("Refresh All").clicked() {
                self.refresh_all_data();
            }
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
                if ui.button("+ Create Switch").clicked() {
                    self.switch_creation_dialog.show = true;
                    self.refresh_interfaces();
                }
                if ui.button("🔄 Refresh").clicked() {
                    self.refresh_switches();
                }
            });
        });

        ui.separator();

        let switches_to_delete = std::cell::RefCell::new(Vec::new());

        egui::ScrollArea::vertical().show(ui, |ui| {
            for switch in &self.switches {
                let switch_name = switch.name.clone();
                egui::Frame::none()
                    .fill(Color32::from_gray(30))
                    .rounding(5.0)
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
                                if ui.button("⚙️ Configure").clicked() {
                                    // Open configuration dialog
                                }
                                if ui.button("🗑️ Delete").clicked() {
                                    switches_to_delete.borrow_mut().push(switch_name);
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
                if ui.button("+ Create Network").clicked() {
                    self.network_creation_dialog.show = true;
                }
                if ui.button("🔄 Refresh").clicked() {
                    self.refresh_libvirt_networks();
                }
            });
        });

        ui.separator();

        let networks_to_toggle = std::cell::RefCell::new(Vec::new());
        let networks_to_delete = std::cell::RefCell::new(Vec::new());

        egui::ScrollArea::vertical().show(ui, |ui| {
            for network in &self.libvirt_networks {
                let network_name = network.name.clone();
                let is_active = network.active;
                egui::Frame::none()
                    .fill(Color32::from_gray(30))
                    .rounding(5.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
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
                                    "⏹️ Stop"
                                } else {
                                    "▶️ Start"
                                };
                                if ui.button(action_text).clicked() {
                                    networks_to_toggle
                                        .borrow_mut()
                                        .push((network_name.clone(), is_active));
                                }
                                if ui.button("⚙️ Edit").clicked() {
                                    // Open edit dialog
                                }
                                if ui.button("🗑️ Delete").clicked() {
                                    networks_to_delete.borrow_mut().push(network_name);
                                }
                            });
                        });
                    });
                ui.add_space(5.0);
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
                let button_text = if self.monitoring_enabled {
                    "⏹️ Stop Monitoring"
                } else {
                    "▶️ Start Monitoring"
                };
                if ui.button(button_text).clicked() {
                    self.toggle_monitoring();
                }
            });
        });

        ui.separator();

        if !self.monitoring_enabled {
            ui.centered_and_justified(|ui| {
                ui.label("Click 'Start Monitoring' to begin collecting network statistics");
            });
            return;
        }

        // Bandwidth charts
        for (interface, bandwidth_history) in &self.bandwidth_data {
            if bandwidth_history.is_empty() {
                continue;
            }

            egui::Frame::none()
                .fill(Color32::from_gray(30))
                .rounding(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.heading(format!("Interface: {}", interface));

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
                if ui.button("🔄 Refresh").clicked() {
                    self.refresh_topology();
                }
            });
        });

        ui.separator();

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
                if ui.button("+ New Capture").clicked() {
                    self.capture_dialog.show = true;
                }
            });
        });

        ui.separator();

        // Active captures
        if !self.capture_dialog.active_captures.is_empty() {
            ui.heading("Active Captures");
            for capture_id in &self.capture_dialog.active_captures.clone() {
                ui.horizontal(|ui| {
                    ui.label(capture_id);
                    if ui.button("⏹️ Stop").clicked() {
                        self.stop_capture(capture_id);
                    }
                });
            }
            ui.separator();
        }

        // Capture files
        ui.heading("Capture Files");
        ui.label("Recent packet capture files will be listed here");

        // Would list actual .pcap files in a real implementation
        ui.horizontal(|ui| {
            ui.label("/tmp/nova-capture.pcap");
            if ui.button("🔍 Open in Wireshark").clicked() {
                self.open_in_wireshark("/tmp/nova-capture.pcap");
            }
        });
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
        if ui.button("Apply Arch Linux KVM Optimizations").clicked() {
            self.apply_arch_optimizations();
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
                    if ui.button("Create").clicked() {
                        self.create_switch();
                        self.switch_creation_dialog.show = false;
                    }
                    if ui.button("Cancel").clicked() {
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
                    if ui.button("Create").clicked() {
                        self.create_libvirt_network();
                        self.network_creation_dialog.show = false;
                    }
                    if ui.button("Cancel").clicked() {
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
                    if ui.button("Start Capture").clicked() {
                        self.start_capture();
                        self.capture_dialog.show = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.capture_dialog.show = false;
                    }
                });
            });
    }

    // Action implementations (these would contain actual async calls in a real implementation)
    fn refresh_all_data(&mut self) {
        log_info!("Refreshing all network data");
        // Would call actual refresh methods
    }

    fn refresh_interfaces(&mut self) {
        // Populate interfaces for switch creation dialog
        self.switch_creation_dialog.interfaces =
            vec!["eth0".to_string(), "eth1".to_string(), "wlan0".to_string()];
        self.switch_creation_dialog.selected_interfaces.clear();
    }

    fn refresh_switches(&mut self) {
        log_info!("Refreshing virtual switches");
        // Would call network_manager.list_switches()
    }

    fn refresh_libvirt_networks(&mut self) {
        log_info!("Refreshing libvirt networks");
        // Would call libvirt_manager.discover_networks()
    }

    fn refresh_topology(&mut self) {
        log_info!("Refreshing network topology");
        // Would call network_monitor.discover_topology()
    }

    fn create_switch(&mut self) {
        log_info!(
            "Creating virtual switch: {}",
            self.switch_creation_dialog.name
        );
        // Would call network_manager.create_virtual_switch()
    }

    fn delete_switch(&mut self, name: &str) {
        log_info!("Deleting virtual switch: {}", name);
        // Would call network_manager.delete_virtual_switch()
    }

    fn create_libvirt_network(&mut self) {
        log_info!(
            "Creating libvirt network: {}",
            self.network_creation_dialog.name
        );
        // Would call libvirt_manager.create_network()
    }

    fn delete_libvirt_network(&mut self, name: &str) {
        log_info!("Deleting libvirt network: {}", name);
        // Would call libvirt_manager.delete_network()
    }

    fn toggle_libvirt_network(&mut self, name: &str, currently_active: bool) {
        if currently_active {
            log_info!("Stopping libvirt network: {}", name);
            // Would call libvirt_manager.stop_network()
        } else {
            log_info!("Starting libvirt network: {}", name);
            // Would call libvirt_manager.start_network()
        }
    }

    fn toggle_monitoring(&mut self) {
        self.monitoring_enabled = !self.monitoring_enabled;
        if self.monitoring_enabled {
            log_info!("Starting network monitoring");
            // Would call network_monitor.start_monitoring()
        } else {
            log_info!("Stopping network monitoring");
            // Would call network_monitor.stop_monitoring()
        }
    }

    fn start_capture(&mut self) {
        log_info!(
            "Starting packet capture on {}",
            self.capture_dialog.interface
        );
        // Would call network_monitor.start_packet_capture()
        // Add to active captures list
        self.capture_dialog
            .active_captures
            .push(format!("capture-{}", self.capture_dialog.interface));
    }

    fn stop_capture(&mut self, capture_id: &str) {
        log_info!("Stopping packet capture: {}", capture_id);
        // Would call network_monitor.stop_packet_capture()
        self.capture_dialog
            .active_captures
            .retain(|id| id != capture_id);
    }

    fn open_in_wireshark(&mut self, file_path: &str) {
        log_info!("Opening {} in Wireshark", file_path);
        // Would call network_monitor.launch_wireshark()
    }

    fn apply_arch_optimizations(&mut self) {
        log_info!("Applying Arch Linux optimizations");
        // Would call arch_manager.optimize_for_virtualization()
    }
}

impl Default for NetworkingGui {
    fn default() -> Self {
        Self::new()
    }
}
