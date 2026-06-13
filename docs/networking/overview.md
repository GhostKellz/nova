# Networking Overview

Nova networking covers virtual switches, Linux bridges, libvirt networks, NAT, DHCP, uplinks, capture discovery, and monitoring.

## Pages

- [technical.md](technical.md) - architecture and implementation details for virtual networking.
- [commands.md](commands.md) - common networking CLI workflows and recovery commands.

## Common Workflows

- Create or inspect bridge-backed VM networks.
- Restore Nova-managed switches after daemon or host restart.
- Attach VMs to NAT, isolated, or external profiles.
- Tune capture auto-scan and monitoring intervals from the GUI.
- Collect diagnostics for bridge, libvirt, and firewall failures.
