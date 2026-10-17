//! Collection read/write against the library index.

use std::sync::Arc;

use rusqlite::{Connection, params};

use crate::data::collection::CollectionEntry;
use crate::data::index::IndexError;

/// Replaces the entire collection list with `entries`, matching the on-disk
/// JSON cache's full-overwrite semantics (`save_collections_cache`).
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if any statement in the transaction fails;
/// the transaction is rolled back and the previous list is left intact.
pub(super) fn replace_all_collections(conn: &mut Connection, entries: &[CollectionEntry])
                                      -> Result<(), IndexError> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM collection_membership", [])?;
    tx.execute("DELETE FROM collections", [])?;

    for entry in entries {
        tx.execute("INSERT INTO collections (id, name, created_at) VALUES (?1, ?2, ?3)",
                   params![entry.id as i64, entry.name.as_ref(), entry.created_at])?;
        for member_id in entry.member_ids.iter() {
            tx.execute(
                "INSERT INTO collection_membership (collection_id, member_id) VALUES (?1, ?2)",
                params![entry.id as i64, *member_id as i64],
            )?;
        }
    }

    tx.commit()?;
    Ok(())
}

/// Returns every collection, each with its member id list populated.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn list_collections(conn: &Connection) -> Result<Vec<CollectionEntry>, IndexError> {
    let mut stmt = conn.prepare("SELECT id, name, created_at FROM collections ORDER BY id")?;
    let mut entries = stmt.query_map([], |row| {
                              Ok(CollectionEntry { id:         row.get::<_, i64>(0)? as u64,
                                                   name:       Arc::from(row.get::<_, String>(1)?),
                                                   member_ids: Arc::from([]),
                                                   created_at: row.get(2)?, })
                          })?
                          .collect::<rusqlite::Result<Vec<_>>>()?;

    for entry in &mut entries {
        let mut member_stmt = conn.prepare(
            "SELECT member_id FROM collection_membership WHERE collection_id = ?1 ORDER BY member_id",
        )?;
        let members = member_stmt.query_map(params![entry.id as i64], |row| {
                                     row.get::<_, i64>(0).map(|v| v as u64)
                                 })?
                                 .collect::<rusqlite::Result<Vec<_>>>()?;
        entry.member_ids = Arc::from(members);
    }

    Ok(entries)
}
