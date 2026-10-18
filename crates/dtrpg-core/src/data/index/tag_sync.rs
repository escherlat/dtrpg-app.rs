//! The tags the index and a file last agreed on.
//!
//! Which side changed since then is what tells a tag removed in Finder (the
//! file changed) from one never written to the file (the index changed), so
//! it is kept for every file whose tags have been synced.

use std::collections::HashSet;

use rusqlite::{Connection, params};

use crate::data::index::IndexError;

/// Joins tag names for storage. A tag cannot contain a control character, so a
/// newline cannot appear inside one.
fn join(tags: &[String]) -> String {
    tags.join("\n")
}

/// Returns the tags `path` last agreed on with the index, or `None` if its tags
/// have never been synced.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn snapshot(conn: &Connection, path: &str) -> Result<Option<Vec<String>>, IndexError> {
    let mut stmt = conn.prepare("SELECT tags FROM file_tag_sync WHERE path = ?1")?;
    let mut rows = stmt.query_map(params![path], |row| row.get::<_, String>(0))?;
    Ok(rows.next().transpose()?.map(|tags| {
                                   tags.split('\n')
                                       .filter(|tag| !tag.is_empty())
                                       .map(str::to_owned)
                                       .collect()
                               }))
}

/// Records that `path`, a file of `item_id`, and the index agree on `tags` as
/// of `synced_at`.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the statement fails.
pub(super) fn record(conn: &Connection, path: &str, item_id: &str, tags: &[String],
                     synced_at: i64)
                     -> Result<(), IndexError> {
    conn.execute("INSERT INTO file_tag_sync (path, item_id, tags, synced_at)
                  VALUES (?1, ?2, ?3, ?4)
                  ON CONFLICT(path) DO UPDATE SET item_id = excluded.item_id,
                                                  tags = excluded.tags,
                                                  synced_at = excluded.synced_at",
                 params![path, item_id, join(tags), synced_at])?;
    Ok(())
}

/// Forgets every record for a path not in `present`, and returns how many that
/// was. A file that is gone and comes back is then synced afresh, rather than
/// compared with tags it had before it left.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if a statement fails.
pub(super) fn forget_all_but(conn: &mut Connection, present: &HashSet<String>)
                             -> Result<usize, IndexError> {
    let tx = conn.transaction()?;
    let stale = {
        let mut stmt = tx.prepare("SELECT path FROM file_tag_sync")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|path| !present.contains(path))
            .collect::<Vec<_>>()
    };
    for path in &stale {
        tx.execute("DELETE FROM file_tag_sync WHERE path = ?1", params![path])?;
    }
    tx.commit()?;
    Ok(stale.len())
}
