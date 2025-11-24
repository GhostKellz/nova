# RTX 50-Series (Blackwell) Passthrough Playbook

The RTX 50-series introduces NVIDIA's Blackwell architecture. Nova now detects these GPUs and surfaces the minimum host requirements directly in diagnostics. Use this guide as a quick reference when validating passthrough builds.

## Host Requirements

- **Kernel:** Linux `6.9` or newer (CONFIG_PREEMPT_DYNAMIC recommended)
- **Drivers:** `nvidia-open` `560.0+` (preferred) or proprietary `560+`
- **Firmware:** Latest motherboard BIOS with Resizable BAR enabled
- **IOMMU:** `intel_iommu=on iommu=pt` (Intel) or `amd_iommu=on iommu=pt` (AMD)
- **VFIO:** `vfio`, `vfio_pci`, and `vfio_iommu_type1` modules loaded
- **TCC Mode:** Enable TCC for Blackwell GPUs when using Looking Glass or low-latency streaming
  ```bash
  sudo nvidia-smi -g <index> -dm 1
  ```

## Validation Matrix

| Scenario | Expectations | Notes |
|----------|--------------|-------|
| Looking Glass (Primary GPU) | 1440p+ 144Hz via TCC | Make sure host compositor is Wayland with DMA-BUF enabled |
| Multi-GPU (50 + 40 Series) | Independent IOMMU groups per GPU | Configure ACS override only if absolutely required |
| NVENC Streaming | Works with `nvidia-open` 560+ | Validate with `ffmpeg -hwaccel cuda` inside guest |
| Fallback Mode | Virtio-GPU for host rendering | Nova flags this automatically if passthrough pre-checks fail |

## Nova Integration

- `nova gpu doctor` warns when a Blackwell GPU is detected without the required driver/kernel
- `nova support bundle` embeds detected GPU generation and recommended remediation
- Prometheus metrics expose `nova_gpu_generation{generation="blackwell"}` (todo) for fleet dashboards

## Quick Checklist

1. Update system and install `nvidia-open` `560+`
2. Reboot with kernel `6.9+` and IOMMU parameters
3. Enable TCC on the passthrough GPU
4. Run `nova gpu doctor` and ensure all checks pass
5. Launch Looking Glass / guest workload and monitor `journalctl -u nova`

For field issues, attach the support bundle (`nova support bundle --redact`) to bug reports.
