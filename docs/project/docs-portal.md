# Documentation Publishing

Nova docs are plain markdown in this repository. The folder layout should stay useful on GitHub and be easy to publish through a static documentation site later.

## Source Layout

```text
docs/
  README.md
  commands/
  networking/
  vfio/
  looking-glass/
  wayland/
  gui/
  operations/
  project/
  migration/
```

## Rules

- Keep filenames lowercase and descriptive.
- Put large subjects in topic folders.
- Keep `docs/README.md` as the main index.
- Use relative links.
- Keep user-facing docs evergreen. Avoid temporary release labels and internal team names.
- Document commands with expected context, not just raw command lists.

## Local Checks

```bash
rg -n "COMMANDS.md|NETWORKING.md|COLORS.md|ROADMAP.md|WAYLAND_|SNAPSHOTS|RC[0-9]|rc[0-9]" docs README.md
find docs -type f | sort
find docs -type f | awk -F/ '{print $NF}' | rg '[A-Z]'
```

## Future Static Site

If the docs are published with Docusaurus, mdBook, or another generator, keep generated site files out of the source docs tree unless they are required for development. The markdown files in `docs/` should remain readable without the site generator.

## Content Review

Review docs changes for:

- Current command names and flags.
- Correct paths after file moves.
- Security-sensitive output or examples.
- Screenshots or assets that need updating.
- Old planning language that should be moved to issues instead of docs.
