# Wayland Quick Start Guide

Nova is optimized for Wayland-based desktop environments. This guide will help you get started quickly.

## ✅ Supported Desktop Environments

Nova has been optimized and tested for:

- **KDE Plasma (Wayland)** - Arch Linux, openSUSE, Fedora
- **GNOME (Wayland)** - Fedora, Ubuntu 22.04+, Pop!_OS
- **Cosmic Desktop (Beta)** - Pop!_OS 24.04+

## 🚀 Quick Start

### 1. Verify You're Running Wayland

```bash
echo $XDG_SESSION_TYPE
# Should output: wayland
```

If it shows `x11`, you're running X11. To switch to Wayland:

**KDE Plasma:**
- Log out
- At login screen, click session selector
- Choose "Plasma (Wayland)"
- Log in

**GNOME:**
- Log out
- Click username at login
- Click gear icon ⚙️
- Select "GNOME" or "GNOME on Wayland"
- Log in

**Cosmic:**
- Cosmic uses Wayland by default

### 2. Install Nova

```bash
# Arch Linux
yay -S nova  # or build from source

# Fedora
sudo dnf copr enable username/nova
sudo dnf install nova

# Pop!_OS / Ubuntu
# Build from source (see main README)
```

### 3. Launch Nova

```bash
nova gui
```

That's it! Nova will automatically detect Wayland and apply optimizations.

## 🎯 What's Optimized?

Nova automatically configures:

✅ **Hardware Acceleration** - Uses wgpu backend for optimal GPU utilization
✅ **High DPI Support** - Native fractional scaling on Wayland
✅ **Window Decorations** - KDE server-side, GNOME/Cosmic client-side
✅ **Frame Pacing** - Smooth 60 FPS rendering with compositor sync
✅ **Smooth Edges** - Anti-aliased UI elements for crisp text
✅ **Multi-Monitor** - Better multi-display handling

## 🔧 Optional: Force Wayland Backend

If Nova falls back to X11 (XWayland), force Wayland:

```bash
export WINIT_UNIX_BACKEND=wayland
nova gui
```

Add to your shell profile for persistence:

```bash
echo 'export WINIT_UNIX_BACKEND=wayland' >> ~/.bashrc
```

## 🎨 Desktop Environment Integration

### KDE Plasma Features

- **Window Rules**: Assign Nova to specific virtual desktops
- **System Tray**: Minimize to tray (coming soon)
- **KWin Integration**: Server-side decorations match Breeze theme
- **Activities**: Nova respects Plasma Activities

### GNOME Features

- **GTK Theme**: Nova integrates with GTK theme colors
- **Notifications**: Desktop notifications for VM events (coming soon)
- **Portals**: File picker uses GNOME file chooser
- **Multi-Monitor**: Excellent support for multiple displays

### Cosmic Features

- **Tiling**: Nova works well with Cosmic's tiling compositor
- **Rust Integration**: Both Cosmic and Nova are Rust-based
- **Modern Rendering**: Optimized for Cosmic's rendering pipeline
- **Pop!_OS Integration**: Native integration with Pop!_OS features

## 📊 Performance Tips

1. **Check GPU Acceleration**:
   ```bash
   nova gpu info
   vulkaninfo | grep deviceName
   ```

2. **Enable Vulkan** (recommended for best performance):
   ```bash
   # Install Vulkan support
   # Arch:
   sudo pacman -S vulkan-radeon  # AMD
   sudo pacman -S nvidia-utils vulkan-icd-loader  # NVIDIA

   # Fedora:
   sudo dnf install mesa-vulkan-drivers  # AMD
   sudo dnf install vulkan  # NVIDIA
   ```

3. **Multi-GPU Systems** (force discrete GPU):
   ```bash
   DRI_PRIME=1 nova gui
   ```

## ❓ Troubleshooting

### Issue: App uses X11 instead of Wayland

**Solution:**
```bash
export WINIT_UNIX_BACKEND=wayland
nova gui
```

### Issue: Blurry text on high DPI

**Solution:** Your compositor should handle scaling. Verify:
```bash
echo $WINIT_HIDPI_FACTOR  # Should be unset (let compositor handle it)
```

### Issue: Window decorations missing

**Solution:** Nova should auto-detect, but you can verify environment:
```bash
echo $XDG_CURRENT_DESKTOP
echo $XDG_SESSION_DESKTOP
```

### Issue: Laggy performance

**Solution:** Check GPU acceleration:
```bash
glxinfo | grep "OpenGL renderer"
# Should show your GPU name, not "llvmpipe" (software rendering)
```

## 📖 Full Documentation

For detailed information, see:

- [Wayland Integration Guide](integration.md) - complete technical documentation
- [Main README](../README.md) - General Nova documentation
- [GUI Theme Notes](../gui/overview.md) - theme customization

## 🐛 Report Issues

Found a Wayland-specific issue?

1. Collect system info:
   ```bash
   uname -a
   echo $XDG_SESSION_TYPE
   echo $XDG_CURRENT_DESKTOP
   nova --version
   ```

2. Report at: https://github.com/your-repo/nova/issues

## 🎉 Enjoy!

Nova on Wayland provides the smoothest, most modern virtualization management experience on Linux. Enjoy tear-free rendering, better security, and native high DPI support!

---

**Next Steps:**
- Create your first VM: `nova vm create my-vm --os ubuntu --cpu 4 --ram 8192`
- Explore the GUI: `nova gui`
- Read the full docs: `docs/wayland/integration.md`
