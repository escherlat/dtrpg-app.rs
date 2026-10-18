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
clears its files through `project_item_tags`. Wiring them into the app (3.1, 3.3, 3.4) is the next slice.

- [ ] 3.1 On launch, in the background, compare each downloaded file with the index and rewrite the differences
- [x] 3.2 Skip and log files that cannot be written; never fail the launch
- [ ] 3.3 Report the number of restored files in the activity panel
- [ ] 3.4 Project an item's tags when one of its files finishes downloading

## 4. UI

- [ ] 4.1 Tag editor in the detail panel
- [ ] 4.2 Catalog filter by tag
- [ ] 4.3 Text describing the file-system projection only where `filesystem_tags` is true

## 5. Tests and docs

- [ ] 5.1 Index tests: add, remove, case-insensitive uniqueness, rename, delete, search by tag
- [ ] 5.2 Projection encode and decode tests for both formats, on every platform
- [ ] 5.3 Reconciliation tests against a temporary directory on Linux, with `user.xdg.tags`
- [ ] 5.4 Manual check on a real Mac: tag in Libri, see it in Finder; remove it in Finder, relaunch, see it return
- [ ] 5.5 Documentation page, including that edits made outside Libri are overwritten on launch
