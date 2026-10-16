# Libri.app (Rust)

[![Build Status](https://github.com/pilgrimagesoftware/dtrpg-app.rs/actions/workflows/ci.yaml/badge.svg)](https://github.com/pilgrimagesoftware/dtrpg-app.rs/actions/workflows/ci.yaml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE.md)

DriveThruRPG desktop frontend in Rust, built with [gpui-kit](https://gpui-kit.com).

## Building

```bash
cargo build
cargo run -p dtrpg-core
```

## Platform Requirements

### macOS

**Apple Silicon only.** Intel Macs are not a supported build or runtime target; packaged releases
ship an `aarch64-apple-darwin` binary only.

No additional setup required. The app links `Security.framework` automatically via the `keyring` crate.

Install Xcode components to get Metal support:

```sh
xcode-select --install
```

### Linux

The credential store requires a running Secret Service daemon (GNOME Keyring or KWallet) and the
`libsecret` development library:

```bash
# Debian / Ubuntu
sudo apt install libsecret-1-dev

# Fedora
sudo dnf install libsecret-devel
```

Install GNOME Keyring if no keyring daemon is running:

```bash
sudo apt install gnome-keyring
```

### Windows

No additional setup required. The app uses Windows Credential Manager via the `keyring` crate.

## Testing

```bash
cargo test --workspace
```

## Releases

Installable packages for macOS (`.dmg`), Linux (`.deb`/`.AppImage`), and Windows (`.msi`/NSIS
`.exe`) are built by [`cargo-packager`](https://github.com/crabnebula-dev/cargo-packager) and
published to GitHub Releases:

- **Nightly**: every push to `develop` rebuilds all three platform packages and
  republishes them under the rolling `nightly` pre-release, replacing the previous nightly's
  assets. Triggered by `.github/workflows/nightly.yaml`.
- **Tagged**: when `develop` is merged into `master`, `.github/workflows/release.yaml` computes
  the next version from [Conventional Commits](https://www.conventionalcommits.org/) since the
  last version tag (`fix:` -> patch, `feat:` -> minor, a `BREAKING CHANGE:` footer or `!` -> major),
  updates `Cargo.toml`, tags the commit, and publishes a full GitHub Release with the packages
  attached.

Both paths share the build/publish logic in `.github/workflows/package.yaml` (a reusable
`workflow_call` workflow). The Windows leg is currently marked `continue-on-error` pending
confirmation that the pinned `gpui` revision builds cleanly on Windows (see
`.github/workflows/windows-spike.yaml` and `openspec/changes/add-release-packaging-workflow`).

Packages are unsigned; macOS Gatekeeper and Windows SmartScreen will warn on first run. Signing is
a follow-up, not yet implemented.

`.github/workflows/build.yaml` remains CI build/test only and does not publish packages.
`.github/workflows/bump-version.yaml` is a manual escape hatch for out-of-band version bumps and
is independent of the automated tagging in `release.yaml`.

## Crash Reporting (Sentry)

Sentry crash/error reporting is opt-in and off by default. A plain `cargo build`/`cargo run` from
source never compiles Sentry in and never contacts Sentry, regardless of any environment
variables set locally.

Sentry is only active when both of the following are true:

- The binary was compiled with the `sentry` Cargo feature (`cargo build --features
  dtrpg-core/sentry`).
- A DSN is available from either:
  - the `DTRPG_SENTRY_DSN` environment variable at process startup, or
  - a value embedded at compile time (see `crates/dtrpg-core/build.rs`), which the packaging
    workflow (`.github/workflows/package.yaml`) sets from the `SENTRY_DSN` repository secret. This
    is how official nightly/release artifacts report crashes without requiring end users to set
    any environment variable themselves.

Other supported variables (all optional, all read at runtime and overridable locally even in a
`sentry`-feature build):

| Variable                     | Purpose                              | Default                |
| ----------------------------- | ------------------------------------ | ---------------------- |
| `DTRPG_SENTRY_DSN`            | Sentry project DSN                   | none (Sentry disabled) |
| `DTRPG_SENTRY_ENVIRONMENT`    | Sentry `environment` tag             | `production`           |
| `DTRPG_SENTRY_RELEASE`        | Sentry `release` tag                 | crate version          |

On startup the app logs exactly one INFO line stating whether Sentry reporting is active, and if
not, why (feature not compiled in, or no DSN configured).

To test locally with the feature enabled:

```bash
DTRPG_SENTRY_DSN=https://<key>@<org>.ingest.sentry.io/<project> cargo run --features dtrpg-core/sentry
```

To confirm an event actually reaches Sentry without launching the GUI, pass
`--trigger-test-error`: this emits one ERROR-level `tracing` event and exits immediately.

```bash
DTRPG_SENTRY_DSN=https://<key>@<org>.ingest.sentry.io/<project> cargo run --features dtrpg-core/sentry -- --trigger-test-error
```

Maintainers: the `SENTRY_DSN` repository secret must be set under repository Settings > Secrets
and variables > Actions for the release build workflow to embed it. This is a plain project DSN,
not a sensitive credential (Sentry DSNs only permit event submission, not read access), but it
should still come from the project's own Sentry organization rather than being hardcoded anywhere
in this repository.

### Release registration

`.github/workflows/package.yaml`'s `sentry-release` job registers each nightly/release build as a
Sentry release (org `pilgrimage-software`, project `dtrpg`) via `getsentry/action-release`,
associating it with the commit range so Sentry can show suspect commits and mark issues resolved
on deploy. This needs a `SENTRY_AUTH_TOKEN` repository secret (a Sentry auth token with
release-management scope) — unlike the DSN, this token is a real credential and must be generated
in Sentry's organization settings and added as a secret, never committed. Registration is
skipped, not failed, when the secret is absent; crash reporting itself only needs `SENTRY_DSN`.
