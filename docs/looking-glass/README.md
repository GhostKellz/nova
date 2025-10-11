# Looking Glass Integration for Nova

## What is Looking Glass?

Looking Glass is an open-source application that allows you to use your main computer with a full performance VM seamlessly. It provides near-native performance by sharing the GPU framebuffer directly between the host and guest via shared memory (IVSHMEM), eliminating the overhead of traditional remote desktop protocols.

## Why Use Looking Glass?

- **Near-Zero Latency**: Direct framebuffer access via shared memory
- **Full GPU Performance**: The guest GPU is passed through entirely to the VM
- **Seamless Integration**: Run Windows games/applications in a VM with host-level performance
- **No Network Overhead**: Unlike SPICE/VNC, no network stack is involved
- **Privacy**: No cloud/network services, all processing happens locally

## Use Cases

### Gaming
- Run Windows-only games on Linux host with near-native performance
- Avoid anti-cheat detection issues with dual-boot
- Stream gameplay to OBS while maintaining low latency

### Development
- Test Windows applications while working in Linux
- Develop cross-platform applications
- Isolate potentially untrusted software

### Productivity
- Run Windows-specific applications (Adobe Suite, AutoCAD, etc.)
- Maintain Linux workflow while accessing Windows tools
- Separate work and personal environments

## How It Works

```
┌─────────────────────────────────────────────────────────┐
│                      Host System                        │
│  ┌────────────────┐          ┌─────────────────────┐   │
│  │  Looking Glass │          │   /dev/shm/lg       │   │
│  │     Client     │◄────────►│   (Shared Memory)   │   │
│  └────────────────┘          └─────────────────────┘   │
│         │                             ▲                 │
│         │                             │                 │
│         ▼                             │                 │
│  ┌────────────────────────────────────┼─────────────┐  │
│  │            IVSHMEM Device          │             │  │
│  │  (Inter-VM Shared Memory)          │             │  │
│  └────────────────────────────────────┼─────────────┘  │
│                                        │                 │
│  ┌─────────────────────────────────────────────────┐   │
│  │              Windows Guest VM                   │   │
│  │  ┌───────────────────────────────────────────┐  │   │
│  │  │      Looking Glass Host Application       │  │   │
│  │  │  (Captures GPU framebuffer)               │  │   │
│  │  └───────────────────────────────────────────┘  │   │
│  │                      │                           │   │
│  │                      ▼                           │   │
│  │  ┌───────────────────────────────────────────┐  │   │
│  │  │       Dedicated GPU (Passthrough)         │  │   │
│  │  │   Renders frames → IVSHMEM → Host         │  │   │
│  │  └───────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Architecture

### Components

1. **Host Client (Linux)**: `looking-glass-client`
   - Reads framebuffer from shared memory
   - Renders on host display
   - Handles input capture and forwarding

2. **Guest Application (Windows)**: `looking-glass-host.exe`
   - Captures GPU framebuffer
   - Writes to shared memory
   - Manages IVSHMEM device

3. **IVSHMEM Device**: Inter-VM Shared Memory
   - PCI device exposing shared memory region
   - Allows zero-copy framebuffer transfer
   - Configured via libvirt/QEMU

4. **KVMFR Module** (Optional): Kernel Virtual Machine FrameBuffer Relay
   - Provides optimized shared memory access
   - Reduces latency further
   - Available via AUR: `looking-glass-module-dkms`

## Requirements

### Host (Linux) Requirements

- **CPU**: Intel VT-x or AMD-V virtualization support
- **Motherboard**: IOMMU support (VT-d for Intel, AMD-Vi for AMD)
- **GPU**: Two GPUs recommended:
  - One for host (can be integrated)
  - One for guest passthrough (dedicated GPU)
- **Memory**: At least 8GB RAM (16GB+ recommended)
- **OS**: Linux with KVM/QEMU/Libvirt
- **Software**:
  - `libvirt` and `qemu`
  - `looking-glass-client` (from AUR on Arch)
  - Optional: `looking-glass-module-dkms` for KVMFR

### Guest (Windows) Requirements

- **OS**: Windows 10/11 (v1803 or later)
- **GPU Drivers**: Latest NVIDIA/AMD drivers
- **Software**: Looking Glass Host Application

## Performance Expectations

With proper configuration, you can expect:

- **Latency**: 1-5ms additional latency over bare metal
- **FPS**: Within 5-10% of native performance
- **Resolution**: Up to 8K @ 60Hz (hardware dependent)
- **Refresh Rate**: Up to 240Hz (with compatible hardware)

## Quick Start

```bash
# 1. Install Looking Glass client
yay -S looking-glass looking-glass-module-dkms

# 2. Configure VM with Nova
nova vm create win11-gaming --gpu 0000:01:00.0 --looking-glass

# 3. Install Windows and Looking Glass host app in guest

# 4. Launch Looking Glass client
looking-glass-client -f /dev/shm/looking-glass
```

## Documentation Index

1. **[Installation Guide](./installation.md)** - Step-by-step setup for Arch Linux
2. **[Configuration Guide](./configuration.md)** - Configure profiles and settings
3. **[Troubleshooting](./troubleshooting.md)** - Common issues and solutions
4. **[Performance Tuning](./performance-tuning.md)** - Optimize for best performance
5. **[Advanced Topics](./advanced.md)** - KVMFR, CPU pinning, huge pages

## Nova Integration

Nova provides first-class support for Looking Glass:

### Configuration Profiles

```rust
// Gaming profile: Low latency, relative mouse, no vsync
nova vm create gaming --looking-glass-profile gaming

// Productivity profile: Absolute mouse, vsync, higher resolution
nova vm create work --looking-glass-profile productivity

// Streaming profile: Balanced settings for capture
nova vm create stream --looking-glass-profile streaming
```

### CLI Commands

```bash
# Check system requirements
nova looking-glass check

# Setup Looking Glass for a VM
nova looking-glass setup <vm-name> --gpu 0000:01:00.0

# Configure shared memory
nova looking-glass shmem setup --size 128

# Launch client
nova looking-glass client <vm-name>

# Get Windows driver instructions
nova looking-glass windows-driver-setup
```

### GUI Integration

Nova's GUI provides:
- Visual system requirements checker
- One-click Looking Glass setup
- Configuration profile selection
- Integrated client launcher
- Real-time performance monitoring

## Support & Community

- **Official Website**: https://looking-glass.io/
- **Documentation**: https://looking-glass.io/docs/
- **Discord**: https://discord.gg/52SMupxkvt
- **GitHub**: https://github.com/gnif/LookingGlass
- **Wiki**: https://looking-glass.io/wiki/

## Contributing

Found an issue or want to improve Looking Glass integration in Nova?
- Report issues: https://github.com/your-repo/nova/issues
- Submit PRs: https://github.com/your-repo/nova/pulls
- Join discussions: https://github.com/your-repo/nova/discussions

## License

- Looking Glass: GPLv2+
- Nova: MIT/Apache-2.0 (check LICENSE file)

---

**Next Steps**: Start with the [Installation Guide](./installation.md) to set up Looking Glass on your system.
