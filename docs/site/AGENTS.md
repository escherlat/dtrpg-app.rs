# Documentation site rules

Edit site content only under `docs/site`.

- Put shared templates in `_layouts`, reusable fragments in `_includes`, and navigation data in `_data/versions.yml`.
- Put current pages in `latest/` and release snapshots in `vX.Y/` (none exist yet — no tagged release has shipped with documentation). Add every supported version to `_data/versions.yml`, including its `pages` list (slugs, used by the version selector's page-preserving fallback) and `nav` (sidebar entries).
- Start every rendered Markdown page with front matter containing `title`, `version`, and `slug` (the page's filename without extension; `index` for a version's landing page).
- Use ATX headings, sentence-case link labels, fenced code blocks with a language, and relative internal links filtered through `relative_url` in Liquid templates.
- Keep Liquid limited to layouts, includes, and the handful of `{{ '...' | relative_url }}` links pages use for cross-references. Do not put secrets, credentials, or private URLs in site content.
- Add images and icons below `assets/`; reuse the application-derived visual tokens in `assets/css/site.css`.
- Theme sources: colors are `crates/dtrpg-ui/src/data/theme.rs` (Parchment for light, Ink for dark — the app's default light theme and first-listed dark theme); fonts are `crates/dtrpg-ui/src/data/constants.rs`'s per-platform `DEFAULT_BODY_FONT`/`DEFAULT_LABEL_FONT`/`DEFAULT_MONO_FONT` constants (the site uses the same fallback *chains*, e.g. `"Hoefler Text", Georgia, serif`, rather than fetching a web font — the app's actual fonts, Hoefler Text and Gotham, aren't web-licensable); the application icon is `assets/icon/icon.png`, copied to `docs/site/assets/icons/icon.png`.
- Validate changes with `bundle install` and `bundle exec jekyll build` from this directory. Check the root, `/latest/`, and every version listed in `_data/versions.yml`.
- The Pages workflow (`.github/workflows/docs-pages.yml`) publishes changes to `docs/site` from `develop`, this repository's continuously-integrated branch (see `docs/git-flow.md`) — not `master`, which only moves on tagged releases. Do not edit a historical snapshot except to correct release documentation.
