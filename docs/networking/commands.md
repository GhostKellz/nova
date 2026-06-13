# Networking Commands

Common Nova networking commands and host validation steps.

## Nova CLI

```bash
# List known networks
nova network list
nova net ls

# Create a persistent NAT bridge
nova net create lab0 --type bridge \
  --profile nat --uplink enp6s0 \
  --subnet 192.168.220.1/24 \
  --dhcp-range 192.168.220.50-192.168.220.150

# Attach a VM to a network
nova network attach win11 lab0

# Detach a VM from a network
nova network detach win11 lab0

# Inspect support data
nova support bundle --redact
```

## Host Checks

```bash
ip link show
bridge link
virsh net-list --all
nmcli connection show
sudo nft list ruleset
```

## Restart Recovery

Nova-managed network state is persisted under the user data directory when possible and falls back to system storage for daemon-managed runs.

```bash
# After restart, verify bridge and libvirt state
nova net ls
virsh net-list --all
ip addr show lab0
```

See [technical.md](technical.md) for architecture details and troubleshooting context.
