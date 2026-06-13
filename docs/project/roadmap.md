# Nova Roadmap

This roadmap tracks product direction. Use issues or project boards for detailed scheduling.

## Current Focus

- Keep the Rust dependency stack current and audited.
- Maintain clean CLI and GUI support for KVM/QEMU, libvirt, containers, virtual networking, VFIO, and Looking Glass workflows.
- Keep documentation organized by user workflow instead of release artifact.
- Improve diagnostics and support bundles so users can report actionable issues quickly.
- Reduce warning and clippy noise so real regressions are easy to spot.

## Core Areas

### Virtual Machines

- VM lifecycle management through KVM/QEMU and libvirt.
- Declarative NovaFile-driven VM creation.
- Snapshot, clone, template, and migration workflows.
- Windows guest presets for TPM, secure boot, and GPU passthrough.

### Containers

- Lightweight container workflow support.
- Runtime fallback paths for Docker/Podman-compatible systems.
- Template-backed container setup where it helps repeatability.

### Networking

- Linux bridge and libvirt network management.
- NAT, isolated, and external network profiles.
- Persistent switch state and restart recovery.
- Monitoring and capture workflows for troubleshooting.

### VFIO and GPU Passthrough

- IOMMU and VFIO readiness checks.
- NVIDIA-focused diagnostics, including RTX 50-series guidance.
- Driver bind, unbind, reset, and reattach workflows.
- Looking Glass integration for low-latency Windows guests.

### GUI

- Wayland-native management shell.
- Dense operational views for instances, GPU passthrough, networking, logs, metrics, and support.
- Theme support without letting theme docs crowd operational docs.

### Operations

- Prometheus-compatible metrics.
- Grafana and alerting examples.
- Redacted support bundles.
- Preflight checks and diagnostics.

## Quality Goals

- `cargo fmt --all` passes.
- `cargo check` passes without warnings.
- `cargo test` passes.
- `cargo audit` passes.
- `cargo clippy --all-targets --all-features` passes without warnings.
- User-facing docs stay evergreen and free of stale release-campaign language.

## Packaging Goals

- Keep Arch, Debian, Fedora, Flatpak, AppImage, systemd, and udev packaging assets current.
- Publish checksums and package metadata for release artifacts.
- Keep packaging docs linked from the operations docs when they become user-facing.
