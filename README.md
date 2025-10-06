## Nova

<div align="center">
  <img src="assets/nova-logo.png" alt="Nova Logo" width="128" height="128">

**Wayland-Native Virtualization & Container Manager**  
*Bare metal speed. Declarative control. GPU-first.*

</div>

---

## Badges

![Rust](https://img.shields.io/badge/Rust-1.82+-orange?logo=rust)  
![VM](https://img.shields.io/badge/Virtualization-KVM%2FQEMU-blue?logo=linux)  
![Containers](https://img.shields.io/badge/Containers-Capsules-green?logo=docker)  
![Networking](https://img.shields.io/badge/Networking-Bridges%20%7C%20Overlay-orange?logo=networkx)  
![GUI](https://img.shields.io/badge/Interface-Wayland-purple?logo=wayland)  

---

## Overview

**Nova** is a high-performance virtualization and container orchestration platform built entirely in Rust. It unifies **KVM/QEMU virtual machines**, **lightweight Capsule containers**, and **software-defined networking** under a single declarative interface.

Designed for modern Linux environments, Nova delivers bare-metal performance with enterprise-grade features through both an intuitive CLI and a native Wayland GUI. Perfect for developers, homelabs, and production deployments seeking alternatives to traditional virtualization stacks.

---

## Key Features

- ⚡ **Zero-Overhead Runtime** – Rust's compile-time optimizations deliver consistent, predictable performance
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

## Networking Persistence & Recovery

Nova now keeps track of managed switches so they survive daemon or host restarts. Key points:

- Every Nova-managed bridge is serialized to `~/.local/share/nova/networks/<switch>.json` (falling back to `/var/lib/nova/networks` when XDG paths are unavailable).
- On startup the networking subsystem recreates missing bridges, reapplies NAT/DHCP settings, and reattaches uplinks for `external` and `nat` profiles.
- The CLI supports profile-aware creation. Example:

```bash
nova net create hyperv0 --type bridge \
  --profile nat --uplink enp6s0 \
  --subnet 192.168.220.1/24 --dhcp-range 192.168.220.50-192.168.220.150
```

### Verifying restart recovery

1. Create a persistent switch (see above) and confirm the JSON state file exists.
2. Restart the Nova service or reload the binary.
3. Run `nova net ls` — the bridge should report `Active`, NAT masquerade should be present, and uplinks should still be attached.
4. For automated regression coverage run:

```bash
cargo test network::tests::hydrate_restoration_behaviors
```

## Roadmap

### Phase 1 – Core
- [ ] VM lifecycle management (KVM + QEMU via Rust bindings)  
- [ ] Capsule containers (namespaces + cgroups)  
- [ ] TOML NovaFile parser  
- [ ] Basic CLI (`nova run`, `nova ls`)  

### Phase 2 – Networking
- [ ] Bridge + tap device support  
- [ ] Overlay networks (QUIC via `quinn`)  
- [ ] Built-in service discovery (`trust-dns`)  
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

- **Performance First**: Rust's zero-cost abstractions and memory safety eliminate runtime overhead
- **Declarative Infrastructure**: TOML-based configuration ensures reproducible and version-controlled deployments  
- **Unified Management**: Single interface for VMs, containers, and networking reduces operational complexity
- **Native Integration**: Built for Wayland compositors and modern Linux distributions

From single-node development environments to distributed production clusters, Nova provides the performance and developer experience that traditional virtualization platforms lack.

---

<div align="center">

✨ *Nova — Light up your compute universe.* ✨

</div>

