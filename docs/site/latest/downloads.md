---
title: Downloads
version: latest
slug: downloads
---

# Downloads

Queue a catalog item's files for download from its item popover or detail
tab.

## Following a download in the catalog

An item with a file queued or downloading shows a small spinner in the
catalog's status column (list view) or next to its publisher (grid and
thumbnail views), in place of the usual cloud or downloaded marker.
Hovering it shows "Downloading…". It switches to the downloaded marker when
the last file lands, or back to the cloud marker if you cancel. A muted
spinner means something different: Libri is checking the item's files
against what is on disk.

## Download progress

A running download appears in the activity panel with a progress bar that
fills as the file arrives. If the server doesn't say how large the file is,
the bar stays indeterminate instead of guessing. When a download retries
after a failure, its label says so, and the bar starts over for the new
attempt.

## Where files go

Downloads save to the folder set in Settings > Downloads' Download
Location field, shown there with a reveal-in-file-manager action. Changing
the location doesn't move files you've already downloaded.

Settings > Downloads also has a "Create collections" toggle. When it's on,
downloading an item that belongs to one or more collections also creates a
symlink for it under a `collections/` subfolder, organized by collection,
so the same downloaded file is reachable there without duplicating the
data on disk. If you add an item you have already downloaded to another
collection, the symlink for that collection is created then. Removing an item
from a collection removes its symlink from that collection's folder. Only a
symlink that points at the item's downloaded file is removed: a file or a link
to somewhere else with the same name is left alone. A link you made yourself
that points at the same file looks just the same, and is removed too. The
collection's folder stays, even when it is empty. On Windows this needs
Developer Mode (or an elevated process) - if symlink creation fails, the toggle
has no effect rather than erroring.

## Concurrency

Settings > Downloads' "Max concurrent downloads" field is a shared limit
for both thumbnail loading and file downloads, not file downloads alone.
Libri dispatches queued files to fill as many of that many concurrency
slots as are free, rather than downloading everything at once or strictly
one at a time.

A download you start goes ahead of thumbnails still waiting to load, so a
large catalog loading its covers doesn't hold up a file you asked for.
Thumbnails already loading finish first; nothing is interrupted.

## Retries and disk space

A transient failure (a dropped connection, a 5xx response) retries with
backoff rather than failing the download outright. Libri also checks
available disk space before starting a download and warns if it's low,
rather than letting a transfer fail partway through from running out of
room.

## Checking availability

Catalog items occasionally get re-checked against the server to confirm
they're still available for download, on a cooldown so repeatedly viewing
the same item's details doesn't trigger a check every time.

Checks run one at a time, and they don't count against the concurrency
limit above, so a busy download queue never holds one up. A check you ask
for, by opening an item's details or choosing Check Item Availability,
goes ahead of the automatic background ones.
