# Nova Roadmap · September 2025

## Current Snapshot

Nova has progressed from concept to a daily driver for hybrid virtualization engineers. Highlights:
- ✅ Rust core managers for VMs, containers, networks, and templates with declarative NovaFile support.
- ✅ Libvirt-backed VM lifecycle wired into both CLI and GUI, including template-assisted provisioning helpers.
- ✅ Container workflows through the Bolt-first runtime with Docker/Podman fallback and TemplateManager scaffolding.
- ✅ NetworkManager refresh with system bridge discovery, origin tracking, libvirt autostart toggles, and the new `nova network` CLI surface.
- ✅ Wayland-native GUI shell offering instance overview, themed experience, and unified logging/telemetry plumbing.

## Active Milestone — RC5 Observability & Documentation

_Target window: January – March 2026_

### Goals
- Ship first-class observability with a Prometheus exporter, Grafana dashboards, and alerting runbooks.
- Launch a versioned docs portal covering install, NovaFile reference, CLI/API usage, and troubleshooting.
- Keep the README/COMMANDS docs in sync with Material Ocean theming, Windows 11 presets, networking monitoring knobs, and GPU board workflows.
- Deliver support tooling (`nova support bundle`, diagnostics helpers) with redaction and escalation guidance.
- Close the feedback loop through office hours, docs analytics, and RC5 advisory outreach.

### Key Deliverables
- Configurable `nova-metrics` exporter with default Grafana dashboard pack checked into `docs/`.
- mdBook/Docusaurus site (docs.nova.dev) with search, link checking, and automated publishing in CI.
- Rolling documentation refresh from the repository root (README, COMMANDS, NETWORKING) so changes land before the full docs portal ships.
- `nova support bundle` and `nova diag` commands bundling logs/configs with scrub options and playbooks.
- Community rollout kit: onboarding guides, webinar plan, and feedback intake widgets feeding the roadmap board.

### Exit Criteria
- Exporter validated on Arch and Ubuntu benches with automated tests and ≥99% scrape success in nightly runs.
- Documentation portal live with versioned releases and release notes mirroring each tag.
- Support bundle workflow exercised by maintainers with <5% failure rate and security review sign-off.
- RC5 advisory group reports ≥80% satisfaction and no unresolved `rc5-blocker` issues remain.

## Recently Completed — RC4 Experience Upgrade

- ✅ `nova wizard vm` CLI and GUI flows deliver diffable NovaFile previews with dry-run and apply support.
- ✅ Template gallery launched with metadata registry, provenance badges, and curated starter stacks.
- ✅ Live metrics panels and topology visualizations landed in the Wayland UI without introducing stalls.
- ✅ Accessibility pass closed high-contrast gaps, keyboard traps, and notification polish feedback.
- ✅ Material Ocean theme, capture auto-scan controls, offline monitoring thresholds, and GPU board persistence/bulk actions keep the GUI feeling cohesive release to release.
- ✅ RC4 retrospective documented validation library reuse and telemetry alignment lessons for RC5.

## Earlier Milestone — RC3 Networking & Infrastructure Parity

- ✅ `nova network` CLI covers list/create/delete/attach/detach alongside libvirt autostart toggles.
- ✅ NetworkManager refresh detects system bridges, tracks origins, and keeps caches tidy for automation.
- ✅ DHCP/NAT scaffolding merged with NovaFile snippets and troubleshooting docs.
- ✅ GUI surfaces bridge health and autostart state to close the loop with CLI outputs.
- ✅ Integration suite runs inside network namespaces across Arch and Ubuntu benches, gating every merge.

## Next Up — RC6 Hardening & Release Packaging

### Planned Scope
- Security review of observability endpoints, support bundles, and dependency stack (threat modeling + mitigations).
- Packaging pipeline for Flatpak/AppImage/tarballs with signed checksums, SBOMs, and upgrade tooling.
- Performance regression gates for VM/container startup, exporter overhead, and docs build times.

### Dependencies
- RC5 deliverables stabilized with clear ownership and documentation in place.
- Infrastructure sign-off for artifact hosting, signing keys, and distribution mirrors.
- Security review bandwidth reserved across internal and community contributors.

### Tracking Progress
- Roadmap board continues to track burndown with RC6 swimlanes and exit criteria.
- Weekly risk register reviews call out security or packaging blockers for escalation.
- Snapshot updates highlight performance benchmarks and package availability as they land.

Have an idea or need a capability sooner? Open an issue labeled `roadmap-input` and join the next community sync—we iterate in the open.