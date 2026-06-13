# Contributing to Nova

Nova is a Rust systems project with a documentation-heavy workflow. Contributions should keep the repository clean, focused, and operationally useful.

## Development Setup

```bash
git clone <repo-url>
cd nova
cargo check
cargo test
```

Useful local checks:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features
cargo audit
```

## Contribution Guidelines

- Keep changes scoped. Separate code, dependency, packaging, and documentation work when possible.
- Follow existing module boundaries under `src/`.
- Prefer explicit error handling with useful context.
- Avoid broad rewrites unless the change is already agreed on.
- Do not commit generated build artifacts or local machine state.
- Preserve user-facing workflows documented under `docs/`.

## Documentation Standards

- Keep `docs/README.md` as the documentation index.
- Use lowercase, descriptive markdown filenames.
- Organize large subjects into folders such as `docs/networking/`, `docs/vfio/`, and `docs/looking-glass/`.
- Update links whenever a file moves.
- Put release planning, snapshots, and docs portal process under `docs/project/`.
- Put support, observability, diagnostics, and runbooks under `docs/operations/`.

## Pull Request Checklist

Before opening a PR:

- `cargo fmt --all` has been run.
- `cargo check` passes, or the failure is documented.
- Tests are added or updated for behavioral changes.
- Documentation is updated for user-facing changes.
- Security-sensitive changes note redaction, privilege, or command-execution impact.

## Review Priorities

Reviews focus on correctness, safety, maintainability, and operational clarity. Expect extra scrutiny for privileged host operations, network changes, passthrough workflows, support bundle contents, and dependency upgrades.
