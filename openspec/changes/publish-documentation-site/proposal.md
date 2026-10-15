## Why

CLAUDE.md already commits to a documentation site ("Docs will be added to
`docs/site/`... versioned, with the latest version showing by default...
installation instructions, including gotchas, caveats, and workarounds for
platform issues, such as macOS quarantine") but nothing exists yet. Knot
(the companion desktop app in this same product family) already built this
exact capability — Jekyll site under `docs/site`, GitHub Pages via GitHub
Actions, version selector with a `latest` default, and scoped
`docs/site/AGENTS.md` authoring rules — and it works well there. This
change ports that pattern to `dtrpg-app/rust`, adapted to Libri's own
theme tokens, fonts, and icon rather than reusing Knot's.

## What Changes

- Add a Jekyll documentation site rooted at `docs/site`, published to
  GitHub Pages via a GitHub Actions workflow on pushes to `develop` (this
  repo's integration branch — see `docs/git-flow.md`) that touch
  `docs/site` or the workflow itself.
- Add a `latest/` content tree plus a `_data/versions.yml` manifest so the
  site supports versioned releases later without restructuring; seed with
  `latest` only for now (no prior release has shipped with documentation,
  so there is no historical snapshot to backfill).
- Add `docs/site/AGENTS.md`: scoped authoring rules for structure, front
  matter, Markdown/Liquid syntax, assets, local validation, and the
  publishing trigger — mirroring Knot's `docs/site/AGENTS.md`.
- Seed real content: a landing page, an installation guide (covering the
  macOS quarantine/unsigned-build gotcha CLAUDE.md already calls out), and
  enough structure that a reader lands somewhere useful, not a stub.
- Derive the site's visual identity (colors, fonts) from Libri's own theme
  system (`crates/dtrpg-ui/src/data/theme.rs`'s Parchment/Ink palettes,
  `data/constants.rs`'s font constants) and app icon
  (`assets/icon/icon.png`), not Knot's.

## Capabilities

### New Capabilities

- `documentation-publishing`: Builds and publishes the Jekyll documentation
  site from `docs/site` to GitHub Pages.
- `documentation-authoring`: Defines the documentation structure and rules
  agents and contributors follow when creating or updating site content.
- `documentation-versioning`: Serves `latest` by default and provides a
  version selector ready for future releases, even though only one version
  exists today.

### Modified Capabilities

None.

## Impact

- New: `docs/site/**` (Jekyll config, layouts, includes, data, assets,
  content), `docs/site/AGENTS.md`, `docs/site/PUBLISHING.md`.
- New: `.github/workflows/docs-pages.yml`.
- No changes to `crates/**` — this is documentation-only; the site reuses
  (reads, does not modify) theme/icon source already in the repo.
- Repository setting: GitHub Pages must be configured to deploy from
  GitHub Actions (documented in `PUBLISHING.md`, not something this change
  can set itself).
