# RC5 Release Playbook

Use this checklist to shepherd Nova RC5 from final smoke-tests to public announcement.

## Milestone Gates

- ✅ RC5 project board shows 0 open `rc5-blocker` issues.
- ✅ Metrics exporter, docs portal, and support tooling all tagged with `rc5-ready`.
- ✅ Integration matrix (Arch/Ubuntu, libvirt present/absent) green for two consecutive nights.
- ✅ Documentation portal content merged and deployed to staging.
- ✅ Support rotation briefed on new tooling and escalation procedure.

## Release Week Timeline

| Day | Owner | Tasks |
| --- | ----- | ----- |
| T-5 | @release-captain | Freeze new feature merges, announce code freeze in `#nova`. |
| T-4 | @observability-crew | Final exporter/grafana regression run, snapshot dashboards for docs. |
| T-3 | @docs-tiger-team | Publish release notes draft, run link/spell checks, stage docs portal build. |
| T-2 | @support-core | Dry-run support bundle generation across environments, update knowledge base. |
| T-1 | @release-captain | Tag release candidate (`v0.1.0-rc5`), broadcast change log preview to advisory group. |
| T | All | Push final tag, deploy docs, publish announcement blog, host community call. |
| T+1 | @observability-crew | Monitor exporter telemetry, triage alerts, confirm dashboards healthy. |

## Release Artifact Checklist

- [ ] Git tag `v0.1.0-rc5` pushed and verified.
- [ ] Release notes published (`docs/releases/v0.1.0-rc5.md` + GitHub release body).
- [ ] Docs portal (prod) redeployed with RC5 sections visible.
- [ ] Sample Grafana dashboards bundled in `examples/grafana-nova.json` with version stamp.
- [ ] Support tooling CLI help updated (`nova support bundle --help`).
- [ ] `ROADMAP.md` and `CODEX_ROADMAP.md` both updated to mark RC5 as complete or in QA.

## Communications

- **Announcement Blog**: Focus on observability, documentation portal, and support automation wins.
- **Community Call**: Live demo of dashboards + docs portal; capture questions in `SNAPSHOTS.md`.
- **Social Media**: Schedule posts highlighting telemetry and docs improvements.
- **Internal Briefing**: Share release deck + key metrics with leadership.

## Post-Release Follow-up

- Gather feedback through the docs feedback widget and RC5 advisory surveys.
- Review `nova_docs_search_failure_ratio` and dashboard usage metrics after 72 hours.
- Triangulate community bug reports, prioritize hotfixes, and plan RC6 scope adjustments.
- Archive the RC5 project board and migrate unfinished items to RC6/GA backlog.

## Rollback Plan

1. Revert docs portal deployment to previous snapshot (`npm run deploy -- --tag v0.1.0-rc4`).
2. Disable exporter feature flag in NovaFile defaults and redeploy configs.
3. Announce rollback in `#nova` and the community call channel with remediation ETA.
4. Capture root cause in the incident template and assign DRIs.

Keep this playbook updated after each dry-run. Improvements should be noted in the RC5 retrospective and fed into RC6 planning.
