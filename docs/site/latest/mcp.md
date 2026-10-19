---
title: Using Libri from an AI agent
version: latest
slug: mcp
---

# Using Libri from an AI agent

Libri has a small [Model Context Protocol](https://modelcontextprotocol.io/) (MCP)
server, the `dtrpg-mcp` program in this repository, that lets an AI agent look at your
library and, if you allow it, change your tags. It reads Libri's own copy of your
library on your computer. It does not talk to DriveThruRPG.

Start it with `cargo run -p dtrpg-mcp` and point your MCP client at it over standard
input and output.

## What it can read

These are always available.

- `list_items` lists the items in your library.
- `get_item` gets one item by its id.
- `list_tags` lists every tag with its color and how many items have it.
- `get_item_tags` lists the tags of one item.
- `list_archive_entries` lists what is inside a downloaded archive file.

## What it can change

Two tools change your library, so they are off unless you turn them on. Start the
server with `--allow-write`:

```bash
cargo run -p dtrpg-mcp -- --allow-write
```

Without it, these tools reply that changing the library is disabled, and nothing changes.

- `set_tags` gives an item exactly the tags you pass it, replacing the ones it had. A
  tag that already exists keeps its spelling and its color; a new tag has no color.
- `set_tag_color` sets a tag's color to gray, green, purple, blue, yellow, red or
  orange, or clears it.

A tag it cannot store, such as one containing a comma, or an item that is not in the
library, is reported as an error and changes nothing.

## When a change shows up

The server changes Libri's library, not your files and not the window that is open.
The new tags reach an item's files the next time Libri starts or you reload your
library, the same way a tag changed in Libri does. A Libri window that is already open
shows the new tags after it reloads them. See [Tags]({{ '/latest/tags/' | relative_url }}).
