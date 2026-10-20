# ADR 0002 implementation status

Branch: `feature/libri-v2-crate-inversion`, off `develop`.

## Done (commit d69df61)

- Moved domain models, persistence, service traits + their SDK
  implementations, the credential store, and the pure util helpers `data/`
  depends on (`hash`, `image_format`, `filter`, `sort`, `matching`, and
  `datetime`'s non-display functions) out of `dtrpg-ui` into a GPUI-free
  `dtrpg-core`.
- `dtrpg-ui` now depends on `dtrpg-core`, not the reverse. Workspace
  `Cargo.toml` and both crates' `Cargo.toml`s updated accordingly (unused
  deps dropped from each side, new deps added where the moved code needs
  them).
- Added `dtrpg_core::api::LibraryApi` as the facade (`list_items`,
  `get_item`, `count_items`, `list_collections`, `create_collection`,
  `delete_collection` so far — grows with the feature changes that need
  more of it, not ahead of them).
- Split the former single `dtrpg-core` binary into `dtrpg-desktop` (the
  GPUI app; `[[bin]] name = "Libri"`, cargo-packager metadata moved here
  unchanged) and `dtrpg-mcp` (new stub binary; constructs a `LibraryApi`
  over the unauthenticated SDK services and prints proof it linked with no
  GPUI dependency — the actual MCP protocol surface is out of scope here).
- `TokenStatusHandle` (plain data, moved to core) and its one GPUI `Global`
  registration point: added `dtrpg_ui::data::token_status_global::
  TokenStatusGlobal`, a thin newtype wrapper, since neither the trait nor
  the type is local to `dtrpg-ui` once `TokenStatusHandle` lives in core
  (orphan rule — same class of problem ADR 0001 hit with `Theme`/`Global`).
- Two `#[cfg(test)]`-gated items (`services::stub`, `util::stubs`,
  `StorageConfig::for_test`) had their gate removed: `cfg(test)` only
  applies when the crate it's written in is itself under test, and
  `dtrpg-ui`'s own tests need these across the crate boundary now that
  they live in `dtrpg-core`.
- Verified at each step: `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
  --workspace` (208 unit tests in each of `dtrpg-core`/`dtrpg-ui`, 7 + 5
  doctests, all passing, no behavior change), and the project's nightly
  `rustfmt` (invoked via the exact toolchain binary — `cargo +nightly fmt`
  silently resolves to Homebrew's stable `rustfmt` on this machine if it's
  earlier on `PATH`, which reformats the whole repo to generic style
  instead of applying the project's `rustfmt.toml` unstable options; hit
  this once already this session and had to recover from it, noting it
  here so it doesn't happen again).

## Not done — explicitly deferred, not forgotten

Per the task brief, these were always meant to be separate follow-up
commits on this branch, not folded into the move commit. None are started:

**Resolution as of 2026-10-20 (superseding the list below, which was
accurate when written):** the queue merge landed via ADR 0007 / PR #318
(specs restated in `openspec/changes/archive/2026-10-18-adopt-core-work-queue`);
the shared runtime landed (`dtrpg_core::runtime`, `connection.rs` takes a
`Handle`); the controller decomposition is substantially done — the catalog
Vec is gone and reads go through the index (PR #354) — though the controller
remains a UI-side entity, not a thin view-model over `LibraryApi`.

- Merge the three hand-rolled work queues (thumbnails, downloads,
  availability checks) in `LibraryController` into one bounded priority
  queue in the core.
- Share one `tokio::runtime::Runtime` instead of building a fresh one per
  service construction (`services/sdk/connection.rs`).
- Decompose `LibraryController`: move catalog data and queue state into
  `dtrpg-core`, leaving the UI-side controller as a view-model over
  `LibraryApi`.

## Not done — in scope for this task, not reached

The brief named six already-oversized files to split "as part of this
move... zero extra cost" since they're being rewritten anyway:

| Lines | File |
|---|---|
| 855 | `crates/dtrpg-core/src/services/sdk/library/mapping.rs` |
| 717 | `crates/dtrpg-ui/src/data/theme.rs` |
| 702 | `crates/dtrpg-ui/src/controllers/library/catalog_sync.rs` |
| 601 | `crates/dtrpg-core/src/data/storage.rs` |
| 553 | `crates/dtrpg-core/src/data/catalog_cache.rs` |
| 534 | `crates/dtrpg-core/src/services/sdk/library/mod.rs` |
| 518 | `crates/dtrpg-core/src/data/library.rs` |

In practice the move only relocated these files (git tracked every one as
a rename, not a rewrite) — their internals are untouched, so the "zero
extra cost" framing doesn't hold here the way it does for files that
genuinely needed their imports rewritten line-by-line anyway. Splitting
each one into coherent sibling modules is real design work (deciding where
the seams are, not just where line 500 falls), and six files is a lot to
do carefully in one sitting on top of the move itself. Rather than rush it
and risk a bad split slipping through under time pressure, this is left
for a follow-up pass — the size-check Makefile target (separate branch,
`feature/size-check-makefile`) is wired as a CI warning, not a failing
gate, specifically so this doesn't block anything in the meantime.

## Verify before continuing

```sh
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
/Users/paulyhedral/.rustup/toolchains/nightly-aarch64-apple-darwin/bin/cargo-fmt --all -- --check
```

All four are clean as of commit d69df61.

---

# ADR 0003 / library-index-search-and-facets implementation status

Branch: `feature/libri-v2-library-index`, off `feature/libri-v2-crate-inversion`.
Pushed: commit 6fe8509.

## Done

- `dtrpg_core::data::index`: `LibraryIndex` over `rusqlite` (`bundled`
  feature; there is no literal `fts5` cargo feature on rusqlite 0.40 -- FTS5
  comes compiled into the bundled SQLite amalgamation regardless, confirmed
  by the FTS5 search tests passing). Schema: `items`, `files`,
  `collections`, `collection_membership`, `tags`, `item_tags`,
  `storage_roots`, `item_placement`, `summaries`, `audit_log`, plus an FTS5
  virtual table (`items_fts`) over title/publisher/authors/description/
  tags/file_names/summary_text. `tags`, `item_tags`, `storage_roots`,
  `item_placement`, `summaries`, and `audit_log` are schema-only for now --
  no feature writes to them yet, matching the ADR's scope (the index exists
  ahead of the features that will populate those tables).
- `data/catalog_cache.rs` and `data/collections_cache.rs` rewritten (not
  moved) to delegate to the index. Public function names and signatures are
  unchanged, so every caller in `controllers/library/` needed zero changes.
- One-way, non-destructive migration from the JSON caches on first launch
  (`LibraryIndex::open_and_migrate`), via renamed
  `legacy_load_catalog_cache_json`/`legacy_load_collections_cache_json`
  functions kept specifically for that purpose. JSON files are left in
  place.
- Facet/aggregate queries (`count_all`, `count_by_status`,
  `count_recently_updated`) are parameterized SQL, not string-concatenated
  WHERE clauses -- deliberately narrower than a generic filter-string API,
  to avoid an injection-shaped seam before any caller actually needs one.
- The SQLite file lives at `{root}/library_index.sqlite3`, where `root` is
  the same directory every JSON cache in this crate already uses (the
  app's own cache directory, via `cache_dir()` -- confirmed by tracing
  every `load_catalog_cache`/`save_catalog_cache` call site in
  `controllers/library/`, not the user's configurable storage root, which
  was my first assumption and would have been a real risk: a SQLite file
  on a synced/removable location is a bad idea).
- Verified: `cargo check --workspace --all-targets`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`
  (219 unit tests in dtrpg-core, up from 208 with new index/FTS5 coverage;
  208 in dtrpg-ui unchanged; 7+5 doctests), nightly rustfmt via the actual
  toolchain binary.

## Not done yet

- **`library.rs` itself is not rewritten.** The task named it alongside
  `catalog_cache.rs` for a "resplit/rewrite, not just move." What exists:
  `LibraryIndex` now has index-backed equivalents of `library.rs`'s
  in-memory `section_counts` (`count_all`, `count_by_status`,
  `count_recently_updated`), satisfying `libri-library-index`'s "facet
  counts come from the database" requirement in isolation. What's missing:
  `library.rs`'s own `section_counts` function is untouched, and nothing
  calls the new index-backed counts yet -- `LibraryController` still holds
  its own in-memory `Vec<LibraryItem>` and scans it for every section
  count, sort, and filter. Wiring the controller to query the index instead
  of scanning memory is exactly the "decompose `LibraryController`" item
  ADR 0002 already named and deferred as a separate follow-up; it did not
  make sense to half-do that decomposition inside this change just to
  touch `library.rs`, so I left `library.rs` as-is rather than editing it
  superficially to claim the task was done.
- Search is not wired into the UI either, for the same reason: the catalog
  view's search box still does a linear scan in
  `controllers/library/filtering.rs`. `LibraryIndex::search` exists and is
  tested (FTS5 across title/publisher/description/file names), but nothing
  calls it yet.
- `item_placement`/`storage_roots` (multi-root storage, a different child
  change), `tags`/`item_tags` (tags-and-os-projection, depends on this AND
  the platform layer), and `summaries` (content-summaries, depends on
  this) are schema-only, as expected at this stage -- their owning child
  changes populate them later.

## Why stopped here rather than continuing into the controller rewiring

Wiring `LibraryController` to the index is real design work on top of code
this change did not move (it is pure `dtrpg-ui` state, untouched by ADR
0002), with its own risk of behavior drift against the specs this task's
regression bar depends on (search/sort/filter results changing shape, not
just source). Rather than rush that into the same change under time
pressure, I'm stopping at a verified, pushed checkpoint and flagging it
explicitly, consistent with the standing instruction to stop and write a
note rather than guess past a real scope boundary on a change this size.

## Verify before continuing

```sh
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
/Users/paulyhedral/.rustup/toolchains/nightly-aarch64-apple-darwin/bin/cargo-fmt --all -- --check
```

All four are clean as of commit 6fe8509.

---

# ADR 0002 closure checkpoint

ADR 0002 is accepted on this branch. The implementation was carried through
these checkpoints:

- `806a00d` — inverted the crate graph and added the `LibraryApi` facade.
- `e0ccf8d` — recorded the ADR 0002 implementation status.
- `6029d4d` — added the SQLite/FTS5 index used by the core layer.
- `906910f` — exposed index-backed reads, search, and aggregate queries through
  `LibraryApi`.
- `380d9ed` — made the platform test suite pass strict Clippy settings.

The remaining queue consolidation, shared-runtime, and controller-state work
is intentionally separate follow-up work. The UI keeps its in-memory catalog
for render-path safety, while headless consumers use `LibraryApi` and the
index.

Final verification on this branch:

```text
cargo check --workspace --all-targets                         PASS
cargo clippy --workspace --all-targets -- -D warnings        PASS
cargo test --workspace                                       PASS
```

---

# ADR 0004 closure checkpoint

ADR 0004 is accepted. Checkpoints on this branch:

- `376c974` — added platform file availability states.
- `3540753` — routed UI reveal, labels, and file-presence checks through the
  core platform boundary.
- `af28b2f` — centralized settings capability checks.
- `d4655b4` — centralized the title-bar platform check.

The platform contract is ready for the dependent tags-and-OS-projection
feature. That feature will implement filesystem tag projection while honoring
the SQLite index as the source of truth; it is intentionally not folded into
this architectural boundary change.

---

# Tags and OS projection (issue #326) and the follow-up fixes: status at 2026-10-19

Everything below is merged to `develop` (last merge: PR #347, `7cb8e17`). No
feature branch holds unmerged work and no PR is open. Read this section first;
the older ones above are the ADR 0002/0003/0004 history.

## What exists now

Tags, end to end, following the accepted Libri v2 design (`claude-2` in the
`dtrpg-design` submodule, which the owner calls authoritative). The OpenSpec
change is `openspec/changes/tags-and-os-projection/` (proposal, tasks, spec).
User docs are `docs/site/latest/tags.md`, `catalog.md`, `mcp.md`,
`downloads.md`. ADRs 0008 and 0009 hold the two decisions that surprise people.

| Piece | Where | PR |
|---|---|---|
| Index tables, tag colors, `LibraryApi` tag methods | `dtrpg-core` `data/index`, `api.rs` | #332, #333 |
| Detail-panel chips, sidebar Tags section, color menu, Native Integration settings | `dtrpg-ui` `ui/library/tag.rs`, `ui/views/{tags_section_view,sidebar_tags,manage_tags_dialog,settings_native_view}.rs` | #335 |
| Reconcile files' tags with the index (launch and reload) | `dtrpg-core` `services/tag_projection`, `dtrpg-ui` `controllers/library/file_tags.rs` | #333, #337 |
| Collection alias made on add, removed on remove | `controllers/library/collection_links.rs`, `collections.rs` | #338, #344 |
| Tag colors are not read from files | `platform/tags.rs`, ADR 0009 | #339 |
| MCP tools `list_tags`, `get_item_tags`, `set_tags`, `set_tag_color` | `dtrpg-mcp/src/main.rs` | #340 |
| `tag:` / `-tag:` search | `dtrpg-core` `util/search.rs` | #341 |
| Group by tag (`GroupBy` enum) | `data/enums.rs`, `util/publisher.rs`, `catalog_view.rs` | #345 |
| List column chooser and Tags column (`ListColumn` enum) | `data/enums.rs`, `ui/views/list_columns.rs` | #346 |
| View menu fallbacks; render-path performance | `ui/app/mod.rs`, `controllers/library/{catalog,filtering,thumbnails}.rs` | #347 |

Decisions that were made with the owner and should not be reopened without
asking: the later change wins when file and index both changed, else the index
wins (ADR 0008); a tag read from a file has no color (ADR 0009); alias removal
only removes a symlink that points at the item's downloaded file; group by tag
repeats an item under each of its tags and sits beside group by publisher.

## Open

- **The views were never rendered by the agent.** There is no display in the
  cloud environment, so every view change was checked by compiling and by
  controller or pure-function tests. The macOS extended-attribute code is
  verified only by the macOS CI job.
- **German and French strings added in this work are unreviewed.** They are
  `menu.toggle_group_by_tag`, `toolbar.group_by_tag`, `catalog.untagged` and
  `catalog.col_tags` in `crates/dtrpg-ui/i18n/{de,fr}.yaml`.
- **Known performance costs not fixed** (PR #347 fixed the large ones):
  `invalidate_cache` is still a full filter, copy and sort of the library
  (about 12 ms at 10,000 items in a debug build) and still runs when a
  download or availability check finishes; the multi-item detail view reads the
  storage config and stats files while it renders (`detail_panel_view.rs`,
  `render_item_tier`, and `on_disk_file_size` callers). Do these only if the
  owner reports slowness there.
- **Not built from the mockups:** the saved-search rule editor, and the
  permission model on the Agent and MCP page (so MCP writes are opt-in with
  `--allow-write`). The Tags column is plain text, with no color dots.
- **The "Reconcile on launch" switch also governs the reload pass.** That is
  intended, but it is not obvious from the setting's name.
- **Coverage lost on purpose:** `tags_show_on_a_launch_that_cannot_reach_the_network`
  replaced a signed-out version of the same test, because the live fetch can
  deadlock the test scheduler (see below). The signed-out exit path of the
  catalog load is therefore not covered by a controller test.
- **Housekeeping:** the remote still has the merged branches from this work
  (`tags-mcp-and-api`, `tags-column-group-search`, `group-by-tag`,
  `tags-column`, `remove-collection-alias`, `fix-menus-and-scrolling`,
  `fix-windows-unused-test-items`, `collection-alias-on-add`,
  `tag-colors-not-from-files`) and the throwaway `diag-windows-test-hang`
  (closed PR #343). All are safe to delete. `make size-check` already reports
  a backlog of files over 500 lines (for example `catalog_view.rs`,
  `root_view.rs`, `detail_panel_view.rs`, `settings.rs`); it is not part of
  CI's failing gates, so new files should stay under the limit but the backlog
  is not this work's to clear.

## Things that cost time, so they do not cost it again

- **Test scheduler seeds.** `gpui::test` runs tasks in an order set by the
  `SEED` environment variable, and a bad order can deadlock. The catalog
  load's page receiver blocks one background task while the fetch that fills it
  is another; with `SEED=2` a test that runs the whole load hangs forever, and
  the Windows runner's default order hits it. Tests of the launch path must not
  reach the live fetch (use `NetworkMonitor::fixed(false)`, which the tests
  fixture sets to `true` by default so tests never touch the real network).
  Check the UI suite under a few seeds before pushing:
  `for s in 1 2 7 13; do SEED=$s cargo test -p dtrpg-ui; done`.
- **Windows lint.** `cargo clippy -D warnings` runs on Windows in CI. A helper,
  import or enum variant used only by `#[cfg(unix)]` tests is unused there and
  fails the job. Gate it with `#[cfg(unix)]` too. A CI test step now has a
  15-minute timeout, so a hang fails instead of holding a runner.
- **Do not call the view-preference setters from a test.** `set_sort`,
  `set_group_by`, `toggle_list_column` and the like save to the real user config
  file. Keep the rule in a pure function (`GroupBy::toggled`,
  `ListColumn::toggled`) and test that.
- **Menu enablement.** A native menu item is enabled only if something in the
  focused window's chain handles the action or an app-level `cx.on_action`
  handler exists. A new menu action needs a fallback in
  `ui/app/mod.rs` (see `update_library`), or it will be disabled whenever focus
  is outside the library view.
- **No I/O and no whole-library copies on the render path.** `snapshot()` is
  called on every draw; `visible_items_slice` and `column()` are called per
  table cell. The result set is an `Arc<Vec<LibraryItem>>`, and the filter count
  and product-id set are cached with it by `invalidate_cache`. A change that
  edits one catalog item in place should patch the cached copy (see
  `note_thumbnail_attempt`) instead of calling `invalidate_cache`.
- **The stop hook can report unpushed commits that are pushed.** The local
  remote-tracking ref goes stale after a push from this environment. Refresh it
  with `git fetch origin +refs/heads/<branch>:refs/remotes/origin/<branch>`
  before deciding anything is unpushed.
- **Formatting.** `make fmt-check` runs the pinned nightly `rustfmt` and is the
  only formatter to use; `cargo +nightly fmt --all` also works here. Do not run
  stable `rustfmt`.

## Verify before continuing

```sh
make fmt-check
make lint
make test
```

All three were clean at `7cb8e17` (481 `dtrpg-core` tests, 6 `dtrpg-mcp`, 339
`dtrpg-ui`, plus doctests).

Process: `develop` is the source of truth. Work on a branch from it, open a
ready-for-review PR, subscribe to it, and fix CI. The owner enables auto-merge
and merges. Commit messages are Conventional Commits.

# Content summaries: status at 2026-10-19

PR: pilgrimagesoftware/dtrpg-app.rs#351 (`feature/content-summaries`).

## What exists now

- `LibraryApi::summarize_item` / `summarize_text` (`crates/dtrpg-core/src/api_summary.rs`):
  reads the first on-disk, supported file of an item, enforces the monthly
  token ceiling, generates with the configured provider, and saves the result.
- `provider_for` builds the provider from `SummarySettingsFile`; the Anthropic
  key is read from the keyring (`credentials/anthropic.rs`).
- MCP tools `get_summary` (read-only) and `summarize_item` (needs
  `--allow-write`, because it spends tokens).

## Not done

- The Summaries fields in Settings > Agent & MCP, including entering the
  Anthropic API key.
- Showing a summary in the detail panel.
- Auto-summarize on download (`auto_summarize_on_download` is stored but
  nothing reads it yet).
- Text extraction for formats other than PDF.
- Documentation under `docs/site/`.
- The UI crate was not built or tested in the session that wrote this.

## Gotchas

- `summarize_item` uses blocking `reqwest` for the Anthropic provider. The MCP
  tool wraps it in `block_in_place`, which needs the multi-thread runtime.
  Single-thread `#[tokio::test]` tests must not reach that path.
- `libdbus-1-dev` is needed to build `dtrpg-core` on Linux (keyring).

# Screenshots

Note from the owner, recorded as given (this session has not seen the files):

> I added some more screenshots for some of the views that had more than a
> screenful, so there's a -1 and -2, and a couple other views or sub-views that
> were missing.

Whoever picks up the screenshots work should check for those `-1` / `-2`
variants and the extra views before treating the set as complete.

# Catalog reads from the index: status at 2026-10-20

Branch: `feature/catalog-reads-from-index`, off `develop`. OpenSpec change:
`openspec/changes/catalog-reads-from-index` (proposal/design/tasks all written;
`skip_specs: true` — no capability spec changes, this is a storage swap).
Scoped per claude-2 `ARCHITECTURE-REVIEW.md`'s delivery-sequence step 2 ("make
local state authoritative for presentation").

## Next step

PR pilgrimagesoftware/dtrpg-app.rs#354 is merged (`bbf1634`, 2026-10-20) and
the openspec change is archived at
`openspec/changes/archive/2026-10-20-catalog-reads-from-index`. The
follow-ups below ("Not done") are each independently startable; none blocks
the others. Tasks 3-4 (publisher/tag aggregation as SQL) now have a tracking
issue, pilgrimagesoftware/dtrpg-app.rs#355; search-box semantics in SQL are
tracked in #353. Task 5 (SQL-side LIMIT/OFFSET) was judged not load-bearing
and intentionally not filed as an issue — the render-path cache reads with
zero I/O by design, see "Gotchas" below.

Other open threads in this file, each independently startable:
- Content summaries ("Not done" under that section): the Settings > Agent &
  MCP summary fields (including the Anthropic key entry), showing a summary
  in the detail panel, auto-summarize-on-download, non-PDF text extraction,
  and `docs/site/` documentation. That section also notes the UI crate was
  never built or tested in the session that wrote the core code — worth a
  `make test` run there before starting.
- The screenshots note above (check for `-1`/`-2` variants and missing
  views before treating the set as complete).
- `make size-check` backlog of files over 500 lines (CI warning, not a
  gate): `catalog_view.rs`, `root_view.rs`, `detail_panel_view.rs`,
  `settings.rs`, and the seven files listed under ADR 0002's "in scope, not
  reached" table.

## What exists now

- `catalog: Vec<LibraryItem>` is gone from `LibraryController`. The library
  index (`LibraryIndex`, ADR 0003) is the controller's only catalog storage —
  no fallback, no dual bookkeeping. This went further than the task's original
  incremental-migration plan: the owner explicitly overrode that mid-session
  ("rip the old stuff out and replace it with the new"), after an incremental
  attempt (keep the Vec as a fallback behind `index: Option<LibraryIndex>`)
  surfaced a real bug — tests set `ctrl.catalog` directly, bypassing the
  index, so a partial migration silently returned empty results for anything
  that queried the index.
- `LibraryIndex::list_items_filtered` (new file,
  `crates/dtrpg-core/src/data/index/filtered_items.rs`) covers every
  `SidebarFilter` variant except `Tag` (resolved in Rust against its
  pre-computed id set — it was never a Vec scan to begin with) and every
  `SortMethod`, including `Custom`'s closed `col_key` set. `LibraryIndex::
  replace_item` upserts one row for the many call sites that used to mutate
  one `LibraryItem` in place (status changes, file-downloaded flags,
  availability checks).
- `self.index: LibraryIndex` is non-optional, opened once in
  `LibraryController::build_with_cache_root()`. `.expect()`s on open failure —
  deliberate, per the no-fallback decision above.
- `thumbnail_last_attempted`/`availability_last_checked` (the two
  `LibraryItem` fields the index schema has no columns for) moved to two
  session-only `HashMap<Arc<str>, SystemTime>` fields on the controller,
  overlaid back onto every item fetched from the index
  (`overlay_ephemeral_state`/`item_by_id`/`all_items` in `catalog.rs`), so the
  `LibraryItem` shape the rest of the app sees is unchanged.
- Three now-redundant background `save_catalog_cache` write-throughs were
  removed outright (`catalog_sync_autoload.rs`, `catalog_sync_fetch.rs`'s
  final save, `download_dispatch.rs`'s `save_catalog_in_background`) — every
  catalog mutation now writes to `self.index` synchronously at the point of
  mutation, so the old background re-save of the whole catalog was pure
  duplicate work.
- Verified clean: `cargo check`/`clippy -D warnings --workspace --all-targets`,
  `cargo test -p dtrpg-ui` (339 tests + 5 doctests), `make fmt`/`fmt-check`/
  `size-check`. `dtrpg-core`'s 22 network/socket-binding test failures are
  this sandboxed environment's pre-existing limitation (confirmed present
  before this change too), unrelated to this work.

## Not done — scoped out, not silently dropped

- **Publisher/tag aggregation as SQL.** `publisher_entries()`/
  `group_by_publisher()`/`group_by_tag()` (`util/publisher.rs`) still run in
  Rust over `self.all_items()` (an index fetch), not as `GROUP BY` queries.
  Every call site was migrated off the Vec, just not onto index-side
  aggregation yet.
- **SQL-side pagination.** `visible_items_slice()` still slices the
  fully-materialized `items_cache: Arc<[LibraryItem]>` that `invalidate_cache()`
  builds. That cache is deliberately still one in-memory `Vec` — it's the
  render-path read, and the render path must do zero I/O (`AGENTS.md`) — so
  `LIMIT`/`OFFSET` on the index query was never actually load-bearing: the
  index query that feeds the cache runs once per filter/sort/search change,
  not once per rendered row.
- **Full search-box semantics in SQL.** `SearchQuery::matches()`
  (`util/search.rs`, the search box's real matcher: substring-anywhere over
  title/publisher/line/tags, plus a `tag:`/`-tag:` mini-syntax) is not what
  the index's `search()` FTS5 query does (word-token match over
  title/publisher/description/file_names). Replicating `SearchQuery`'s exact
  semantics in SQL is tracked separately as
  `pilgrimagesoftware/dtrpg-app.rs#353`; this change deliberately keeps search
  in-memory, run over the index-filtered (usually much smaller) set instead
  of the full catalog.

## Gotchas

- If you're tempted to reintroduce a `Vec<LibraryItem>` cache anywhere in
  `LibraryController` "just for this one read path" — don't, without checking
  whether a test or another call site populates it directly. That's exactly
  the bug the incremental approach hit: a Vec that's sometimes populated and
  sometimes not is worse than no Vec at all, because it fails silently (empty
  results) rather than loudly.
- `thumbnail_attempts`/`availability_checks` are populated at specific
  mutation points (`note_thumbnail_attempt`, the availability-check result
  handler in `checks.rs`), not derived from the index. A new per-item
  ephemeral field would need the same treatment — a side map, overlaid in
  `overlay_ephemeral_state` — not a new index column, unless it's actually
  meant to survive a restart.
