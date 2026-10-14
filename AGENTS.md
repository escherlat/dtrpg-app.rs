# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code), Codex (openai.com/codex/), GitHub Copilot (copilot.github.com) when working with code in this repository.

## About This Project

This is the repository for the DriveThruRPG frontend application implemented in Rust.

## Frameworks

The UI is implemented using `gpui-kit` (https://github.com/longbridge/gpui-kit),
Longbridge's actively maintained successor to `gpui-component`. `gpui-kit`
forks zed-industries/zed's `gpui` under its own package family, published to
crates.io as `gpui-pre`/`gpui-pre-platform`/etc. — this app's own
`gpui`/`gpui_platform` workspace dependencies are pinned to that same fork
(not zed's original), so this codebase's `Context`/`Window`/`App`/`Global`
types are gpui-kit's own types, not a separate compatible set. See
`docs/adr/0001-migrate-to-gpui-kit.md` for why: an incremental,
coexistence-based migration was attempted first and found to be impossible
— gpui-kit's widget layer is foreign to zed's original `gpui` at the type
level (the orphan rule blocks any bridge), so the whole app moved to
gpui-kit's fork in one change, not file-by-file.

## UI Policy

- Preserve the separation between UI rendering, interaction state, and service adapters.
- Keep code structured so stubs can be replaced by SDK adapters without rewriting views.

## gpui-kit Usage

Always prefer components from `gpui_kit::component`/`gpui_kit::base` over rolling custom implementations:

- **Tabular data**: Use `DataTable` (virtualized, delegate-based, handles header/row alignment) or `Table`/`TableHeader`/`TableRow`/`TableHead`/`TableCell` (simple, stateless). Never build custom flex-row column layouts for table-like views — they cannot reliably align headers with rows when virtualization is involved.
- Check `gpui_kit::component`/`gpui_kit::base` for any other UI primitive before implementing from scratch.

## Credential Storage

All DriveThruRPG account credentials (API key, access token, refresh token) MUST be stored
exclusively in the platform-native secure store via `dtrpg_ui::credentials::KeyringCredentialStore`.

- Service namespace: `com.pilgrimagesoftware.dtrpg` (constant: `credentials::keys::SERVICE`)
- Account keys: `api-key`, `access-token`, `refresh-token` (constants in `credentials::keys`)
- Never write credentials to config files, environment variables, or unencrypted databases.
- On Linux, the app requires `libsecret` (GNOME Keyring or KWallet). Add `libsecret-1-dev`
  to the Linux CI build environment.
- The SDK (`dtrpg-sdk`) does NOT persist credentials — that is the app's responsibility.

## Documentation 

- Docs MUST be kept up to date; if a new feature is added, documentation MUST be 
  written for it, with clear, concise examples. Avoid contrived tutorials.
- Docs will be added to `docs/site/`.
- Docs will be kept logically organized.
- When documentation is published, it MUST be versioned, with the latest version
  showing by default.
- Documentation MUST include installation instructions, including gotchas, caveats,
  and workarounds for platform issues, such as macOS quarantine.

## Releases

`prepare-release.yml` / `tag-release.yml` / `release.yml` automate the
git-flow release cycle (version bump + changelog via `cargo-edit`/`git-cliff`,
tag `main`, cut a GitHub Release, merge `main` back into `develop`), calling
the reusable `pilgrimagesoftware/github-actions` `rust-*-release` workflows, and
`package.yml` builds `Knot.app` with `cargo-packager`. Details and
required secrets: `CONTRIBUTING.md` - Releases. Rationale: `docs/adr/0001-git-flow.md`.

## Architecture Decisions

Non-trivial design choices are recorded as ADRs under `docs/adr/`. Index and
process: `docs/adr/README.md`. `/adr "<title>"` scaffolds a new record from
`docs/adr/0000-template.md`.

## Conventions

- **No `.rs` file over 500 lines.** Enforced by `make size-check` in CI. Split
  by concern, not by line count; move colocated tests to a sibling `tests.rs`
  first. Do not raise the limit to make a change fit.
- **`mod.rs` declares; it does not implement.** Module declarations,
  re-exports and the module doc comment only - implementation goes in sibling
  files named for what they do. Being under the line limit is not evidence the
  code is in the right place. The six that predated the rule were split under
  issue #300; the ones still holding code elsewhere in the workspace are a
  backlog, not exceptions.
- **No crate-wide `allow`.** Allow on the item, with a comment saying why.
  `UNWIRED` marks ported-but-unconnected code, `SUPERSEDED` marks code a newer
  path replaced - both greppable.
- Constants live in a single `consts.rs` per crate.
- Errors: `thiserror` enums per crate (`GitError`, `DiscoveryError`, core
  `Error`), re-exported with a crate `Result` alias.
- User-facing text goes through `knot_core::l10n::t`; tests assert the key
  resolves, never the English copy.
- Keep functions to <= 5-6 args; group related args in a struct.
- Closed vocabularies are enums with `Display`/`FromStr`, not `String` matched
  with a `_ => default` arm. An open one (`agent_type`) keeps its `String` and
  gets one roster - `knot_core::agent_type` - that every crate reads.
- Locks are `parking_lot::Mutex`: no poisoning, so no `.unwrap()` or
  `if let Ok(..)` around a guard. `tokio::sync::Mutex` only where a guard is
  held across an `.await` (`knot-acp`'s transport).
- No I/O on the render path - GPUI re-renders per keystroke. Diff stats go
  through `knot/src/diff_stats.rs`, which has caught this three times.
- Off-thread results have to reach a frame. A new cache, dirty flag or
  `spawn_blocking` result belongs in `repaint_poll_tick`'s `if` chain, and a
  clearing read (`take_changed`, `take_dirty`) must not be reachable on a path
  that discards it. Four separate breaks of that chain are named in
  `.claude/rules/rust-structure.md`; none of them failed a test.
- No statement-hugging brace style; format with nightly `rustfmt`.

`.claude/rules/rust-structure.md` has the reasoning behind each of these, with
the defect that produced it. Read it before a refactor; every rule names the
cost of breaking it, so you can tell when it genuinely does not apply.
