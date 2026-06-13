# Security Policy

## Supported Versions

Nova is pre-1.0 software. Security fixes are prioritized for the current `main` branch and the most recent tagged release when release branches exist.

## Reporting a Vulnerability

Do not open a public issue for a suspected vulnerability.

Report security concerns privately with:

- A clear summary of the issue.
- Affected component: CLI, GUI, support bundle, networking, VFIO, packaging, or documentation.
- Reproduction steps or proof of concept when available.
- Expected impact and any known mitigations.
- Nova commit, release tag, host distribution, kernel version, and relevant dependency versions.

If private repository security advisories are available, use that channel first. Otherwise contact the project maintainer through the repository owner profile.

## Handling Expectations

- Initial triage target: 72 hours.
- Confirmed issues receive a severity rating and remediation plan.
- Sensitive details stay private until a fix or mitigation is available.
- Credits are included in the advisory unless the reporter requests anonymity.

## Security-Sensitive Areas

Nova maintainers treat these areas as high risk:

- Support bundles and diagnostics redaction.
- VFIO, PCI, USB, and GPU passthrough helpers.
- Network creation, NAT, firewall, bridge, and tap device management.
- Privileged packaging hooks, udev rules, and systemd services.
- Guest configuration generation and command execution paths.

## Dependency Auditing

Run the Rust dependency audit before releases:

```bash
cargo audit
cargo update
cargo check
```

Unmaintained transitive dependencies should be removed when practical. Vulnerabilities with reachable impact should block release unless a documented mitigation exists.
