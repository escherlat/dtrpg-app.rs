## Status

Accepted

## Context

Container preview is hardcoded to Zip specifically, and only as a popover.
Goal 9, container contents, is currently partial: Zip only, popover only,
no tree, no single-entry extract, and no format abstraction. The target
design (`dtrpg-design/claude-2`) replaces the popover with an inspector
Contents tab showing a lazy tree, per-entry preview, and selective extract,
and none of that is reachable while the reading code assumes Zip at every
call site.

The immediate, named need is Tar support, including compressed variants
(`.tar.gz`, `.tar.bz2`, `.tar.xz`), which DriveThruRPG distributions use
alongside Zip. Adding Tar on the current structure means a second module
mirroring the Zip-specific one, a second set of UI and controller names,
and a second set of i18n strings, repeated per format from here on.

The stated intent is broader than "support Tar": any container format
should be supportable, so the format list must not be baked into the UI or
the index. The container trait that answers this is also the one the
index's file-name text rows and the planned MCP `list_archive_entries` tool
both need to talk to, so this decision has three callers from the start,
not one.

Entry listing must be lazy by contract, not an optional optimization added
later. Reading a directory of a 200 MB archive to show a tree must not
materialize the archive, which also keeps the inspector's Contents tab
usable when the underlying file is a cloud placeholder that has not been
materialized locally yet (see 0004's platform-capability work on
materialize/download state).

Alternative considered and rejected: per-format branching at each call
site that needs container contents (the inspector, the index, the MCP
tool), rather than one trait and a registry. This was rejected because it
means every new caller has to know the full format list and every new
format has to be threaded through every caller, which is the opposite of
the stated goal that any format should be supportable without touching
callers.

## Decision

Define `dtrpg_core::container`: one trait over the three operations a
caller needs (list entries lazily, read one entry, extract a subset), and a
registry that resolves a file to a reader implementation by file extension
and magic-byte sniffing, not extension alone, so a renamed or double-
extension file still resolves correctly. The inspector's Contents tab, the
index's file-name text rows, and the MCP `list_archive_entries` tool all
talk to the trait, never to a concrete format.

Zip and Tar ship first, including the compressed Tar variants, which are
the first real test of the trait's load-bearing claim: adding a format is
one trait implementation plus one registry line, with no change to any
caller. Compressed Tar variants test this specifically because they
compose a decompressor with the Tar reader rather than being a wholly
separate format, which is exactly the kind of addition the trait needs to
absorb without a callers change. Candidates after that, in rough order of
demand for this library, are 7z, RAR, and `.cbz`/`.cbr` comic archives,
which are Zip and RAR underneath and so mostly need format recognition
rather than new reading code. RAR, if added, is read-only by format
reality, since RAR's compression scheme is proprietary.

Lazy entry listing is part of the trait's contract: the listing operation
returns results incrementally rather than requiring the whole archive to be
read up front, which is what keeps a 200 MB archive's directory listing
from stalling the inspector or from forcing a cloud placeholder file to
materialize just to show its contents.

## Consequences

- Adding Tar support becomes implementing the trait and adding one
  registry line, not duplicating the existing Zip-specific module, its
  controller state, and its i18n strings under a new name.
- The current Zip-specific naming in the UI, controller, and i18n layers
  needs to be renamed to container-generic wording as part of this change,
  across every locale file, not just the English one, or the UI ends up
  with mismatched terminology between languages.
- Magic-byte sniffing adds a small amount of up-front reading for every
  archive opened, even when the extension already made the format obvious,
  a deliberate correctness-over-speed tradeoff for the renamed or double-
  extension case, and cheap relative to the entry-listing work that
  follows.
- Lazy listing changes the UI contract: the inspector's Contents tab needs
  a loading or partial state it does not need today for Zip, whose current
  listing is eager and effectively instant at the archive sizes tested so
  far. This is new UI state to design, not only a backend change the UI is
  unaffected by.
- This decision only records the trait and registry design; the
  implementation itself ships with the `container-browse-and-extract` child
  change, not with this ADR.
- This ADR depends on 0002: the trait lives in the headless `dtrpg-core` so
  the MCP server's `list_archive_entries` tool and the desktop inspector
  both reach it through `LibraryApi` without either one depending on the
  other.
