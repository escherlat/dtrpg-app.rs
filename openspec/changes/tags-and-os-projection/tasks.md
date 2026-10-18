## 1. Index and API

- [ ] 1.1 Index methods for tags: list all, list for an item, add, remove, rename, delete; unique ignoring case
- [ ] 1.2 The same methods on `LibraryApi`
- [ ] 1.3 Keep the FTS `tags` column in step with `item_tags`

## 2. Projection

- [ ] 2.1 Add the `xattr` dependency under `[target.'cfg(unix)'.dependencies]`
- [ ] 2.2 `PlatformIntegration` methods to read and write a file's tags; `Capabilities::filesystem_tags`
      decides whether they are used
- [ ] 2.3 macOS: `com.apple.metadata:_kMDItemUserTags` as a binary property list of strings, keeping any
      existing color suffix
- [ ] 2.4 Linux: `user.xdg.tags` as a comma-separated list
- [ ] 2.5 Windows: reading and writing report `Unsupported`, and nothing calls them

## 3. Reconciliation

- [ ] 3.1 On launch, in the background, compare each downloaded file with the index and rewrite the differences
- [ ] 3.2 Skip and log files that cannot be written; never fail the launch
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
