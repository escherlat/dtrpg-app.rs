//! Item (and file) read/write/search against the library index.

use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::data::enums::ItemStatus;
use crate::data::index::IndexError;
use crate::data::library::{LibraryItem, LibraryItemFile};

fn status_to_text(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Downloaded => "downloaded",
        ItemStatus::Cloud => "cloud",
    }
}

fn status_from_text(text: &str) -> ItemStatus {
    match text {
        "downloaded" => ItemStatus::Downloaded,
        _ => ItemStatus::Cloud,
    }
}

/// Replaces the entire item catalog with `items`, matching the on-disk JSON
/// cache's full-overwrite semantics (`save_catalog_cache`) -- the caller
/// always passes the complete current catalog, not a delta.
///
/// Runs as a single transaction: every item's row, its files, and its FTS5
/// entry are all replaced together, so a reader never observes a catalog
/// mid-replacement and the text index never lags the catalog it describes
/// (`libri-library-index`'s "index follows catalog changes" requirement).
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if any statement in the transaction fails;
/// the transaction is rolled back and the previous catalog is left intact.
pub(super) fn replace_all_items(
    conn: &mut Connection, items: &[LibraryItem],
) -> Result<(), IndexError> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM files", [])?;
    tx.execute("DELETE FROM items", [])?;
    tx.execute("DELETE FROM items_fts", [])?;

    for item in items {
        insert_item(&tx, item)?;
    }

    tx.commit()?;
    Ok(())
}

fn insert_item(conn: &Connection, item: &LibraryItem) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO items (
            id, numeric_id, order_product_id, product_id, title, publisher, line, kind,
            format, pages, size_mb, year, added_order, status, color, description,
            cover_url, detail_cover_url, date_added, date_updated, is_available
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                  ?17, ?18, ?19, ?20, ?21)",
        params![
            item.id.as_ref(),
            item.numeric_id as i64,
            item.order_product_id as i64,
            item.product_id as i64,
            item.title.as_ref(),
            item.publisher.as_ref(),
            item.line.as_ref(),
            item.kind.as_ref(),
            item.format.as_ref(),
            item.pages,
            item.size_mb,
            item.year,
            item.added_order,
            status_to_text(item.status),
            item.color.as_ref(),
            item.desc.as_ref(),
            item.cover_url.as_deref(),
            item.detail_cover_url.as_deref(),
            item.date_added,
            item.date_updated,
            item.is_available,
        ],
    )?;

    for file in &item.files {
        conn.execute(
            "INSERT INTO files (item_id, file_id, file_index, name, format, size_mb, downloaded, offloaded)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                item.id.as_ref(),
                file.id.as_ref(),
                file.index,
                file.name.as_ref(),
                file.format.as_ref(),
                file.size_mb,
                file.downloaded,
                file.offloaded,
            ],
        )?;
    }

    let file_names = item
        .files
        .iter()
        .map(|f| f.name.as_ref())
        .collect::<Vec<_>>()
        .join(" ");
    conn.execute(
        "INSERT INTO items_fts (item_id, title, publisher, authors, description, tags,
                                 file_names, summary_text)
         VALUES (?1, ?2, ?3, '', ?4, '', ?5, '')",
        params![
            item.id.as_ref(),
            item.title.as_ref(),
            item.publisher.as_ref(),
            item.desc.as_ref(),
            file_names,
        ],
    )?;
    Ok(())
}

fn row_to_item(row: &Row<'_>) -> rusqlite::Result<LibraryItem> {
    Ok(LibraryItem {
        id: Arc::from(row.get::<_, String>("id")?),
        numeric_id: row.get::<_, i64>("numeric_id")? as u64,
        order_product_id: row.get::<_, i64>("order_product_id")? as u64,
        product_id: row.get::<_, i64>("product_id")? as u64,
        title: Arc::from(row.get::<_, String>("title")?),
        publisher: Arc::from(row.get::<_, String>("publisher")?),
        line: Arc::from(row.get::<_, String>("line")?),
        kind: Arc::from(row.get::<_, String>("kind")?),
        format: Arc::from(row.get::<_, String>("format")?),
        pages: row.get("pages")?,
        size_mb: row.get("size_mb")?,
        year: row.get("year")?,
        added_order: row.get("added_order")?,
        status: status_from_text(&row.get::<_, String>("status")?),
        color: Arc::from(row.get::<_, String>("color")?),
        desc: Arc::from(row.get::<_, String>("description")?),
        cover_url: row.get::<_, Option<String>>("cover_url")?.map(Arc::from),
        detail_cover_url: row
            .get::<_, Option<String>>("detail_cover_url")?
            .map(Arc::from),
        date_added: row.get("date_added")?,
        date_updated: row.get("date_updated")?,
        thumbnail_last_attempted: None,
        is_available: row.get("is_available")?,
        availability_last_checked: None,
        files: Vec::new(),
    })
}

fn load_files_for(conn: &Connection, item_id: &str) -> rusqlite::Result<Vec<LibraryItemFile>> {
    let mut stmt = conn.prepare(
        "SELECT file_id, file_index, name, format, size_mb, downloaded, offloaded
         FROM files WHERE item_id = ?1 ORDER BY file_index",
    )?;
    let files = stmt
        .query_map(params![item_id], |row| {
            Ok(LibraryItemFile {
                id: Arc::from(row.get::<_, String>(0)?),
                index: row.get(1)?,
                name: Arc::from(row.get::<_, String>(2)?),
                format: Arc::from(row.get::<_, String>(3)?),
                size_mb: row.get(4)?,
                downloaded: row.get(5)?,
                offloaded: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(files)
}

/// Returns every item in the catalog, in `added_order` order (matching the
/// JSON cache's existing on-load ordering).
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn list_items(conn: &Connection) -> Result<Vec<LibraryItem>, IndexError> {
    let mut stmt = conn.prepare("SELECT * FROM items ORDER BY added_order")?;
    let mut items = stmt
        .query_map([], row_to_item)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut items {
        item.files = load_files_for(conn, &item.id)?;
    }
    Ok(items)
}

/// Returns the item identified by `id`, or `None` if no such item exists.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn get_item(conn: &Connection, id: &str) -> Result<Option<LibraryItem>, IndexError> {
    let mut stmt = conn.prepare("SELECT * FROM items WHERE id = ?1")?;
    let item = stmt.query_row(params![id], row_to_item).optional()?;
    let Some(mut item) = item else {
        return Ok(None);
    };
    item.files = load_files_for(conn, &item.id)?;
    Ok(Some(item))
}

/// Full-text searches title, publisher, description, and file names for
/// `query`, returning matching items ranked by FTS5's default relevance
/// order.
///
/// `query` is free text, not FTS5 query syntax -- see [`sanitize_fts_query`].
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn search(conn: &Connection, query: &str) -> Result<Vec<LibraryItem>, IndexError> {
    let fts_query = sanitize_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT items.* FROM items_fts
         JOIN items ON items.id = items_fts.item_id
         WHERE items_fts MATCH ?1
         ORDER BY rank",
    )?;
    let mut items = stmt
        .query_map(params![fts_query], row_to_item)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut items {
        item.files = load_files_for(conn, &item.id)?;
    }
    Ok(items)
}

/// Turns free-text user input into a safe FTS5 query: each whitespace-
/// separated token becomes a double-quoted phrase (FTS5 special characters
/// inside -- apostrophes, colons, hyphens, parentheses -- are neutralized by
/// the quoting, so a title like "Player's Handbook" or "Sci-Fi" searches as
/// literal text rather than being parsed as query syntax), ANDed together
/// via FTS5's default bareword-adjacency operator.
///
/// A literal double-quote in the input is escaped by doubling it, per FTS5's
/// own string-literal escaping rule.
fn sanitize_fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Counts every item in the catalog.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn count_all(conn: &Connection) -> Result<usize, IndexError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))?;
    Ok(count as usize)
}

/// Counts items whose `status` matches `status`.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn count_by_status(conn: &Connection, status: ItemStatus) -> Result<usize, IndexError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM items WHERE status = ?1",
        params![status_to_text(status)],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

/// Counts items whose most recent timestamp (`date_updated`, falling back
/// to `date_added`) falls within `window_days` of `now_secs` -- the same
/// recency rule as [`crate::util::matching::item_recently_updated`], run as
/// a query instead of an in-memory scan.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn count_recently_updated(
    conn: &Connection, now_secs: i64, window_days: u32,
) -> Result<usize, IndexError> {
    let window_secs = i64::from(window_days) * 24 * 60 * 60;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM items
         WHERE COALESCE(date_updated, date_added) >= ?1 - ?2",
        params![now_secs, window_secs],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod sanitize_tests {
    use super::sanitize_fts_query;

    #[test]
    fn single_word_is_quoted() {
        assert_eq!(sanitize_fts_query("Moria"), "\"Moria\"");
    }

    #[test]
    fn apostrophe_is_neutralized_by_quoting() {
        assert_eq!(
            sanitize_fts_query("Player's Handbook"),
            "\"Player's\" \"Handbook\""
        );
    }

    #[test]
    fn literal_double_quote_is_escaped() {
        assert_eq!(sanitize_fts_query("say \"hi\""), "\"say\" \"\"\"hi\"\"\"");
    }

    #[test]
    fn empty_input_produces_empty_query() {
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("   "), "");
    }
}
