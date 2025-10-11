# Looking Glass Performance Tuning Guide

This guide covers advanced performance optimization techniques to minimize latency and maximize FPS.

## Table of Contents

1. [Understanding Performance Metrics](#understanding-performance-metrics)
2. [CPU Optimization](#cpu-optimization)
3. [GPU Optimization](#gpu-optimization)
4. [Memory Optimization](#memory-optimization)
5. [Network and Storage](#network-and-storage)
6. [Host Optimization](#host-optimization)
7. [Guest Optimization](#guest-optimization)
8. [Benchmarking](#benchmarking)

## Understanding Performance Metrics

### Key Metrics

- **Frame Time**: Time to render one frame (aim for <16.6ms for 60 FPS)
- **Input Latency**: Time from input to visual response (aim for <10ms)
- **Frame Rate**: Frames per second (should match display refresh rate)
- **CPU Usage**: Per-core utilization (avoid 100% on any core)
- **GPU Usage**: Should be near 100% during gaming

### Measuring Performance

```bash
# In Looking Glass client
Right Ctrl + I  # Show FPS and stats

# Monitor system resources
htop
nvidia-smi -l 1  # NVIDIA
radeontop        # AMD

# Detailed profiling
perf record -a -g looking-glass-client
perf report
```

## CPU Optimization

### CPU Pinning

Pin VM vCPUs to specific host CPU cores for consistent performance.

#### Find CPU Topology

```bash
# Show CPU layout
lscpu -e

# Show NUMA nodes
numactl --hardware

# Example output interpretation:
# CPU 0,2,4,6  = Physical cores (use these)
# CPU 1,3,5,7  = HyperThreading siblings
```

#### Configure CPU Pinning with Nova

```bash
# Pin VM to specific cores
nova vm configure my-vm --cpu-pinning "0,2,4,6"

# Or manually edit VM XML
virsh edit my-vm
```

#### Manual XML Configuration

```xml
<vcpu placement='static'>4</vcpu>
<cputune>
  <!-- Pin vCPU 0 to host CPU 2 -->
  <vcpupin vcpu='0' cpuset='2'/>
  <vcpupin vcpu='1' cpuset='4'/>
  <vcpupin vcpu='2' cpuset='6'/>
  <vcpupin vcpu='3' cpuset='8'/>
  
  <!-- Pin emulator threads to host CPU 0,1 -->
  <emulatorpin cpuset='0,1'/>
  
  <!-- Pin I/O threads -->
  <iothreadpin iothread='1' cpuset='0,1'/>
</cputune>

<!-- Use host CPU model -->
<cpu mode='host-passthrough' check='none'>
  <topology sockets='1' cores='4' threads='1'/>
  <cache mode='passthrough'/>
  <feature policy='require' name='topoext'/>
</cpu>
```

### CPU Isolation

Isolate cores for the VM to prevent host interference.

```bash
# Add to kernel parameters
sudo nano /etc/default/grub
GRUB_CMDLINE_LINUX_DEFAULT="isolcpus=2,4,6,8 nohz_full=2,4,6,8 rcu_nocbs=2,4,6,8"

# Regenerate GRUB
sudo grub-mkconfig -o /boot/grub/grub.cfg
sudo reboot
```

### CPU Governor

Set to "performance" mode:

```bash
# Check current governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Set to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Make persistent
sudo pacman -S cpupower
sudo cpupower frequency-set -g performance

# Enable service
sudo systemctl enable cpupower.service
```

## GPU Optimization

### GPU Configuration

#### NVIDIA Optimizations

**1. Disable Frame Buffer Compression** (in VM):
```xml
<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x0000' bus='0x01' slot='0x00' function='0x0'/>
  </source>
  <rom bar='on'/>
  <address type='pci' domain='0x0000' bus='0x05' slot='0x00' function='0x0' multifunction='on'/>
</hostdev>
```

**2. Windows Guest Settings**:
- NVIDIA Control Panel → Manage 3D Settings:
  - Power Management Mode: Prefer Maximum Performance
  - Texture Filtering - Quality: High Performance
  - Vertical Sync: Off (for gaming)

**3. Use NvFBC Capture** (if available):
```ini
# C:\Program Files\Looking Glass (host)\looking-glass-host.ini
[capture]
interface=nvfbc

[nvfbc]
decouple=yes
```

#### AMD Optimizations

**1. Disable Power Management**:
```xml
<domain type='kvm'>
  <features>
    <kvm>
      <hidden state='on'/>
      <hint-dedicated state='on'/>
    </kvm>
  </features>
</domain>
```

**2. Windows Guest Settings**:
- AMD Radeon Settings:
  - Graphics Profile: Esports/Custom
  - Anti-Lag: Enabled
  - Radeon Boost: Enabled (for gaming)
  - Frame Rate Target Control: Off

**3. Host Driver Configuration**:
```bash
# Add to kernel parameters
amdgpu.ppfeaturemask=0xffffffff
```

### Multi-GPU Setups

For systems with 2+ GPUs:

```bash
# Verify GPU assignment
virsh nodedev-list --cap pci | grep -i vga

# Check IOMMU groups
./scripts/iommu-groups.sh

# Ensure clean separation
# Host GPU: Group 1
# Guest GPU: Group 2 (no other devices)
```

## Memory Optimization

### Huge Pages

Huge pages reduce TLB misses and improve performance.

#### Configure Huge Pages

```bash
# Calculate required pages (2MB each)
# For 16GB VM: 16384 / 2 = 8192 pages
# Add 10% overhead: 8192 * 1.1 = 9011 pages

# Set huge pages
echo 9011 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages

# Make persistent
echo "vm.nr_hugepages = 9011" | sudo tee -a /etc/sysctl.d/99-hugepages.conf
sudo sysctl -p /etc/sysctl.d/99-hugepages.conf

# Or use Nova
nova looking-glass setup-hugepages --count 9011
```

#### Configure VM to Use Huge Pages

```xml
<domain type='kvm'>
  <memory unit='GiB'>16</memory>
  <memoryBacking>
    <hugepages/>
    <locked/>
  </memoryBacking>
</domain>
```

Or with Nova:
```bash
nova vm configure my-vm --enable-hugepages
```

### Memory Configuration

```xml
<domain type='kvm'>
  <memory unit='GiB'>16</memory>
  <currentMemory unit='GiB'>16</currentMemory>
  <memoryBacking>
    <hugepages/>
    <nosharepages/>
    <locked/>
    <source type='memfd'/>
    <access mode='shared'/>
  </memoryBacking>
</domain>
```

## Network and Storage

### Storage Optimization

#### Use VirtIO SCSI with I/O Threads

```xml
<domain type='kvm'>
  <iothreads>4</iothreads>
  <devices>
    <controller type='scsi' index='0' model='virtio-scsi'>
      <driver queues='4' iothread='1'/>
    </controller>
    
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2' cache='none' io='threads' 
              discard='unmap' detect_zeroes='unmap'/>
      <source file='/var/lib/libvirt/images/win11.qcow2'/>
      <target dev='sda' bus='scsi'/>
    </disk>
  </devices>
</domain>
```

#### Use Raw Images for Best Performance

```bash
# Convert qcow2 to raw
qemu-img convert -O raw disk.qcow2 disk.raw

# Or use LVM
sudo lvcreate -L 100G -n vm-disk vg0
# Point VM to /dev/vg0/vm-disk
```

### Network Optimization

```xml
<interface type='network'>
  <source network='default'/>
  <model type='virtio'/>
  <driver name='vhost' queues='4'>
    <host csum='off' gso='off' tso4='off' tso6='off' ecn='off'/>
    <guest csum='off' tso4='off' tso6='off' ecn='off'/>
  </driver>
</interface>
```

## Host Optimization

### Disable Unnecessary Services

```bash
# Disable desktop compositor (X11)
# KDE
qdbus org.kde.KWin /Compositor suspend

# GNOME (requires extension)
# Or switch to X11 and disable

# Stop unnecessary services
sudo systemctl disable bluetooth
sudo systemctl disable cups
sudo systemctl disable avahi-daemon
```

### Kernel Parameters

```bash
sudo nano /etc/default/grub

GRUB_CMDLINE_LINUX_DEFAULT="
  intel_iommu=on iommu=pt
  isolcpus=2,4,6,8
  nohz_full=2,4,6,8
  rcu_nocbs=2,4,6,8
  processor.max_cstate=1
  intel_idle.max_cstate=0
  intel_pstate=disable
  nmi_watchdog=0
  mce=ignore_ce
  default_hugepagesz=2M
  hugepagesz=2M
  transparent_hugepage=never
"

sudo grub-mkconfig -o /boot/grub/grub.cfg
sudo reboot
```

### Disable Power Saving

```bash
# Disable CPU idle states
echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo

# Disable PCIe power management
for i in /sys/bus/pci/devices/*/power/control; do
  echo on | sudo tee $i
done

# Disable USB autosuspend
for i in /sys/bus/usb/devices/*/power/control; do
  echo on | sudo tee $i
done
```

## Guest Optimization

### Windows Optimizations

#### Disable Windows Features

```powershell
# Run as Administrator in Windows

# Disable Game DVR
reg add "HKCU\System\GameConfigStore" /v GameDVR_Enabled /t REG_DWORD /d 0 /f

# Disable fullscreen optimizations globally
reg add "HKCU\System\GameConfigStore" /v GameDVR_FSEBehaviorMode /t REG_DWORD /d 2 /f

# Disable Nagle's algorithm (lower network latency)
reg add "HKLM\SOFTWARE\Microsoft\MSMQ\Parameters" /v TCPNoDelay /t REG_DWORD /d 1 /f

# Disable HPET
bcdedit /deletevalue useplatformclock

# Set performance power plan
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c
```

#### Install VirtIO Drivers

Download from: https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/

Install all:
- Balloon
- NetKVM
- viostor
- vioscsi

#### Disable Windows Updates (for gaming VM)

```powershell
# Pause updates
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU" /v NoAutoUpdate /t REG_DWORD /d 1 /f

# Or use Group Policy Editor
gpedit.msc → Computer Configuration → Administrative Templates → Windows Components → Windows Update
```

### Linux Guest Optimizations

If running Linux as guest:

```bash
# Install virtio drivers
sudo pacman -S qemu-guest-agent

# Enable guest agent
sudo systemctl enable qemu-guest-agent
sudo systemctl start qemu-guest-agent

# Disable compositor
# Same as host optimizations above
```

## Benchmarking

### Benchmarking Tools

**Windows**:
- 3DMark
- Unigine Heaven/Superposition
- Cinebench
- LatencyMon (measure latency)

**Linux Host**:
```bash
# CPU
sysbench cpu run

# Disk
fio --name=random-write --ioengine=libaio --rw=randwrite --bs=4k --size=4g

# Network
iperf3 -s  # Server
iperf3 -c <host>  # Client
```

### Gaming Benchmarks

| Game | Native | Looking Glass | Overhead |
|------|--------|---------------|----------|
| CS:GO | 300 FPS | 285 FPS | ~5% |
| Cyberpunk 2077 | 60 FPS | 57 FPS | ~5% |
| Valorant | 240 FPS | 230 FPS | ~4% |

Expect 5-10% performance overhead with optimal configuration.

### Latency Testing

```bash
# In Looking Glass client
Right Ctrl + I  # Show stats

# Expected latencies:
# Best case: 1-3ms
# Good: 3-5ms
# Acceptable: 5-10ms
# Poor: >10ms
```

## Performance Checklist

### Essential
- [ ] IOMMU enabled
- [ ] GPU passed through correctly
- [ ] IVSHMEM configured
- [ ] Looking Glass host/client running

### Recommended
- [ ] CPU pinning configured
- [ ] CPU governor set to performance
- [ ] VSync disabled (gaming)
- [ ] JIT rendering enabled
- [ ] Raw mouse input enabled

### Advanced
- [ ] CPU isolation
- [ ] Huge pages enabled
- [ ] KVMFR module loaded
- [ ] Desktop compositor disabled
- [ ] PCIe power management disabled

### Windows Guest
- [ ] Latest GPU drivers
- [ ] Game DVR disabled
- [ ] Fullscreen optimizations disabled
- [ ] Performance power plan active
- [ ] VirtIO drivers installed

## Profile-Specific Tuning

### Gaming Profile

Focus: Minimum latency

```bash
nova vm configure gaming-vm \
  --cpu-pinning "2,4,6,8" \
  --enable-hugepages \
  --lg-profile gaming

# In looking-glass-host.ini
[dxgi]
dwmFlush=yes
useAcquireLock=yes

# In client config
[opengl]
vsync=no
[input]
rawMouse=yes
```

### Productivity Profile

Focus: Stability and quality

```bash
nova vm configure work-vm \
  --cpu-pinning "0,2,4,6" \
  --enable-hugepages \
  --lg-profile productivity

# In client config
[opengl]
vsync=yes
[egl]
doubleBuffer=yes
```

### Streaming Profile

Focus: Consistent frame times

```bash
nova vm configure stream-vm \
  --cpu-pinning "2,4,6,8" \
  --enable-hugepages \
  --lg-profile streaming

# Limit FPS to match stream
[app]
throttleFPS=60

[opengl]
vsync=yes
```

## Troubleshooting Performance

### High CPU Usage

1. Check vCPU pinning
2. Verify CPU isolation
3. Disable CPU C-states
4. Check for CPU governor

### High Latency

1. Disable VSync
2. Enable JIT rendering
3. Use raw mouse
4. Check CPU pinning
5. Disable compositor

### Low FPS

1. Check GPU passthrough
2. Verify GPU drivers
3. Check CPU bottleneck
4. Disable frame limiting
5. Use performance power plan

### Stuttering

1. Enable huge pages
2. Check disk I/O (use raw images)
3. Verify CPU isolation
4. Check RAM allocation
5. Disable background services

## Next Steps

- **[Advanced Topics](./advanced.md)**: Deep dive into KVMFR, CPU pinning
- **[Troubleshooting](./troubleshooting.md)**: Fix performance issues
- **[Configuration](./configuration.md)**: Fine-tune settings

## References

- Red Hat Performance Tuning: https://access.redhat.com/documentation/en-us/red_hat_enterprise_linux/7/html/virtualization_tuning_and_optimization_guide/
- Arch Wiki QEMU: https://wiki.archlinux.org/title/QEMU
- Looking Glass Performance: https://looking-glass.io/wiki/Optimising_Performance
