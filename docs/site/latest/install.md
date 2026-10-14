---
title: Installation
version: latest
slug: install
---

# Installation

Packaged releases are published to [GitHub Releases](https://github.com/pilgrimagesoftware/dtrpg-app.rs/releases):

- **macOS**: `.dmg`, Apple Silicon (`aarch64-apple-darwin`) only — Intel Macs
  are not a supported build or runtime target.
- **Linux**: `.deb` or `.AppImage`.
- **Windows**: `.msi` or NSIS `.exe`.

Two release channels exist:

- **Nightly** - rebuilt from every push to `develop`; replaces the previous
  nightly's assets each time. Use this to try unreleased changes.
- **Tagged** - a numbered release cut when `develop` merges into `master`,
  versioned from [Conventional Commits](https://www.conventionalcommits.org/)
  since the last tag.

## Packages are unsigned

Code signing isn't implemented yet. Expect a warning on first launch:

- **macOS**: Gatekeeper blocks the app with "Libri can't be opened because
  Apple cannot check it for malicious software." Right-click (or Control-click)
  the app in Finder, choose **Open**, then confirm **Open** in the dialog that
  follows. This is only required once per downloaded copy.
- **Windows**: SmartScreen shows "Windows protected your PC." Click **More
  info**, then **Run anyway**.

## Platform-specific requirements

### macOS

Apple Silicon only. No additional setup is required to *run* a packaged
build — the app links `Security.framework` automatically for credential
storage.

Building from source needs Xcode's command-line tools (for Metal support):

```sh
xcode-select --install
```

### Linux

The credential store needs a running Secret Service daemon (GNOME Keyring or
KWallet) and the `libsecret` runtime/development library:

```bash
# Debian / Ubuntu
sudo apt install libsecret-1-dev

# Fedora
sudo dnf install libsecret-devel
```

If no keyring daemon is running:

```bash
sudo apt install gnome-keyring
```

### Windows

No additional setup required — the app uses Windows Credential Manager for
credential storage.

## Building from source

```bash
cargo build
cargo run -p dtrpg-core
```

See the [repository README](https://github.com/pilgrimagesoftware/dtrpg-app.rs)
for the full build matrix and release workflow.
