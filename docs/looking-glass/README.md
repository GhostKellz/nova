# Looking Glass Integration for Nova

## Terminology Clarification

**IMPORTANT**: Before we begin, let's clarify the confusing terminology used by the Looking Glass project:

- **Linux HOST** = Your main operating system (Arch Linux, Fedora, PopOS/Debian) where Nova runs
- **Windows GUEST** = The Windows VM running **inside** your Linux host
- **Looking Glass Client** = Application that runs on your **Linux HOST** (your main desktop)
- **Looking Glass Host Application** = Application that runs **inside your Windows GUEST VM** (confusing name from the LG project!)

**Summary**: You run Linux (Arch/Fedora/PopOS) as your main OS. Windows runs inside a VM. Looking Glass lets you view the Windows VM's display with near-zero latency.

## What is Looking Glass?

Looking Glass is an open-source application that allows you to use your main Linux computer with a full-performance Windows VM seamlessly. It provides near-native performance by sharing the GPU framebuffer directly between the Linux host and Windows guest via shared memory (IVSHMEM), eliminating the overhead of traditional remote desktop protocols.

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
│              Linux HOST System (Arch/Fedora)            │
│                    (Your main desktop)                  │
│  ┌────────────────┐          ┌─────────────────────┐   │
│  │  Looking Glass │          │   /dev/shm/lg       │   │
│  │     Client     │◄────────►│   (Shared Memory)   │   │
│  │ (Linux binary) │          └─────────────────────┘   │
│  └────────────────┘                   ▲                 │
│         │                             │                 │
│         │  Displays on your monitor   │                 │
│         ▼                             │                 │
│  ┌────────────────────────────────────┼─────────────┐  │
│  │            IVSHMEM Device          │             │  │
│  │  (Inter-VM Shared Memory)          │             │  │
│  └────────────────────────────────────┼─────────────┘  │
│                                        │                 │
│  ┌─────────────────────────────────────────────────┐   │
│  │        Windows GUEST VM (runs inside Linux)     │   │
│  │  ┌───────────────────────────────────────────┐  │   │
│  │  │  Looking Glass Host Application (.exe)   │  │   │
│  │  │  (Runs inside Windows - captures frames) │  │   │
│  │  └───────────────────────────────────────────┘  │   │
│  │                      │                           │   │
│  │                      ▼                           │   │
│  │  ┌───────────────────────────────────────────┐  │   │
│  │  │   Dedicated GPU (Passthrough to VM)       │  │   │
│  │  │   Renders Windows display → IVSHMEM       │  │   │
│  │  └───────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Architecture

### Components

1. **Looking Glass Client** - Runs on your **Linux HOST** (your main system): `looking-glass-client`
   - Reads framebuffer from shared memory
   - Renders the Windows VM display on your Linux desktop
   - Handles input capture and forwarding to the VM

2. **Looking Glass Host Application** - Runs **inside your Windows GUEST VM**: `looking-glass-host.exe`
   - Captures GPU framebuffer inside the Windows VM
   - Writes to shared memory that the Linux host can read
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
