# Support Tooling

Nova support tooling collects enough context to debug VM, container, network, GPU, and host issues while keeping sensitive data redacted by default.

## Support Bundle CLI

The `nova support bundle` command collects diagnostics, logs, and environment metadata into a shareable archive.

```bash
# Generate a redacted support bundle
nova support bundle --output ./nova-support.tar.gz --redact

# Limit log collection to the last day
nova support bundle --output ./nova-support-24h.tar.gz --redact --since 24h
```

| Flag | Description |
| --- | --- |
| `--output <path>` | Destination archive path. |
| `--redact` | Scrub secrets, tokens, hostnames, MAC addresses, and IP addresses where supported. |
| `--since <duration>` | Limit log collection window. |
| `--include <group>` | Include optional collectors such as `network` or `metrics` when available. |

## Bundle Contents

- `logs/` - Nova, libvirt, QEMU, and container runtime logs.
- `metrics/` - Snapshot of relevant metrics when the exporter is enabled.
- `system/` - Kernel, CPU virtualization, distribution, and service state.
- `network/` - Bridge, interface, libvirt network, and firewall context.
- `gpu/` - PCI, driver, IOMMU, and VFIO readiness context.
- `metadata.json` - Bundle manifest, timestamps, Nova version, and checksums.

## Diagnostics Helpers

```bash
nova support preflight
nova gpu doctor
nova network list
nova support bundle --redact
```

Diagnostic commands should exit non-zero on failure and print actionable remediation steps. Prefer adding new checks to existing diagnostics before creating new one-off scripts.

## Triage Workflow

1. Reproduce or identify the failing workflow.
2. Run the narrow diagnostic command first, such as `nova gpu doctor` or `nova support preflight`.
3. Generate a redacted support bundle if local diagnostics are not enough.
4. Attach the specific command, failure output, and bundle checksum to the issue.
5. Update the relevant docs when the resolution exposes a repeatable troubleshooting step.

## Security Considerations

- Use `--redact` before sharing bundles outside a trusted environment.
- Validate redaction rules after adding new collectors.
- Do not include private keys, cloud credentials, VM disk contents, or guest secrets.
- Treat support archives as sensitive operational data even when redacted.

## Maintenance

- Add tests for new bundle collectors.
- Keep support bundle manifests stable enough for automated triage.
- Document new diagnostics in [overview.md](overview.md) or the relevant topic folder.
