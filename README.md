<p align="center">
  <img src="assets/nova-logo.png" alt="Nova" width="180" height="180">
</p>

<h1 align="center">Nova</h1>

<p align="center">
  <strong>Wayland-Native Virtualization and Container Manager</strong>
</p>

<p align="center">
  <strong>Bare metal speed. Declarative control. GPU-first Linux infrastructure.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.96+-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Linux-KVM%2FQEMU-FCC624?style=for-the-badge&logo=linux&logoColor=black" alt="Linux KVM/QEMU">
  <img src="https://img.shields.io/badge/Wayland-Native-76B900?style=for-the-badge&logo=wayland&logoColor=white" alt="Wayland Native">
  <img src="https://img.shields.io/badge/GUI-egui-3178C6?style=for-the-badge" alt="egui">
  <img src="https://img.shields.io/badge/GPU-VFIO%20Passthrough-8A2BE2?style=for-the-badge&logo=nvidia&logoColor=white" alt="VFIO GPU Passthrough">
  <img src="https://img.shields.io/badge/Networking-Bridges%20%7C%20NAT%20%7C%20Overlay-E57000?style=for-the-badge" alt="Networking">
  <img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="MIT License">
</p>

---

## Overview

Nova is a Rust virtualization and container management platform for modern Linux desktops and homelabs. It brings KVM/QEMU virtual machines, lightweight container workflows, GPU passthrough, virtual networking, monitoring, and a Wayland-native GUI into one cohesive tool.

Nova is built for users who want a cleaner local alternative to sprawling virtualization stacks: declarative configuration, practical diagnostics, low-latency display paths, and a first-class Arch/NVIDIA/VFIO workflow without losing sight of general Linux support.

## Core Features

- **KVM/QEMU VM lifecycle**: Create, start, stop, clone, snapshot, and inspect virtual machines from the CLI or GUI.
- **Wayland-native management UI**: egui desktop shell with Tokyo Night defaults, Material Ocean preset, monitoring panes, and GPU/network dashboards.
- **Declarative NovaFile config**: TOML project definitions for repeatable VM, container, and network setup.
- **VFIO and GPU passthrough**: NVIDIA-focused diagnostics, RTX 50-series guidance, bulk bind/reset workflows, and Looking Glass support paths.
- **Looking Glass integration**: Host/guest guidance for IVSHMEM, KVMFR, Windows guests, and low-latency display capture.
- **Virtual networking**: Linux bridge, NAT, libvirt network, uplink, DHCP, capture, and monitoring workflows.
- **Support tooling**: Redacted support bundles, preflight checks, diagnostics output, and operational runbooks.
- **Observability**: Prometheus/Grafana examples and exporter-oriented documentation for lab and fleet monitoring.

## Quick Start

```bash
# Build the CLI and GUI
cargo build --release

# Inspect available commands
cargo run --bin nova -- --help

# Run host readiness checks
cargo run --bin nova -- support preflight

# Launch the GUI
cargo run --bin nova-gui
```

Common workflows:

```bash
# Generate a VM definition interactively
nova wizard vm win11 --preset gpu-labs --apply

# List known virtual machines and networks
nova list --all
nova network list

# Create a support bundle with redaction
nova support bundle --redact

# Inspect GPU passthrough readiness
nova gpu doctor
```

## NovaFile Example

```toml
project = "gpu-lab"

[vm.win11]
image = "/var/lib/nova/images/win11.qcow2"
cpu = 8
memory = "16Gi"
gpu_passthrough = true
network = "lab-nat"

[container.api]
capsule = "ubuntu:24.04"
volumes = ["./api:/srv/api"]
network = "lab-overlay"

[network.lab-nat]
type = "bridge"
profile = "nat"
subnet = "192.168.220.1/24"
dhcp_range = "192.168.220.50-192.168.220.150"
```

## Documentation

The documentation is organized by operational topic under [docs/README.md](docs/README.md).

| Area | Start Here |
| --- | --- |
| Commands | [docs/commands/commands.md](docs/commands/commands.md) |
| Networking | [docs/networking/overview.md](docs/networking/overview.md) |
| VFIO / GPU passthrough | [docs/vfio/overview.md](docs/vfio/overview.md) |
| Looking Glass | [docs/looking-glass/overview.md](docs/looking-glass/overview.md) |
| Wayland | [docs/wayland/overview.md](docs/wayland/overview.md) |
| GUI and themes | [docs/gui/overview.md](docs/gui/overview.md) |
| Operations and support | [docs/operations/overview.md](docs/operations/overview.md) |
| Project planning | [docs/project/overview.md](docs/project/overview.md) |

## Project Structure

```text
nova/
├── src/                  # Rust CLI, GUI, VM, network, GPU, and support modules
├── docs/                 # Topic-organized documentation
├── examples/             # NovaFile, Prometheus, Grafana, and systemd examples
├── packaging/            # Arch, Debian, Fedora, Flatpak, AppImage, systemd, udev
├── tests/                # Integration and feature tests
├── assets/               # Logo and icon assets
├── SECURITY.md           # Vulnerability reporting policy
└── CONTRIBUTING.md       # Development and documentation contribution guide
```

## Development

```bash
# Format and check
cargo fmt --all
cargo check

# Run tests
cargo test

# Audit dependencies
cargo audit
```

Nova currently targets current stable Rust and modern Linux hosts with KVM, libvirt, QEMU, and Wayland compositor support. Some GPU passthrough and Looking Glass workflows require host-specific kernel modules, IOMMU configuration, and guest setup.

## License

Nova is licensed under the [MIT License](LICENSE).
