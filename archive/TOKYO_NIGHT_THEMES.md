# Tokyo Night Themes for Nova

Nova now includes all three official Tokyo Night variants, based on the popular [folke/tokyonight.nvim](https://github.com/folke/tokyonight.nvim) theme.

## Available Variants

### 🌙 Night (Default)
The classic Tokyo Night with deep blue backgrounds

**Background Colors:**
- Main Background: `#1a1b26` - Deep night blue
- Dark Background: `#16161e` - Darker areas
- Highlight: `#292e42` - Hover/selection states
- Foreground: `#c0caf5` - Light text

**Accent Colors:**
- Blue: `#7aa2f7` - Primary actions
- Cyan/Teal: `#7dcfff` - Highlights, links
- Green/Mint: `#9ece6a` - Running status, success
- Purple: `#bb9af7` - Secondary actions
- Orange: `#ff9e64` - Warnings, paused state
- Red: `#f7768e` - Errors, destructive actions
- Yellow: `#e0af68` - Alerts

**Best For:** General use, night-time coding, modern aesthetic

---

### ⛈️ Storm
Lighter grey-blue variant with softer contrasts

**Background Colors:**
- Main Background: `#24283b` - Grey-blue
- Dark Background: `#1e2030` - Darker grey-blue
- Highlight: `#2f344a` - Hover/selection
- Foreground: `#c0caf5` - Light text

**Accent Colors:**
- Same vibrant accent colors as Night variant
- Slightly better contrast on lighter backgrounds
- Still maintains the Tokyo Night aesthetic

**Best For:** Daytime use, reduced eye strain, softer look

---

### 🌕 Moon
Softest variant with muted purple-blue backgrounds

**Background Colors:**
- Main Background: `#222436` - Purple-blue
- Dark Background: `#1e1e2e` - Darker purple-blue
- Highlight: `#2f334d` - Hover/selection
- Foreground: `#c0caf5` - Light text

**Accent Colors:**
- Slightly different accent palette
- Blue: `#82aaff` - Brighter blue
- Cyan: `#86e1fc` - Lighter cyan
- Green: `#c3e88d` - Softer green
- Purple: `#c099ff` - Softer purple
- Red: `#ff757f` - Softer red

**Best For:** Softest on the eyes, cozy atmosphere, long sessions

---

## Color Comparison

| Element | Night | Storm | Moon |
|---------|-------|-------|------|
| **Background** | `#1a1b26` (Deep Blue) | `#24283b` (Grey-Blue) | `#222436` (Purple-Blue) |
| **Running Status** | `#9ece6a` (Mint) | `#9ece6a` (Mint) | `#c3e88d` (Soft Green) |
| **Primary Action** | `#7aa2f7` (Blue) | `#7aa2f7` (Blue) | `#82aaff` (Bright Blue) |
| **Highlight** | `#7dcfff` (Cyan) | `#7dcfff` (Cyan) | `#86e1fc` (Light Cyan) |
| **Error** | `#f7768e` (Soft Red) | `#f7768e` (Soft Red) | `#ff757f` (Softer Red) |

---

## How to Switch Themes

### In Code (Current Implementation)

Currently set to Night variant by default. In `src/gui_main.rs`:

```rust
// Change this line to switch variants:
theme::configure_tokyo_night_theme(&cc.egui_ctx, theme::TokyoNightVariant::Night);

// Options:
theme::TokyoNightVariant::Night  // Default
theme::TokyoNightVariant::Storm  // Lighter
theme::TokyoNightVariant::Moon   // Softest
```

### Future: GUI Settings (Coming Soon)

```
Settings → Appearance → Theme
● Night   (Deep blue, vibrant)
○ Storm   (Grey-blue, softer)
○ Moon    (Purple-blue, muted)
```

---

## Status Color Indicators

All variants use consistent status indicators:

| Status | Icon | Color (Night) | Color (Storm) | Color (Moon) |
|--------|------|---------------|---------------|--------------|
| **Running** | ● | Mint Green | Mint Green | Soft Green |
| **Stopped** | ○ | Muted Grey | Muted Grey | Muted Grey |
| **Starting** | ◐ | Cyan | Cyan | Light Cyan |
| **Error** | ✕ | Soft Red | Soft Red | Softer Red |
| **Suspended** | ⏸ | Orange | Orange | Orange |

---

## Design Philosophy

### Night
- **High Contrast**: Deep blues with vibrant accents
- **Modern**: Sharp, clean aesthetic
- **Energetic**: Bright highlights pop against dark background
- **Use Case**: Default experience, best for most users

### Storm
- **Medium Contrast**: Grey-blue base with same accents
- **Balanced**: Less extreme than Night
- **Professional**: Softer, more subdued look
- **Use Case**: Daytime use, professional environments

### Moon
- **Lower Contrast**: Purple-blue with muted accents
- **Gentle**: Easiest on the eyes
- **Calming**: Warm, cozy atmosphere
- **Use Case**: Long sessions, late night work

---

## Technical Implementation

### Theme Structure

```rust
pub enum TokyoNightVariant {
    Night,  // #1a1b26 - Deep blue
    Storm,  // #24283b - Grey-blue
    Moon,   // #222436 - Purple-blue
}
```

### Full Color Palettes

Each variant includes:
- **15+ background shades** for depth and hierarchy
- **10+ accent colors** for different UI elements
- **Status colors** for VM states
- **Consistent spacing** and rounded corners
- **Modern shadows** for depth perception

### Widget States

All variants include proper styling for:
- Noninteractive (static labels, text)
- Inactive (unfocused inputs)
- Hovered (mouse over)
- Active (clicked, focused)
- Selected (chosen items)

---

## Examples

### VM List with Night Theme
```
┌─────────────────────────────────────────────┐
│ ● win11-gaming          [Start ▼] [Connect]│
│   VM  •  Running for 1h 23m                 │
│   ↗ CPU: 23%  •  RAM: 4.2/8GB              │
│   Background: #1a1b26  •  Green: #9ece6a    │
└─────────────────────────────────────────────┘
```

### Dashboard with Storm Theme
```
┌─────────────────────────────────────────────┐
│ 📊 SYSTEM OVERVIEW                          │
│ ● VMs: 5 Running, 3 Stopped                │
│ Background: #24283b  •  Cyan: #7dcfff       │
└─────────────────────────────────────────────┘
```

### Settings Panel with Moon Theme
```
┌─────────────────────────────────────────────┐
│ ⚙️ Settings                                  │
│ Appearance: Moon Theme                      │
│ Background: #222436  •  Purple: #c099ff     │
└─────────────────────────────────────────────┘
```

---

## Accessibility

All Tokyo Night variants maintain WCAG AA compliance:
- **Text Contrast**: Minimum 4.5:1 ratio
- **UI Elements**: Clear distinction between states
- **Status Colors**: Both color and icon indicators
- **Hover Effects**: Visual feedback on all interactive elements

---

## Credits

Based on the excellent [Tokyo Night](https://github.com/folke/tokyonight.nvim) theme by [@folke](https://github.com/folke).

Colors sourced from the official Tokyo Night color palette specifications.

---

## Next Steps

1. ✅ Implement all three variants
2. ⏳ Add theme selector to GUI settings
3. ⏳ Add keyboard shortcut to cycle themes
4. ⏳ Persist theme choice in config file
5. ⏳ Add theme preview in settings

---

**Current Status:** All three Tokyo Night variants are fully implemented and working! Default is set to **Night** variant.

To change the theme, modify the variant in `src/gui_main.rs` (lines 185 and 2364).
