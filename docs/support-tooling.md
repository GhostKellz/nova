# Support Tooling · RC5 Runbook

RC5 equips the support core team with diagnostics, log bundling, and escalation workflows that reduce time-to-resolution for Nova operators.

## Support Bundle CLI

The `nova support bundle` command collects diagnostics, logs, and environment metadata into a redacted archive suitable for sharing with maintainers.

### Usage

```bash
# Generate a bundle with default redaction rules
nova support bundle --output ./nova-support-$(date +%Y%m%d).tar.gz

# Include experimental collectors and enable SCRUB mode
nova support bundle \
  --output ./nova-support-scrubbed.tar.gz \
  --include experimental \
  --scrub
```

| Flag | Description |
| ---- | ----------- |
| `--output <path>` | Destination archive path (defaults to `./nova-support-bundle.tar.gz`). |
| `--include <group>` | Optional collector groups (`experimental`, `network`, `metrics`). |
| `--scrub` | Apply redaction rules (secrets, tokens, MAC/IP addresses). |
| `--since <duration>` | Limit log collection window (e.g., `24h`). |

### Contents

- `logs/` – Nova daemon, exporter, libvirt, container runtime logs.
- `metrics/` – Snapshot of key gauges and counters (`nova_vm_status`, `nova_docs_search_failure_ratio`).
- `system/` – Kernel version, virtualization extensions, libvirt capabilities.
- `network/` – Output from `nova network inspect` and bridge membership.
- `metadata.json` – Bundle manifest with timestamps, Nova version, checksum.

After generation, the CLI prints the checksum and hints for secure upload. Bundle metadata is tracked by the `nova_support_bundle_generated_total` metric.

## Diagnostics Helpers

- `nova diag vm <name>` – Checks QEMU process health, libvirt domain, disk backing files, and recent events.
- `nova diag container <name>` – Verifies namespaces, process tree, and service reachability.
- `nova diag network <bridge>` – Validates bridge membership, STP status, and interface counters.

Each diagnostic command exits non-zero on failure and prints actionable remediation steps. Surface these commands in docs, release notes, and triage templates.

## Escalation Workflow

1. **Initial triage** (Community moderators)
   - Confirm Nova version (`nova --version` >= `0.1.0-rc5`).
   - Request support bundle with `--scrub` enabled.
   - Log issue in the support project board column `Needs Triage`.
2. **Engineering hand-off**
   - Assign DRI based on subsystem (VM, containers, networking, docs).
   - Import bundle locally via `nova support bundle extract <archive>`.
   - Document findings in the issue using the triage template.
3. **Resolution & Closure**
   - Link fix PRs and document steps in `SNAPSHOTS.md`.
   - Update the knowledge base (`docs/` + docs portal) with new troubleshooting guidance.
   - Close the issue with resolution summary and next-step automation (if any).

## Templates & Automation

- `.github/ISSUE_TEMPLATE/support.md` (tracked separately) includes prompts for bundle checksums and exporter status.
- The CI bot adds a `needs-bundle` label when diagnostics are missing, auto-pinging the reporter.
- The support rotation schedule lives in `SNAPSHOTS.md` with escalation contacts.

## Security Considerations

- Redaction rules scrub secrets, tokens, hostnames, MAC/IP addresses. Run `nova support bundle validate <archive>` before sharing externally.
- Bundle archives expire after 14 days in the Nova S3 bucket via lifecycle policies.
- For high-sensitivity environments, disable bundle upload automation and provide manual hand-off instructions.

## Next Steps

- Add integration tests covering bundle generation on Arch + Ubuntu benches.
- Publish a "Support Bundle 101" screen recording for new moderators.
- Extend diagnostic commands with remediation suggestions (e.g., auto-running `nova network repair`).

Feedback is welcome—tag `@support-core` in issues or ping the weekly RC5 support sync.
