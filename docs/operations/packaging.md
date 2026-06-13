# Packaging

Nova keeps packaging source files under `packaging/`. That directory is the source of truth for distro metadata, service units, udev rules, default config, package build helpers, and smoke tests.

Use this page as the maintainer-facing overview. For target-specific commands, see [../../packaging/README.md](../../packaging/README.md).

## Layout

```text
packaging/
├── README.md
├── Makefile
├── arch/
├── debian/
├── fedora/
├── flatpak/
├── appimage/
├── systemd/
├── udev/
├── config/
└── scripts/
```

## Policy

- Keep package manifests beside the files they build or install.
- Keep distro-specific metadata under its distro folder.
- Keep shared service units, udev rules, and default config in shared folders.
- Keep packaging smoke tests under `packaging/scripts/`.
- Do not add a top-level `release/` folder unless release automation grows beyond packaging concerns.

If release automation is needed, prefer `packaging/scripts/` for:

- artifact builds
- smoke tests
- checksums
- signing helpers
- package validation

## Supported Targets

| Target | Source |
| --- | --- |
| Arch / AUR | `packaging/arch/` |
| Debian / Ubuntu / Pop!_OS | `packaging/debian/` |
| Fedora / RHEL family | `packaging/fedora/` |
| Flatpak | `packaging/flatpak/` |
| AppImage | `packaging/appimage/` |
| systemd units | `packaging/systemd/` |
| udev rules | `packaging/udev/` |
| default config | `packaging/config/` |

## Common Commands

```bash
# Build all package targets supported by the local host tooling
make -C packaging all

# Build one target
make -C packaging arch
make -C packaging flatpak
make -C packaging appimage

# Run package smoke tests
make -C packaging smoke-test
```

## Release Artifact Expectations

Every packaged release should have:

- reproducible build inputs committed under `packaging/`
- package smoke test coverage where practical
- install and uninstall behavior checked
- systemd units validated
- udev rules reviewed for least privilege
- checksums for published artifacts
- signing when the release channel supports it

## Documentation Boundary

- Put exact package build commands in [../../packaging/README.md](../../packaging/README.md).
- Put operator-facing packaging policy and release expectations here.
- Link to this page from release or operations docs when packaging behavior changes.
