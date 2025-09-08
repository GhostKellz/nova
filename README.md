## Nova

<div align="center">
  <img src="assets/icons/nova.png" alt="nova icon" width="128" height="128">

**Wayland-Native Virtualization & Container Manager**  
*Bare metal speed. Declarative control. GPU-first.*

</div>

---

## Badges

![Zig](https://img.shields.io/badge/Zig-v0.16-yellow?logo=zig)  
![VM](https://img.shields.io/badge/Virtualization-KVM%2FQEMU-blue?logo=linux)  
![Containers](https://img.shields.io/badge/Containers-Capsules-green?logo=docker)  
![Networking](https://img.shields.io/badge/Networking-Bridges%20%7C%20Overlay-orange?logo=networkx)  
![GUI](https://img.shields.io/badge/Interface-Wayland-purple?logo=wayland)  

---

## Overview

**Nova** is a high-performance virtualization and container orchestration platform built entirely in Zig. It unifies **KVM/QEMU virtual machines**, **lightweight Capsule containers**, and **software-defined networking** under a single declarative interface.

Designed for modern Linux environments, Nova delivers bare-metal performance with enterprise-grade features through both an intuitive CLI and a native Wayland GUI. Perfect for developers, homelabs, and production deployments seeking alternatives to traditional virtualization stacks.

---

## Key Features

- ⚡ **Zero-Overhead Runtime** – Zig's compile-time optimizations deliver consistent, predictable performance
- 🖥 **Enterprise Virtualization** – Full KVM/QEMU integration with advanced GPU passthrough capabilities  
- 📦 **Lightweight Capsules** – Container technology with built-in snapshots, persistence, and isolation
- 🧩 **Infrastructure as Code** – Declarative TOML configuration for reproducible, version-controlled deployments
- 🌐 **Software-Defined Networking** – Advanced bridge, overlay, and QUIC-based mesh networking
- 🎨 **Modern Interface** – Native Wayland GUI with real-time monitoring and intuitive controls
- 🔐 **Security-First Design** – Cryptographic signing, encrypted networking, and secure defaults  

---

## Example: NovaFile (TOML)

```toml
project = "dev-lab"

[vm.win11]
image = "/var/lib/nova/images/win11.qcow2"
cpu = 8
memory = "16Gi"
gpu_passthrough = true
network = "bridge0"

[container.api]
capsule = "ubuntu:22.04"
volumes = ["./api:/srv/api"]
network = "nova-net"
env.API_KEY = "secret"

[network.bridge0]
type = "bridge"
interfaces = ["enp6s0"]

[network.nova-net]
type = "overlay"
driver = "quic"
dns = true
```

---

## Quick Start

```bash
# Launch a virtual machine
nova run vm win11

# Start a container
nova run container api

# List all networks
nova net ls

# Create VM snapshot
nova snapshot vm win11

# View container logs
nova logs container api
``` 

## Roadmap

### Phase 1 – Core
- [ ] VM lifecycle management (KVM + QEMU via Zig bindings)  
- [ ] Capsule containers (namespaces + cgroups)  
- [ ] TOML NovaFile parser  
- [ ] Basic CLI (`nova run`, `nova ls`)  

### Phase 2 – Networking
- [ ] Bridge + tap device support  
- [ ] Overlay networks (QUIC via `zquic`)  
- [ ] Built-in service discovery (`zdns`)  
- [ ] Firewall rules + NAT  

### Phase 3 – GUI
- [ ] Wayland-native GUI (Nova Manager)  
- [ ] Resource graphs (CPU, memory, disk, network)  
- [ ] VM/Container lifecycle dashboard  
- [ ] Network topology viewer  

### Phase 4 – Advanced
- [ ] GPU passthrough (NVIDIA VFIO, SR-IOV)  
- [ ] Live migration between hosts  
- [ ] Cluster management with Surge integration  
- [ ] Declarative reproducible builds (Nix-inspired)  

---

## Comparisons

| Feature              | Virt-Manager | Proxmox | LXC | Docker | **Nova** |
|----------------------|--------------|---------|-----|--------|----------|
| Wayland-native GUI   | ❌           | ❌      | ❌  | ❌     | ✅ |
| VMs (KVM/QEMU)       | ✅           | ✅      | ❌  | ❌     | ✅ |
| Lightweight containers | ❌         | ✅      | ✅  | ✅     | ✅ (Capsules) |
| Declarative configs  | XML          | Conf    | Conf| YAML   | ✅ (TOML) |
| GPU passthrough      | Limited      | ✅      | ❌  | ❌     | ✅ |
| Overlay networking   | Limited      | ✅      | ❌  | ❌     | ✅ |
| Arch/NVIDIA focus    | ❌           | ❌      | ❌  | ❌     | ✅ |

---

## Architecture & Design Philosophy

Nova embodies the principles of modern systems design:

- **Performance First**: Zig's zero-cost abstractions and manual memory management eliminate runtime overhead
- **Declarative Infrastructure**: TOML-based configuration ensures reproducible and version-controlled deployments  
- **Unified Management**: Single interface for VMs, containers, and networking reduces operational complexity
- **Native Integration**: Built for Wayland compositors and modern Linux distributions

From single-node development environments to distributed production clusters, Nova provides the performance and developer experience that traditional virtualization platforms lack.

---

<div align="center">

✨ *Nova — Light up your compute universe.* ✨

</div>

