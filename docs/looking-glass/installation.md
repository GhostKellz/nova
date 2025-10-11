# Looking Glass Installation Guide (Arch Linux)

This guide walks you through installing and configuring Looking Glass on Arch Linux.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Enable IOMMU](#enable-iommu)
3. [Install Looking Glass](#install-looking-glass)
4. [Configure Shared Memory](#configure-shared-memory)
5. [Configure VM](#configure-vm)
6. [Install Windows Guest Components](#install-windows-guest-components)
7. [Verify Installation](#verify-installation)

## Prerequisites

### Hardware Requirements

1. **Check CPU Virtualization Support**:
```bash
# Check for Intel VT-x or AMD-V
lscpu | grep -E 'vmx|svm'

# Should show 'vmx' for Intel or 'svm' for AMD
```

2. **Check IOMMU Support**:
```bash
# Check kernel command line
cat /proc/cmdline | grep iommu

# List IOMMU groups
find /sys/kernel/iommu_groups/ -type l
```

3. **Identify Your GPUs**:
```bash
# List all GPUs
lspci | grep -E 'VGA|3D'

# Detailed info
lspci -vnn | grep -A 12 VGA
```

You need at least **two GPUs**:
- **Primary GPU**: For host (can be integrated)
- **Secondary GPU**: For guest passthrough (dedicated)

## Enable IOMMU

### 1. Enable in BIOS/UEFI

Boot into BIOS and enable:
- **Intel**: VT-d (Virtualization Technology for Directed I/O)
- **AMD**: AMD-Vi or IOMMU

### 2. Enable in Kernel

Edit your bootloader configuration:

#### For GRUB:
```bash
sudo nano /etc/default/grub

# For Intel CPUs:
GRUB_CMDLINE_LINUX_DEFAULT="intel_iommu=on iommu=pt"

# For AMD CPUs:
GRUB_CMDLINE_LINUX_DEFAULT="amd_iommu=on iommu=pt"

# Regenerate GRUB config
sudo grub-mkconfig -o /boot/grub/grub.cfg
```

#### For systemd-boot:
```bash
sudo nano /boot/loader/entries/arch.conf

# Add to options line (Intel):
options ... intel_iommu=on iommu=pt

# Or for AMD:
options ... amd_iommu=on iommu=pt
```

### 3. Reboot and Verify

```bash
sudo reboot

# After reboot, verify IOMMU is enabled:
dmesg | grep -i iommu

# Should see messages about IOMMU being enabled
```

## Install Looking Glass

### 1. Install AUR Helper (if not installed)

```bash
# Install yay
sudo pacman -S --needed git base-devel
git clone https://aur.archlinux.org/yay.git
cd yay
makepkg -si
cd .. && rm -rf yay
```

### 2. Install Looking Glass Packages

```bash
# Install Looking Glass client
yay -S looking-glass

# Install KVMFR kernel module (optional but recommended)
yay -S looking-glass-module-dkms

# Install libvirt/QEMU if not already installed
sudo pacman -S qemu-full libvirt virt-manager edk2-ovmf
```

### 3. Enable Libvirt Service

```bash
sudo systemctl enable libvirtd
sudo systemctl start libvirtd

# Add user to libvirt group
sudo usermod -aG libvirt $USER
sudo usermod -aG kvm $USER

# Log out and back in for groups to take effect
```

## Configure Shared Memory

### 1. Load KVMFR Module (if installed)

```bash
# Load module
sudo modprobe kvmfr static_size_mb=128

# Make it load on boot
echo 'kvmfr' | sudo tee /etc/modules-load.d/kvmfr.conf

# Configure module options
echo 'options kvmfr static_size_mb=128' | sudo tee /etc/modprobe.d/kvmfr.conf
```

Size recommendations:
- **64MB**: 1920x1080 (1080p)
- **128MB**: 2560x1440 (1440p) or 3840x2160 (4K)
- **256MB**: Multi-display or 4K with high refresh

### 2. Setup udev Rules

```bash
# Create udev rule for KVMFR device
sudo tee /etc/udev/rules.d/99-kvmfr.rules << EOF
SUBSYSTEM=="kvmfr", OWNER="libvirt-qemu", GROUP="kvm", MODE="0660"
EOF

# If using /dev/shm instead (no KVMFR module):
sudo tee /etc/tmpfiles.d/looking-glass.conf << EOF
# Type Path               Mode UID          GID         Age Argument
f /dev/shm/looking-glass 0660 libvirt-qemu kvm         -
EOF

# Reload udev rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### 3. Create Shared Memory File (if not using KVMFR)

```bash
# Create shared memory file
sudo touch /dev/shm/looking-glass
sudo chown libvirt-qemu:kvm /dev/shm/looking-glass
sudo chmod 660 /dev/shm/looking-glass
```

## Configure VM

### Option 1: Using Nova (Recommended)

```bash
# Check system requirements
nova looking-glass check

# Setup Looking Glass for VM
nova vm create win11-gaming \
  --os windows11 \
  --memory 16G \
  --cpus 8 \
  --disk 100G \
  --gpu 0000:01:00.0 \
  --looking-glass \
  --lg-profile gaming

# Or configure existing VM
nova looking-glass setup my-vm --gpu 0000:01:00.0 --profile gaming
```

### Option 2: Manual Libvirt Configuration

1. **Bind GPU to VFIO Driver**:

```bash
# Find GPU IDs
lspci -nn | grep -E 'VGA|3D'
# Example output: 01:00.0 VGA compatible controller [0300]: NVIDIA Corporation ... [10de:1b80]

# Create vfio config
sudo tee /etc/modprobe.d/vfio.conf << EOF
options vfio-pci ids=10de:1b80,10de:10f0
EOF

# Regenerate initramfs
sudo mkinitcpio -P
sudo reboot
```

2. **Edit VM XML**:

```bash
virsh edit your-vm-name
```

Add IVSHMEM device:

```xml
<domain type='kvm'>
  <!-- ... existing config ... -->

  <devices>
    <!-- Looking Glass IVSHMEM Device -->
    <shmem name='looking-glass'>
      <model type='ivshmem-plain'/>
      <size unit='M'>128</size>
    </shmem>

    <!-- GPU Passthrough -->
    <hostdev mode='subsystem' type='pci' managed='yes'>
      <source>
        <address domain='0x0000' bus='0x01' slot='0x00' function='0x0'/>
      </source>
    </hostdev>

    <!-- Hide KVM from guest -->
    <features>
      <kvm>
        <hidden state='on'/>
      </kvm>
    </features>

    <!-- Use video type 'none' to disable emulated video -->
    <video>
      <model type='none'/>
    </video>
  </devices>
</domain>
```

## Install Windows Guest Components

### 1. Install Windows in VM

Start the VM and install Windows normally:

```bash
# Using Nova
nova vm start win11-gaming

# Using virsh
virsh start your-vm-name
```

### 2. Install GPU Drivers

1. Boot into Windows
2. Install latest drivers:
   - **NVIDIA**: Download from nvidia.com
   - **AMD**: Download from amd.com

### 3. Download Looking Glass Host Application

Visit: https://looking-glass.io/downloads

Download: `looking-glass-host-setup.exe`

### 4. Install Looking Glass Host

1. Run `looking-glass-host-setup.exe` **as Administrator**
2. Follow installation wizard
3. IVSHMEM driver will be installed automatically

### 5. Configure Looking Glass Host

Create: `C:\Program Files\Looking Glass (host)\looking-glass-host.ini`

```ini
[app]
shmFile=looking-glass
throttleFPS=0

[os]
shmSize=128

[capture]
interface=dxgi
captureOnStart=true

[dxgi]
; For NVIDIA GPUs - enable NvFBC if available
nvfbc=true
dwmFlush=true
useAcquireLock=true

; For AMD GPUs
amdHybrid=no
```

### 6. Start Looking Glass Host

Run "Looking Glass (host)" from Start Menu.

The application will:
- Minimize to system tray
- Show a green icon when capturing
- Display any errors in Event Viewer

## Verify Installation

### 1. Check IVSHMEM Device (Windows)

Open Device Manager and verify:
- Look under "System devices"
- Find "IVSHMEM" or "Red Hat PCI Device"
- Status should be "Working properly"

### 2. Check Host Application (Windows)

- System tray icon should be green
- Right-click → "Show Log" to see capture status
- Should show: "Capturing at [resolution] @ [fps] FPS"

### 3. Launch Looking Glass Client (Linux Host)

```bash
# Using Nova
nova looking-glass client win11-gaming

# Or directly
looking-glass-client -f /dev/shm/looking-glass

# With specific options
looking-glass-client \
  -f /dev/shm/looking-glass \
  -p 5900 \
  -m KEY_RIGHTCTRL \
  --opengl-vsync \
  --input-rawMouse
```

### 4. Test Input

1. Press **Right Ctrl** (or configured key) to capture mouse/keyboard
2. Mouse should move inside VM
3. Press **Right Ctrl** again to release

## Troubleshooting Common Installation Issues

### Issue: "IOMMU not enabled"

**Solution**:
1. Check BIOS settings
2. Verify kernel parameters: `cat /proc/cmdline`
3. Check dmesg: `dmesg | grep -i iommu`

### Issue: "No IVSHMEM device in Windows"

**Solution**:
1. Verify VM XML has `<shmem>` device
2. Check libvirt logs: `sudo journalctl -u libvirtd`
3. Reinstall Looking Glass host setup

### Issue: "Permission denied on /dev/shm/looking-glass"

**Solution**:
```bash
sudo chown libvirt-qemu:kvm /dev/shm/looking-glass
sudo chmod 660 /dev/shm/looking-glass

# Add your user to kvm group
sudo usermod -aG kvm $USER
```

### Issue: "GPU not detected by Windows"

**Solution**:
1. Verify GPU is bound to vfio-pci: `lspci -k`
2. Check VM XML has correct GPU address
3. Install latest GPU drivers in Windows

### Issue: "Black screen / No output"

**Solution**:
1. Ensure Windows booted fully (use VNC/SPICE to verify)
2. Check Looking Glass host app is running
3. Try different capture interface in looking-glass-host.ini
4. Check Windows Event Viewer for errors

## Performance Optimization

After installation, see [Performance Tuning Guide](./performance-tuning.md) for:
- CPU pinning and isolation
- Huge pages configuration
- GPU-specific optimizations
- Latency reduction techniques

## Next Steps

1. **[Configuration Guide](./configuration.md)**: Customize your setup
2. **[Performance Tuning](./performance-tuning.md)**: Optimize for best performance
3. **[Troubleshooting](./troubleshooting.md)**: Fix common issues

## Additional Resources

- Official Installation Guide: https://looking-glass.io/docs/stable/install/
- Arch Wiki: https://wiki.archlinux.org/title/PCI_passthrough_via_OVMF
- Looking Glass Discord: https://discord.gg/52SMupxkvt
