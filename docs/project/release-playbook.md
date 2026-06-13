# Release Playbook

Use this checklist when preparing a Nova release. Keep the process version-neutral so it can be reused for every tag.

## Readiness Gates

- All required tests pass on supported Linux targets.
- Dependency audit is clean or documented with an accepted mitigation.
- Packaging artifacts build successfully.
- User-facing docs match current CLI, GUI, and configuration behavior.
- Support bundle and diagnostic commands work on a clean host.

## Validation

```bash
cargo fmt --all
cargo check
cargo test
cargo audit
cargo clippy --all-targets --all-features
```

Package-specific checks should also be run for any changed packaging target under `packaging/`.

## Artifact Checklist

- Git tag and release notes prepared.
- Source archive and binary/package artifacts generated.
- Checksums generated for release assets.
- Packaging metadata updated where applicable.
- Example NovaFiles and dashboard examples still load.
- `README.md`, `docs/README.md`, and changed topic docs reviewed.

## Release Notes

Release notes should be practical and short:

- New user-visible features.
- Breaking changes and migration steps.
- Security fixes.
- Packaging or dependency changes.
- Known issues and workarounds.

Avoid internal project labels and stale planning language in release notes.

## Rollback

1. Stop publishing the affected artifact.
2. Restore the previous known-good package or tag.
3. Document the failure mode and affected users.
4. Publish a mitigation or hotfix plan.

## Post-Release

- Watch issue reports and support bundle failures.
- Update troubleshooting docs for repeated problems.
- Move unfinished work back into the roadmap or issue tracker.
