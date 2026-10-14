## 1. Site foundation and authoring rules

- [x] 1.1 Inventoried Parchment/Ink palettes (`data/theme.rs`), font fallback
      chains (`data/constants.rs`), and the app icon
      (`assets/icon/icon.png`); recorded source locations in
      `docs/site/AGENTS.md`.
- [x] 1.2 Created `docs/site`'s Jekyll config, `Gemfile`/`Gemfile.lock`,
      shared layout, version-selector include, and `site.css` tokens;
      `bundle exec jekyll build` completes locally (verified with Homebrew
      Ruby 4.0.7 — system Ruby 2.6.10 is too old for Jekyll 4.3).
- [x] 1.3 Created `docs/site/AGENTS.md` with structure, front matter,
      Markdown/Liquid, links, assets, local validation, version-snapshot,
      and publishing rules.
- [x] 1.4 Seeded the `latest/` landing page and an installation guide
      (covering the macOS Gatekeeper / Windows SmartScreen unsigned-build
      warnings); local render confirmed navigation, styles, icon, and
      appearance picker all present in generated output.

## 2. Versioned documentation navigation

- [x] 2.1 Added `_data/versions.yml` with a single `latest` entry
      (`pages` + `nav` lists for the three seeded pages).
- [x] 2.2 Implemented the root-to-latest redirect and version-aware
      layout; confirmed built root `index.html` redirects to `/latest/`.
- [x] 2.3 Implemented the version selector (`<select>` + `<noscript>`
      fallback); confirmed it renders correctly with one entry.
- [x] 2.4 Implemented the page-preserving fallback: the selector computes
      the target version's matching page path when the current page's
      `slug` is in that version's `pages` list, falling back to the
      version's landing page otherwise. Confirmed on the built
      `/latest/install/` page: the selector option points at
      `/latest/install/`, not `/latest/`.

## 3. GitHub Pages publication

- [x] 3.1 Added `.github/workflows/docs-pages.yml`: builds `docs/site`
      with Jekyll, uploads the Pages artifact, deploys with
      least-privilege permissions.
- [x] 3.2 Triggers on pushes to `develop` touching `docs/site/**` or the
      workflow file, plus `workflow_dispatch`; build and deploy are
      separate jobs with `deploy` gated on `needs: build`, so a failed
      build cannot reach deploy.
- [x] 3.3 Wrote `docs/site/PUBLISHING.md`: the repository Pages
      source-setting requirement and post-deploy verification steps.

## 4. Integration validation

- [x] 4.1 `bundle exec jekyll build` ran clean; inspected generated
      `_site/` output directly — root, `/latest/`, `/latest/install/`,
      `/latest/themes/` all present with correct relative links.
- [x] 4.2 Reviewed rendered output against Libri's actual theme tokens
      (Parchment light / Ink dark CSS custom properties) and the copied
      app icon.
- [x] 4.3 `openspec validate publish-documentation-site --strict` passes.

## Workflow follow-up

- Configure the repository's GitHub Pages source to "GitHub Actions" —
  flagged in `PUBLISHING.md`; requires repository admin access this
  session doesn't have, so left for the user.
- Archive this change once the workflow has successfully deployed once
  (verification also left for the user, since it requires the Pages
  source setting above and a push to `develop`).
