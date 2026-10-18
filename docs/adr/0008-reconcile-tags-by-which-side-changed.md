## Status

Accepted. Supersedes the reconciliation rule in 0004 ("the index wins when the two disagree"); the rest of 0004
stands.

## Context

0004 made the library index the source of truth for tags and a file's tags a projection of it, reconciled on
launch with the index winning. That is simple, and it is safe on Windows, where files hold no tags. It also means a
tag the user adds or removes in Finder is undone at the next launch, which the accepted Libri v2 design does not
want: its Native Integration settings have "Read Finder tags back into Libri" (on by default), next to "Write
Libri tags to Finder tags" and "Reconcile on launch".

Reading Finder's tags back looks like a small change to the rule, but the current tags alone cannot support it.
A file with no "solo" tag and an index with one could mean the user removed it in Finder, or that Libri never
wrote it to that file. Choosing between them takes what the two last agreed on.

A file's tags carry no per-tag times either. The only time the platform gives is the change time of the file's
metadata, which an attribute write moves, and which a rename or a permission change moves too.

## Decision

Reconcile a file's tags by which side changed since the file and the index last agreed.

The index keeps, for each file it has synced, the tag names the two last agreed on (the snapshot). At launch, and
for each file, the file's tags, the index's tags and the snapshot are compared as sets, ignoring case and order:

- Only the index differs from the snapshot: write the index's tags to the file.
- Only the file differs: read the file's tags into the index, including removals.
- Both differ: the later change wins, comparing the file's metadata change time with the time the index's tags
  last changed. If either time is missing, or they are equal, the index wins.
- A file never synced has no snapshot, which counts as having agreed on no tags. Tags a file had before it was
  ever synced are read into an index that has none for the item.

The index's change time for tags read from a file is the file's change time, so a later comparison with another
file of the same item is consistent. The snapshot of a file that is no longer in the library is forgotten, so a
file that comes back is synced afresh, not read as having lost its tags.

Tag colors are not part of the comparison. When tags are written, the index's color is written (macOS only,
and the seven colors are Finder's own, so none is approximated). When a tag is read from a file, it takes the
file's color only if the library has none for it.

Each direction, and the launch check, can be turned off in settings. A direction that is off is skipped without
touching the snapshot, so the pending difference is applied when it is turned back on. A change made in Libri
itself, and a file that has just been downloaded, are projected onto the file directly, and the index wins.

## Consequences

- A tag edited in Finder, or removed there, survives and shows up in Libri. This is the behavior users expect of
  a tag that appears in both places.
- Where both sides changed one file's tags between launches, the outcome depends on a time that can be wrong, so
  the rule can pick the wrong side. The index winning on a tie or an unknown time keeps this the safe, familiar
  choice, and the activity panel reports what was written and what was read, so it is not silent.
- Files of one item can disagree. The first one read changes the index, and the others are then written to match,
  so one file's edit reaches the rest.
- The index grows a row for each synced file, one per downloaded file in the library.
- Index schema version 4: a color on tags, and two new tables, one for when an item's tags last changed and one
  for the snapshots.
- Windows is unchanged: files hold no tags, so there is nothing to compare, and the index remains the only copy.
- A file downloaded over an older one at the same path would be compared with the old one's snapshot, and read as
  having lost every tag. A finished download therefore forgets the snapshot of its path, whether or not the item's
  tags are written to it.
