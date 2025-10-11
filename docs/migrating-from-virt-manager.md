# Migrating from virt-manager to Nova

This guide helps you transition from virt-manager to Nova, Arch Linux's modern virtualization platform with GPU passthrough excellence and AI/ML-first design.

## Why Switch to Nova?

| Feature | virt-manager | Nova |
|---------|--------------|------|
| **Interface** | GTK (X11/Wayland) | Native Wayland + CLI |
| **GPU Passthrough** | Manual XML editing | One-command auto-config |
| **Configuration** | XML files | Declarative TOML (NovaFile) |
| **Performance** | Python + libvirt | Rust + libvirt (zero-overhead) |
| **AI/ML Support** | Manual setup | Pre-built templates |
| **Container Integration** | None | Built-in (Bolt/Docker/Podman) |
| **Storage Management** | Basic | Btrfs/ZFS/NFS/Ceph |
| **Arch Integration** | Generic Linux | Arch-optimized |
| **NVIDIA Support** | Proprietary only | nvidia-open preferred |

---

## Quick Migration Path

### Step 1: Install Nova

```bash
# Install from AUR
yay -S nova-virtualization

# Or build from source
git clone https://github.com/nova-project/nova
cd nova
cargo build --release
sudo cp target/release/nova /usr/bin/
sudo cp target/release/nova-gui /usr/bin/
```

### Step 2: Import Existing VMs

Nova works alongside libvirt, so your existing VMs are automatically available:

```bash
# List all libvirt VMs (managed by virt-manager)
nova list

# View VM details
nova status vm <vm-name>

# Start an existing VM
nova start vm <vm-name>
```

**No migration needed!** Nova uses the same libvirt backend.

### Step 3: Convert to NovaFile (Optional)

For better management, convert your VMs to Nova's declarative format:

```bash
# Export VM configuration to NovaFile
nova export <vm-name> > NovaFile

# Edit and customize
$EDITOR NovaFile

# Apply configuration
nova apply NovaFile
```

---

## Feature Comparison & Migration Guide

### 1. GPU Passthrough

#### virt-manager Approach:
```xml
<!-- Edit XML manually -->
<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x0000' bus='0x01' slot='0x00' function='0x0'/>
  </source>
</hostdev>
```

#### Nova Approach:
```bash
# One command
nova gpu doctor  # Check system readiness

# Auto-configure GPU passthrough
nova wizard vm my-vm --gpu auto --apply

# Or in NovaFile:
[vm.my-vm]
gpu_passthrough = true
gpu_device = "auto"
```

**Migration Steps:**
1. Run `nova gpu doctor` to verify your system
2. Note any warnings and follow recommended fixes
3. Use `nova wizard vm` to recreate VMs with GPU support

---

### 2. Network Configuration

#### virt-manager:
- Click through GUI to create bridges
- Manual bridge-utils commands
- Limited visibility

#### Nova:
```bash
# List all networks (libvirt + system)
nova network list

# Create bridge with profile
nova network create hyperv0 \
  --type bridge \
  --profile nat \
  --uplink enp6s0 \
  --subnet 192.168.220.1/24

# Inspect topology
nova network topology
```

**Migration:**
Your existing libvirt networks continue to work. Nova adds enhanced management:
- System bridge discovery
- Origin tracking (Nova vs system)
- CLI + GUI management
- Persistent state across reboots

---

### 3. Storage Pools

#### virt-manager:
- Directory-based pools
- Manual pool creation
- Limited filesystem support

#### Nova:
```bash
# Create Btrfs pool with compression
nova storage create-pool nova-btrfs \
  --type btrfs \
  --path /var/lib/nova/storage \
  --compression zstd:3

# Create volume
nova storage create-volume nova-btrfs my-vm-disk 100G --format qcow2

# List pools
nova storage list-pools
```

**Migration:**
Existing storage pools are automatically discovered. To leverage Nova features:
1. `nova storage list-pools` - verify detection
2. Create new pools with Btrfs/ZFS for snapshots
3. Migrate VMs to new pools gradually

---

### 4. VM Creation Workflow

#### virt-manager:
1. Click "New VM"
2. 12+ dialog boxes
3. Manual configuration
4. Hope GPU passthrough works

#### Nova:
```bash
# Interactive wizard
nova wizard vm ml-workstation --apply

# Or use pre-built template
nova create ml-workstation \
  --template ml-pytorch \
  --cpu 16 \
  --memory 32G \
  --gpu auto

# Launch GUI for visual management
nova-gui
```

**Templates Available:**
- `ml-pytorch` - PyTorch + CUDA + Jupyter
- `ml-tensorflow` - TensorFlow GPU environment
- `stable-diffusion` - AI art generation (ComfyUI + Automatic1111)
- `arch-nvidia-dev` - Arch + KDE + NVIDIA dev environment
- `arch-gnome-nvidia` - Arch + GNOME + Wayland

---

## Common Migration Scenarios

### Scenario 1: Windows Gaming VM with GPU Passthrough

**Before (virt-manager):**
1. Manual VFIO configuration
2. Edit XML for GPU devices
3. Configure Looking Glass manually
4. Hope it boots

**After (Nova):**
```bash
# System check
nova gpu doctor

# Create VM
nova wizard vm win11-gaming \
  --template gaming-windows \
  --gpu auto \
  --display looking-glass \
  --memory 16G \
  --disk 100G \
  --apply

# Start VM
nova start vm win11-gaming

# Connect with Looking Glass
nova connect win11-gaming --protocol looking-glass
```

---

### Scenario 2: ML Development Environment

**Before (virt-manager):**
- Create Ubuntu VM
- Install CUDA manually
- Configure PyTorch
- Set up Jupyter
- Fight with GPU drivers for hours

**After (Nova):**
```bash
# One command creates everything
nova create ml-dev \
  --template ml-pytorch \
  --gpu auto \
  --cpu 16 \
  --memory 32G

# Start and connect
nova start vm ml-dev
nova connect ml-dev

# Jupyter auto-starts on port 8888
```

Pre-configured with:
- PyTorch 2.1 + CUDA 12.1
- Jupyter Lab
- TensorBoard
- All ML libraries
- Docker with NVIDIA runtime

---

### Scenario 3: Arch Linux Testing

**Before (virt-manager):**
- Create VM manually
- Install Arch step-by-step
- Configure GPU drivers
- Set up development tools

**After (Nova):**
```bash
# Use pre-built Arch template
nova create arch-test \
  --template arch-nvidia-dev \
  --gpu auto \
  --cpu 8 \
  --memory 16G

# Includes:
# - Arch Linux with KDE Plasma
# - nvidia-open drivers
# - Development tools (Rust, C++, etc.)
# - AUR helpers (yay, paru)
# - Wayland + GPU acceleration
```

---

## Configuration File Migration

### virt-manager XML → Nova NovaFile

**XML (virt-manager):**
```xml
<domain type='kvm'>
  <name>my-vm</name>
  <memory unit='GiB'>16</memory>
  <vcpu>8</vcpu>
  <devices>
    <disk type='file' device='disk'>
      <source file='/var/lib/libvirt/images/my-vm.qcow2'/>
    </disk>
  </devices>
</domain>
```

**TOML (Nova):**
```toml
project = "my-project"

[vm.my-vm]
image = "/var/lib/libvirt/images/my-vm.qcow2"
cpu = 8
memory = "16Gi"
network = "default"
gpu_passthrough = false
autostart = false
```

**Much cleaner!** And version-controllable with git.

---

## Feature Equivalence Table

| virt-manager Feature | Nova Command | Notes |
|---------------------|--------------|-------|
| New VM Wizard | `nova wizard vm` | Interactive + templates |
| Start VM | `nova start vm <name>` | Same functionality |
| Stop VM | `nova stop vm <name>` | Graceful shutdown |
| Force Stop | `nova destroy <name>` | Immediate stop |
| VM Details | `nova status vm <name>` | More detailed info |
| Console | `nova connect <name>` | Multiple protocols |
| Snapshots | `nova snapshot create <vm> <name>` | Enhanced features |
| Clone VM | `nova clone <src> <dst>` | Fast cloning |
| Delete VM | `nova delete <name>` | With confirmation |
| Network Manager | `nova network list` | Enhanced visibility |
| Storage Pools | `nova storage list-pools` | More backends |

---

## Troubleshooting Migration Issues

### Issue 1: "GPU passthrough not working"

**Solution:**
```bash
# Run diagnostics
nova gpu doctor

# Common fixes:
# 1. Enable IOMMU in GRUB:
sudo nano /etc/default/grub
# Add: intel_iommu=on iommu=pt (or amd_iommu=on)
sudo grub-mkconfig -o /boot/grub/grub.cfg

# 2. Load VFIO modules:
sudo modprobe vfio-pci

# 3. Bind GPU to VFIO:
nova gpu bind <pci-address>
```

### Issue 2: "Existing VMs not appearing"

**Solution:**
```bash
# Ensure libvirt is running
sudo systemctl start libvirtd

# Check connection
nova list

# If still not working, check permissions:
sudo usermod -aG libvirt,kvm $USER
# Log out and back in
```

### Issue 3: "Network bridge missing"

**Solution:**
```bash
# List all networks
nova network list

# Recreate bridge if needed
nova network create bridge0 --type bridge --interface enp6s0
```

---

## Advanced: Side-by-Side Comparison

You can run virt-manager and Nova simultaneously:

```bash
# Use virt-manager for existing VMs
virt-manager &

# Use Nova for new VMs with GPU
nova-gui &

# Both connect to same libvirt daemon
# No conflicts, seamless coexistence
```

**Recommended Migration Strategy:**
1. Install Nova
2. Keep virt-manager installed initially
3. Create new VMs with Nova
4. Gradually convert old VMs using `nova export`
5. Uninstall virt-manager when comfortable

---

## Nova Exclusive Features

These features **only** work with Nova:

### 1. Hybrid VM + Container Workflows
```toml
[vm.ml-vm]
gpu_passthrough = true
# ...

[container.jupyter]
capsule = "jupyter/tensorflow-notebook"
gpu_access = true  # Shares VM's GPU
network = "nova-ml"
```

### 2. Infrastructure as Code
```bash
# Version control your entire VM infrastructure
git init
git add NovaFile
git commit -m "Add ML development environment"
git push
```

### 3. GPU Auto-Detection
```bash
# Nova automatically finds best GPU
nova wizard vm --gpu auto
# No PCI address memorization needed!
```

### 4. Btrfs Snapshots
```bash
# Instant VM snapshots with Btrfs
nova snapshot create ml-vm "before-update"
# Restore in seconds, not minutes
```

### 5. Prometheus Metrics
```bash
# Built-in monitoring
sudo systemctl enable --now nova-metrics
# Scrape at http://localhost:9090/metrics
```

---

## Getting Help

### Documentation
```bash
# Built-in help
nova --help
nova network --help
nova gpu --help

# Read docs
man nova
cat /usr/share/doc/nova/README.md
```

### Community
- GitHub Issues: https://github.com/nova-project/nova/issues
- Arch Wiki: https://wiki.archlinux.org/title/Nova
- IRC: #nova on Libera.Chat

### Professional Support
- Email: support@nova-project.org
- Consulting: Available for enterprise deployments

---

## Conclusion

Nova is the **modern replacement** for virt-manager on Arch Linux, designed for:
- **GPU passthrough** without the pain
- **AI/ML workloads** out of the box
- **Developer-first** experience with declarative config
- **Arch Linux integration** (AUR, nvidia-open, KDE/GNOME)

**Migration is seamless** - your existing VMs continue to work while you explore Nova's enhanced features.

**Ready to switch?**
```bash
yay -S nova-virtualization
nova gpu doctor
nova-gui
```

Welcome to the future of Arch virtualization! 🚀
