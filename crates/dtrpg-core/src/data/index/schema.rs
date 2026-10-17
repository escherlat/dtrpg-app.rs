//! DDL for the library index. See
//! `docs/adr/0003-sqlite-library-index.md`.

use rusqlite::Connection;

/// Bump whenever the schema changes in a way that requires existing
/// databases to be rebuilt, mirroring `catalog_cache.rs`'s
/// `CACHE_SCHEMA_VERSION` convention for the JSON cache it replaces.
pub(super) const SCHEMA_VERSION: i64 = 2;

/// Creates every table, index, and the FTS5 virtual table if they don't
/// already exist, then records (or checks) the schema version via SQLite's
/// `user_version` pragma.
///
/// # Errors
///
/// Returns a [`rusqlite::Error`] if any DDL statement fails.
pub(super) fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    let previous_version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    conn.execute_batch(
                       "
        CREATE TABLE IF NOT EXISTS items (
            id                TEXT PRIMARY KEY,
            numeric_id        INTEGER NOT NULL,
            order_product_id  INTEGER NOT NULL,
            product_id        INTEGER NOT NULL,
            title             TEXT NOT NULL,
            publisher         TEXT NOT NULL,
            line              TEXT NOT NULL,
            kind              TEXT NOT NULL,
            format            TEXT NOT NULL,
            pages             INTEGER NOT NULL,
            size_mb           REAL NOT NULL,
            year              INTEGER NOT NULL,
            added_order       INTEGER NOT NULL,
            status            TEXT NOT NULL,
            color             TEXT NOT NULL,
            description       TEXT NOT NULL,
            cover_url         TEXT,
            detail_cover_url  TEXT,
            date_added        INTEGER,
            date_updated      INTEGER,
            is_available      INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS files (
            item_id    TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
            file_id    TEXT NOT NULL,
            file_index INTEGER NOT NULL,
            name       TEXT NOT NULL,
            format     TEXT NOT NULL,
            size_mb    REAL NOT NULL,
            downloaded INTEGER NOT NULL DEFAULT 0,
            offloaded INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (item_id, file_id, name)
        );
        CREATE INDEX IF NOT EXISTS files_by_item ON files(item_id);

        CREATE TABLE IF NOT EXISTS collections (
            id         INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS collection_membership (
            collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            member_id     INTEGER NOT NULL,
            PRIMARY KEY (collection_id, member_id)
        );

        CREATE TABLE IF NOT EXISTS tags (
            id   INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS item_tags (
            item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
            tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (item_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS storage_roots (
            id   INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS item_placement (
            item_id        TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
            storage_root_id INTEGER NOT NULL REFERENCES storage_roots(id) ON DELETE CASCADE,
            PRIMARY KEY (item_id, storage_root_id)
        );

        CREATE TABLE IF NOT EXISTS summaries (
            item_id    TEXT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
            text       TEXT NOT NULL,
            model      TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            token_count INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS audit_log (
            id     INTEGER PRIMARY KEY AUTOINCREMENT,
            ts     INTEGER NOT NULL,
            action TEXT NOT NULL,
            detail TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
            item_id UNINDEXED,
            title,
            publisher,
            authors,
            description,
            tags,
            file_names,
            summary_text
        );
        ",
    )?;
    if previous_version == 1 {
        conn.execute("ALTER TABLE files ADD COLUMN offloaded INTEGER NOT NULL DEFAULT 0", [])?;
    }
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}
