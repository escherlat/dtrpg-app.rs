
## 0.3.0 - 2026-10-15

### Added
- Add bulk selection mode and bulk actions
- Migrate from gpui-component to gpui-kit (#233)
- Publish a versioned documentation site via GitHub Pages
- Make the bug/feature report dialog its own window
- Download all from sidebar smart filters and publishers


### Documentation
- Add ADR 0001 and OpenSpec proposal for gpui-kit 0.7 migration (#233)
- Check off task 0.2 (gpui-kit dependency added)
- Check off task 0.2 (gpui-kit dependency added)
- Amend 0001 for gpui-kit fork incompatibility, collapse plan to single rewrite
- Backfill documentation site with guides for core features


### Fixed
- Show bulk-action bar as soon as selection mode starts
- Confirm large bulk actions before dispatching
- Stop avatar photo squishing under gpui-kit's Img layout
- Force aspect_square() on avatar images to stop squish
- Map accent/list/group-box/switch/slider/link tokens
- Wrap title-bar avatar image in its own sized/clipped div
- Apply rounded_full() to the avatar img, not its wrapper div
- Pin title-bar avatar to flex_none, not flex-basis sizing
- Restore gpui-kit Avatar component for account avatars
- Cancel Button's internal padding on the title-bar avatar
- Pin Ruby version for the Pages build
- Handle fallible ZipFile::name() in zip 9.0
- Make activity panel item list scroll



## 0.2.4 - 2026-10-13

### Changed
- Split the library controller into concern modules


### Fixed
- Remove stale cover files when the cached format changes
- Do not start a retry or download after cancellation
- Present native file pickers asynchronously


### Performance
- Compute settings cache counts off the render path



## 0.2.3 - 2026-10-11

### Fixed
- Make Zip content preview popover list scroll for large archives
- Defer cache reload after settings change



## 0.2.2 - 2026-10-09

### Fixed
- Stop verifying a macOS .app bundle cargo-packager already deleted
- Stop duplicating catalog items on every sync tick



## 0.2.1 - 2026-10-05

### Fixed
- Use the ClientOptions builder, not a non-exhaustive struct literal
- Stop git-cliff from skipping fix:/feat: commits scoped "(release)"



## 0.2.0 - 2026-10-04

### Added
- Add Report a Feature/Bug menu item and dialog (#243)



## 0.1.3 - 2026-10-04

### Fixed
- Stop catalog title duplication and toolbar title clipping (#239)


# Changelog

All notable changes to this project are documented in this file.

prepare-release.yml prepends each release's entry above this line via
`git-cliff`, using `cliff.toml`'s template.
