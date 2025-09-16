use egui::Color32;

// Deep Dark Material Blue Ocean Theme - User's Custom Palette

// Background hierarchy
pub const BG_MAIN: Color32 = Color32::from_rgb(0, 7, 45);           // #00072D - User's deep blue
pub const BG_PANEL: Color32 = Color32::from_rgb(10, 27, 61);        // #0A1B3D - Slightly lighter
pub const BG_SECONDARY: Color32 = Color32::from_rgb(26, 47, 82);    // #1A2F52 - For contrast
pub const BG_ELEVATED: Color32 = Color32::from_rgb(36, 55, 95);     // #24375F - Cards/modals
pub const BG_HOVER: Color32 = Color32::from_rgb(26, 67, 191);       // #1A43BF - User's blue for hover
pub const BG_CONSOLE: Color32 = Color32::from_rgb(0, 5, 16);        // #000510 - Deep black-blue

// Text colors - User's specified palette
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(137, 207, 240);   // #89CFF0 - Baby mint blue
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(48, 213, 200);  // #30D5C8 - Turquoise mint
pub const TEXT_ACCENT: Color32 = Color32::from_rgb(26, 67, 191);      // #1A43BF - Deep blue for labels
pub const TEXT_BRIGHT: Color32 = Color32::from_rgb(48, 213, 200);     // #30D5C8 - Turquoise for highlights

// Status colors
pub const STATUS_RUNNING: Color32 = Color32::from_rgb(80, 250, 123);   // #50FA7B - Green
pub const STATUS_STOPPED: Color32 = Color32::from_rgb(255, 85, 85);    // #FF5555 - Red
pub const STATUS_WARNING: Color32 = Color32::from_rgb(241, 250, 140);  // #F1FA8C - Yellow
pub const STATUS_SUSPENDED: Color32 = Color32::from_rgb(139, 233, 253); // #8BE9FD - Cyan
pub const STATUS_UNKNOWN: Color32 = Color32::from_rgb(98, 114, 164);   // #6272A4 - Grey-blue

// Action colors - Updated with user's palette
pub const ACTION_PRIMARY: Color32 = Color32::from_rgb(48, 213, 200);      // #30D5C8 - Turquoise mint
pub const ACTION_PRIMARY_HOVER: Color32 = Color32::from_rgb(26, 67, 191); // #1A43BF - Deep blue hover
pub const ACTION_SECONDARY: Color32 = Color32::from_rgb(26, 47, 82);      // #1A2F52 - Muted ocean blue
pub const ACTION_DANGER: Color32 = Color32::from_rgb(255, 85, 85);        // #FF5555 - Red
pub const ACTION_SUCCESS: Color32 = Color32::from_rgb(80, 250, 123);      // #50FA7B - Green

// Border & Divider colors
pub const BORDER_DEFAULT: Color32 = Color32::from_rgb(36, 55, 95);        // #24375F - Subtle blue
pub const BORDER_FOCUS: Color32 = Color32::from_rgb(48, 213, 200);        // #30D5C8 - Turquoise focus
pub const DIVIDER: Color32 = Color32::from_rgb(26, 47, 82);              // #1A2F52 - Subtle divider

// Selection colors
pub const SELECTION_BG: Color32 = Color32::from_rgb(26, 67, 191);         // #1A43BF - User's blue
pub const SELECTION_HOVER: Color32 = Color32::from_rgb(10, 27, 61);       // #0A1B3D - Hover state

// Graph colors - Using user's palette
pub const GRAPH_CPU: Color32 = Color32::from_rgb(48, 213, 200);           // #30D5C8 - Turquoise
pub const GRAPH_MEMORY: Color32 = Color32::from_rgb(137, 207, 240);       // #89CFF0 - Baby mint blue
pub const GRAPH_DISK: Color32 = Color32::from_rgb(26, 67, 191);           // #1A43BF - Deep blue
pub const GRAPH_NETWORK: Color32 = Color32::from_rgb(80, 250, 123);       // #50FA7B - Green

pub fn configure_ocean_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Configure visuals for deep ocean theme
    let mut visuals = egui::Visuals::dark();

    // Window and panel backgrounds
    visuals.window_fill = BG_MAIN;
    visuals.panel_fill = BG_PANEL;
    visuals.extreme_bg_color = BG_CONSOLE;
    visuals.faint_bg_color = BG_SECONDARY;

    // Text colors
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.hyperlink_color = ACTION_PRIMARY;

    // Widget colors - non-interactive
    visuals.widgets.noninteractive.bg_fill = BG_SECONDARY;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER_DEFAULT);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);

    // Widget colors - inactive
    visuals.widgets.inactive.bg_fill = BG_ELEVATED;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER_DEFAULT);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_SECONDARY);

    // Widget colors - hovered
    visuals.widgets.hovered.bg_fill = BG_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, ACTION_PRIMARY);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_BRIGHT);

    // Widget colors - active
    visuals.widgets.active.bg_fill = SELECTION_BG;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, BORDER_FOCUS);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT_BRIGHT);

    // Selection
    visuals.selection.bg_fill = SELECTION_BG;
    visuals.selection.stroke = egui::Stroke::new(1.0, ACTION_PRIMARY);

    // Popup shadow for depth
    visuals.popup_shadow = egui::epaint::Shadow {
        extrusion: 8.0,
        color: Color32::from_black_alpha(180),
    };

    // Window shadow for depth
    visuals.window_shadow = egui::epaint::Shadow {
        extrusion: 12.0,
        color: Color32::from_black_alpha(140),
    };

    // Resize and interaction colors
    visuals.resize_corner_size = 12.0;
    visuals.clip_rect_margin = 3.0;

    ctx.set_visuals(visuals);

    // Configure spacing for a modern look
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(8.0);
    style.spacing.indent = 20.0;

    // Configure interaction
    style.interaction.resize_grab_radius_side = 5.0;
    style.interaction.resize_grab_radius_corner = 10.0;

    ctx.set_style(style);
}

// Helper function to get status color
pub fn get_status_color(status: &crate::instance::InstanceStatus) -> Color32 {
    use crate::instance::InstanceStatus;
    match status {
        InstanceStatus::Running => STATUS_RUNNING,
        InstanceStatus::Stopped => STATUS_STOPPED,
        InstanceStatus::Starting | InstanceStatus::Stopping => STATUS_WARNING,
        InstanceStatus::Error => STATUS_STOPPED,
        InstanceStatus::Suspended => STATUS_SUSPENDED,
    }
}

// Helper function for status icon
pub fn get_status_icon(status: &crate::instance::InstanceStatus) -> &'static str {
    use crate::instance::InstanceStatus;
    match status {
        InstanceStatus::Running => "●",    // Filled circle
        InstanceStatus::Stopped => "○",    // Empty circle
        InstanceStatus::Starting => "◐",   // Half circle
        InstanceStatus::Stopping => "◑",   // Half circle
        InstanceStatus::Error => "✕",      // X mark
        InstanceStatus::Suspended => "⏸",  // Pause symbol
    }
}