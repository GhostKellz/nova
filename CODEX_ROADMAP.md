# Nova Codex Roadmap · September 2025

A living execution plan that guides Nova from the current RC3 networking push through general availability.

---

## Phase Status Overview

| Phase | Target Window | Focus | Status |
|-------|---------------|-------|--------|
| Alpha | 2024 Q4 | Architecture & core lifecycle | ✅ Complete |
| Beta | 2025 Q1 | Feature completion & UX baseline | ✅ Complete |
| RC1 | 2025 Q2 | Libvirt-first VM stability | ✅ Complete |
| RC2 | 2025 Q3 | Container runtime maturity | ✅ Complete |
| RC3 | 2025 Q4 | Networking & infrastructure parity | ✅ Complete |
| RC4 | 2025 Q4 → 2026 Q1 | GUI empowerment & templates | ✅ Complete |
| RC5 | 2026 Q1 | Observability & documentation | 🟢 In progress |
| RC6 | 2026 Q1 → 2026 Q2 | Hardening & release packaging | ⚪ Planned |
| GA | 2026 Q2 | Launch readiness & hand-off | ⚪ Planned |

---

## Active Phase — RC5 Observability & Documentation (🟢)

### Objectives
- Deliver first-class observability: Prometheus exporter, Grafana dashboards, alerting recipes.
- Stand up a comprehensive documentation portal (install guides, NovaFile reference, troubleshooting playbooks).
- Equip support with log bundle tooling, diagnostics commands, and escalation workflows.
- Fold community feedback loops into RC5 deliverables (docs issue triage, telemetry-driven improvements).

### Workstreams
1. **Metrics Platform** – Prometheus scrape endpoint, exporter configuration, optional Grafana bundle.
2. **Documentation Portal** – mdBook/Docusaurus build pipeline, content migration, versioned publishing.
3. **Support Tooling** – Log bundle generator, `nova diag` commands, error glossary automation.
4. **Feedback & Adoption** – Community office hours, docs analytics, success metrics instrumentation.

### Sprint 0 Kickoff (Week 1)
- Create `rc5-observability` GitHub Project board with swimlanes and automation hooks.
- Assign DRIs:
  - Metrics Platform → @observability-crew
  - Documentation Portal → @docs-tiger-team
  - Support Tooling → @support-core
  - Feedback & Adoption → @community-ops
- Finalize metrics schema (host, VM, container, network) and alert thresholds.
- Choose documentation stack (mdBook vs Docusaurus) and bootstrap theme aligned with Nova branding.
- Schedule weekly metrics/docs sync plus bi-weekly customer advisory review.

### Upcoming Sprint Backlog (Weeks 2-3)
- Metrics Platform: implement exporter prototype, add integration tests, produce default Grafana dashboards.
- Documentation Portal: migrate existing markdown, set up navigation/search, add CI link checker + spell check.
- Support Tooling: design log bundle schema, implement `nova support bundle` CLI, document redaction story.
- Feedback & Adoption: launch docs feedback widget, prepare beta program announcements, collect telemetry for doc search queries.

### Dependencies & Staffing
- Requires telemetry endpoints from monitoring module (completed during RC4) plus network stats parity.
- Documentation stack hosting (GitHub Pages/S3) coordination with infra.
- Security review for log bundle contents (avoid sensitive data leakage).
- Design resources for docs theming and dashboard polish.

### Key Deliverables
- `nova-metrics` exporter enabled via config, with sample Grafana dashboards committed to `docs/`.
- Docs site (docs.nova.dev) with install, NovaFile spec, CLI reference, troubleshooting, and API overview.
- `nova support bundle` command bundling logs, configs, and diagnostics with redaction options.
- Community rollout plan: office hours, changelog updates, onboarding emails, docs analytics dashboard.

### Exit Criteria
- Metrics endpoint gated behind config flag, validated on Arch/Ubuntu hosts (single-node + multi-node) with automated tests.
- Documentation portal live with versioned releases and automated publishing pipeline in CI.
- Support tooling adopted by maintainers; sample ticket resolved using new bundle + docs flow.
- Satisfaction signals: ≥80% positive feedback from RC5 advisory group, no open `rc5-blocker` issues.

### Metrics & Signals
- Prometheus scrape success rate ≥99% across nightly CI runs.
- Docs site uptime via synthetic check; analytics capturing search-to-click success ≥70%.
- Support bundle usage tracked (opt-in telemetry) with <5% failure rate.
- Feedback backlog triaged within 48 hours, status visible on project board.

### Risks & Mitigations
- **Dashboard bloat** → curate default panels, document customization, run usability reviews.
- **Docs drift** → enforce docs-as-code workflow, add PR checklist items, automate stale content alerts.
- **Sensitive data leakage** → implement redaction rules, run security review, add `--scrub` flag to bundles.
- **Adoption lag** → pair metrics/docs release with webinars, provide quick-start videos, capture community questions.

### Coordination & Reporting
- Daily stand-up remains 10:30 UTC with rotating note taker; highlights mirrored to `SNAPSHOTS.md` weekly.
- Metrics/demo review every other Friday featuring exporter status and docs site walkthroughs.
- Risk register reviewed Mondays; blockers escalated in `#nova-rc5` with 24h SLA.
- Dedicated Grafana dashboard tracks exporter health, docs analytics, and support bundle adoption.

## Phase Retrospective — RC4 Experience Upgrade

- Completed: `nova wizard vm` CLI delivering diffable NovaFile snippets with apply/dry-run flows; GUI wizard foundations integrated.
- Delivered template metadata registry scaffolding and version checks; baseline gallery populated with curated stacks.
- Embedded real-time metrics panels and topology readouts; added async task executor improvements to avoid UI stalls.
- Accessibility audit closed high-contrast issues, keyboard navigation gaps, and notification UX polish; documentation drafted for RC5 hand-off.
- Lessons: invest in reusable validation libraries for both CLI and GUI, align template metadata workflow early with community maintainers, and co-schedule telemetry with GUI changes for testing efficiency.

## Phase Retrospective — RC3 Networking & Infrastructure Parity

- Completed: `nova network` CLI + libvirt commands, NetworkManager refresh, DHCP/NAT scaffolding, GUI status indicators, and namespace-backed integration suite.
- Lessons: invest early in host-variance testing (Arch + Ubuntu benches), maintain idempotent command design, and document prerequisites prominently.
- Follow-ups: finish publishing networking cookbook, expand dry-run/no-op options, and monitor autostart telemetry during RC4.

---

## Upcoming Phase — RC6 Hardening & Release Packaging (⚪)

### Focus Areas
- Security review of the expanded observability stack (Prometheus endpoints, support bundles) including threat modeling, dependency scanning, and RBAC validation.
- Packaging and distribution: Flatpak/AppImage/tarball builds, signed checksums/SBOM, systemd units, and guided upgrade tooling.
- Performance regression benchmarking across VM/container startup, exporter overhead, and docs site build times.

### Dependencies
- RC5 deliverables (metrics exporter, docs site, support tooling) stabilized and annotated with ownership.
- Infrastructure sign-off for artifact hosting (package repo, CDN) and key management for signing.
- Coordinated security review bandwidth (internal + external) and access to dependency SBOM tooling.

### Exit Criteria
- Security issues identified are mitigated or accepted with documented rationale; SECURITY.md updated.
- Reproducible build pipeline produces signed artifacts and SBOMs validated in CI and release dry runs.
- Performance baselines established with automated regression gates in CI (tolerances agreed upon with product).

---

## 1.0 GA — Launch Readiness (⚪)

### Goals
- Tag v1.0.0, publish release notes, migration guide, and hosted binaries.
- Stand up support channels (forum, issue triage rotations) and maintenance cadence.
- Capture retrospective and plan post-GA backlog (minor releases, long-term support strategy).

### Exit Criteria
- GA checklist signed by engineering, product, design, and support.
- Monitoring/alerting dashboards operational for production deployments.
- Post-release roadmap drafted for 1.1+ features.

---

## Cross-Phase Operating Principles

- **Quality Gates**: Every PR must pass `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test`, integration suites (network namespaces, libvirt smoke), and UI lint/screenshot tests once RC5 lands.
- **Documentation Discipline**: Update README excerpts, SNAPSHOTS, and docs site whenever behavior changes; include migration notes for breaking changes.
- **Telemetry First**: Instrument new features with `tracing` spans and metrics before feature freeze; avoid retrofitting after the fact.
- **Security Mindset**: Run `cargo audit` weekly, capture threat model updates per milestone, and document mitigations in SECURITY.md.
- **Feedback Loop**: Close the loop with community interviews and issue reviews at the end of each RC; feed insights into backlog grooming.

---

## Supporting Workstreams

- **Testing & Automation**: Maintain nightly regression matrix (Arch/Ubuntu, GPU presence, libvirt availability). Add scenario-specific fixtures as new features land.
- **Documentation & Comms**: Publish fortnightly status updates in `SNAPSHOTS.md`, keep ROADMAP and CODEX aligned, and prep release notes early.
- **Community & Support**: Label roadmap input (`roadmap-input`), track beta champions, and schedule bi-weekly office hours.
- **Risk & Issue Management**: Maintain risk register with owner/mitigation, review weekly during RC phases, and escalate blockers within 24h.

This codex stays dynamic—update it at the close of each sprint or milestone review so everyone shares the same execution picture.