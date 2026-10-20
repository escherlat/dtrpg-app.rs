## Status

Accepted

## Context

Persistence today is four hand-rolled caches: catalog and collections
caches are schema-versioned JSON (`CACHE_SCHEMA_VERSION` is currently at
3), settings and window state are TOML, covers and avatars are raw bytes on
disk, and credentials live in the platform keyring. There is also a grouped-
item cache derived from the catalog cache at load time. Search is a linear
scan over an in-memory `Vec` in `controllers/library/filtering.rs`.

Five of the twelve product goals each want an index of their own: search
(goal 3, which today is a linear scan with no facets and no scoped query
syntax), tags (part of goal 6's OS-native integration), multi-root storage
placement (goal 5), content summaries (goal 10), and stats (goal 12,
currently spec-only). Writing five more bespoke caches, each with its own
schema version and its own save and load and corruption-recovery code, is
strictly more code and more invariants to keep consistent than adopting one
database that can serve all five from the same underlying tables.

Two bindings were considered for that database: `rusqlite` and `sqlx`.

`sqlx` is async-first, built around a connection pool and `Future`-based
execution. The index this decision covers is a local, single-writer file,
not a networked database serving concurrent clients, so the async story
buys nothing here. What it does cost is a connection pool to manage, a
runtime dependency sitting in the query path, and compile-time macro setup
that assumes either a live database or generated offline metadata at build
time. `rusqlite` is a synchronous, direct binding that matches the app's
existing synchronous service and cache call pattern with no async-runtime
bridge needed. This was a judgment call made in the absence of a strong
preference either way, decided on these grounds, with the explicit note
that it should be reconsidered if the index ever moves off-device (a
networked, multi-writer database would change the calculus back toward
`sqlx` or a server-side engine entirely).

`bundled` (compiling SQLite from source as part of the build rather than
linking the system's `libsqlite3`) matters specifically for goal 1 (macOS,
Windows, and Linux support): it removes any system SQLite version skew
across the three platforms and needs nothing pre-installed on a build
agent.

Alternative considered and rejected: keep the JSON caches and add an
in-memory inverted index on top for search. This was rejected because it is
a search engine nobody asked the team to build and maintain, duplicating
work SQLite's FTS5 extension already does, and it does nothing for the
other four goals (tags, placement, summaries, stats) that also want
queryable, joinable state rather than just full-text search.

## Decision

Adopt `rusqlite` 0.40.2 (the current release in the 0.40 line) with the
`bundled` and `fts5` features as the single
on-disk library index, superseding the catalog, collections, and
grouped-item JSON caches. One SQLite database holds tables for items,
files, collections, collection membership, tags, item tags, storage roots,
item placement, summaries, and an audit log, plus an FTS5 virtual table
over title, publisher, authors, description, tags, file names, and summary
text. Facet counts and the planned Insights aggregates (goal 12) are both
`GROUP BY` queries over the same `WHERE` clause as the current view, which
is what makes facet numbers trustworthy rather than decorative. Cover and
thumbnail bytes stay as files on disk, since that already works and gains
nothing from moving into the database.

Existing JSON caches become a one-way migration source: on first launch
after this change, the app reads the existing catalog and collections
caches and writes the index from them, then keeps the JSON files in place
rather than deleting them, so a downgrade still has data to read and the
caches remain rebuildable from the DriveThruRPG API in the worst case.

This decision depends on 0002: the index lives in the headless
`dtrpg-core`, reached only through `LibraryApi`, so both the desktop UI and
the MCP server query the same index through the same facade rather than
each holding their own cache.

## Consequences

- Search, faceting, tags, placement, and stats all become queries against
  one index instead of five independent bespoke caches, which is a net
  reduction in the amount of persistence code the app carries even though
  it adds a database dependency.
- FTS5 full-text search becomes available without hand-rolling a tokenizer
  or inverted index.
- The app takes on a schema to version and migrate, which JSON caches never
  needed. A wrong migration is a new failure mode.
- `rusqlite`'s synchronous API means database calls still have to go
  through the same off-render-path discipline the app already follows for
  I/O; this decision does not relax that rule, it changes what kind of I/O
  runs off the render path.
- Build time and binary size grow modestly from compiling SQLite's
  amalgamation (`bundled`), traded for removing system-SQLite version skew
  across macOS, Windows, and Linux and removing any build-agent
  installation requirement.
- The migration from JSON caches to the index is one-way and needs to run
  correctly on first launch; keeping the JSON files in place rather than
  deleting them is the stated mitigation for a bad migration, not a
  guarantee against one.
- `unwrap` and `expect` are banned in production paths by this codebase's
  existing CI grep; database and FTS code is a common place for those to
  creep in under time pressure, and review should watch for it
  specifically on this change.
