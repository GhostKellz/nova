# Nova Command Reference

Nova provides both GUI and CLI interfaces for comprehensive VM management on Arch Linux + KDE + KVM + libvirt.

## Table of Contents
- [VM Management](#vm-management)
- [Console Connections](#console-connections)
- [Templates](#templates)
- [Snapshots](#snapshots)
- [Networking](#networking)
- [Migration](#migration)
- [Storage](#storage)
- [System](#system)
- [Diagnostics & Support](#diagnostics--support)

## VM Management

### Basic VM Operations

```bash
# List all VMs
nova list
nova list --all  # Include stopped VMs
nova list --running  # Only running VMs

# Create a new VM
nova create <vm-name> \
  --template <template-id> \
  --cpu 4 \
  --memory 8G \
  --disk 50G \
  --network bridge0

# Start/Stop/Restart VMs
nova start <vm-name>
nova stop <vm-name>
nova restart <vm-name>
nova pause <vm-name>
nova resume <vm-name>

# Force stop (destroy)
nova destroy <vm-name>

# Get VM information
nova info <vm-name>
nova status <vm-name>

# Delete VM (with confirmation)
nova delete <vm-name>
nova delete <vm-name> --force  # Skip confirmation
```

### Guided VM Configuration Wizard

```bash
# Generate a NovaFile entry and interactively choose a configured network
nova wizard vm my-vm --cpu 4 --memory 8Gi --apply

# Skip the prompt by specifying a network explicitly
nova wizard vm my-vm --network bridge0 --apply
```

> The wizard inspects the networks defined in your NovaFile and lets you pick one when `--network` is omitted.

### Advanced VM Operations

```bash
# Clone a VM
nova clone <source-vm> <new-vm-name>

# Configure VM resources
nova configure <vm-name> \
  --cpu 8 \
  --memory 16G \
  --add-disk 100G \
  --network-add vswitch1

# GPU passthrough
nova configure <vm-name> --gpu-passthrough 01:00.0

# Boot order configuration
nova configure <vm-name> --boot-order cdrom,hd,network

# VM autostart
nova autostart <vm-name> --enable
nova autostart <vm-name> --disable
```

## Console Connections

### RustDesk Integration (High Performance)

```bash
# Connect with RustDesk (best performance)
nova connect <vm-name>
nova connect <vm-name> --protocol rustdesk
nova connect <vm-name> --protocol rustdesk --profile ultra-high

# Performance profiles
nova connect <vm-name> --protocol rustdesk --profile high
nova connect <vm-name> --protocol rustdesk --profile balanced
nova connect <vm-name> --protocol rustdesk --profile low-bandwidth

# Custom performance settings
nova connect <vm-name> --protocol rustdesk \
  --fps 60 \
  --quality 100 \
  --compression low \
  --hardware-accel

# Enable file transfer and clipboard
nova connect <vm-name> --protocol rustdesk \
  --file-transfer \
  --clipboard-sync \
  --multi-monitor
```

### Traditional Console Protocols

```bash
# SPICE (good performance, libvirt native)
nova connect <vm-name> --protocol spice
nova connect <vm-name> --protocol spice --multi-monitor

# VNC (universal compatibility)
nova connect <vm-name> --protocol vnc
nova connect <vm-name> --protocol vnc --enhanced

# RDP (for Windows VMs)
nova connect <vm-name> --protocol rdp

# Serial console (debugging)
nova connect <vm-name> --protocol serial

# Web console (browser-based)
nova connect <vm-name> --protocol web
```

### Console Management

```bash
# List active console sessions
nova console list

# Get console connection info
nova console info <session-id>

# Close console session
nova console close <session-id>

# Console performance metrics
nova console metrics <session-id>

# Optimize console performance
nova console optimize <session-id>
```

## Templates

### Template Creation

```bash
# Create template from existing VM
nova template create <vm-name> \
  --name "Ubuntu 22.04 Desktop" \
  --description "Ubuntu desktop with development tools" \
  --tags "ubuntu,desktop,dev"

# Create template with custom settings
nova template create <vm-name> \
  --name "Windows 11 Pro" \
  --compress \
  --install-tools \
  --optimize
```

### Template Management

```bash
# List available templates
nova template list
nova template list --tags ubuntu
nova template search "windows"

# Get template information
nova template info <template-id>

# Delete template
nova template delete <template-id>

# Export/Import templates
nova template export <template-id> --output template.nvt
nova template import template.nvt

# Update template
nova template update <template-id> \
  --name "New Name" \
  --description "Updated description"
```

### VM Creation from Templates

```bash
# Create VM from template (default settings)
nova create <vm-name> --template <template-id>

# Create VM with customizations
nova create <vm-name> --template <template-id> \
  --cpu 8 \
  --memory 16G \
  --disk 100G \
  --network vswitch1 \
  --enable-guest-tools

# Batch create multiple VMs
nova create-batch --template <template-id> \
  --prefix "test-vm" \
  --count 5 \
  --cpu 2 \
  --memory 4G
```

## Snapshots

### Snapshot Creation

```bash
# Create disk-only snapshot
nova snapshot create <vm-name> \
  --name "before-update" \
  --description "Before system update"

# Create memory + disk snapshot
nova snapshot create <vm-name> \
  --name "running-state" \
  --memory \
  --description "VM running state"

# Create external snapshot
nova snapshot create <vm-name> \
  --name "external-snap" \
  --external \
  --path "/storage/snapshots/"
```

### Snapshot Management

```bash
# List snapshots for a VM
nova snapshot list <vm-name>

# Show snapshot tree
nova snapshot tree <vm-name>

# Get snapshot information
nova snapshot info <vm-name> <snapshot-id>

# Revert to snapshot
nova snapshot revert <vm-name> <snapshot-id>

# Delete snapshot
nova snapshot delete <vm-name> <snapshot-id>
nova snapshot delete <vm-name> <snapshot-id> --delete-children

# Merge snapshots
nova snapshot merge <vm-name> <snapshot-id>
```

### Advanced Snapshot Operations

```bash
# Create snapshot chain
nova snapshot create <vm-name> --name "base"
# Make changes...
nova snapshot create <vm-name> --name "v1" --parent "base"
# Make more changes...
nova snapshot create <vm-name> --name "v2" --parent "v1"

# Export snapshot
nova snapshot export <vm-name> <snapshot-id> --output snapshot.qcow2

# Clone VM from snapshot
nova clone <vm-name> <new-vm> --snapshot <snapshot-id>
```

## Networking

### Virtual Switch Management

```bash
# Create virtual switch
nova network create-switch vswitch1 --type linux-bridge
nova network create-switch vswitch2 --type ovs

# Configure switch
nova network configure-switch vswitch1 \
  --stp-enable \
  --vlan 100 \
  --interfaces eth0,eth1

# List switches
nova network list-switches

# Delete switch
nova network delete-switch vswitch1
```

### Network Configuration

```bash
# Create libvirt network
nova network create net1 \
  --type nat \
  --subnet 192.168.100.0/24 \
  --dhcp-start 192.168.100.10 \
  --dhcp-end 192.168.100.200

# Create isolated network
nova network create isolated1 --type isolated

# Bridge to physical interface
nova network create bridge1 \
  --type bridge \
  --interface eth0

# List networks
nova network list

# Get network info
nova network info net1

# Start/stop network
nova network start net1
nova network stop net1
```

### Advanced Networking

```bash
# DHCP management
nova network dhcp net1 \
  --enable \
  --range 192.168.100.50-192.168.100.150 \
  --lease-time 86400

# NAT configuration
nova network nat net1 \
  --enable \
  --external-interface eth0 \
  --masquerade

# Port forwarding
nova network forward net1 \
  --host-port 8080 \
  --guest-ip 192.168.100.10 \
  --guest-port 80

# Network monitoring
nova network monitor net1
nova network topology
```

## Migration

### Live Migration

```bash
# Simple live migration
nova migrate <vm-name> --destination host2.lan

# Migration with options
nova migrate <vm-name> \
  --destination host2.lan \
  --type live \
  --bandwidth 1000 \
  --compress \
  --parallel 4

# Post-copy migration (for large VMs)
nova migrate <vm-name> \
  --destination host2.lan \
  --type postcopy

# Offline migration
nova migrate <vm-name> \
  --destination host2.lan \
  --type offline
```

### Migration Management

```bash
# List active migrations
nova migrate list

# Get migration status
nova migrate status <job-id>

# Monitor migration progress
nova migrate monitor <job-id>

# Cancel migration
nova migrate cancel <job-id>

# Migration history
nova migrate history <vm-name>
```

### Storage Migration

```bash
# Migrate with storage
nova migrate <vm-name> \
  --destination host2.lan \
  --migrate-storage \
  --storage-path /shared/storage

# Block migration (non-shared storage)
nova migrate <vm-name> \
  --destination host2.lan \
  --block-migration
```

## Storage

### Storage Pool Management

```bash
# Create storage pool
nova storage create-pool pool1 \
  --type nfs \
  --source server.lan:/export/vms \
  --target /mnt/nfs-pool

# iSCSI storage pool
nova storage create-pool iscsi1 \
  --type iscsi \
  --portal 192.168.1.100:3260 \
  --target iqn.2023-01.com.example:storage

# List storage pools
nova storage list-pools

# Storage pool info
nova storage pool-info pool1
```

### Volume Management

```bash
# Create volume
nova storage create-volume pool1 vm-disk1 50G

# Clone volume
nova storage clone-volume pool1 vm-disk1 vm-disk1-clone

# Resize volume
nova storage resize-volume pool1 vm-disk1 100G

# Delete volume
nova storage delete-volume pool1 vm-disk1

# List volumes
nova storage list-volumes pool1
```

## System
- `nova metrics snapshot` – emit one-shot Prometheus metrics (saved to stdout)
- `nova metrics serve --port 9100` – run long-lived exporter for Prometheus scraping
- `nova support diagnostics` – run system checks and print a condensed report
- `nova support bundle --redact --output ./support` – collect logs, config, metrics, and GPU capabilities into a tarball (redacts IP/MAC addresses)

Generated bundles now add `nova/gpu-capabilities.json`, capturing detected GPU generation, VRAM, minimum driver, kernel recommendations, and TCC support flags — perfect for RTX 50-series troubleshooting.

## Diagnostics & Support

### GPU Insights

```bash
# Overview table with driver/kernel guidance
nova gpu list

# Detailed capabilities (generation, VRAM, compute, TCC)
nova gpu info 0000:01:00.0
nova gpu info all  # dump every device

# Passthrough readiness report (flags driver/kernel issues, TCC requirement)
nova gpu doctor
```

When a Blackwell/RTX 50-series GPU is detected, the CLI surfaces the minimum NVIDIA driver (`560+`), recommended kernel (`6.9+`), and encourages enabling TCC for low-latency Looking Glass workflows. See `docs/rtx50-series.md` for the full playbook.

### Support Tooling

```bash
# Generate support bundle with optional redaction (default includes metrics, logs, system info)
nova support bundle --output ./support --redact

# On-demand diagnostics (same engine Nova support uses)
nova support diagnostics

# Collect only system data and GPU capabilities (skip logs/metrics)
nova support bundle --no-logs --no-metrics
```

Support bundles now include:
- `system/` snapshots of `uname`, kernel modules, `cpuinfo`, `meminfo`
- `nova/virsh_list.txt`, `nova/docker_ps.txt`
- `nova/gpu-capabilities.json` capturing per-GPU requirements (driver/kernel/TCC)
- `nova/observability/prometheus-metrics.txt` when metrics capture is enabled
- Optional logs (`logs/`) from `journalctl` and `dmesg`

Bundles are written as `nova-support-<timestamp>.tar.gz` in the requested output directory (defaults to `/tmp`).


### Host Management

```bash
# System information
nova system info
nova system resources

# Host performance
nova system monitor
nova system top

# Service management
nova system status
nova system start
nova system stop
nova system restart
```

### Configuration

```bash
# Show configuration
nova config show

# Edit configuration
nova config edit

# Set configuration options
nova config set console.preferred_protocol rustdesk
nova config set migration.bandwidth_limit 1000
nova config set network.default_bridge virbr0

# Reset to defaults
nova config reset
```

### Arch Linux Integration

```bash
# Optimize for Arch/KDE
nova system optimize-arch

# Install required packages
nova system install-deps

# Configure kernel modules
nova system setup-kvm

# NetworkManager integration
nova system setup-networkmanager

# systemd-networkd integration
nova system setup-systemd-networkd
```

## GUI Commands

### Launch GUI Applications

```bash
# Main Nova GUI
nova gui

# Network manager GUI
nova gui --network

# Console manager
nova gui --console

# Template manager
nova gui --templates

# Migration manager
nova gui --migration
```

### Desktop Integration

```bash
# Install desktop files
nova install-desktop

# System tray integration
nova system-tray --enable

# Notifications
nova notifications --enable

# KDE integration
nova kde-integrate
```

## Environment Variables

```bash
# Configuration
export NOVA_CONFIG_DIR="/etc/nova"
export NOVA_DATA_DIR="/var/lib/nova"
export NOVA_LOG_LEVEL="info"

# RustDesk integration
export NOVA_RUSTDESK_SERVER="localhost:21116"
export NOVA_RUSTDESK_KEY="/etc/nova/rustdesk.key"

# Performance tuning
export NOVA_CONSOLE_PERFORMANCE="high"
export NOVA_MIGRATION_PARALLEL="4"
```

## Configuration Files

### Main Configuration (`/etc/nova/nova.toml`)

```toml
[general]
data_dir = "/var/lib/nova"
log_level = "info"

[console]
preferred_protocol = "rustdesk"
auto_optimize = true
enable_clipboard = true
enable_file_transfer = true

[migration]
bandwidth_limit = 1000
auto_converge = true
compress = true
parallel_connections = 4

[network]
default_bridge = "virbr0"
enable_stp = true
```

## Exit Codes

- `0`: Success
- `1`: General error
- `2`: VM not found
- `3`: Network error
- `4`: Permission denied
- `5`: Resource unavailable
- `6`: Configuration error
- `7`: Migration failed
- `8`: Console connection failed

## Examples

### Complete VM Lifecycle

```bash
# 1. Create VM from template
nova create dev-vm --template ubuntu-22.04 --cpu 4 --memory 8G

# 2. Start VM
nova start dev-vm

# 3. Connect with high-performance console
nova connect dev-vm --protocol rustdesk --profile ultra-high

# 4. Create snapshot before major changes
nova snapshot create dev-vm --name "clean-install" --memory

# 5. Make changes, create another snapshot
nova snapshot create dev-vm --name "configured"

# 6. If something breaks, revert
nova snapshot revert dev-vm "clean-install"

# 7. Migrate to another host
nova migrate dev-vm --destination server2.lan

# 8. Create template from configured VM
nova template create dev-vm --name "My Dev Environment"
```

### Network Setup

```bash
# 1. Create virtual switch
nova network create-switch vswitch1 --type ovs

# 2. Create NAT network
nova network create dev-net --type nat --subnet 192.168.200.0/24

# 3. Create VM on custom network
nova create test-vm --template ubuntu-22.04 --network dev-net

# 4. Monitor network topology
nova network topology
```

This command reference provides comprehensive coverage of all Nova functionality with practical examples for each feature.