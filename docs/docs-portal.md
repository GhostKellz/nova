# Documentation Portal · Build & Publishing Guide

RC5 delivers a dedicated documentation portal at `https://docs.nova.dev`. This guide captures the tooling decisions, build steps, and governance rules that keep the site healthy.

## Stack Decisions

- **Generator**: Docusaurus 3 (mdBook fallback for offline bundles).
- **Language switcher**: Disabled for RC5; revisit post-GA when localization plans are formalized.
- **Theme**: Custom Nova palette aligned with the ocean-inspired GUI theme.
- **Search**: Algolia DocSearch (community tier) with nightly index refresh.
- **Link checking**: `npm run lint:links` (Docusaurus) + `cargo xtask docs:lint` for cross-language checks.

## Directory Layout

```
docs/
  README.md
  docs-portal.md          <-- this file
  observability.md
  support-tooling.md
  release-playbook.md
  SNAPSHOTS.md
  static/
    images/
      dashboards/
  sidebar.js              <-- generated during scaffold (committed)
```

When migrating these files into the portal structure, map them into `docs/guides/rc5/*.mdx` and add appropriate front matter:

```mdx
---
id: rc5-observability
slug: /rc5/observability
sidebar_label: Observability
---
```

## Local Development

```bash
# Install dependencies
npm install

# Start dev server with live reload
npm run start

# Build production assets (used in CI/CD)
npm run build
```

> **Note**: Use Node.js 20 LTS. The `docs/.nvmrc` file pins the version; run `nvm use` before building.

## CI/CD Pipeline

1. **Lint Stage**
   - `npm run lint:links`
   - `npm run lint:md` (markdownlint)
   - `cargo xtask docs:lint` (spellcheck, front-matter validation once implemented)
2. **Build Stage**
   - `npm run build`
   - Artifacts archived to `dist/`
3. **Deploy Stage**
   - Upload to GitHub Pages via `actions/deploy-pages@v4`
   - Mirror to the S3 CDN bucket (`nova-docs-prod`) with CloudFront invalidation

Promotion to production happens automatically from `main` after CI success. Preview builds are generated for every PR via the `pull_request` workflow.

## Content Governance

- Every docs PR must reference a tracking issue or RC5 board item.
- Add screenshots for UI changes and store them in `docs/static/images/`.
- Run `npm run format` before committing to maintain consistent MDX/JS formatting.
- Document behavioural changes in `SNAPSHOTS.md` and cross-link from release notes.
- For breaking changes, include a "Migration" section explaining upgrade steps and potential downtime.

## Analytics & Feedback

- Algolia analytics feed weekly search reports into the RC5 feedback workstream.
- `nova_docs_search_failure_ratio` metric (via the exporter) measures searches with no results; funnel into backlog triage within 48 hours.
- Embed the feedback widget (Hotjar fallback) on the docs portal; all submissions route to `#nova-rc5-docs`.

## Open Follow-ups

- Automate sidebar generation from the filesystem tree.
- Publish a short "Docs Portal Tour" video and embed it on the landing page.
- Evaluate adding an offline snapshot generator (`mdbook build`) for air-gapped environments.

Questions? Ping `@docs-tiger-team` in Slack or open a docs issue with the `docs-portal` label.
