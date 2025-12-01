use egui::{self, Color32};

// ===== TOKYO NIGHT - NIGHT VARIANT (Default) =====
// The classic Tokyo Night with deep blue backgrounds

pub const TN_NIGHT_BG: Color32 = Color32::from_rgb(26, 27, 38); // #1a1b26
pub const TN_NIGHT_BG_DARK: Color32 = Color32::from_rgb(22, 22, 30); // #16161e
pub const TN_NIGHT_BG_HIGHLIGHT: Color32 = Color32::from_rgb(41, 46, 66); // #292e42
pub const TN_NIGHT_TERMINAL_BLACK: Color32 = Color32::from_rgb(21, 24, 44); // #15161e
pub const TN_NIGHT_FG: Color32 = Color32::from_rgb(192, 202, 245); // #c0caf5
pub const TN_NIGHT_FG_DARK: Color32 = Color32::from_rgb(169, 177, 214); // #a9b1d6
pub const TN_NIGHT_FG_GUTTER: Color32 = Color32::from_rgb(59, 66, 97); // #3b4261
pub const TN_NIGHT_DARK3: Color32 = Color32::from_rgb(68, 75, 106); // #444b6a
pub const TN_NIGHT_COMMENT: Color32 = Color32::from_rgb(86, 95, 137); // #565f89
pub const TN_NIGHT_DARK5: Color32 = Color32::from_rgb(115, 127, 173); // #737aa2
pub const TN_NIGHT_BLUE0: Color32 = Color32::from_rgb(61, 89, 161); // #3d59a1
pub const TN_NIGHT_BLUE: Color32 = Color32::from_rgb(122, 162, 247); // #7aa2f7
pub const TN_NIGHT_CYAN: Color32 = Color32::from_rgb(125, 207, 255); // #7dcfff
pub const TN_NIGHT_BLUE1: Color32 = Color32::from_rgb(42, 195, 222); // #2ac3de
pub const TN_NIGHT_BLUE2: Color32 = Color32::from_rgb(0, 122, 204); // #007acc
pub const TN_NIGHT_BLUE5: Color32 = Color32::from_rgb(137, 221, 255); // #89ddff
pub const TN_NIGHT_BLUE6: Color32 = Color32::from_rgb(180, 249, 248); // #b4f9f8
pub const TN_NIGHT_BLUE7: Color32 = Color32::from_rgb(148, 226, 213); // #94e2d5
pub const TN_NIGHT_MAGENTA: Color32 = Color32::from_rgb(187, 154, 247); // #bb9af7
pub const TN_NIGHT_MAGENTA2: Color32 = Color32::from_rgb(255, 0, 127); // #ff007c
pub const TN_NIGHT_PURPLE: Color32 = Color32::from_rgb(157, 124, 216); // #9d7cd8
pub const TN_NIGHT_ORANGE: Color32 = Color32::from_rgb(255, 158, 100); // #ff9e64
pub const TN_NIGHT_YELLOW: Color32 = Color32::from_rgb(224, 175, 104); // #e0af68
pub const TN_NIGHT_GREEN: Color32 = Color32::from_rgb(158, 206, 106); // #9ece6a
pub const TN_NIGHT_GREEN1: Color32 = Color32::from_rgb(115, 218, 202); // #73daca
pub const TN_NIGHT_GREEN2: Color32 = Color32::from_rgb(65, 166, 181); // #41a6b5
pub const TN_NIGHT_TEAL: Color32 = Color32::from_rgb(26, 188, 156); // #1abc9c
pub const TN_NIGHT_RED: Color32 = Color32::from_rgb(247, 118, 142); // #f7768e
pub const TN_NIGHT_RED1: Color32 = Color32::from_rgb(219, 75, 75); // #db4b4b

// ===== TOKYO NIGHT - STORM VARIANT =====
// Lighter variant with grey-blue backgrounds

pub const TN_STORM_BG: Color32 = Color32::from_rgb(36, 40, 59); // #24283b
pub const TN_STORM_BG_DARK: Color32 = Color32::from_rgb(30, 33, 49); // #1e2030
pub const TN_STORM_BG_HIGHLIGHT: Color32 = Color32::from_rgb(47, 52, 74); // #2f344a
pub const TN_STORM_TERMINAL_BLACK: Color32 = Color32::from_rgb(6, 10, 33); // #06101e
pub const TN_STORM_FG: Color32 = Color32::from_rgb(192, 202, 245); // #c0caf5
pub const TN_STORM_FG_DARK: Color32 = Color32::from_rgb(169, 177, 214); // #a9b1d6
pub const TN_STORM_FG_GUTTER: Color32 = Color32::from_rgb(59, 66, 97); // #3b4261
pub const TN_STORM_DARK3: Color32 = Color32::from_rgb(68, 75, 106); // #444b6a
pub const TN_STORM_COMMENT: Color32 = Color32::from_rgb(86, 95, 137); // #565f89
pub const TN_STORM_DARK5: Color32 = Color32::from_rgb(115, 127, 173); // #737aa2
pub const TN_STORM_BLUE0: Color32 = Color32::from_rgb(61, 89, 161); // #3d59a1
pub const TN_STORM_BLUE: Color32 = Color32::from_rgb(122, 162, 247); // #7aa2f7
pub const TN_STORM_CYAN: Color32 = Color32::from_rgb(125, 207, 255); // #7dcfff
pub const TN_STORM_BLUE1: Color32 = Color32::from_rgb(42, 195, 222); // #2ac3de
pub const TN_STORM_BLUE2: Color32 = Color32::from_rgb(0, 122, 204); // #007acc
pub const TN_STORM_BLUE5: Color32 = Color32::from_rgb(137, 221, 255); // #89ddff
pub const TN_STORM_BLUE6: Color32 = Color32::from_rgb(180, 249, 248); // #b4f9f8
pub const TN_STORM_BLUE7: Color32 = Color32::from_rgb(148, 226, 213); // #94e2d5
pub const TN_STORM_MAGENTA: Color32 = Color32::from_rgb(187, 154, 247); // #bb9af7
pub const TN_STORM_MAGENTA2: Color32 = Color32::from_rgb(255, 0, 127); // #ff007c
pub const TN_STORM_PURPLE: Color32 = Color32::from_rgb(157, 124, 216); // #9d7cd8
pub const TN_STORM_ORANGE: Color32 = Color32::from_rgb(255, 158, 100); // #ff9e64
pub const TN_STORM_YELLOW: Color32 = Color32::from_rgb(224, 175, 104); // #e0af68
pub const TN_STORM_GREEN: Color32 = Color32::from_rgb(158, 206, 106); // #9ece6a
pub const TN_STORM_GREEN1: Color32 = Color32::from_rgb(115, 218, 202); // #73daca
pub const TN_STORM_GREEN2: Color32 = Color32::from_rgb(65, 166, 181); // #41a6b5
pub const TN_STORM_TEAL: Color32 = Color32::from_rgb(26, 188, 156); // #1abc9c
pub const TN_STORM_RED: Color32 = Color32::from_rgb(247, 118, 142); // #f7768e
pub const TN_STORM_RED1: Color32 = Color32::from_rgb(219, 75, 75); // #db4b4b

// ===== TOKYO NIGHT - MOON VARIANT =====
// Softest variant with muted purple-blue backgrounds

pub const TN_MOON_BG: Color32 = Color32::from_rgb(34, 37, 63); // #222436
pub const TN_MOON_BG_DARK: Color32 = Color32::from_rgb(29, 31, 55); // #1e1e2e
pub const TN_MOON_BG_HIGHLIGHT: Color32 = Color32::from_rgb(42, 47, 74); // #2f334d
pub const TN_MOON_TERMINAL_BLACK: Color32 = Color32::from_rgb(17, 18, 39); // #1b1d2b
pub const TN_MOON_FG: Color32 = Color32::from_rgb(192, 202, 245); // #c0caf5
pub const TN_MOON_FG_DARK: Color32 = Color32::from_rgb(169, 177, 214); // #a9b1d6
pub const TN_MOON_FG_GUTTER: Color32 = Color32::from_rgb(59, 66, 97); // #3b4261
pub const TN_MOON_DARK3: Color32 = Color32::from_rgb(68, 75, 106); // #444a73
pub const TN_MOON_COMMENT: Color32 = Color32::from_rgb(86, 95, 137); // #636da6
pub const TN_MOON_DARK5: Color32 = Color32::from_rgb(115, 127, 173); // #737aa2
pub const TN_MOON_BLUE0: Color32 = Color32::from_rgb(61, 89, 161); // #3d59a1
pub const TN_MOON_BLUE: Color32 = Color32::from_rgb(130, 170, 255); // #82aaff
pub const TN_MOON_CYAN: Color32 = Color32::from_rgb(134, 230, 255); // #86e1fc
pub const TN_MOON_BLUE1: Color32 = Color32::from_rgb(101, 177, 239); // #65bcef
pub const TN_MOON_BLUE2: Color32 = Color32::from_rgb(0, 122, 204); // #0db9d7
pub const TN_MOON_BLUE5: Color32 = Color32::from_rgb(137, 221, 255); // #89ddff
pub const TN_MOON_BLUE6: Color32 = Color32::from_rgb(180, 249, 248); // #b4f9f8
pub const TN_MOON_BLUE7: Color32 = Color32::from_rgb(148, 226, 213); // #394b70
pub const TN_MOON_MAGENTA: Color32 = Color32::from_rgb(199, 146, 234); // #c099ff
pub const TN_MOON_MAGENTA2: Color32 = Color32::from_rgb(255, 117, 181); // #ff757f
pub const TN_MOON_PURPLE: Color32 = Color32::from_rgb(186, 154, 255); // #fca7ea
pub const TN_MOON_ORANGE: Color32 = Color32::from_rgb(255, 152, 0); // #ff9800
pub const TN_MOON_YELLOW: Color32 = Color32::from_rgb(255, 199, 119); // #ffc777
pub const TN_MOON_GREEN: Color32 = Color32::from_rgb(195, 232, 141); // #c3e88d
pub const TN_MOON_GREEN1: Color32 = Color32::from_rgb(77, 213, 196); // #4fd6be
pub const TN_MOON_GREEN2: Color32 = Color32::from_rgb(65, 166, 181); // #41a6b5
pub const TN_MOON_TEAL: Color32 = Color32::from_rgb(76, 220, 221); // #4fd6be
pub const TN_MOON_RED: Color32 = Color32::from_rgb(255, 117, 127); // #ff757f
pub const TN_MOON_RED1: Color32 = Color32::from_rgb(201, 74, 83); // #c94f6d

// ===== CATPPUCCIN - MOCHA VARIANT =====
// Warm, modern palette with rich contrasts

pub const CAT_BG: Color32 = Color32::from_rgb(24, 24, 37); // #181825
pub const CAT_BG_DARK: Color32 = Color32::from_rgb(17, 17, 27); // #11111b
pub const CAT_BG_HIGHLIGHT: Color32 = Color32::from_rgb(36, 37, 54); // #242438
pub const CAT_TERMINAL_BLACK: Color32 = Color32::from_rgb(30, 32, 48); // #1e2030
pub const CAT_FG: Color32 = Color32::from_rgb(205, 214, 244); // #cdd6f4
pub const CAT_FG_DIM: Color32 = Color32::from_rgb(180, 190, 214); // #b4befe
pub const CAT_COMMENT: Color32 = Color32::from_rgb(137, 145, 168); // #8991a8
pub const CAT_LAVENDER: Color32 = Color32::from_rgb(180, 190, 254); // #b4befe
pub const CAT_SKY: Color32 = Color32::from_rgb(137, 220, 235); // #89dceb
pub const CAT_TEAL: Color32 = Color32::from_rgb(148, 226, 213); // #94e2d5
pub const CAT_GREEN: Color32 = Color32::from_rgb(166, 227, 161); // #a6e3a1
pub const CAT_YELLOW: Color32 = Color32::from_rgb(249, 226, 175); // #f9e2af
pub const CAT_PEACH: Color32 = Color32::from_rgb(250, 179, 135); // #fab387
pub const CAT_ROSEWATER: Color32 = Color32::from_rgb(245, 224, 220); // #f5e0dc
pub const CAT_RED: Color32 = Color32::from_rgb(243, 139, 168); // #f38ba8

// ===== DRACULA =====
// Classic high-contrast purple/green palette with deep midnight background

pub const DRACULA_BG: Color32 = Color32::from_rgb(30, 31, 41); // #1e1f29
pub const DRACULA_BG_DARK: Color32 = Color32::from_rgb(22, 23, 31); // #16171f
pub const DRACULA_BG_HIGHLIGHT: Color32 = Color32::from_rgb(46, 48, 65); // #2e3041
pub const DRACULA_TERMINAL_BLACK: Color32 = Color32::from_rgb(16, 17, 25); // #101119
pub const DRACULA_FG: Color32 = Color32::from_rgb(248, 248, 242); // #f8f8f2
pub const DRACULA_COMMENT: Color32 = Color32::from_rgb(98, 114, 164); // #6272a4
pub const DRACULA_PINK: Color32 = Color32::from_rgb(255, 121, 198); // #ff79c6
pub const DRACULA_CYAN: Color32 = Color32::from_rgb(139, 233, 253); // #8be9fd
pub const DRACULA_BLUE: Color32 = Color32::from_rgb(189, 147, 249); // #bd93f9
pub const DRACULA_GREEN: Color32 = Color32::from_rgb(80, 250, 123); // #50fa7b
pub const DRACULA_ORANGE: Color32 = Color32::from_rgb(255, 184, 108); // #ffb86c
pub const DRACULA_YELLOW: Color32 = Color32::from_rgb(241, 250, 140); // #f1fa8c
pub const DRACULA_RED: Color32 = Color32::from_rgb(255, 85, 85); // #ff5555

// ===== THEME VARIANT ENUM =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokyoNightVariant {
    Night, // Default - Deep blue
    Storm, // Lighter grey-blue
    Moon,  // Softest purple-blue
}

impl Default for TokyoNightVariant {
    fn default() -> Self {
        TokyoNightVariant::Storm
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiTheme {
    TokyoNight(TokyoNightVariant),
    CatppuccinMocha,
    Dracula,
    Ocean,
}

impl Default for GuiTheme {
    fn default() -> Self {
        GuiTheme::TokyoNight(TokyoNightVariant::Storm)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonRole {
    Primary,
    Start,
    Stop,
    Restart,
    Secondary,
}

#[derive(Debug, Clone, Copy)]
pub struct ButtonPalette {
    pub fill: Color32,
    pub hover: Color32,
    pub stroke: Color32,
    pub text: Color32,
}

#[derive(Debug, Clone, Copy)]
pub struct ButtonOptions {
    pub min_width: f32,
    pub min_height: f32,
}

impl Default for ButtonOptions {
    fn default() -> Self {
        Self {
            min_width: 120.0,
            min_height: 30.0,
        }
    }
}

pub const DEFAULT_THEME_NAME: &str = "tokyo-night-storm";

pub const ALL_THEMES: [GuiTheme; 6] = [
    GuiTheme::TokyoNight(TokyoNightVariant::Storm),
    GuiTheme::TokyoNight(TokyoNightVariant::Night),
    GuiTheme::TokyoNight(TokyoNightVariant::Moon),
    GuiTheme::CatppuccinMocha,
    GuiTheme::Dracula,
    GuiTheme::Ocean,
];

impl GuiTheme {
    pub const fn name(self) -> &'static str {
        match self {
            GuiTheme::TokyoNight(TokyoNightVariant::Night) => "tokyo-night-night",
            GuiTheme::TokyoNight(TokyoNightVariant::Storm) => "tokyo-night-storm",
            GuiTheme::TokyoNight(TokyoNightVariant::Moon) => "tokyo-night-moon",
            GuiTheme::CatppuccinMocha => "catppuccin-mocha",
            GuiTheme::Dracula => "dracula",
            GuiTheme::Ocean => "ocean",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            GuiTheme::TokyoNight(TokyoNightVariant::Night) => "Tokyo Night (Night)",
            GuiTheme::TokyoNight(TokyoNightVariant::Storm) => "Tokyo Night (Storm)",
            GuiTheme::TokyoNight(TokyoNightVariant::Moon) => "Tokyo Night (Moon)",
            GuiTheme::CatppuccinMocha => "Catppuccin (Mocha)",
            GuiTheme::Dracula => "Dracula",
            GuiTheme::Ocean => "Material Ocean",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "tokyo-night-night" => Some(GuiTheme::TokyoNight(TokyoNightVariant::Night)),
            "tokyo-night-storm" => Some(GuiTheme::TokyoNight(TokyoNightVariant::Storm)),
            "tokyo-night-moon" => Some(GuiTheme::TokyoNight(TokyoNightVariant::Moon)),
            "catppuccin-mocha" => Some(GuiTheme::CatppuccinMocha),
            "dracula" => Some(GuiTheme::Dracula),
            "ocean" => Some(GuiTheme::Ocean),
            _ => None,
        }
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: GuiTheme) {
    match theme {
        GuiTheme::TokyoNight(variant) => configure_tokyo_night_theme(ctx, variant),
        GuiTheme::CatppuccinMocha => configure_catppuccin_theme(ctx),
        GuiTheme::Dracula => configure_dracula_theme(ctx),
        GuiTheme::Ocean => configure_ocean_theme(ctx),
    }
}

// ===== THEME CONFIGURATION FUNCTIONS =====

pub fn configure_tokyo_night_theme(ctx: &egui::Context, variant: TokyoNightVariant) {
    match variant {
        TokyoNightVariant::Night => configure_tokyo_night_night(ctx),
        TokyoNightVariant::Storm => configure_tokyo_night_storm(ctx),
        TokyoNightVariant::Moon => configure_tokyo_night_moon(ctx),
    }
}

fn configure_tokyo_night_night(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    // Backgrounds
    visuals.window_fill = TN_NIGHT_BG;
    visuals.panel_fill = TN_NIGHT_BG;
    visuals.extreme_bg_color = TN_NIGHT_BG_DARK;
    visuals.faint_bg_color = TN_NIGHT_BG_HIGHLIGHT;

    // Text
    visuals.override_text_color = Some(TN_NIGHT_FG);
    visuals.hyperlink_color = TN_NIGHT_CYAN;

    // Widgets - noninteractive
    visuals.widgets.noninteractive.bg_fill = TN_NIGHT_BG_HIGHLIGHT;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, TN_NIGHT_DARK3);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TN_NIGHT_FG);

    // Widgets - inactive
    visuals.widgets.inactive.bg_fill = TN_NIGHT_BG_HIGHLIGHT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, TN_NIGHT_BLUE0);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TN_NIGHT_FG_DARK);

    // Widgets - hovered
    visuals.widgets.hovered.bg_fill = TN_NIGHT_DARK3;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, TN_NIGHT_CYAN);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, TN_NIGHT_CYAN);

    // Widgets - active
    visuals.widgets.active.bg_fill = TN_NIGHT_BLUE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, TN_NIGHT_CYAN);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, TN_NIGHT_FG);

    // Selection
    visuals.selection.bg_fill = TN_NIGHT_BLUE0;
    visuals.selection.stroke = egui::Stroke::new(1.5, TN_NIGHT_CYAN);

    apply_modern_theme_style(&mut visuals, &mut style, ctx);
}

fn configure_tokyo_night_storm(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    // Backgrounds
    visuals.window_fill = TN_STORM_BG;
    visuals.panel_fill = TN_STORM_BG;
    visuals.extreme_bg_color = TN_STORM_BG_DARK;
    visuals.faint_bg_color = TN_STORM_BG_HIGHLIGHT;

    // Text
    visuals.override_text_color = Some(TN_STORM_FG);
    visuals.hyperlink_color = TN_STORM_CYAN;

    // Widgets - noninteractive
    visuals.widgets.noninteractive.bg_fill = TN_STORM_BG_HIGHLIGHT;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, TN_STORM_DARK3);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TN_STORM_FG);

    // Widgets - inactive
    visuals.widgets.inactive.bg_fill = TN_STORM_BG_HIGHLIGHT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, TN_STORM_BLUE0);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TN_STORM_FG_DARK);

    // Widgets - hovered
    visuals.widgets.hovered.bg_fill = TN_STORM_DARK3;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, TN_STORM_CYAN);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, TN_STORM_CYAN);

    // Widgets - active
    visuals.widgets.active.bg_fill = TN_STORM_BLUE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, TN_STORM_CYAN);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, TN_STORM_FG);

    // Selection
    visuals.selection.bg_fill = TN_STORM_BLUE0;
    visuals.selection.stroke = egui::Stroke::new(1.5, TN_STORM_CYAN);

    apply_modern_theme_style(&mut visuals, &mut style, ctx);
}

fn tokyo_button_palette(variant: TokyoNightVariant, role: ButtonRole) -> ButtonPalette {
    match variant {
        TokyoNightVariant::Night => match role {
            ButtonRole::Primary => ButtonPalette {
                fill: TN_NIGHT_BLUE7,
                hover: TN_NIGHT_CYAN,
                stroke: TN_NIGHT_BLUE5,
                text: TN_NIGHT_TERMINAL_BLACK,
            },
            ButtonRole::Start => ButtonPalette {
                fill: TN_NIGHT_GREEN,
                hover: TN_NIGHT_GREEN1,
                stroke: TN_NIGHT_BLUE1,
                text: TN_NIGHT_TERMINAL_BLACK,
            },
            ButtonRole::Stop => ButtonPalette {
                fill: TN_NIGHT_RED,
                hover: TN_NIGHT_RED1,
                stroke: TN_NIGHT_BLUE1,
                text: TN_NIGHT_TERMINAL_BLACK,
            },
            ButtonRole::Restart => ButtonPalette {
                fill: TN_NIGHT_ORANGE,
                hover: TN_NIGHT_YELLOW,
                stroke: TN_NIGHT_BLUE1,
                text: TN_NIGHT_TERMINAL_BLACK,
            },
            ButtonRole::Secondary => ButtonPalette {
                fill: TN_NIGHT_BG_HIGHLIGHT,
                hover: TN_NIGHT_BLUE0,
                stroke: TN_NIGHT_DARK3,
                text: TN_NIGHT_FG,
            },
        },
        TokyoNightVariant::Storm => match role {
            ButtonRole::Primary => ButtonPalette {
                fill: TN_STORM_BLUE7,
                hover: TN_STORM_CYAN,
                stroke: TN_STORM_BLUE5,
                text: TN_STORM_BG_DARK,
            },
            ButtonRole::Start => ButtonPalette {
                fill: TN_STORM_GREEN,
                hover: TN_STORM_GREEN1,
                stroke: TN_STORM_BLUE1,
                text: TN_STORM_BG_DARK,
            },
            ButtonRole::Stop => ButtonPalette {
                fill: TN_STORM_RED,
                hover: TN_STORM_RED1,
                stroke: TN_STORM_BLUE1,
                text: TN_STORM_BG_DARK,
            },
            ButtonRole::Restart => ButtonPalette {
                fill: TN_STORM_ORANGE,
                hover: TN_STORM_YELLOW,
                stroke: TN_STORM_BLUE1,
                text: TN_STORM_BG_DARK,
            },
            ButtonRole::Secondary => ButtonPalette {
                fill: TN_STORM_BG_HIGHLIGHT,
                hover: TN_STORM_BLUE0,
                stroke: TN_STORM_DARK3,
                text: TN_STORM_FG,
            },
        },
        TokyoNightVariant::Moon => match role {
            ButtonRole::Primary => ButtonPalette {
                fill: TN_MOON_BLUE7,
                hover: TN_MOON_CYAN,
                stroke: TN_MOON_BLUE5,
                text: TN_MOON_TERMINAL_BLACK,
            },
            ButtonRole::Start => ButtonPalette {
                fill: TN_MOON_GREEN,
                hover: TN_MOON_GREEN1,
                stroke: TN_MOON_BLUE1,
                text: TN_MOON_TERMINAL_BLACK,
            },
            ButtonRole::Stop => ButtonPalette {
                fill: TN_MOON_RED,
                hover: TN_MOON_RED1,
                stroke: TN_MOON_BLUE1,
                text: TN_MOON_TERMINAL_BLACK,
            },
            ButtonRole::Restart => ButtonPalette {
                fill: TN_MOON_ORANGE,
                hover: TN_MOON_YELLOW,
                stroke: TN_MOON_BLUE1,
                text: TN_MOON_TERMINAL_BLACK,
            },
            ButtonRole::Secondary => ButtonPalette {
                fill: TN_MOON_BG_HIGHLIGHT,
                hover: TN_MOON_BLUE0,
                stroke: TN_MOON_DARK3,
                text: TN_MOON_FG,
            },
        },
    }
}

fn catppuccin_button_palette(role: ButtonRole) -> ButtonPalette {
    match role {
        ButtonRole::Primary => ButtonPalette {
            fill: CAT_TEAL,
            hover: CAT_SKY,
            stroke: CAT_TEAL,
            text: CAT_BG_DARK,
        },
        ButtonRole::Start => ButtonPalette {
            fill: CAT_GREEN,
            hover: CAT_TEAL,
            stroke: CAT_TEAL,
            text: CAT_BG_DARK,
        },
        ButtonRole::Stop => ButtonPalette {
            fill: CAT_RED,
            hover: CAT_PEACH,
            stroke: CAT_TEAL,
            text: CAT_BG_DARK,
        },
        ButtonRole::Restart => ButtonPalette {
            fill: CAT_PEACH,
            hover: CAT_YELLOW,
            stroke: CAT_TEAL,
            text: CAT_BG_DARK,
        },
        ButtonRole::Secondary => ButtonPalette {
            fill: CAT_BG_HIGHLIGHT,
            hover: CAT_TEAL,
            stroke: CAT_TEAL,
            text: CAT_FG,
        },
    }
}

fn dracula_button_palette(role: ButtonRole) -> ButtonPalette {
    match role {
        ButtonRole::Primary => ButtonPalette {
            fill: DRACULA_PINK,
            hover: DRACULA_CYAN,
            stroke: DRACULA_CYAN,
            text: DRACULA_BG,
        },
        ButtonRole::Start => ButtonPalette {
            fill: DRACULA_GREEN,
            hover: DRACULA_CYAN,
            stroke: DRACULA_CYAN,
            text: DRACULA_BG,
        },
        ButtonRole::Stop => ButtonPalette {
            fill: DRACULA_RED,
            hover: DRACULA_ORANGE,
            stroke: DRACULA_CYAN,
            text: DRACULA_BG,
        },
        ButtonRole::Restart => ButtonPalette {
            fill: DRACULA_ORANGE,
            hover: DRACULA_YELLOW,
            stroke: DRACULA_CYAN,
            text: DRACULA_BG,
        },
        ButtonRole::Secondary => ButtonPalette {
            fill: DRACULA_BG_HIGHLIGHT,
            hover: DRACULA_CYAN,
            stroke: DRACULA_CYAN,
            text: DRACULA_FG,
        },
    }
}

fn ocean_button_palette(role: ButtonRole) -> ButtonPalette {
    match role {
        ButtonRole::Primary => ButtonPalette {
            fill: MO_ACTION_PRIMARY,
            hover: MO_ACTION_PRIMARY_HOVER,
            stroke: MO_ACTION_PRIMARY,
            text: MO_BG_MAIN,
        },
        ButtonRole::Start => ButtonPalette {
            fill: MO_ACTION_SUCCESS,
            hover: MO_GRAPH_NETWORK,
            stroke: MO_ACTION_PRIMARY,
            text: MO_BG_MAIN,
        },
        ButtonRole::Stop => ButtonPalette {
            fill: MO_ACTION_DANGER,
            hover: MO_STATUS_STOPPED,
            stroke: MO_ACTION_PRIMARY,
            text: MO_BG_MAIN,
        },
        ButtonRole::Restart => ButtonPalette {
            fill: MO_STATUS_WARNING,
            hover: MO_STATUS_SUSPENDED,
            stroke: MO_ACTION_PRIMARY,
            text: MO_BG_MAIN,
        },
        ButtonRole::Secondary => ButtonPalette {
            fill: MO_BG_SECONDARY,
            hover: MO_ACTION_PRIMARY,
            stroke: MO_ACTION_PRIMARY,
            text: MO_TEXT_PRIMARY,
        },
    }
}

pub fn button_palette(theme: GuiTheme, role: ButtonRole) -> ButtonPalette {
    match theme {
        GuiTheme::TokyoNight(variant) => tokyo_button_palette(variant, role),
        GuiTheme::CatppuccinMocha => catppuccin_button_palette(role),
        GuiTheme::Dracula => dracula_button_palette(role),
        GuiTheme::Ocean => ocean_button_palette(role),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonIntent {
    Create,
    Refresh,
    Delete,
    Configure,
    Start,
    Stop,
    Open,
    Inspect,
    Cancel,
    Diagnostics,
    Load,
    Assign,
    Release,
    Select,
    Bind,
    Unbind,
    Preferences,
    Launch,
    ConfirmDelete,
}

#[derive(Debug, Clone, Copy)]
struct ButtonIntentSpec {
    icon: &'static str,
    verb: &'static str,
    tooltip_prefix: &'static str,
    role: ButtonRole,
    min_width: f32,
}

impl ButtonIntentSpec {
    fn label(&self, subject: Option<&str>) -> String {
        let core = match subject {
            Some(value) if !value.is_empty() => format!("{} {}", self.verb, value),
            _ => self.verb.to_string(),
        };

        if self.icon.is_empty() {
            core
        } else {
            format!("{} {}", self.icon, core)
        }
    }

    fn tooltip(&self, subject: Option<&str>) -> String {
        match (self.tooltip_prefix, subject) {
            (prefix, Some(value)) if !prefix.is_empty() => {
                format!("{} {}", prefix, value)
            }
            (_, Some(value)) => format!("{} {}", self.verb, value),
            (prefix, None) if !prefix.is_empty() => prefix.to_string(),
            _ => self.verb.to_string(),
        }
    }
}

fn intent_spec(intent: ButtonIntent) -> ButtonIntentSpec {
    match intent {
        ButtonIntent::Create => ButtonIntentSpec {
            icon: "➕",
            verb: "Create",
            tooltip_prefix: "Create new",
            role: ButtonRole::Primary,
            min_width: 150.0,
        },
        ButtonIntent::Refresh => ButtonIntentSpec {
            icon: "🔄",
            verb: "Refresh",
            tooltip_prefix: "Refresh",
            role: ButtonRole::Secondary,
            min_width: 150.0,
        },
        ButtonIntent::Delete => ButtonIntentSpec {
            icon: "🗑️",
            verb: "Delete",
            tooltip_prefix: "Permanently remove",
            role: ButtonRole::Stop,
            min_width: 150.0,
        },
        ButtonIntent::Configure => ButtonIntentSpec {
            icon: "⚙️",
            verb: "Configure",
            tooltip_prefix: "Configure",
            role: ButtonRole::Secondary,
            min_width: 160.0,
        },
        ButtonIntent::Start => ButtonIntentSpec {
            icon: "▶️",
            verb: "Start",
            tooltip_prefix: "Start",
            role: ButtonRole::Start,
            min_width: 150.0,
        },
        ButtonIntent::Stop => ButtonIntentSpec {
            icon: "⏹️",
            verb: "Stop",
            tooltip_prefix: "Stop",
            role: ButtonRole::Stop,
            min_width: 150.0,
        },
        ButtonIntent::Open => ButtonIntentSpec {
            icon: "🔍",
            verb: "Open",
            tooltip_prefix: "Open",
            role: ButtonRole::Secondary,
            min_width: 150.0,
        },
        ButtonIntent::Inspect => ButtonIntentSpec {
            icon: "👁",
            verb: "Preview",
            tooltip_prefix: "Preview",
            role: ButtonRole::Secondary,
            min_width: 170.0,
        },
        ButtonIntent::Cancel => ButtonIntentSpec {
            icon: "✖",
            verb: "Cancel",
            tooltip_prefix: "Cancel",
            role: ButtonRole::Secondary,
            min_width: 130.0,
        },
        ButtonIntent::Diagnostics => ButtonIntentSpec {
            icon: "🩺",
            verb: "Run diagnostics",
            tooltip_prefix: "Run diagnostics for",
            role: ButtonRole::Secondary,
            min_width: 190.0,
        },
        ButtonIntent::Load => ButtonIntentSpec {
            icon: "📦",
            verb: "Load",
            tooltip_prefix: "Load",
            role: ButtonRole::Primary,
            min_width: 160.0,
        },
        ButtonIntent::Assign => ButtonIntentSpec {
            icon: "🎯",
            verb: "Assign",
            tooltip_prefix: "Assign",
            role: ButtonRole::Primary,
            min_width: 170.0,
        },
        ButtonIntent::Release => ButtonIntentSpec {
            icon: "🔓",
            verb: "Release",
            tooltip_prefix: "Release",
            role: ButtonRole::Secondary,
            min_width: 150.0,
        },
        ButtonIntent::Bind => ButtonIntentSpec {
            icon: "🔗",
            verb: "Bind",
            tooltip_prefix: "Bind",
            role: ButtonRole::Primary,
            min_width: 170.0,
        },
        ButtonIntent::Unbind => ButtonIntentSpec {
            icon: "✂",
            verb: "Unbind",
            tooltip_prefix: "Unbind",
            role: ButtonRole::Stop,
            min_width: 170.0,
        },
        ButtonIntent::Select => ButtonIntentSpec {
            icon: "☑",
            verb: "Select",
            tooltip_prefix: "Select",
            role: ButtonRole::Secondary,
            min_width: 140.0,
        },
        ButtonIntent::Preferences => ButtonIntentSpec {
            icon: "⚙️",
            verb: "Preferences",
            tooltip_prefix: "Open preferences",
            role: ButtonRole::Secondary,
            min_width: 180.0,
        },
        ButtonIntent::Launch => ButtonIntentSpec {
            icon: "🖥️",
            verb: "Open",
            tooltip_prefix: "Open",
            role: ButtonRole::Secondary,
            min_width: 180.0,
        },
        ButtonIntent::ConfirmDelete => ButtonIntentSpec {
            icon: "⚠",
            verb: "Confirm delete",
            tooltip_prefix: "Confirm deletion for",
            role: ButtonRole::Stop,
            min_width: 200.0,
        },
    }
}

pub fn themed_button_preset(
    ui: &mut egui::Ui,
    theme: GuiTheme,
    intent: ButtonIntent,
    subject: Option<&str>,
    enabled: bool,
) -> egui::Response {
    let spec = intent_spec(intent);
    let label = spec.label(subject);
    let options = ButtonOptions {
        min_width: spec.min_width,
        ..ButtonOptions::default()
    };

    let response = themed_button_with_options(ui, &label, theme, spec.role, enabled, options);
    let tooltip = spec.tooltip(subject);

    if enabled {
        response.on_hover_text(tooltip)
    } else {
        response.on_disabled_hover_text(tooltip)
    }
}

pub fn themed_button(
    ui: &mut egui::Ui,
    label: &str,
    theme: GuiTheme,
    role: ButtonRole,
    enabled: bool,
) -> egui::Response {
    let options = ButtonOptions::default();
    themed_button_with_options(ui, label, theme, role, enabled, options)
}

pub fn themed_button_with_options(
    ui: &mut egui::Ui,
    label: &str,
    theme: GuiTheme,
    role: ButtonRole,
    enabled: bool,
    options: ButtonOptions,
) -> egui::Response {
    let palette = button_palette(theme, role);
    let rounding = egui::Rounding::same(6.0);

    let (fill, hover, stroke, text) = if enabled {
        (palette.fill, palette.hover, palette.stroke, palette.text)
    } else {
        (
            palette.fill.gamma_multiply(0.6),
            palette.hover.gamma_multiply(0.6),
            palette.stroke.gamma_multiply(0.5),
            palette.text.gamma_multiply(0.7),
        )
    };

    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals;

        visuals.widgets.inactive.bg_fill = fill;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, stroke);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
        visuals.widgets.inactive.rounding = rounding;

        visuals.widgets.hovered.bg_fill = hover;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, stroke);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, text);
        visuals.widgets.hovered.rounding = rounding;

        visuals.widgets.active.bg_fill = hover;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, stroke);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, text);
        visuals.widgets.active.rounding = rounding;

        let button = egui::Button::new(egui::RichText::new(label).color(text))
            .min_size(egui::vec2(options.min_width, options.min_height))
            .stroke(egui::Stroke::new(1.0, stroke))
            .rounding(rounding)
            .fill(fill);

        if enabled {
            ui.add(button)
        } else {
            ui.add_enabled(false, button)
        }
    })
    .inner
}

fn configure_tokyo_night_moon(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    // Backgrounds
    visuals.window_fill = TN_MOON_BG;
    visuals.panel_fill = TN_MOON_BG;
    visuals.extreme_bg_color = TN_MOON_BG_DARK;
    visuals.faint_bg_color = TN_MOON_BG_HIGHLIGHT;

    // Text
    visuals.override_text_color = Some(TN_MOON_FG);
    visuals.hyperlink_color = TN_MOON_CYAN;

    // Widgets - noninteractive
    visuals.widgets.noninteractive.bg_fill = TN_MOON_BG_HIGHLIGHT;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, TN_MOON_DARK3);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TN_MOON_FG);

    // Widgets - inactive
    visuals.widgets.inactive.bg_fill = TN_MOON_BG_HIGHLIGHT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, TN_MOON_BLUE0);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TN_MOON_FG_DARK);

    // Widgets - hovered
    visuals.widgets.hovered.bg_fill = TN_MOON_DARK3;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, TN_MOON_CYAN);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, TN_MOON_CYAN);

    // Widgets - active
    visuals.widgets.active.bg_fill = TN_MOON_BLUE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, TN_MOON_CYAN);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, TN_MOON_FG);

    // Selection
    visuals.selection.bg_fill = TN_MOON_BLUE0;
    visuals.selection.stroke = egui::Stroke::new(1.5, TN_MOON_CYAN);

    apply_modern_theme_style(&mut visuals, &mut style, ctx);
}

pub fn configure_catppuccin_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    visuals.window_fill = CAT_BG;
    visuals.panel_fill = CAT_BG;
    visuals.extreme_bg_color = CAT_BG_DARK;
    visuals.faint_bg_color = CAT_BG_HIGHLIGHT;

    visuals.override_text_color = Some(CAT_FG);
    visuals.hyperlink_color = CAT_SKY;

    visuals.widgets.noninteractive.bg_fill = CAT_BG_HIGHLIGHT;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, CAT_COMMENT);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, CAT_FG);

    visuals.widgets.inactive.bg_fill = CAT_BG_HIGHLIGHT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, CAT_LAVENDER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, CAT_FG_DIM);

    visuals.widgets.hovered.bg_fill = CAT_BG_HIGHLIGHT;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, CAT_TEAL);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, CAT_TEAL);

    visuals.widgets.active.bg_fill = CAT_PEACH;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, CAT_TEAL);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, CAT_BG_DARK);

    visuals.selection.bg_fill = CAT_TEAL;
    visuals.selection.stroke = egui::Stroke::new(1.5, CAT_SKY);

    apply_modern_theme_style(&mut visuals, &mut style, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_spec_generates_expected_labels() {
        let create_spec = intent_spec(ButtonIntent::Create);
        assert_eq!(
            create_spec.label(Some("Virtual Switch")),
            "➕ Create Virtual Switch"
        );
        assert_eq!(
            create_spec.tooltip(Some("Virtual Switch")),
            "Create new Virtual Switch"
        );

        let delete_spec = intent_spec(ButtonIntent::Delete);
        assert_eq!(delete_spec.label(None), "🗑️ Delete");
        assert_eq!(
            delete_spec.tooltip(Some("Network")),
            "Permanently remove Network"
        );

        let confirm_spec = intent_spec(ButtonIntent::ConfirmDelete);
        assert_eq!(
            confirm_spec.label(Some("Networks")),
            "⚠ Confirm delete Networks"
        );
        assert_eq!(
            confirm_spec.tooltip(Some("Networks")),
            "Confirm deletion for Networks"
        );
    }
}

pub fn configure_dracula_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    visuals.window_fill = DRACULA_BG;
    visuals.panel_fill = DRACULA_BG;
    visuals.extreme_bg_color = DRACULA_BG_DARK;
    visuals.faint_bg_color = DRACULA_BG_HIGHLIGHT;

    visuals.override_text_color = Some(DRACULA_FG);
    visuals.hyperlink_color = DRACULA_PINK;

    visuals.widgets.noninteractive.bg_fill = DRACULA_BG_HIGHLIGHT;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, DRACULA_COMMENT);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, DRACULA_FG);

    visuals.widgets.inactive.bg_fill = DRACULA_BG_HIGHLIGHT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, DRACULA_BLUE);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, DRACULA_COMMENT);

    visuals.widgets.hovered.bg_fill = DRACULA_BG_HIGHLIGHT;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, DRACULA_CYAN);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, DRACULA_CYAN);

    visuals.widgets.active.bg_fill = DRACULA_PINK;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, DRACULA_CYAN);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, DRACULA_BG);

    visuals.selection.bg_fill = DRACULA_BLUE;
    visuals.selection.stroke = egui::Stroke::new(1.5, DRACULA_CYAN);

    apply_modern_theme_style(&mut visuals, &mut style, ctx);
}

fn apply_modern_theme_style(
    visuals: &mut egui::Visuals,
    style: &mut egui::Style,
    ctx: &egui::Context,
) {
    // Shadows for depth
    visuals.popup_shadow = egui::epaint::Shadow {
        extrusion: 12.0,
        color: Color32::from_black_alpha(200),
    };

    visuals.window_shadow = egui::epaint::Shadow {
        extrusion: 16.0,
        color: Color32::from_black_alpha(160),
    };

    // Modern rounded corners
    visuals.window_rounding = egui::Rounding::same(8.0);
    visuals.menu_rounding = egui::Rounding::same(6.0);

    // Resize and interaction
    visuals.resize_corner_size = 12.0;
    visuals.clip_rect_margin = 3.0;

    ctx.set_visuals(visuals.clone());

    // Keep visuals in sync with style so later `set_style` calls retain the theme colors.
    style.visuals = visuals.clone();

    // Spacing for modern, airy feel
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(10.0);
    style.spacing.indent = 24.0;
    style.spacing.window_margin = egui::Margin::same(12.0);

    // Interaction
    style.interaction.resize_grab_radius_side = 6.0;
    style.interaction.resize_grab_radius_corner = 12.0;

    ctx.set_style(style.clone());
}

// ===== STATUS COLOR HELPERS =====

pub fn get_status_color_tokyo_night(
    status: &crate::instance::InstanceStatus,
    variant: TokyoNightVariant,
) -> Color32 {
    use crate::instance::InstanceStatus;
    match variant {
        TokyoNightVariant::Night => match status {
            InstanceStatus::Running => TN_NIGHT_GREEN,
            InstanceStatus::Stopped => TN_NIGHT_DARK3,
            InstanceStatus::Starting | InstanceStatus::Stopping => TN_NIGHT_CYAN,
            InstanceStatus::Error => TN_NIGHT_RED,
            InstanceStatus::Suspended => TN_NIGHT_ORANGE,
        },
        TokyoNightVariant::Storm => match status {
            InstanceStatus::Running => TN_STORM_GREEN,
            InstanceStatus::Stopped => TN_STORM_DARK3,
            InstanceStatus::Starting | InstanceStatus::Stopping => TN_STORM_CYAN,
            InstanceStatus::Error => TN_STORM_RED,
            InstanceStatus::Suspended => TN_STORM_ORANGE,
        },
        TokyoNightVariant::Moon => match status {
            InstanceStatus::Running => TN_MOON_GREEN,
            InstanceStatus::Stopped => TN_MOON_DARK3,
            InstanceStatus::Starting | InstanceStatus::Stopping => TN_MOON_CYAN,
            InstanceStatus::Error => TN_MOON_RED,
            InstanceStatus::Suspended => TN_MOON_ORANGE,
        },
    }
}

pub fn get_status_color(status: &crate::instance::InstanceStatus, theme: GuiTheme) -> Color32 {
    use crate::instance::InstanceStatus;
    match theme {
        GuiTheme::TokyoNight(variant) => get_status_color_tokyo_night(status, variant),
        GuiTheme::CatppuccinMocha => match status {
            InstanceStatus::Running => CAT_GREEN,
            InstanceStatus::Stopped => CAT_RED,
            InstanceStatus::Starting | InstanceStatus::Stopping => CAT_TEAL,
            InstanceStatus::Error => CAT_RED,
            InstanceStatus::Suspended => CAT_YELLOW,
        },
        GuiTheme::Dracula => match status {
            InstanceStatus::Running => DRACULA_GREEN,
            InstanceStatus::Stopped => DRACULA_RED,
            InstanceStatus::Starting | InstanceStatus::Stopping => DRACULA_CYAN,
            InstanceStatus::Error => DRACULA_RED,
            InstanceStatus::Suspended => DRACULA_YELLOW,
        },
        GuiTheme::Ocean => get_status_color_ocean(status),
    }
}

// Helper function for status icon (same for all variants)
pub fn get_status_icon(status: &crate::instance::InstanceStatus) -> &'static str {
    use crate::instance::InstanceStatus;
    match status {
        InstanceStatus::Running => "●",   // Filled circle
        InstanceStatus::Stopped => "○",   // Empty circle
        InstanceStatus::Starting => "◐",  // Half circle
        InstanceStatus::Stopping => "◑",  // Half circle
        InstanceStatus::Error => "✕",     // X mark
        InstanceStatus::Suspended => "⏸", // Pause symbol
    }
}

// ===== MATERIAL OCEAN THEME =====
// Deep azure panels with neon accents, inspired by the Material Theme Oceanic palette

pub const MO_BG_MAIN: Color32 = Color32::from_rgb(10, 18, 34); // #0a1222
pub const MO_BG_PANEL: Color32 = Color32::from_rgb(13, 26, 46); // #0d1a2e
pub const MO_BG_SECONDARY: Color32 = Color32::from_rgb(20, 38, 62); // #14263e
pub const MO_BG_ELEVATED: Color32 = Color32::from_rgb(24, 47, 78); // #182f4e
pub const MO_BG_HOVER: Color32 = Color32::from_rgb(36, 72, 118); // #244876
pub const MO_BG_CONSOLE: Color32 = Color32::from_rgb(9, 14, 24); // #090e18

pub const MO_TEXT_PRIMARY: Color32 = Color32::from_rgb(198, 208, 245); // #c6d0f5
pub const MO_TEXT_SECONDARY: Color32 = Color32::from_rgb(126, 206, 255); // #7eceff
pub const MO_TEXT_ACCENT: Color32 = Color32::from_rgb(64, 156, 255); // #409cff
pub const MO_TEXT_BRIGHT: Color32 = Color32::from_rgb(142, 229, 245); // #8ee5f5

pub const MO_STATUS_RUNNING: Color32 = Color32::from_rgb(66, 215, 172); // #42d7ac
pub const MO_STATUS_STOPPED: Color32 = Color32::from_rgb(255, 110, 120); // #ff6e78
pub const MO_STATUS_WARNING: Color32 = Color32::from_rgb(255, 204, 128); // #ffcc80
pub const MO_STATUS_SUSPENDED: Color32 = Color32::from_rgb(120, 210, 255); // #78d2ff
pub const MO_STATUS_UNKNOWN: Color32 = Color32::from_rgb(92, 122, 180); // #5c7ab4

pub const MO_ACTION_PRIMARY: Color32 = Color32::from_rgb(64, 156, 255); // #409cff
pub const MO_ACTION_PRIMARY_HOVER: Color32 = Color32::from_rgb(92, 180, 255); // #5cb4ff
pub const MO_ACTION_SECONDARY: Color32 = MO_BG_SECONDARY;
pub const MO_ACTION_DANGER: Color32 = Color32::from_rgb(255, 110, 120); // #ff6e78
pub const MO_ACTION_SUCCESS: Color32 = Color32::from_rgb(66, 215, 172); // #42d7ac

pub const MO_BORDER_DEFAULT: Color32 = Color32::from_rgb(30, 54, 88); // #1e3658
pub const MO_BORDER_FOCUS: Color32 = Color32::from_rgb(64, 156, 255); // #409cff
pub const MO_DIVIDER: Color32 = Color32::from_rgb(22, 38, 62); // #16263e

pub const MO_SELECTION_BG: Color32 = Color32::from_rgb(32, 96, 173); // #2060ad
pub const MO_SELECTION_HOVER: Color32 = Color32::from_rgb(20, 64, 114); // #144072

pub const MO_GRAPH_CPU: Color32 = Color32::from_rgb(64, 156, 255); // #409cff
pub const MO_GRAPH_MEMORY: Color32 = Color32::from_rgb(126, 206, 255); // #7eceff
pub const MO_GRAPH_DISK: Color32 = Color32::from_rgb(142, 229, 245); // #8ee5f5
pub const MO_GRAPH_NETWORK: Color32 = Color32::from_rgb(66, 215, 172); // #42d7ac

// Backwards-compatible aliases for existing callers
pub const TEXT_PRIMARY: Color32 = MO_TEXT_PRIMARY;
pub const TEXT_SECONDARY: Color32 = MO_TEXT_SECONDARY;
pub const TEXT_ACCENT: Color32 = MO_TEXT_ACCENT;
pub const TEXT_BRIGHT: Color32 = MO_TEXT_BRIGHT;

pub const STATUS_RUNNING: Color32 = MO_STATUS_RUNNING;
pub const STATUS_STOPPED: Color32 = MO_STATUS_STOPPED;
pub const STATUS_WARNING: Color32 = MO_STATUS_WARNING;
pub const STATUS_SUSPENDED: Color32 = MO_STATUS_SUSPENDED;
pub const STATUS_UNKNOWN: Color32 = MO_STATUS_UNKNOWN;

pub const ACTION_PRIMARY: Color32 = MO_ACTION_PRIMARY;
pub const ACTION_PRIMARY_HOVER: Color32 = MO_ACTION_PRIMARY_HOVER;
pub const ACTION_SECONDARY: Color32 = MO_ACTION_SECONDARY;
pub const ACTION_DANGER: Color32 = MO_ACTION_DANGER;
pub const ACTION_SUCCESS: Color32 = MO_ACTION_SUCCESS;

pub const BORDER_DEFAULT: Color32 = MO_BORDER_DEFAULT;
pub const BORDER_FOCUS: Color32 = MO_BORDER_FOCUS;
pub const DIVIDER: Color32 = MO_DIVIDER;

pub const SELECTION_BG: Color32 = MO_SELECTION_BG;
pub const SELECTION_HOVER: Color32 = MO_SELECTION_HOVER;

pub const GRAPH_CPU: Color32 = MO_GRAPH_CPU;
pub const GRAPH_MEMORY: Color32 = MO_GRAPH_MEMORY;
pub const GRAPH_DISK: Color32 = MO_GRAPH_DISK;
pub const GRAPH_NETWORK: Color32 = MO_GRAPH_NETWORK;

pub fn configure_ocean_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    visuals.window_fill = MO_BG_MAIN;
    visuals.panel_fill = MO_BG_PANEL;
    visuals.extreme_bg_color = MO_BG_CONSOLE;
    visuals.faint_bg_color = MO_BG_SECONDARY;

    visuals.override_text_color = Some(MO_TEXT_PRIMARY);
    visuals.hyperlink_color = MO_ACTION_PRIMARY;

    visuals.widgets.noninteractive.bg_fill = MO_BG_SECONDARY;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, MO_BORDER_DEFAULT);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, MO_TEXT_PRIMARY);

    visuals.widgets.inactive.bg_fill = MO_BG_ELEVATED;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, MO_BORDER_DEFAULT);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, MO_TEXT_SECONDARY);

    visuals.widgets.hovered.bg_fill = MO_BG_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, MO_ACTION_PRIMARY);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, MO_TEXT_BRIGHT);

    visuals.widgets.active.bg_fill = MO_SELECTION_BG;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, MO_BORDER_FOCUS);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, MO_TEXT_BRIGHT);

    visuals.selection.bg_fill = MO_SELECTION_BG;
    visuals.selection.stroke = egui::Stroke::new(1.0, MO_ACTION_PRIMARY);

    visuals.popup_shadow = egui::epaint::Shadow {
        extrusion: 8.0,
        color: Color32::from_black_alpha(180),
    };

    visuals.window_shadow = egui::epaint::Shadow {
        extrusion: 12.0,
        color: Color32::from_black_alpha(140),
    };

    visuals.resize_corner_size = 12.0;
    visuals.clip_rect_margin = 3.0;

    ctx.set_visuals(visuals);

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(8.0);
    style.spacing.indent = 20.0;

    style.interaction.resize_grab_radius_side = 5.0;
    style.interaction.resize_grab_radius_corner = 10.0;

    ctx.set_style(style);
}

pub fn get_status_color_ocean(status: &crate::instance::InstanceStatus) -> Color32 {
    use crate::instance::InstanceStatus;
    match status {
        InstanceStatus::Running => MO_STATUS_RUNNING,
        InstanceStatus::Stopped => MO_STATUS_STOPPED,
        InstanceStatus::Starting | InstanceStatus::Stopping => MO_STATUS_WARNING,
        InstanceStatus::Error => MO_STATUS_STOPPED,
        InstanceStatus::Suspended => MO_STATUS_SUSPENDED,
    }
}
