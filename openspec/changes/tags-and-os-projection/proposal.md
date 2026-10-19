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
- Reconcile on launch: for each downloaded file on a platform that supports projection, compare the file's tags,
  the index's, and the tags the two last agreed on, and bring them in step by which side changed. A tag edited in
  Finder is read back; if both sides changed, the later change wins, and the index wins when that cannot be told
  (ADR 0008, which supersedes the "index wins" rule of ADR 0004). What it did is shown to the user, not done
  silently.
- Give tags a color, one of the seven Finder uses, and keep it through the file's tags on macOS.
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
- **Tag colors.** (Revised 2026-10-18, after the Libri v2 design was accepted.) Tags have a color, one of
  Finder's seven, which Libri manages and writes on macOS.
- **Colors are not read from files.** (2026-10-19, ADR 0009.) The color number in a file is the color the tag had
  when it was applied, not the color Finder shows now, so a tag read from a file has no color.
- **Reading Finder's tags back.** (2026-10-18.) Yes. If both sides changed, the later change wins, and the index
  wins when that cannot be told. Each direction and the launch check have a switch in settings.
- **Dependency.** The `xattr` crate, for macOS and Linux builds only, so Windows builds do not pull it in.

## Impact

- `dtrpg-core`: `data/index` (tag queries), `api` (tag methods), a new `platform` tag projection, and the
  `xattr` dependency under `[target.'cfg(unix)'.dependencies]`.
- `dtrpg-ui`: detail panel tag editor, catalog tag filter, reconciliation on launch.
- `dtrpg-mcp`: the tag methods are available to expose as tools, not part of this change.
- Documentation: a page in `docs/site/` on tags, including that edits made outside Libri are overwritten on
  launch, and which platforms project them.
