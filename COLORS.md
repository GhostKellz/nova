# Nova Color Scheme

## Theme: Deep Dark Material Blue Ocean

### Primary Palette

#### Background Colors
- **Main Background**: `#00072D` - Deep ocean blue (almost black)
- **Panel Background**: `#0A1B3D` - Slightly lighter ocean blue
- **Secondary Panel**: `#1A2F52` - Dark blue-grey for contrast
- **Elevated Surface**: `#24375F` - For cards/modals
- **Hover Surface**: `#1A43BF` - Interactive hover state (using user's blue)

#### Text Colors
- **Primary Text**: `#89CFF0` - Baby mint blue (main text)
- **Secondary Text**: `#30D5C8` - Turquoise mint (highlights/accents)
- **Accent Text**: `#1A43BF` - Deep blue for labels/headers
- **Bright Accent**: `#30D5C8` - Turquoise mint for interactive elements

#### Status Colors
- **Running/Success**: `#50FA7B` - Bright green
- **Stopped/Error**: `#FF5555` - Soft red
- **Warning/Pending**: `#F1FA8C` - Yellow
- **Suspended/Paused**: `#8BE9FD` - Cyan blue
- **Unknown/Disabled**: `#6272A4` - Grey-blue

#### Action Colors
- **Primary Button**: `#30D5C8` - Turquoise mint
- **Primary Hover**: `#1A43BF` - Deep blue hover
- **Secondary Button**: `#1A2F52` - Muted ocean blue
- **Danger Button**: `#FF5555` - Red for destructive actions
- **Success Button**: `#50FA7B` - Green for positive actions

#### Border & Divider Colors
- **Border Default**: `#24375F` - Subtle blue border
- **Border Focus**: `#30D5C8` - Turquoise for focus states
- **Divider**: `#1A2F52` - Very subtle divider

### Component Specific

#### Tree View (VM/Container List)
- **Tree Background**: `#00072D`
- **Selected Item**: `#1A43BF` with `#89CFF0` text
- **Hover Item**: `#0A1B3D`
- **Group Header**: `#30D5C8` text

#### Properties Panel
- **Property Label**: `#1A43BF` (deep blue for labels)
- **Property Value**: `#89CFF0` (baby mint blue)
- **Section Header**: `#30D5C8` (turquoise mint)

#### Console/Terminal
- **Console Background**: `#000510` (deeper black-blue)
- **Console Text**: `#89CFF0`
- **Console Error**: `#FF5555`
- **Console Success**: `#50FA7B`
- **Console Warning**: `#F1FA8C`

#### Graphs & Charts
- **CPU Usage**: `#30D5C8` (turquoise)
- **Memory Usage**: `#89CFF0` (baby mint blue)
- **Disk I/O**: `#1A43BF` (deep blue)
- **Network**: `#50FA7B` (green)

### Implementation Notes

The theme is inspired by:
- Deep ocean aesthetics (dark blues)
- Material Design principles
- Mint/teal accent colors for a fresh, modern look
- High contrast for readability
- Dracula color scheme influence for status colors

### Accessibility
- All text colors maintain WCAG AA contrast ratios against their backgrounds
- Status colors are distinguishable for color-blind users
- Focus states have clear visual indicators

### Usage Example

```rust
// Background hierarchy
const BG_MAIN: Color32 = Color32::from_rgb(0, 7, 45);          // #00072D
const BG_PANEL: Color32 = Color32::from_rgb(10, 27, 61);       // #0A1B3D
const BG_SECONDARY: Color32 = Color32::from_rgb(26, 47, 82);   // #1A2F52

// Text colors
const TEXT_PRIMARY: Color32 = Color32::from_rgb(137, 207, 240);   // #89CFF0 - Baby mint blue
const TEXT_SECONDARY: Color32 = Color32::from_rgb(48, 213, 200);  // #30D5C8 - Turquoise mint
const TEXT_ACCENT: Color32 = Color32::from_rgb(26, 67, 191);      // #1A43BF - Deep blue

// Status colors
const STATUS_RUNNING: Color32 = Color32::from_rgb(80, 250, 123);  // #50FA7B - Green
const STATUS_STOPPED: Color32 = Color32::from_rgb(255, 85, 85);   // #FF5555 - Red
const STATUS_WARNING: Color32 = Color32::from_rgb(241, 250, 140); // #F1FA8C - Yellow
```