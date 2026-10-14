## Context

See `proposal.md` for motivation and the three specification deltas for
behavior. `dtrpg-app/rust` has no documentation-site capability yet.
Knot (`pilgrimagesoftware/Knot-App`) already shipped the same capability
for its own Rust desktop app and its OpenSpec history
(`openspec/changes/archive/2026-10-11-docs-github-pages` there) records
the design reasoning this change reuses — cited inline below rather than
re-derived, since the constraints (static GitHub Pages deployment, no
server, versioned content without a CMS) are identical.

## Goals / Non-Goals

**Goals:**

- Build `docs/site` with Jekyll in GitHub Actions, deployed through the
  GitHub Pages artifact/deployment actions, triggered on pushes to
  `develop` (this repo's continuously-integrated branch — see
  `docs/git-flow.md`) that touch `docs/site` or the workflow file.
- Group authored content under `latest/`, with a `_data/versions.yml`
  manifest declaring supported versions — seeded with just `latest` today.
- Make the root path resolve to `latest` and render a version selector
  from the manifest, ready for a `vX.Y` snapshot whenever one is cut.
- Derive the site's colors and fonts from Libri's own theme system
  (`crates/dtrpg-ui/src/data/theme.rs`, `data/constants.rs`) and app icon
  (`assets/icon/icon.png`), not Knot's — same mechanism, different source.
- Place agent-facing authoring instructions next to the site source
  (`docs/site/AGENTS.md`).

**Non-Goals:**

- A documentation CMS or dynamic rendering.
- Deriving historical documentation from Git tags — there is no prior
  tagged release with documentation to backfill, so this starts at one
  version (`latest`) rather than inventing retroactive snapshots.
- A page-for-page content audit across versions (moot with one version).
- PR preview deployments.

## Decisions

### Use Jekyll with a repository-managed Pages workflow (same as Knot)

Built with Jekyll in GitHub Actions, deployed with the official
`actions/upload-pages-artifact` + `actions/deploy-pages`. `pages: write`
and `id-token: write` permissions, triggered on `develop`, deploy gated on
a successful build. GitHub's branch-source Pages build was considered and
rejected for the same reason Knot's design record gives: less control over
the Jekyll toolchain and no deployment-verification step.

### Keep versions as static source trees with a manifest

`docs/site` holds a `latest/` source tree and a `_data/versions.yml`
manifest listing supported versions in display order. The build emits
`/latest/...`; the root redirects there. When a release ships with a
documentation snapshot worth freezing, add a `vX.Y/` tree and a manifest
entry — the mechanism already supports it, so no restructuring is needed
later, only new content.

### Render static version navigation with a graceful page fallback

Mirrors Knot's `_includes/version-selector.html`: a `<select>` that
navigates to the chosen version's landing page via `onchange`, plus a
`<noscript>` fallback list of direct links. With only one version
currently published, the selector renders a single entry — exercising the
same code path a second version will use, rather than adding version
support only once it's needed.

### Define a shared Jekyll layout and application-derived design tokens

One `_layouts/default.html` and a `site.css` stylesheet with CSS custom
properties sourced from Libri's theme. Libri ships six named themes
(Parchment, Slate, Sage, Ink, Moss, Rosewood) across light/dark; the site
uses Parchment (light) and Ink (dark) — the two the app treats as primary
light/dark pairs (`ThemeKey::default()` is Parchment; Ink is the first
listed dark theme) — selected via `prefers-color-scheme`, with a manual
override picker matching Knot's pattern (`localStorage`-backed, three-way
System/Light/Dark). Fonts: `DEFAULT_BODY_FONT`/`DEFAULT_LABEL_FONT`/
`DEFAULT_MONO_FONT` per-platform fallback chains
(`crates/dtrpg-ui/src/data/constants.rs`) rather than a Google Fonts
import — Hoefler Text and Gotham aren't web-licensable, and the app's own
fallback chains (`Georgia, serif` / system sans / `Menlo, Consolas,
monospace`) already describe exactly what the site should render without
an external font fetch.

### Put authoring rules under the site root

`docs/site/AGENTS.md` defines directory roles, front matter, Markdown,
Liquid boundaries, linking, assets, version snapshots, local Jekyll
validation, and publishing — same shape as Knot's, content adapted to
this app's structure.

## Risks / Trade-offs

- [Only one version exists, so the versioning mechanism is unexercised by
  real content] -> Build it correctly now per the spec deltas (manifest,
  selector, fallback) rather than deferring; verify the single-entry case
  explicitly so adding a second version later is purely additive.
- [Application styling changes over time] -> Centralize token values in
  one stylesheet layer and document their source locations in
  `AGENTS.md`, same mitigation Knot's design record uses.
- [GitHub Pages isn't configured on this repo yet] -> Document the
  repository Pages source setting in `PUBLISHING.md` and let the workflow
  fail visibly until deployment is enabled, rather than silently no-op.
- [Local and CI Jekyll builds differ] -> Pin Ruby/Bundler versions in
  `Gemfile`/`Gemfile.lock` and use the identical `bundle exec jekyll
  build` command in both places.

## Migration Plan

1. Add the Jekyll source tree, `latest/` seed content (landing page,
   install guide), shared layout, stylesheet, and version manifest.
2. Add `docs/site/AGENTS.md` and `docs/site/PUBLISHING.md`.
3. Add the GitHub Pages workflow; document the required repository Pages
   setting (deploy from GitHub Actions).
4. Verify locally: `bundle install && bundle exec jekyll build` from
   `docs/site`, confirm root output redirects to `latest`, confirm the
   version selector renders and links resolve.
5. Roll back a faulty deployment by reverting the publishing commit or
   rerunning the workflow from the last known good commit on `develop`.
