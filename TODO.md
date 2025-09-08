# TODO – Nova

Implementation roadmap for **Nova**:  
Wayland-native virtualization + container manager, written in Zig.

---

## Phase 1 – Core Runtime
- [ ] **VM Support**
  - [ ] Wrap KVM syscalls in Zig
  - [ ] Minimal QEMU integration (launch, stop, monitor)
  - [ ] Disk image support (qcow2, raw)
  - [ ] Basic VM lifecycle: start, stop, reboot, destroy
- [ ] **Capsules**
  - [ ] Namespaces: pid, net, mount, ipc, uts
  - [ ] Cgroups v2 resource limits (CPU, memory, IO)
  - [ ] Overlay filesystem support
  - [ ] Snapshot & restore for containers
- [ ] **Config**
  - [ ] NovaFile (TOML) schema
  - [ ] Parser + validation
  - [ ] Default paths for images, networks, volumes
- [ ] **CLI**
  - [ ] `nova run`
  - [ ] `nova ls`
  - [ ] `nova stop`
  - [ ] Logging basics

---

## Phase 2 – Networking
- [ ] **Bridge Networks**
  - [ ] Create and destroy Linux bridges
  - [ ] Attach VMs/containers to bridges
- [ ] **TAP / VETH Devices**
  - [ ] TAP device integration for VMs
  - [ ] VETH pairs for containers
- [ ] **Overlay Networks**
  - [ ] QUIC-based fabric via `zquic`
  - [ ] Built-in encrypted overlay support
- [ ] **DNS & Discovery**
  - [ ] Internal DNS resolver (`zdns`)
  - [ ] Service registration
  - [ ] Hostname resolution across VMs/containers
- [ ] **Firewall & NAT**
  - [ ] Basic NAT rules for outbound traffic
  - [ ] Port forwarding
  - [ ] Isolated network mode

---

## Phase 3 – GUI
- [ ] **Wayland-Native GUI**
  - [ ] Build Nova Manager with Wayland libraries
  - [ ] List VMs, containers, and networks
  - [ ] Start/stop/snapshot actions
- [ ] **Monitoring**
  - [ ] CPU, RAM, network usage graphs
  - [ ] Real-time log streaming
- [ ] **Network Topology Viewer**
  - [ ] Visualize networks, bridges, overlays
  - [ ] Interactive connection management

---

## Phase 4 – Advanced Features
- [ ] **GPU Passthrough**
  - [ ] VFIO integration
  - [ ] NVIDIA SR-IOV support
  - [ ] Hotplug GPU devices
- [ ] **Live Migration**
  - [ ] Move running VM between hosts
  - [ ] Disk state sync
  - [ ] Network session continuity
- [ ] **Cluster Mode**
  - [ ] Multi-node orchestration with Surge
  - [ ] Distributed scheduling
  - [ ] Remote NovaFile deployment
- [ ] **Declarative Builds**
  - [ ] Reproducible VM/container environments
  - [ ] Image caching + store
  - [ ] Build pipelines like Nix

---

## Phase 5 – Ecosystem Integration
## ALL libraries are main archive refs when you zig fetch --save https://github.com 
### ALL projects are in github.com/ghostkellz/PROJECTNAME  location
#### example https://github.com/ghostkellz/zcrypto 

- [ ] `zcrypto` → Secure capsule networking & signed configs
- [ ] `zquic` → Overlay networks, low-latency fabric
- [ ] `zdns` → Service discovery and DNS
- [ ] `zwl` → Wayland Library 
- [ ] `jaguar` -> GUI Framework for wasm, App Gui's etc github.com/ghostkellz/jaguar
- [ ] `zsync` → Async event loop for VM/container tasks

---

## Phase 6 – Stretch Goals
- [ ] Cross-platform support (Windows/macOS clients)
- [ ] WASM capsules
- [ ] GPU-accelerated containers
- [ ] Mobile/edge mode for IoT nodes

