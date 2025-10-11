# Looking Glass Advanced Topics

This guide covers advanced configuration topics for power users.

## Table of Contents

1. [KVMFR Kernel Module](#kvmfr-kernel-module)
2. [CPU Pinning and Isolation](#cpu-pinning-and-isolation)
3. [Huge Pages Deep Dive](#huge-pages-deep-dive)
4. [Multi-Display Setup](#multi-display-setup)
5. [USB Passthrough](#usb-passthrough)
6. [Audio Optimization](#audio-optimization)
7. [Custom VBIOS](#custom-vbios)
8. [Dual Boot Migration](#dual-boot-migration)
9. [Security Considerations](#security-considerations)
10. [Automation Scripts](#automation-scripts)

## KVMFR Kernel Module

KVMFR (Kernel Virtual Machine FrameBuffer Relay) provides optimized shared memory access.

### Installation

```bash
# Install from AUR
yay -S looking-glass-module-dkms

# Verify installation
modprobe kvmfr
lsmod | grep kvmfr
```

### Configuration

```bash
# Configure module parameters
sudo tee /etc/modprobe.d/kvmfr.conf << EOF
options kvmfr static_size_mb=128 static_size_mb_0=128
EOF

# Load at boot
echo 'kvmfr' | sudo tee /etc/modules-load.d/kvmfr.conf

# Setup udev rules
sudo tee /etc/udev/rules.d/99-kvmfr.rules << EOF
SUBSYSTEM=="kvmfr", OWNER="libvirt-qemu", GROUP="kvm", MODE="0660"
SUBSYSTEM=="kvmfr", TAG+="uaccess"
EOF

# Reload udev
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### VM XML Configuration

```xml
<shmem name='looking-glass'>
  <model type='ivshmem-plain'/>
  <size unit='M'>128</size>
  <alias name='shmem0'/>
  <address type='pci' domain='0x0000' bus='0x00' slot='0x01' function='0x0'/>
</shmem>
```

### Benefits

- **Lower Latency**: Direct kernel access
- **Better Performance**: Optimized memory operations
- **Reliability**: More stable than tmpfs

## CPU Pinning and Isolation

### Understanding CPU Topology

```bash
# View topology
lscpu -e

# Recommended: Use physical cores only or core+sibling pairs
```

### Advanced Pinning Strategy

```xml
<domain type='kvm'>
  <vcpu placement='static'>6</vcpu>
  <cputune>
    <!-- VM vCPUs → Host physical cores -->
    <vcpupin vcpu='0' cpuset='2'/>
    <vcpupin vcpu='1' cpuset='3'/>
    <vcpupin vcpu='2' cpuset='4'/>
    <vcpupin vcpu='3' cpuset='5'/>
    <vcpupin vcpu='4' cpuset='6'/>
    <vcpupin vcpu='5' cpuset='7'/>

    <!-- Emulator threads → Separate cores -->
    <emulatorpin cpuset='0,1'/>

    <!-- I/O threads → Dedicated cores -->
    <iothreadpin iothread='1' cpuset='0'/>
    <iothreadpin iothread='2' cpuset='1'/>
  </cputune>

  <cpu mode='host-passthrough' check='none' migratable='off'>
    <topology sockets='1' cores='6' threads='1'/>
    <cache mode='passthrough'/>
    <feature policy='require' name='topoext'/>
  </cpu>
</domain>
```

### CPU Isolation

Complete isolation for maximum performance:

```bash
# Edit GRUB
sudo nano /etc/default/grub

GRUB_CMDLINE_LINUX_DEFAULT="isolcpus=2-7 nohz_full=2-7 rcu_nocbs=2-7"

# Update GRUB
sudo grub-mkconfig -o /boot/grub/grub.cfg
sudo reboot
```

## Huge Pages Deep Dive

### Types of Huge Pages

| Type | Size | Use Case |
|------|------|----------|
| Regular | 4KB | Default |
| Huge | 2MB | Recommended |
| Gigantic | 1GB | Large VMs (64GB+) |

### Calculating Requirements

```bash
# For 16GB VM with 2MB pages:
# 16GB = 16384 MB
# Pages needed = 16384 / 2 = 8192
# Add 10% overhead = 8192 * 1.1 = 9011 pages
```

### Configuration

```bash
# Static allocation (recommended)
echo "vm.nr_hugepages = 9075" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p

# VM configuration
<memoryBacking>
  <hugepages/>
  <locked/>
</memoryBacking>
```

## Multi-Display Setup

### Limitations

Currently, Looking Glass captures **one display** at a time.

### Workarounds

1. **Multiple Instances**: Run multiple Looking Glass clients
2. **Hybrid Approach**: Primary display via Looking Glass, secondary via SPICE
3. **Virtual Display**: Use virtual display software in Windows

## USB Passthrough

### Individual USB Devices

```bash
# Find device
lsusb

# Create XML and attach
cat > gamepad.xml << EOF
<hostdev mode='subsystem' type='usb'>
  <source>
    <vendor id='0x046d'/>
    <product id='0xc52b'/>
  </source>
</hostdev>
EOF

virsh attach-device my-vm gamepad.xml
```

### USB Controller Passthrough

Pass entire USB controller for all connected devices:

```bash
# Find controller
lspci | grep USB

# Add to VM XML
<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x0000' bus='0x00' slot='0x14' function='0x0'/>
  </source>
</hostdev>
```

## Audio Optimization

### Scream Audio

Lower latency alternative to SPICE:

```bash
# Host: Install Scream receiver
yay -S scream-git
scream -i virbr0 -o pulse

# Guest: Install Scream driver from GitHub
# https://github.com/duncanthrax/scream
```

### PipeWire Low Latency

```bash
mkdir -p ~/.config/pipewire/pipewire.conf.d/
cat > ~/.config/pipewire/pipewire.conf.d/10-low-latency.conf << EOF
context.properties = {
    default.clock.rate = 48000
    default.clock.quantum = 512
}
EOF

systemctl --user restart pipewire pipewire-pulse
```

## Custom VBIOS

### When Needed

- GPU not initializing in VM
- Code 43 errors
- Reset bug workarounds

### Extracting VBIOS

```bash
# From running system
cd /sys/bus/pci/devices/0000:01:00.0/
echo 1 > rom
cat rom > /tmp/gpu-vbios.rom
echo 0 > rom
```

### Using Custom VBIOS

```xml
<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x0000' bus='0x01' slot='0x00' function='0x0'/>
  </source>
  <rom bar='on' file='/tmp/gpu-patched.rom'/>
</hostdev>
```

## Automation Scripts

### VM Startup Script

```bash
#!/bin/bash
# /usr/local/bin/start-gaming-vm.sh

VM_NAME="gaming-vm"

# Setup huge pages
echo 9075 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages

# Set CPU governor
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Start VM
virsh start $VM_NAME

# Wait and launch Looking Glass
sleep 30
looking-glass-client -f /dev/shm/looking-glass &
```

### systemd Service

```ini
# /etc/systemd/system/gaming-vm.service
[Unit]
Description=Gaming VM with Looking Glass
After=libvirtd.service

[Service]
Type=forking
User=your-username
ExecStart=/usr/local/bin/start-gaming-vm.sh
ExecStop=/usr/local/bin/stop-gaming-vm.sh

[Install]
WantedBy=multi-user.target
```

## Security Considerations

### Isolating Guest

```xml
<!-- Prevent guest from accessing host memory -->
<memoryBacking>
  <locked/>
  <nosharepages/>
</memoryBacking>

<!-- Secure random number generation -->
<rng model='virtio'>
  <backend model='random'>/dev/urandom</backend>
</rng>

<!-- TPM for BitLocker -->
<tpm model='tpm-crb'>
  <backend type='emulator' version='2.0'/>
</tpm>
```

### Network Isolation

```xml
<network>
  <name>isolated</name>
  <bridge name='virbr-isolated'/>
  <ip address='192.168.100.1' netmask='255.255.255.0'>
    <dhcp>
      <range start='192.168.100.100' end='192.168.100.200'/>
    </dhcp>
  </ip>
</network>
```

## Best Practices

1. **Start Simple**: Get basic setup working before optimization
2. **Benchmark**: Test before and after each change
3. **One Change at a Time**: Easier to identify issues
4. **Document Configuration**: Keep notes on what works
5. **Backup VM**: Before major changes
6. **Monitor Temperatures**: Ensure adequate cooling

## Next Steps

- **[Back to README](./README.md)**: Overview
- **[Installation](./installation.md)**: Setup guide
- **[Configuration](./configuration.md)**: Basic config
- **[Performance Tuning](./performance-tuning.md)**: Optimization
- **[Troubleshooting](./troubleshooting.md)**: Fix issues

## References

- VFIO Tips and Tricks: https://vfio.blogspot.com/
- Arch Wiki PCI Passthrough: https://wiki.archlinux.org/title/PCI_passthrough_via_OVMF
- Looking Glass GitHub: https://github.com/gnif/LookingGlass
- r/VFIO Subreddit: https://reddit.com/r/VFIO
