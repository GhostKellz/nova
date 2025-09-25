# Nova Documentation Suite · RC5

Welcome to the RC5 "Observability & Documentation" documentation bundle. These files capture the work-in-progress guidance that powers the new docs portal, metrics stack, and support operations. Treat this directory as the source of truth for the mdBook/Docusaurus site that will go live at `https://docs.nova.dev` once RC5 ships.

## Structure

| File | Purpose |
| ---- | ------- |
| `observability.md` | Prometheus exporter, Grafana dashboards, and alerting runbooks. |
| `docs-portal.md` | How we build, lint, and publish the Nova documentation portal. |
| `support-tooling.md` | Details for the `nova support bundle` CLI, diagnostics, and escalation workflows. |
| `release-playbook.md` | Checklist for RC5 release notes, change management, and communications cadence. |
| `SNAPSHOTS.md` | (Existing) Weekly status digests mirrored from community calls. |

Each document is scoped to a portion of the RC5 initiative and written to be directly reusable inside the docs portal. Additions should follow a docs-as-code mindset: open a PR, ensure CI link checks pass, and reference the relevant RC5 workstream board item.

## Writing Conventions

- **Style guide**: Prefer actionable, task-oriented language. Use full sentences, avoid passive voice, and keep call-outs concise.
- **Front matter**: When migrating to the portal, add the appropriate Docusaurus/mdBook front matter. Keep these markdown files portable by avoiding tool-specific syntax here.
- **Links**: Use relative links within `docs/` so they render both on GitHub and in the generated site.
- **Command formatting**: Show shell commands in fenced blocks with comments instead of prose. Mark optional steps explicitly.
- **Versioning**: Note minimum Nova versions (`v0.1.0-rc5+`) when introducing new flags or CLI subcommands.

## Contribution Workflow

1. Create or update the markdown file in this directory.
2. Run the docs lint pipeline (link and spell checks) using `cargo xtask docs:lint` once it lands in CI.
3. Generate the static site locally via `npm run build` (Docusaurus) or `mdbook build` depending on the chosen stack.
4. Attach rendered screenshots for GUI-heavy docs snippets to the PR description.
5. Announce merged docs during the Monday RC5 stand-up and log the change in `SNAPSHOTS.md`.

## Open Tasks

- Add screenshots for the Nova metrics dashboard once the design polish lands.
- Backfill troubleshooting examples for common exporter/CLI misconfigurations.
- Draft a short video walkthrough for the docs portal landing page (tracked separately in the RC5 board).

Have feedback or found a gap? File an issue with the `roadmap-input` label and link back to the relevant section here.
