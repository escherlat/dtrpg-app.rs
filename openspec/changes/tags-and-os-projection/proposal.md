## Why

The library index already has `tags` and `item_tags` tables (ADR 0003), but nothing reads or writes them: there is
no tag API, no way to tag an item in the UI, and no connection to the operating system's tags. Users of a
file-based library expect a tag they set in Libri to show up in Finder, and a tag they set in Finder to show up
in Libri.

ADR 0004 decided how: the index is the source of truth, and the file system is a projection of it, reconciled on
launch. This change builds that. It was split out of stage 4 of `libri-v2-architecture` (task 4.7) because it
needs its own design for tag editing, projection and reconciliation, and it adds the first dependency on
file-system extended attributes.

## What Changes

- Add tag operations to the index and to `LibraryApi`: list all tags, list an item's tags, add and remove a tag on
  an item, rename a tag, and delete a tag. Tags are free text, unique ignoring case.
- Add tag editing to the desktop UI: show an item's tags in the detail panel, add and remove them there, and
  filter the catalog by tag.
- Project an item's tags onto each of its downloaded files, where the platform supports it:
  macOS `com.apple.metadata:_kMDItemUserTags`, Linux `user.xdg.tags`. Windows has nothing to project onto, so
  its tags live only in the index.
- Reconcile on launch: for each downloaded file on a platform that supports projection, compare the file's tags
  with the index and make the file match the index. The index wins. A tag edited outside Libri is overwritten,
  and this is shown to the user, not done silently (ADR 0004, Consequences).
- Report tag projection through `Capabilities::filesystem_tags`, which already exists. The UI offers tag
  editing on every platform, because the index holds the tags; it only describes the file-system projection
  where `filesystem_tags` is true.

## Capabilities

### New Capabilities

- `library-tags`: tags on library items, stored in the index and editable through `LibraryApi` and the UI.
- `tag-file-projection`: writing an item's tags to its files' extended attributes, and reconciling them on launch.

### Modified Capabilities

None. `libri-platform-integration` already requires "the index is authoritative for tags"; this change meets it.

## Decisions

Settled with the project owner on 2026-10-18.

- **Which files carry the tags?** Every downloaded file of the item carries all the item's tags, since the file is
  what Finder shows.
- **What does reconciliation show the user?** A count in the activity panel ("2 files' tags restored from your
  library") and a line in the log naming each file. No dialog, and never silent.
- **Tag colors.** Libri writes the name only. When it rewrites a file's tags, a color an existing tag of the same
  name already has is kept. Libri does not manage colors.
- **Dependency.** The `xattr` crate, for macOS and Linux builds only, so Windows builds do not pull it in.

## Impact

- `dtrpg-core`: `data/index` (tag queries), `api` (tag methods), a new `platform` tag projection, and the
  `xattr` dependency under `[target.'cfg(unix)'.dependencies]`.
- `dtrpg-ui`: detail panel tag editor, catalog tag filter, reconciliation on launch.
- `dtrpg-mcp`: the tag methods are available to expose as tools, not part of this change.
- Documentation: a page in `docs/site/` on tags, including that edits made outside Libri are overwritten on
  launch, and which platforms project them.
