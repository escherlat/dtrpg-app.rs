# Proposal

## Why

The Libri v2 mockups (`dtrpg-design/claude-2/screenshots/`, 22 images covering
all 15 screens of the prototype's `SCREENS` list) are the desired look and
feel. They were waiting on ADR 0002's structural work, which is now done: the
library state, queries and mutations are headless in `dtrpg_core::Library`, and
`LibraryController` is a thin view model over it.

This change records where the current app stands against those mockups, screen
by screen, and proposes the order to close the gaps. It is analysis, not
implementation: it changes no code.

## What Changes

- `design.md` holds the gap matrix (built, partial or missing, per screen), the
  data-model and query work that several screens share, and the proposed slices.
- `tasks.md` lists the slices as a checklist, so each can become its own issue
  and OpenSpec change when it starts.

## Outcomes

A shared, ordered backlog for the v2 UI, with the shared foundations (item
availability states, per-file location, a query language, facet queries)
identified up front so the screens built on them are not blocked or reworked.

## How this was checked

By reading the mockups and the code: the views under
`crates/dtrpg-ui/src/ui/views/`, the core types in `crates/dtrpg-core`, and the
`dtrpg-mcp` tool list. The app was not run and compared visually, so "built"
means the feature exists in code, not that it matches the mockup pixel for
pixel. Items I could not confirm from code are marked "unverified".
