# Looking Glass Configuration Guide

This guide covers configuration options for both the Looking Glass client and the Windows guest application.

## Terminology Clarification

**IMPORTANT**: The Looking Glass project uses confusing terminology:
- **Linux HOST** = Your main OS (Arch/Fedora/PopOS) where Nova runs
- **Windows GUEST** = The Windows VM running inside Linux
- **Looking Glass Client** = Application that runs on your **Linux HOST**
- **Looking Glass Host Application** = Application that runs in your **Windows GUEST** (confusing name!)

Throughout this guide:
- "Linux host" or "client" = **Your Linux system** (Arch/Fedora/PopOS)
- "Windows guest" or "guest application" = **The Windows VM**

## Table of Contents

1. [Nova Configuration Profiles](#nova-configuration-profiles)
2. [Client Configuration (Linux Host)](#client-configuration-linux-host)
3. [Windows Guest Application Configuration](#windows-guest-application-configuration)
4. [Resolution and Display Settings](#resolution-and-display-settings)
5. [Input Configuration](#input-configuration)
6. [Audio Configuration](#audio-configuration)
7. [Performance Settings](#performance-settings)

## Nova Configuration Profiles

Nova provides pre-configured profiles optimized for different use cases.

### Gaming Profile

Optimized for low-latency gaming:

```bash
nova vm create gaming-vm --looking-glass-profile gaming
```

**Settings**:
- Resolution: 1920x1080
- Framebuffer: 64MB
- Mouse: Relative (raw input)
- VSync: Disabled
- Audio Latency: 10ms

**Best For**: First-person shooters, competitive gaming, rhythm games

### Productivity Profile

Optimized for desktop work:

```bash
nova vm create work-vm --looking-glass-profile productivity
```

**Settings**:
- Resolution: 2560x1440
- Framebuffer: 128MB
- Mouse: Absolute (cursor synchronization)
- VSync: Enabled
- Audio Latency: 20ms

**Best For**: Office work, content creation, general desktop use

### Streaming Profile

Balanced for streaming/recording:

```bash
nova vm create stream-vm --looking-glass-profile streaming
```

**Settings**:
- Resolution: 1920x1080
- Framebuffer: 64MB
- Mouse: Relative
- VSync: Enabled
- Audio Latency: 15ms

**Best For**: Game streaming, video recording, content creation

### Custom Profile

Start with defaults and customize:

```bash
nova vm create custom-vm --looking-glass-profile custom
nova looking-glass configure custom-vm --resolution 3840x2160 --vsync off
```

## Client Configuration (Linux Host)

### Configuration File Location

Create: `~/.config/looking-glass/client.ini`

### Basic Configuration

```ini
[app]
# Shared memory file
shmFile=/dev/shm/looking-glass

# Renderer (opengl, egl, or auto)
renderer=auto

# Capture mode key (KEY_SCROLLLOCK, KEY_RIGHTCTRL, etc.)
captureInputOnly=yes

[win]
# Window size (or use fullScreen=yes)
size=1920x1080
fullScreen=no

# Keep aspect ratio
keepAspect=yes

# Always on top
alwaysOnTop=no

# Borderless window
borderless=no

# Enable JIT rendering
jitRender=yes

[input]
# Mouse mode (absolute or relative/raw)
rawMouse=yes

# Auto capture on focus
autoCapture=yes
captureOnFocus=yes

# Release keys when focus lost
releaseKeysOnFocusLoss=yes

# Hide host cursor
hideCursor=yes

# Mouse sensitivity (1.0 = normal)
mouseSens=1.0

[spice]
# Enable SPICE for clipboard/audio
enable=yes
host=127.0.0.1
port=5900

# Enable audio through SPICE
audio=yes

# Clipboard synchronization
clipboardToVM=yes
clipboardToLocal=yes

[egl]
# EGL-specific settings
vsync=no
doubleBuffer=yes

# Damage tracking
damage=auto

[opengl]
# OpenGL-specific settings
vsync=no

# Mipmap textures
mipmap=yes

# Prevent screen tearing
preventBuffering=no

# NVIDA-specific
nvGainMax=1
nvGain=0
```

### Command-Line Options

Override config file settings:

```bash
looking-glass-client \
  -f /dev/shm/looking-glass \
  -F  # Fullscreen \
  -s  # Disable screensaver \
  --input-rawMouse=yes \
  --opengl-vsync=no \
  --spice-audio=yes \
  -m KEY_RIGHTCTRL  # Capture key
```

### Key Bindings

Default hotkeys:

| Key Combination | Action |
|----------------|--------|
| Right Ctrl | Toggle input capture |
| Right Ctrl + F | Toggle fullscreen |
| Right Ctrl + N | Toggle frame limiting |
| Right Ctrl + V | Toggle video sync |
| Right Ctrl + R | Rotate display |
| Right Ctrl + Q | Quit application |
| Right Ctrl + I | Show FPS/stats |
| Right Ctrl + Z | Alert/activate window |

Custom bindings in config:

```ini
[input]
# Change capture key
captureKey=KEY_SCROLLLOCK

# Disable specific bindings
grabKeyboard=yes
escapeKey=KEY_ESC
```

## Windows Guest Application Configuration

**Note**: Despite the confusing name "Looking Glass Host Application", this runs **inside your Windows GUEST VM**, not on your Linux host!

### Configuration File Location

Inside the Windows VM, create: `C:\Program Files\Looking Glass (host)\looking-glass-host.ini`

### Basic Configuration

```ini
[app]
# Name of shared memory file
shmFile=looking-glass

# Throttle FPS (0 = disabled)
throttleFPS=0

# Exit when not capturing
exitOnGuestShutdown=yes

[os]
# Shared memory size in MB
shmSize=128

[capture]
# Capture interface (dxgi, nvfbc, or auto)
interface=dxgi

# Start capturing immediately
captureOnStart=yes

# Try all interfaces if first fails
tryAllInterfaces=yes

[dxgi]
# Use D3D11 device capture
useD3D11=yes

# DWM flush (reduces latency)
dwmFlush=yes

# Use acquire lock (NVIDIA)
useAcquireLock=yes

# NVIDIA NvFBC capture (if available)
nvfbc=yes

# AMD-specific options
amdHybrid=no

# Target adapter (0 = auto)
adapter=0

# Capture outputs (0 = all)
output=0

[nvfbc]
# NVIDIA-specific options
decouple=yes
diffMap=0
h264Profile=0
tuningInfo=0
```

### Autostart Configuration

To start Looking Glass automatically:

1. Press `Win + R`
2. Type: `shell:startup`
3. Create shortcut to: `C:\Program Files\Looking Glass (host)\looking-glass-host.exe`

Or via Registry:

```batch
reg add "HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v "LookingGlass" /t REG_SZ /d "\"C:\Program Files\Looking Glass (host)\looking-glass-host.exe\"" /f
```

## Resolution and Display Settings

### Supported Resolutions

Looking Glass supports any resolution your GPU can handle:

| Resolution | Aspect Ratio | Framebuffer Size |
|-----------|--------------|------------------|
| 1920x1080 | 16:9 | 64MB |
| 2560x1440 | 16:9 | 128MB |
| 3840x2160 (4K) | 16:9 | 128MB |
| 5120x1440 (Ultrawide) | 21:9 | 256MB |
| 7680x4320 (8K) | 16:9 | 512MB |

### Calculating Framebuffer Size

```
Size (MB) = (Width × Height × 4 bytes × 2 buffers) / 1024 / 1024
```

Round up to nearest power of 2 and add 10MB overhead.

### Changing Resolution

**Windows Guest**:
1. Right-click desktop → Display settings
2. Change resolution
3. Looking Glass will adapt automatically

**Client Config**:
```ini
[win]
size=2560x1440  # Window size (not guest resolution)
```

### Multi-Display Support

Currently, Looking Glass captures one display at a time.

For multi-display:
1. Configure Windows to use one monitor
2. Or use SPICE for secondary displays

## Input Configuration

### Mouse Modes

#### Relative (Raw) Mode

Best for gaming:

```ini
[input]
rawMouse=yes
mouseSens=1.0
```

```bash
looking-glass-client --input-rawMouse=yes
```

**Pros**:
- Lower latency
- Better for gaming
- 1:1 mouse movement

**Cons**:
- Must capture/release input
- No cursor synchronization

#### Absolute Mode

Best for productivity:

```ini
[input]
rawMouse=no
```

**Pros**:
- Seamless cursor movement
- No capture/release needed
- Better for desktop work

**Cons**:
- Slightly higher latency
- Requires guest driver support

### Keyboard Configuration

```ini
[input]
# Grab keyboard exclusively
grabKeyboard=yes

# Escape key
escapeKey=KEY_ESC

# Release keys on focus loss
releaseKeysOnFocusLoss=yes
```

### Controller/Gamepad

Pass through via SPICE or USB passthrough:

```bash
# Via Nova
nova vm attach-usb my-vm --device xbox-controller

# Via libvirt
virsh attach-device my-vm gamepad.xml
```

## Audio Configuration

### Through SPICE

```ini
[spice]
enable=yes
audio=yes
host=127.0.0.1
port=5900

# Audio quality
audioBitrate=128
```

### Through Looking Glass (experimental)

Looking Glass B6+ supports audio directly:

**Windows Guest** (inside the VM):
```ini
[audio]
enabled=yes
periodSize=1024
```

**Linux Host** (your main system):
```ini
[audio]
# PipeWire or PulseAudio
backend=auto

# Buffer periods
periods=2
```

### Through Scream

Alternative: Use Scream for lower latency:

1. Install Scream in Windows
2. Configure to use IVSHMEM
3. Run scream receiver on host

## Performance Settings

### Latency Optimization

**Linux Host** (your main system):
```ini
[app]
# JIT rendering
jitRender=yes

[opengl]
vsync=no
preventBuffering=yes

[egl]
vsync=no
damage=auto
```

**Windows Guest** (inside the VM):
```ini
[dxgi]
dwmFlush=yes
useAcquireLock=yes

[app]
throttleFPS=0
```

### Frame Rate Limiting

**Limit to display refresh rate**:
```ini
[opengl]
vsync=yes
```

**Limit to specific FPS**:
```ini
[app]
throttleFPS=144
```

### Quality vs Performance

**Maximum Quality**:
```ini
[opengl]
mipmap=yes
nvGainMax=1

[egl]
doubleBuffer=yes
```

**Maximum Performance**:
```ini
[opengl]
mipmap=no
preventBuffering=yes

[egl]
doubleBuffer=no
damage=auto
```

## Profile Comparison

| Setting | Gaming | Productivity | Streaming |
|---------|--------|--------------|-----------|
| Resolution | 1080p | 1440p | 1080p |
| VSync | Off | On | On |
| Raw Mouse | Yes | No | Yes |
| Audio Latency | 10ms | 20ms | 15ms |
| JIT Render | Yes | Yes | Yes |
| Buffer | Single | Double | Double |

## Advanced Configuration

### Using KVMFR

If KVMFR module is installed:

```bash
# Load with specific size
sudo modprobe kvmfr static_size_mb=128

# Configure in VM XML
<shmem name='looking-glass' type='ivshmem-plain'>
  <model type='ivshmem-plain'/>
  <size unit='M'>128</size>
  <alias name='shmem0'/>
  <address type='pci' domain='0x0000' bus='0x00' slot='0x01' function='0x0'/>
</shmem>
```

### Capture Interface Selection

**DXGI** (Default, recommended):
- Works on all GPUs
- Good performance
- Windows 8+

**NvFBC** (NVIDIA only):
- Lower latency
- Requires Quadro/Tesla GPU or driver mod
- Best performance

**Auto**:
```ini
[capture]
interface=auto
tryAllInterfaces=yes
```

### Debug Mode

**Linux Host** (your main system):
```bash
looking-glass-client --opengl-debug
```

**Windows Guest** (inside the VM):
```ini
[app]
debugMode=yes
```

Check logs:
- **Windows Guest**: Event Viewer (inside the VM)
- **Linux Host**: `~/.local/share/looking-glass/client.log` (on your main system)

## Next Steps

- **[Performance Tuning](./performance-tuning.md)**: Optimize further
- **[Troubleshooting](./troubleshooting.md)**: Fix issues
- **[Advanced Topics](./advanced.md)**: CPU pinning, huge pages

## References

- Official Configuration: https://looking-glass.io/docs/stable/usage/
- Client Options: https://looking-glass.io/docs/stable/client_options/
- Host Options: https://looking-glass.io/docs/stable/install_host/
