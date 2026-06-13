# Nova Documentation

Nova documentation is organized by workflow. Keep the root of `docs/` small: this index should be the only top-level uppercase markdown file, and topic folders should use lowercase, descriptive filenames.

## Start Here

| Topic | Description |
| --- | --- |
| [commands/commands.md](commands/commands.md) | CLI command reference for VM, template, snapshot, network, storage, migration, and support workflows. |
| [networking/overview.md](networking/overview.md) | Virtual switch, bridge, NAT, capture, monitoring, and recovery documentation. |
| [vfio/overview.md](vfio/overview.md) | GPU passthrough, RTX 50-series, IOMMU, VFIO, and host readiness guidance. |
| [looking-glass/overview.md](looking-glass/overview.md) | Looking Glass architecture, install, configuration, tuning, and troubleshooting. |
| [wayland/overview.md](wayland/overview.md) | Wayland quick start and integration notes for the Nova GUI. |
| [gui/overview.md](gui/overview.md) | Theme, palette, and GUI design references. |
| [operations/overview.md](operations/overview.md) | Observability, support bundles, diagnostics, packaging, and operational runbooks. |
| [migration/from-virt-manager.md](migration/from-virt-manager.md) | Migration guide for users moving from virt-manager. |
| [project/overview.md](project/overview.md) | Roadmap, release process, docs portal notes, and project snapshots. |

## Layout Rules

- Use lowercase filenames with hyphens: `networking/commands.md`, not `NETWORKING_COMMANDS.md`.
- Put large subjects in folders. Looking Glass, VFIO, networking, operations, GUI, and project process each own their own section.
- Keep root-level project files limited to `README.md`, `SECURITY.md`, `CONTRIBUTING.md`, `LICENSE`, and build/config files.
- Prefer task-oriented docs with commands, expected output, and troubleshooting notes.
- Link with relative paths so pages work on GitHub and in a future docs portal.

## Maintenance

When adding or moving docs:

1. Add the page under the correct topic folder.
2. Update this index and the relevant folder `overview.md`.
3. Check for stale links with `rg "old-file-name|OLD_FILE_NAME"`.
4. Keep release notes and planning material under `project/`.
