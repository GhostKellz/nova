# Nova Packaging

This directory contains packaging files for multiple Linux distributions.

## Supported Distributions

### ⭐ Arch Linux (Premier Support)
**Status:** Full support, primary target, AUR package available

**Install:**
```bash
yay -S nova-virtualization
```

**Build from source (Make target):**
```bash
make -C packaging arch
```

**Files:**
- `arch/PKGBUILD` - AUR package definition
- `arch/nova.install` - Post-install hooks
- `arch/.SRCINFO` - AUR metadata

**Features:**
- nvidia-open kernel module support
- Wayland KDE/GNOME integration
- Btrfs/ZFS support
- Looking Glass integration
- Complete ML/AI templates

---

### Debian / Pop!_OS / Ubuntu
**Status:** Community support, DEB packages

**Install:**
```bash
# From .deb package
sudo dpkg -i nova-virtualization_0.1.0_amd64.deb

# Install dependencies
sudo apt-get install -f
```

**Files:**
- `debian/control` - Package metadata
- `debian/rules` - Build rules
- `debian/nova-virtualization.postinst` - Post-install script

**Build:**
```bash
# Install build dependencies
sudo apt-get install debhelper cargo rustc libvirt-dev

# Build package
cd /data/projects/nova
dpkg-buildpackage -us -uc -b

# Install
sudo dpkg -i ../nova-virtualization_0.1.0_amd64.deb
```

**Pop!_OS Notes:**
- Pop!_OS 22.04+ fully supported
- NVIDIA drivers: Use System76's nvidia-graphics-drivers
- Wayland works out of the box with Pop!_OS COSMIC

---

### Fedora / RHEL / CentOS Stream
**Status:** Community support, RPM packages

**Install:**
```bash
# From COPR (when available)
sudo dnf copr enable nova/nova-virtualization
sudo dnf install nova-virtualization

# Or build from source
sudo dnf install rpm-build rust cargo libvirt-devel
rpmbuild -bb packaging/fedora/nova-virtualization.spec
```

**Files:**
- `fedora/nova-virtualization.spec` - RPM spec file

**Build:**
```bash
# Install dependencies
sudo dnf install @development-tools rust cargo libvirt-devel

# Build RPM
cd /data/projects/nova
rpmbuild -bb packaging/fedora/nova-virtualization.spec

# Install
sudo dnf install ~/rpmbuild/RPMS/x86_64/nova-virtualization-*.rpm
```

---

### Flatpak (Beta)
**Status:** Beta (requires Flatpak runtime and Rust SDK extension)

**Build & Install:**
```bash
flatpak install flathub org.freedesktop.Platform//23.08 org.freedesktop.Sdk//23.08
flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable//23.08

# From repository root
flatpak-builder build/flatpak packaging/flatpak/com.nova.Virtualization.yml --user --install
flatpak run com.nova.Virtualization --version
```

**Manifest:** `flatpak/com.nova.Virtualization.yml`
- Uses local checkout as source (no network access required)
- Bundles both `nova` CLI and `nova-gui`
- Declares Wayland/X11 sockets, device passthrough, and logind/systemd D-Bus access for virtualization workflows

**Notes:**
- Requires Flatpak 1.12+ and the Rust SDK extension
- Builder outputs cached repo under `target/package-build/flatpak`
- Use `flatpak-builder --run build/flatpak com.nova.Virtualization.sh --help` for debugging

---

### AppImage (Beta)
**Status:** Beta (requires `appimage-builder`)

**Build:**
```bash
pip install --user appimage-builder
cd /data/projects/nova
appimage-builder --recipe packaging/appimage/AppImageBuilder.yml
```

**Output:** `target/package-build/appimage/Nova-*.AppImage`

**Notes:**
- Bundles `nova` and `nova-gui` with default configuration
- Uses `packaging/appimage/AppRun` launcher to expose GUI by default
- Requires `desktop-file-utils` (for `desktop-file-edit`) during build
- Run with `APPIMAGE_EXTRACT_AND_RUN=1 ./Nova-x86_64.AppImage --version` when FUSE is unavailable

## Common Files

### Systemd Services
- `systemd/nova.service` - Main Nova daemon
- `systemd/nova-metrics.service` - Prometheus metrics exporter

### Configuration
- `config/nova.conf` - Default configuration (TOML)

### Udev Rules
- `udev/99-nova-vfio.rules` - GPU passthrough device permissions

---

## Automation

The packaging directory now ships with automation helpers:

- `packaging/Makefile` — unified entrypoint for building Arch, Fedora, Flatpak, and AppImage artifacts under `target/package-build/`
- `packaging/scripts/smoke-tests.sh` — optional smoke test that validates package contents (requires corresponding tooling on the host)

Examples:

```bash
# Build every packaging target
make -C packaging all

# Run smoke tests (builds missing artifacts on demand)
make -C packaging smoke-test
```

---

## Building from Source (All Distros)

### Prerequisites

**Arch:**
```bash
sudo pacman -S base-devel rust cargo libvirt qemu-desktop
```

**Debian/Ubuntu:**
```bash
sudo apt install build-essential cargo rustc libvirt-dev qemu-system-x86
```

**Fedora:**
```bash
sudo dnf groupinstall "Development Tools"
sudo dnf install rust cargo libvirt-devel qemu-kvm
```

### Build

```bash
git clone https://github.com/nova-project/nova
cd nova

# Build release binaries
cargo build --release \
    --bin nova \
    --bin nova-gui \
    --features "gpu-passthrough,btrfs,prometheus"

# Install
sudo cp target/release/nova /usr/bin/
sudo cp target/release/nova-gui /usr/bin/
sudo cp packaging/systemd/*.service /usr/lib/systemd/system/
sudo cp packaging/config/nova.conf /etc/nova/
```

---

## Distribution-Specific Notes

### Arch Linux
- **nvidia-open** is recommended over proprietary drivers
- Use `yay` or `paru` for AUR installation
- KDE Plasma + Wayland works perfectly with GPU passthrough
- Btrfs is the default filesystem on many Arch installations

### Pop!_OS
- System76 provides excellent NVIDIA driver support
- COSMIC desktop (Pop Shell) integrates well with Nova GUI
- Pop!_OS 22.04 LTS recommended
- GPU passthrough works great with System76's kernel patches

### Ubuntu
- Use Ubuntu 22.04 LTS or 24.04 LTS
- Install `nvidia-driver-535` or later for GPU passthrough
- Wayland may need manual enabling on Ubuntu
- Consider using Pop!_OS for better NVIDIA support

### Fedora
- Excellent KVM/QEMU support out of the box
- Install from RPM Fusion for better multimedia support
- Wayland is default on Fedora Workstation
- SELinux policies included

---

## Feature Matrix by Distribution

| Feature | Arch | Debian | Pop!_OS | Ubuntu | Fedora |
|---------|------|--------|---------|--------|--------|
| **GPU Passthrough** | ✅ Full | ✅ Full | ✅ Full | ⚠️ Manual | ✅ Full |
| **nvidia-open** | ✅ Yes | ⚠️ Manual | ✅ Yes | ⚠️ Manual | ⚠️ Manual |
| **Wayland Native** | ✅ Yes | ✅ Yes | ✅ Yes | ⚠️ Opt-in | ✅ Yes |
| **Btrfs Support** | ✅ Native | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Native |
| **ZFS Support** | ✅ AUR | ✅ Yes | ✅ Yes | ✅ Yes | ⚠️ Limited |
| **Looking Glass** | ✅ AUR | ⚠️ Build | ⚠️ Build | ⚠️ Build | ⚠️ Build |
| **Container Runtime** | ✅ All | ✅ All | ✅ All | ✅ All | ✅ Podman |

**Legend:**
- ✅ Fully supported out of the box
- ⚠️ Requires manual setup or not default
- ❌ Not supported

---

## Package Maintainers

### Arch Linux
- Maintainer: Nova Team
- AUR: https://aur.archlinux.org/packages/nova-virtualization
- Updates: Every release

### Debian
- Status: Community maintained
- Help wanted: Debian Developer sponsorship

### Fedora
- Status: Community maintained
- COPR: Coming soon

---

## Contributing Packaging

Want to help package Nova for your distribution?

1. Fork the repository
2. Add packaging files to `packaging/<distro>/`
3. Test thoroughly
4. Submit PR with documentation

**Needed:**
- OpenSUSE/SLES packaging
- Gentoo ebuild
- NixOS package
- Flatpak/AppImage universal builds

---

## Support

### Arch Linux (Primary)
- GitHub Issues: https://github.com/nova-project/nova/issues
- IRC: #nova on Libera.Chat
- Email: nova@example.com

### Other Distributions
- Community forum (coming soon)
- GitHub Discussions

---

**Note:** Arch Linux receives priority support and new features first. Other distributions are community-maintained with best-effort support.
