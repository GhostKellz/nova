# VFIO and GPU Passthrough

Nova's VFIO documentation covers host readiness, NVIDIA/AMD GPU passthrough, RTX 50-series requirements, IOMMU validation, driver binding, and Looking Glass handoff.

## Pages

- [rtx-50-series.md](rtx-50-series.md) - Blackwell/RTX 50-series passthrough checklist.
- [../looking-glass/overview.md](../looking-glass/overview.md) - low-latency display path for Windows guests.
- [../commands/commands.md](../commands/commands.md) - GPU diagnostics and support commands.

## Baseline Host Checklist

```bash
# Confirm virtualization support
lscpu | rg 'Virtualization|Hypervisor'

# Confirm IOMMU groups
find /sys/kernel/iommu_groups -type l

# Confirm VFIO modules
lsmod | rg 'vfio|vfio_pci|vfio_iommu_type1'

# Nova diagnostics
nova support preflight
nova gpu doctor
```

## Notes

- Prefer clean IOMMU grouping before using ACS override.
- Keep host and guest display paths separate when using a dedicated passthrough GPU.
- Use support bundles with redaction when sharing diagnostics.
