# Nova GUI Redesign - Hyper-V Manager Inspired

## Vision

Create a modern, user-friendly GUI that combines the best aspects of Windows 11 Hyper-V Manager with an Arch Linux aesthetic. Focus on clarity, efficiency, and visual appeal.

## Hyper-V Manager Analysis

### What Makes Hyper-V Manager Great

1. **Three-Pane Layout**:
   - Left: Navigation (Actions/Tools)
   - Center: VM List with status
   - Right: Action pane (context-sensitive)

2. **Visual Clarity**:
   - Clear status indicators (Running = Green, Stopped = Red)
   - Large, readable buttons
   - Grouped actions
   - Consistent spacing

3. **Quick Actions**:
   - New VM Wizard
   - Import/Export
   - Virtual Switch Manager
   - VM Settings
   - Connect (to console)

4. **Information Density**:
   - VM name, state, CPU usage, memory, uptime
   - Snapshot count
   - Network adapters

5. **Contextual Actions**:
   - Actions change based on VM state
   - Right-click context menus
   - Action pane updates per selection

## Nova GUI Design

### Layout Structure

```
┌──────────────────────────────────────────────────────────────┐
│ ☰ Nova Manager                    [🔄] [⚙️] [?] [Minimize] ├─┘
├──────────────────────────────────────────────────────────────┤
│ 🏠 Dashboard  | 💻 Virtual Machines  | 🌐 Networks  | 🔧 Tools│
├──────────────────────────────────────────────────────────────┤
│                                                                │
│  ┌────────────────────────────────────────────────────┐       │
│  │ 📊 SYSTEM OVERVIEW                                 │       │
│  ├────────────────────────────────────────────────────┤       │
│  │ ● VMs: 5 Running, 3 Stopped    [+New] [Import]   │       │
│  │ ● Networks: 3 Switches Active                     │       │
│  │ ● Resources: CPU 45%, RAM 32GB/64GB               │       │
│  └────────────────────────────────────────────────────┘       │
│                                                                │
│  ┌────────────────────────────────────────────────────┐       │
│  │ 💻 VIRTUAL MACHINES                    [Search 🔍]│       │
│  ├────────────────────────────────────────────────────┤       │
│  │ ● win11-gaming       Running  ↗45%  16GB  [Actions▼]│     │
│  │ ○ ubuntu-dev         Stopped  --    8GB   [Actions▼]│     │
│  │ ● arch-test          Running  ↗12%  4GB   [Actions▼]│     │
│  └────────────────────────────────────────────────────┘       │
│                                                                │
│  [Selected: win11-gaming]                                     │
│  ┌────────────────────────────────────────────────────┐       │
│  │ Overview | Snapshots | Network | Performance | ... │       │
│  ├────────────────────────────────────────────────────┤       │
│  │ 📋 Details                                         │       │
│  │   • Status: Running for 2h 34m                    │       │
│  │   • CPU: 8 cores (45% usage)                      │       │
│  │   • Memory: 16GB                                  │       │
│  │   • GPU: NVIDIA RTX 3080 (Passthrough)           │       │
│  │   • Network: br0 (192.168.1.50)                  │       │
│  │                                                    │       │
│  │ [🚀 Connect] [⏸️ Pause] [🔄 Restart] [⏹️ Stop]     │       │
│  └────────────────────────────────────────────────────┘       │
└──────────────────────────────────────────────────────────────┘
```

### Color Scheme (Tokyo Night Theme)

```rust
// Primary colors
const TN_BG: Color32 = Color32::from_rgb(26, 27, 38);         // #1a1b26 - Deep night blue
const TN_BG_LIGHT: Color32 = Color32::from_rgb(36, 40, 59);   // #24283b - Lighter panel
const TN_BG_HIGHLIGHT: Color32 = Color32::from_rgb(47, 52, 78); // #2f344e - Hover state
const TN_FG: Color32 = Color32::from_rgb(192, 202, 245);      // #c0caf5 - Light text

// Accent colors
const TN_BLUE: Color32 = Color32::from_rgb(122, 162, 247);    // #7aa2f7 - Bright blue
const TN_TEAL: Color32 = Color32::from_rgb(125, 207, 255);    // #7dcfff - Cyan/teal
const TN_MINT: Color32 = Color32::from_rgb(158, 206, 106);    // #9ece6a - Mint green
const TN_PURPLE: Color32 = Color32::from_rgb(187, 154, 247);  // #bb9af7 - Purple
const TN_RED: Color32 = Color32::from_rgb(247, 118, 142);     // #f7768e - Soft red
const TN_ORANGE: Color32 = Color32::from_rgb(255, 158, 100);  // #ff9e64 - Orange
const TN_YELLOW: Color32 = Color32::from_rgb(224, 175, 104);  // #e0af68 - Yellow

// Status colors
const STATUS_RUNNING: Color32 = TN_MINT;      // Mint green for running
const STATUS_STOPPED: Color32 = Color32::from_rgb(68, 71, 90);  // #44475a - Muted
const STATUS_ERROR: Color32 = TN_RED;         // Red for errors
const STATUS_PAUSED: Color32 = TN_ORANGE;     // Orange for paused
const STATUS_STARTING: Color32 = TN_TEAL;     // Teal for transitional

// UI Element colors
const BUTTON_PRIMARY: Color32 = TN_BLUE;      // Primary actions
const BUTTON_SUCCESS: Color32 = TN_MINT;      // Success/start actions
const BUTTON_DANGER: Color32 = TN_RED;        // Destructive actions
const BUTTON_SECONDARY: Color32 = TN_PURPLE;  // Secondary actions
const ACCENT_HIGHLIGHT: Color32 = TN_TEAL;    // Highlights, links, selections
```

## Key Features to Implement

### 1. Dashboard Page (New)

**Purpose**: System overview at a glance

**Elements**:
- System resource summary (CPU, RAM, Storage)
- VM quick stats (Total, Running, Stopped)
- Recent activity feed
- Quick action buttons
- System alerts/warnings

### 2. Enhanced VM List

**Current**:
```
[Name]        [Type]  [Status]
ubuntu-test   VM      Running
```

**Improved**:
```
┌─────────────────────────────────────────────────────┐
│ ● ubuntu-test                    [Start▼] [Connect] │
│   VM  •  Running for 1h 23m                         │
│   ↗ CPU: 23%  •  RAM: 4.2/8GB  •  📸 3 snapshots   │
└─────────────────────────────────────────────────────┘
```

### 3. VM Creation Wizard

**Step-by-step process**:
1. **Name and Location**: VM name, description, location
2. **Generation**: BIOS vs UEFI, Secure Boot
3. **Memory**: RAM allocation, dynamic memory
4. **Networking**: Select virtual switch
5. **Storage**: Create VHD or use existing
6. **Install Options**: ISO, network, template
7. **Summary**: Review and create

### 4. Quick Action Toolbar

```
[+ New VM] [📥 Import] [📤 Export] [🔄 Refresh] [🌐 Networks] [🔧 Settings]
```

### 5. Status Indicators

**Visual States**:
- ● Green dot = Running
- ○ Gray dot = Stopped
- ⏸ Orange dot = Paused
- ⚠ Red dot = Error
- 🔄 Blue spinner = Starting/Stopping

### 6. Resource Monitoring

**Real-time graphs** (using egui_plot):
- CPU usage over time
- Memory usage
- Network throughput
- Disk I/O

### 7. Context-Sensitive Actions

**When VM is selected**:
- Start/Stop/Pause/Reset
- Connect (console)
- Settings
- Snapshots
- Clone
- Export

**Actions disabled when not applicable**:
- Can't start running VM
- Can't connect to stopped VM

## Implementation Plan

### Phase 1: Foundation (Week 1)
- [ ] Add MainView enum (Dashboard, VMs, Networks, Tools)
- [ ] Implement tabbed navigation bar
- [ ] Create dashboard page structure
- [ ] Add color constants (Nord theme)

### Phase 2: Visual Polish (Week 1-2)
- [ ] Improve VM list cards with status dots
- [ ] Add hover effects and animations
- [ ] Implement better spacing and margins
- [ ] Add icons throughout

### Phase 3: VM Wizard (Week 2)
- [ ] Create wizard state machine
- [ ] Implement 7-step wizard UI
- [ ] Add validation per step
- [ ] Connect to VM creation backend

### Phase 4: Dashboard (Week 2-3)
- [ ] System resource monitoring
- [ ] VM statistics cards
- [ ] Quick action buttons
- [ ] Activity feed

### Phase 5: Resource Monitoring (Week 3)
- [ ] Integrate egui_plot
- [ ] CPU usage graphs
- [ ] Memory usage graphs
- [ ] Network throughput graphs

### Phase 6: Polish and Testing (Week 3-4)
- [ ] Add keyboard shortcuts
- [ ] Implement context menus
- [ ] Add tooltips everywhere
- [ ] Performance optimization
- [ ] User testing

## Technical Considerations

### Dependencies to Add

```toml
[dependencies]
egui_plot = "0.24"         # For graphs
egui_extras = "0.24"       # Extra widgets
image = "0.24"             # For icons
```

### Performance

- Use lazy loading for large VM lists
- Cache resource metrics
- Debounce UI updates
- Use Arc/Mutex efficiently

### Accessibility

- Keyboard navigation
- Screen reader support (labels)
- High contrast mode option
- Scalable UI (respect DPI)

## Inspiration Gallery

### Hyper-V Manager (Windows 11)
- Clean three-pane layout
- Large action buttons
- Clear status indicators

### GNOME Boxes
- Modern card-based UI
- Simple creation wizard
- Smooth animations

### Proxmox VE
- Comprehensive feature set
- Tree-view navigation
- Real-time monitoring

### VMware Workstation
- Tabbed interface
- Library view
- Visual VM thumbnails

## Success Metrics

1. **Usability**: New user can create VM in < 3 minutes
2. **Discoverability**: All features accessible within 2 clicks
3. **Performance**: UI remains responsive with 50+ VMs
4. **Visual Appeal**: Modern, cohesive design language
5. **Efficiency**: Power users can work faster with shortcuts

## Future Enhancements

- VM thumbnails/screenshots
- Drag-and-drop file sharing
- Built-in VNC/SPICE client
- Template gallery
- Cloud integration
- Backup manager
- Multi-server management

---

## Next Steps

1. Get feedback on design
2. Create mockups/wireframes
3. Start Phase 1 implementation
4. Iterate based on testing
