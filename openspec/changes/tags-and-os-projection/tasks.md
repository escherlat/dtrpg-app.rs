## 1. Index and API

Index schema version 3 drops the foreign key from `item_tags.item_id`. The bundled SQLite enforces foreign keys,
so rewriting the catalog (which deletes every item row) would have deleted every tag, and the catalog is saved in
checkpoints while pages are still loading. A tag on an item missing from a partial catalog is kept and is there
again when the item returns; `list_tags` only lists tags on items that are in the catalog. Tags cannot contain a
comma, because Linux stores a file's tags as a comma-separated list.

- [x] 1.1 Index methods for tags: list all, list for an item, add, remove, rename, delete; unique ignoring case
- [x] 1.2 The same methods on `LibraryApi`
- [x] 1.3 Keep the FTS `tags` column in step with `item_tags`

## 2. Projection

`PlatformIntegration::read_tags` and `write_tags` read and write a file's tags; they default to `Unsupported`, which
is what Windows reports. macOS and Linux share one module (`platform/tags.rs`) and the `plist` and `xattr` crates,
so the macOS format is encoded, decoded and tested on Linux too. Only the attribute name differs by platform.
Writing an empty list removes the attribute. Not verified on a real Mac: reading and writing the attribute on an
actual APFS volume and having Finder show the tags (task 5.4).

- [x] 2.1 Add the `xattr` dependency under `[target.'cfg(unix)'.dependencies]`
- [x] 2.2 `PlatformIntegration` methods to read and write a file's tags; `Capabilities::filesystem_tags`
      decides whether they are used
- [x] 2.3 macOS: `com.apple.metadata:_kMDItemUserTags` as a binary property list of strings, keeping any
      existing color suffix
- [x] 2.4 Linux: `user.xdg.tags` as a comma-separated list
- [x] 2.5 Windows: reading and writing report `Unsupported`, and nothing calls them

## 3. Reconciliation

The comparison and rewriting are `dtrpg_core::services::tag_projection` (`reconcile_file_tags` for the launch pass,
`project_item_tags` for a change or a finished download). They take the platform as a parameter and are tested
against one kept in memory. The launch pass leaves a file alone when its item has no tags in the index, so tags
the user set in Finder before using Libri's are not wiped on first launch; removing an item's last tag in Libri
clears its files through `project_item_tags`.

In the app, the launch pass runs once, in the background, after the first full catalog load and the verification of
downloads that follows it (`LibraryController`, `file_tags.rs`); a partial page does not trigger it. It reports a
count in the activity panel, as "Tags restored from your library (files: N)" (the wording avoids a plural), an error
entry when files could not be written, and logs each file. A finished download gets its item's tags in the
background. Not covered by an automated test: a download actually writing tags (the scripted library does not
create the file), and the panel entries appearing after a real launch.

- [x] 3.1 On launch, in the background, compare each downloaded file with the index and rewrite the differences
- [x] 3.2 Skip and log files that cannot be written; never fail the launch
- [x] 3.3 Report the number of restored files in the activity panel
- [x] 3.4 Project an item's tags when one of its files finishes downloading

## 3b. Colors, reading tags back, and settings

Settled 2026-10-18 after the Libri v2 design was accepted (ADR 0008 supersedes the "index wins" rule of ADR 0004).
Index schema version 4 adds a color to tags, when an item's tags last changed, and per-file snapshots of the tags
the file and the index last agreed on. `TagColor` is Finder's seven colors, so none is approximated.
`reconcile_files` decides per file from the snapshot, the file and the index (`tag_projection/decision.rs`), and
`NativePreferences` holds the three switches from the mockup's Native Integration page. The launch pass reports
files written and files read back separately.

- [x] 3b.1 `TagColor`, a color on tags in the index, and `LibraryIndex::{tag_colors, set_tag_color}`
- [x] 3b.2 Record when an item's tags changed, and each file's snapshot; forget the snapshot of a file that is gone
      or was downloaded again
- [x] 3b.3 `FileTag` with a color; macOS reads and writes Finder's color number; Linux keeps none
- [x] 3b.4 Decide which side wins from the snapshot; later change wins, the index wins on a tie or an unknown time
- [x] 3b.5 Read a file's tags back into the index. Not their colors: the number in a file is not the color Finder
      shows (ADR 0009)
- [x] 3b.6 `NativePreferences`: write to files, read from files, reconcile on launch; the launch pass and a
      finished download honor them
- [x] 3b.7 ADR 0008, and the spec
- [x] 3b.8 `LibraryApi` methods for the tag library, a tag's color, and replacing an item's tags
- [x] 3b.9 The Native Integration settings page with the three switches

## 4. UI

The controller keeps a `TagLibrary` read from the index in the background (the views never read the index), and
edits go through `services::tag_editing::apply_tag_edit`, which changes the index and writes the files of the items
it affected. When an edit finishes, the tags are read again from the index rather than taken from the edit, because
two edits made quickly would otherwise each show the other out of date. A tag filter (`SidebarFilter::Tag`) carries
the ids of the items that have the tag, so filter matching needs no change at its callers.

Following the accepted Libri v2 design (`claude-2` in `dtrpg-design`): chips with a color dot in the detail panel,
a Tags section in the sidebar, a Native Integration settings page. A tag's color is set by right-clicking it. The
views are checked by compiling and by controller tests only: the app cannot be run in the environment this was
written in.

- [x] 4.1 Tag editor in the detail panel: chips, a remove button, and a dialog to tick existing tags or make one
- [x] 4.2 Catalog filter by tag: a Tags section in the sidebar with counts
- [x] 4.3 Text describing the file-system projection only where `filesystem_tags` is true
- [x] 4.4 Right-click a tag to set its color
- [x] 4.5 The Native Integration settings page with the three switches
- [x] 4.6 The optional Tags column, with a way to show and hide the other list columns, and group by tag
- [x] 4.7 `tag:` and `-tag:` in the search box (the saved-search rule editor in the mockup is not in the app)
- [x] 4.8 MCP tools `list_tags`, `get_item_tags`, `set_tags` and `set_tag_color`. The two that change the library
      refuse unless the server is started with `--allow-write`, because the permission model in the mockup's Agent &
      MCP page is not built

## 5. Tests and docs

- [x] 5.1 Index tests: add, remove, case-insensitive uniqueness, rename, delete, search by tag
- [x] 5.2 Projection encode and decode tests for both formats, on every platform
- [x] 5.3 Reconciliation tests against a temporary directory on Linux, with `user.xdg.tags`
- [ ] 5.4 Manual check on a real Mac: tag in Libri with a color, see both in Finder; remove the tag in Finder,
      relaunch, see it gone from Libri and the activity panel say so; click through the new views
      (2026-10-19: the View menu being enabled and scrolling speed are confirmed on a Mac. Still to check: the
      Finder round trip, the column heading menu and Tags column, and Group by Tag in all three layouts.)
- [x] 5.5 Documentation page (`docs/site/latest/tags.md`), including how edits made outside Libri are read back and when the later change wins
