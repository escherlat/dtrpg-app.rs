---
title: Tags
version: latest
slug: tags
---

# Tags

Tags are short labels you put on catalog items, such as "Running", "To Read" or
"Printed", to find them again by what they mean to you. An item can have any
number of tags, and a tag can be on any number of items.

On macOS and Linux a tag is also written to the item's downloaded files, so it
shows up in Finder or your file manager too. A tag you add or remove there is
read back into Libri.

## Tagging an item

Open an item's detail tab. The Tags row shows its tags as chips.

- Choose **Add tag** to open the tag dialog. Tick a tag you have already made to
  put it on the item, or type a new name and choose **Add**.
- Choose the **x** on a chip to take that tag off the item.

A tag name is trimmed of spaces, is not case-sensitive ("Solo" and "solo" are the
same tag, spelled the way you first typed it), and can be up to 64 characters. It
cannot contain a comma, because Linux stores a file's tags as a comma-separated
list.

## Colors

A tag can have one of seven colors: gray, green, purple, blue, yellow, red or
orange. Right-click a tag, either a chip in the detail tab or a row in the
sidebar, and choose a color, or **No color**. The color applies to the tag
wherever it appears.

These are the colors Finder uses for its tags. Libri writes the color to the file on
macOS, but colors are Libri's: Finder keeps its own color for each tag name and
shows that, so a tag can look different in Finder than in Libri. A tag you create in
Finder comes into Libri with no color until you choose one, because the color Finder
stores in a file is only the color the tag had when it was applied. Linux keeps no
color for a file's tags, so the color stays in Libri.

## Browsing by tag

Once something is tagged, a Tags section appears in the sidebar below Publishers,
with the number of items for each tag. Selecting a tag shows only the items that
have it. Removing a tag from its last item ends the filter.

## Searching by tag

Type a tag in the search box to find items by it:

- `tag:solo` shows only the items tagged "solo".
- `-tag:solo` shows only the items that are not.
- Put a name with spaces in quotes: `tag:"to run"`.
- Combine them with each other and with ordinary words: `tag:solo -tag:printed moria`.
  An item has to meet every term.

A tag term matches the whole tag name, ignoring case, so `tag:sol` does not find an
item tagged "solo". Ordinary words in the search box also match an item's tag names,
alongside its title, publisher and game line.

## Tags and your files

Where the platform keeps tags on files, Libri writes an item's tags to each of its
downloaded files, and to a file as soon as it finishes downloading.

| Platform | Where the tags are written | Colors |
|----------|----------------------------|--------|
| macOS | Finder tags | Kept |
| Linux | The file's `user.xdg.tags` attribute | Not kept |
| Windows | Nowhere: files have no tags, so tags stay in Libri's own index | Stay in Libri |

A file that cannot hold tags, for example one on a network share or a volume that
does not support extended attributes, is skipped. Its tags stay in Libri, and the
activity panel says how many files could not be written.

## Changes made outside Libri

Libri keeps its own copy of your tags and treats it as the source of truth, but it
does not overwrite what you do in Finder. When Libri starts, and each time you reload
your library, it compares the tags of every file it finds on disk with its own and
with what the two last agreed on. A tag you change in Finder while Libri is open
comes through at the next reload:

- If you changed only the file's tags, for example by removing "To Read" in
  Finder while Libri was closed, Libri takes the change.
- If you changed only the tags in Libri, Libri writes them to the file.
- If you changed both, the later change wins. If Libri cannot tell which was
  later, Libri's tags win.

The activity panel reports what it did, for example "Tags read from files into your
library (files: 3)". It says nothing if every file already matched.

If a file has tags from before Libri ever looked at it, and the item has none in
Libri, those tags are read into Libri.

If you replace a file by downloading it again, Libri gives the new file the item's
tags, and does not read the missing tags as something you removed.

## Settings

The Tags section of **Settings > Native Integration** has three switches, all on by
default. They are shown only on platforms where files keep tags.

- **Write Libri tags to the file's tags.** Turn it off and Libri stops changing
  your files. The difference is applied when you turn it back on.
- **Read tags changed outside Libri back into Libri.** Turn it off and edits made
  in Finder are ignored.
- **Reconcile on launch.** Turn it off and Libri does not compare files with its
  tags when it starts or when you reload.
