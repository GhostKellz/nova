# Looking Glass Troubleshooting Guide

This guide covers common issues and their solutions when using Looking Glass with Nova.

## Table of Contents

1. [Installation Issues](#installation-issues)
2. [VM Configuration Issues](#vm-configuration-issues)
3. [Windows Guest Issues](#windows-guest-issues)
4. [Client Issues](#client-issues)
5. [Performance Issues](#performance-issues)
6. [Display Issues](#display-issues)
7. [Input Issues](#input-issues)
8. [Audio Issues](#audio-issues)
9. [Diagnostic Tools](#diagnostic-tools)

## Installation Issues

### IOMMU Not Enabled

**Symptoms**:
- Cannot see `/sys/kernel/iommu_groups/`
- Error: "IOMMU not available"

**Solution**:
```bash
# 1. Check if enabled in kernel
dmesg | grep -i iommu

# 2. If not, add to bootloader
# For Intel:
sudo nano /etc/default/grub
GRUB_CMDLINE_LINUX_DEFAULT="intel_iommu=on iommu=pt"

# For AMD:
GRUB_CMDLINE_LINUX_DEFAULT="amd_iommu=on iommu=pt"

# 3. Regenerate GRUB config
sudo grub-mkconfig -o /boot/grub/grub.cfg

# 4. Reboot
sudo reboot
```

### GPU Not in Separate IOMMU Group

**Symptoms**:
```bash
$ ls /sys/kernel/iommu_groups/*/devices
# GPU shares group with other devices
```

**Solution**:
1. **Check if ACS override is needed**:
```bash
# Check current grouping
./scripts/iommu-groups.sh

# If GPU shares group with PCH/other devices, enable ACS override
# Add to kernel parameters (use with caution):
pcie_acs_override=downstream,multifunction
```

2. **Alternative**: Use different PCIe slot if available

### Looking Glass Package Not Found

**Symptoms**:
```bash
$ yay -S looking-glass
error: target not found: looking-glass
```

**Solution**:
```bash
# Update AUR database
yay -Sy

# Install from AUR with explicit source
yay -S looking-glass-git

# Or manually build
git clone https://aur.archlinux.org/looking-glass.git
cd looking-glass
makepkg -si
```

### Permission Denied on Shared Memory

**Symptoms**:
```
Error: Permission denied: /dev/shm/looking-glass
```

**Solution**:
```bash
# Fix permissions
sudo chown libvirt-qemu:kvm /dev/shm/looking-glass
sudo chmod 660 /dev/shm/looking-glass

# Add user to kvm group
sudo usermod -aG kvm $USER

# Make persistent with tmpfiles.d
sudo tee /etc/tmpfiles.d/looking-glass.conf << EOF
f /dev/shm/looking-glass 0660 libvirt-qemu kvm -
EOF

# Create now
sudo systemd-tmpfiles --create /etc/tmpfiles.d/looking-glass.conf

# Log out and back in
```

## VM Configuration Issues

### VM Won't Start After Adding IVSHMEM

**Symptoms**:
```
Error: internal error: process exited while connecting to monitor
```

**Solution**:

1. **Check VM XML syntax**:
```bash
virsh dumpxml my-vm | grep -A 5 shmem
```

Should look like:
```xml
<shmem name='looking-glass'>
  <model type='ivshmem-plain'/>
  <size unit='M'>128</size>
</shmem>
```

2. **Check shared memory exists**:
```bash
ls -l /dev/shm/looking-glass
# Should exist and have correct permissions
```

3. **Check libvirt logs**:
```bash
sudo journalctl -u libvirtd -f
# Start VM and watch for errors
```

### GPU Passthrough Not Working

**Symptoms**:
- Code 43 in Windows Device Manager
- GPU not visible in guest

**Solution**:

1. **Verify GPU is bound to vfio-pci**:
```bash
lspci -nnk -d 10de:  # For NVIDIA
lspci -nnk -d 1002:  # For AMD

# Should show:
# Kernel driver in use: vfio-pci
```

2. **Add vendor ID hiding** (NVIDIA):
```xml
<features>
  <hyperv>
    <vendor_id state='on' value='1234567890ab'/>
  </hyperv>
  <kvm>
    <hidden state='on'/>
  </kvm>
</features>
```

3. **Check ROM file** (if needed):
```xml
<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x0000' bus='0x01' slot='0x00' function='0x0'/>
  </source>
  <rom file='/path/to/vbios.rom'/>
</hostdev>
```

4. **Reset GPU before passthrough**:
```bash
# Use vendor-reset module for AMD
yay -S vendor-reset-dkms-git
```

### Wrong IVSHMEM Size

**Symptoms**:
- Looking Glass client error: "Insufficient shared memory"
- Windows host app error: "Failed to initialize IVSHMEM"

**Solution**:

Calculate required size:
```
Width × Height × 4 × 2 buffers / 1024 / 1024 + 10MB overhead
```

Examples:
- 1080p: 64MB
- 1440p: 128MB
- 4K: 128MB-256MB

Update VM XML:
```xml
<shmem name='looking-glass'>
  <model type='ivshmem-plain'/>
  <size unit='M'>128</size>  <!-- Increase this -->
</shmem>
```

Or with Nova:
```bash
nova looking-glass configure my-vm --framebuffer-size 128
```

## Windows Guest Issues

### IVSHMEM Device Not Detected

**Symptoms**:
- Device Manager shows no "Red Hat PCI Device"
- Looking Glass host app error: "IVSHMEM device not found"

**Solution**:

1. **Verify device in VM XML**:
```bash
virsh dumpxml my-vm | grep -A 3 shmem
```

2. **Check in Windows Device Manager**:
   - Look under "System devices"
   - Check "Hidden devices" view

3. **Manually install driver**:
   - Download from: https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/
   - Extract and point Windows to driver folder

4. **Reinstall Looking Glass host setup**:
   - Uninstall current version
   - Run looking-glass-host-setup.exe as Administrator
   - Choose "Repair" installation

### Looking Glass Host Not Capturing

**Symptoms**:
- Host app shows "Not capturing"
- Client shows "Waiting for host"

**Solution**:

1. **Check host configuration**:
```ini
C:\Program Files\Looking Glass (host)\looking-glass-host.ini

[capture]
interface=dxgi
captureOnStart=yes
```

2. **Try different capture interface**:
```ini
[capture]
interface=auto
tryAllInterfaces=yes
```

3. **Check Windows Event Viewer**:
   - Windows Logs → Application
   - Look for "Looking Glass Host" source

4. **Run as Administrator**:
   - Right-click icon → Run as Administrator

5. **Disable Secure Boot** (if enabled):
   - Some capture methods don't work with Secure Boot

### Code 43 on GPU

**Symptoms**:
- GPU shows Code 43 in Device Manager
- NVIDIA/AMD drivers won't install

**Solution**:

1. **Hide KVM from guest**:
```xml
<features>
  <kvm>
    <hidden state='on'/>
  </kvm>
</features>
```

2. **For NVIDIA**: Add vendor ID spoofing:
```xml
<features>
  <hyperv>
    <vendor_id state='on' value='1234567890ab'/>
  </hyperv>
</features>
```

3. **Use Q35 chipset**:
```xml
<os>
  <type arch='x86_64' machine='q35'>hvm</type>
</os>
```

4. **Patch GPU ROM** (last resort):
```bash
# Extract ROM
cd /sys/bus/pci/devices/0000:01:00.0/
echo 1 > rom
cat rom > /tmp/gpu.rom
echo 0 > rom

# Patch with rom-parser
git clone https://github.com/Matoking/NVIDIA-vBIOS-VFIO-Patcher
python nvidia_vbios_vfio_patcher.py -i /tmp/gpu.rom -o /tmp/gpu_patched.rom

# Use in VM
<rom file='/tmp/gpu_patched.rom'/>
```

## Client Issues

### Client Won't Start

**Symptoms**:
```
looking-glass-client: error while loading shared libraries
```

**Solution**:
```bash
# Reinstall Looking Glass
yay -S looking-glass --rebuild

# Check dependencies
ldd $(which looking-glass-client)

# Install missing libraries
sudo pacman -S libx11 libxcursor libxi libxinerama libxss \
               libxrandr fontconfig freetype2 spice-protocol \
               nettle libsamplerate
```

### "Waiting for Host" Message

**Symptoms**:
- Client shows "Waiting for host" indefinitely
- No video output

**Checklist**:

1. **VM is running**:
```bash
virsh list --all
# State should be "running"
```

2. **Windows fully booted**:
   - Check via SPICE/VNC first

3. **Looking Glass host app running in Windows**:
   - Check system tray
   - Should show green icon

4. **Correct shared memory file**:
```bash
# Client should match VM config
looking-glass-client -f /dev/shm/looking-glass

# Check file exists
ls -l /dev/shm/looking-glass
```

5. **Firewall not blocking** (if using SPICE):
```bash
sudo systemctl status firewalld
# Allow port 5900 if needed
```

### Black Screen

**Symptoms**:
- Client window opens but shows black screen
- Mouse cursor visible but no video

**Solution**:

1. **Check Windows host app log**:
   - Right-click tray icon → Show Log
   - Look for capture errors

2. **Try different renderer**:
```ini
[app]
renderer=opengl  # or egl, or auto
```

Or command line:
```bash
looking-glass-client --opengl
looking-glass-client --egl
```

3. **Check GPU output**:
   - Verify Windows is outputting to passed-through GPU
   - Check Windows display settings

4. **Update GPU drivers** (in Windows)

5. **Restart both host app and client**

### Poor Performance/Stuttering

See [Performance Issues](#performance-issues) section below.

## Performance Issues

### High Latency

**Symptoms**:
- Noticeable mouse lag
- Delayed response to inputs

**Solutions**:

1. **Disable VSync**:
```ini
[opengl]
vsync=no

[egl]
vsync=no
```

2. **Enable JIT rendering**:
```ini
[win]
jitRender=yes
```

3. **Use raw mouse input**:
```ini
[input]
rawMouse=yes
```

4. **Host configuration**:
```ini
[dxgi]
dwmFlush=yes
useAcquireLock=yes

[app]
throttleFPS=0
```

5. **CPU pinning** (see [Performance Tuning](./performance-tuning.md))

### Low FPS

**Symptoms**:
- Frames per second below expected
- Choppy video

**Solutions**:

1. **Check FPS in client**:
   - Press Right Ctrl + I for stats

2. **Disable frame limiting**:
```ini
[app]
throttleFPS=0
```

3. **Use EGL renderer**:
```bash
looking-glass-client --egl
```

4. **Check CPU/GPU usage**:
```bash
htop
nvidia-smi  # or radeontop for AMD
```

5. **Optimize Windows**:
   - Disable Game DVR
   - Disable fullscreen optimizations
   - Set power plan to "High Performance"

### Screen Tearing

**Symptoms**:
- Horizontal line artifacts during motion

**Solutions**:

1. **Enable VSync**:
```ini
[opengl]
vsync=yes
```

2. **Enable double buffering**:
```ini
[egl]
doubleBuffer=yes
```

3. **Disable compositor** (if using X11):
```bash
# KDE
Alt+Shift+F12

# Or permanently
System Settings → Display → Compositor → Disable
```

## Display Issues

### Wrong Resolution

**Symptoms**:
- Display appears stretched or squashed
- Incorrect aspect ratio

**Solutions**:

1. **Check Windows resolution**:
   - Should match your preferred resolution
   - Looking Glass adapts automatically

2. **Keep aspect ratio**:
```ini
[win]
keepAspect=yes
```

3. **Specify window size**:
```ini
[win]
size=1920x1080
```

### Cursor Desync

**Symptoms**:
- Client cursor doesn't match guest cursor position

**Solutions**:

1. **Use absolute mouse mode**:
```ini
[input]
rawMouse=no
```

2. **Calibrate mouse**:
```ini
[input]
mouseSens=1.0  # Adjust if needed
```

3. **Disable Windows pointer acceleration**:
   - Control Panel → Mouse → Pointer Options
   - Uncheck "Enhance pointer precision"

## Input Issues

### Can't Capture Input

**Symptoms**:
- Pressing capture key doesn't work
- Mouse/keyboard not captured

**Solutions**:

1. **Check capture key**:
```ini
[input]
captureKey=KEY_RIGHTCTRL
```

Or try different key:
```bash
looking-glass-client -m KEY_SCROLLLOCK
```

2. **Enable auto-capture**:
```ini
[input]
autoCapture=yes
captureOnFocus=yes
```

3. **Check keyboard permissions**:
```bash
ls -l /dev/input/event*
# User should have access
```

### Input Lag

See [High Latency](#high-latency) above.

### Keys Stuck After Losing Focus

**Solution**:
```ini
[input]
releaseKeysOnFocusLoss=yes
```

## Audio Issues

### No Audio

**Symptoms**:
- No sound from guest
- SPICE audio not working

**Solutions**:

1. **Enable SPICE audio**:
```ini
[spice]
enable=yes
audio=yes
```

```bash
looking-glass-client --spice-audio=yes
```

2. **Check SPICE port**:
```bash
# In VM XML
virsh dumpxml my-vm | grep spice

# Should show:
<graphics type='spice' autoport='yes'>
```

3. **Check PulseAudio/PipeWire**:
```bash
pactl list sinks
# Should show SPICE sink

# Or for PipeWire
pw-cli ls Node
```

4. **Windows audio settings**:
   - Check output device is correct
   - Volume not muted

### Audio Crackling/Stuttering

**Solutions**:

1. **Increase buffer size**:
```ini
[audio]
periods=4
```

2. **Check CPU usage**:
```bash
htop
# If maxed out, see performance tuning
```

3. **Use Scream** as alternative:
```bash
yay -S scream
# Configure in Windows
```

## Diagnostic Tools

### Check System Requirements

```bash
# With Nova
nova looking-glass check

# Manual checks
ls /sys/kernel/iommu_groups/
ls /dev/kvm
virsh version
which looking-glass-client
```

### Get Detailed VM Info

```bash
# VM configuration
virsh dumpxml my-vm > vm-config.xml

# VM state
virsh dominfo my-vm

# VM devices
virsh domblklist my-vm
virsh domiflist my-vm
```

### Monitor Performance

```bash
# CPU/Memory
htop

# GPU
nvidia-smi -l 1  # NVIDIA
radeontop        # AMD

# Disk I/O
iotop

# Network
iftop
```

### Looking Glass Debug Mode

**Client**:
```bash
looking-glass-client --opengl-debug
```

**Host**:
```ini
[app]
debugMode=yes
```

### Collect Logs

**Linux**:
```bash
# Looking Glass client log
~/.local/share/looking-glass/client.log

# Libvirt logs
sudo journalctl -u libvirtd -n 100

# Kernel messages
dmesg | grep -i vfio
dmesg | grep -i iommu
```

**Windows**:
- Event Viewer → Windows Logs → Application
- Filter by "Looking Glass Host"

### Test IVSHMEM

```bash
# Check device in VM
virsh qemu-monitor-command my-vm --hmp info qtree | grep ivshmem

# Check from Windows
# Device Manager → System devices → IVSHMEM
```

## Still Having Issues?

1. **Check Looking Glass Discord**: https://discord.gg/52SMupxkvt
2. **Search GitHub Issues**: https://github.com/gnif/LookingGlass/issues
3. **Arch Wiki**: https://wiki.archlinux.org/title/PCI_passthrough_via_OVMF
4. **Nova Issues**: https://github.com/your-repo/nova/issues

## Quick Reference

| Issue | Quick Fix |
|-------|-----------|
| Black screen | Restart host app + client |
| No input | Check capture key (Right Ctrl) |
| High latency | Disable VSync, enable raw mouse |
| Low FPS | Check throttleFPS=0 |
| No audio | Enable SPICE audio |
| Code 43 | Hide KVM, spoof vendor ID |
| Permission denied | Fix shmem permissions |
| VM won't start | Check XML syntax |

## Next Steps

- **[Performance Tuning](./performance-tuning.md)**: Optimize your setup
- **[Advanced Topics](./advanced.md)**: CPU pinning, huge pages
- **[Configuration](./configuration.md)**: Fine-tune settings
