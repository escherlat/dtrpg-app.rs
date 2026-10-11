
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
