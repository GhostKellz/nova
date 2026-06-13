# Wayland Overview

Nova's GUI is designed for modern Wayland desktops while keeping host virtualization workflows visible and inspectable.

## Pages

- [quickstart.md](quickstart.md) - quick setup and launch flow.
- [integration.md](integration.md) - compositor integration, environment, and troubleshooting notes.

## Common Checks

```bash
echo "$XDG_SESSION_TYPE"
echo "$WAYLAND_DISPLAY"
cargo run --bin nova-gui
```
