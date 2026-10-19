//! SQLite operations for persisted content summaries.

use rusqlite::{Connection, params};

use crate::data::summary::ContentSummary;

pub(super) fn save(conn: &mut Connection, summary: &ContentSummary) -> rusqlite::Result<()> {
    let transaction = conn.transaction()?;
    transaction.execute(
                        "INSERT INTO summaries (item_id, text, model, created_at, token_count)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(item_id) DO UPDATE SET text = excluded.text,
             model = excluded.model, created_at = excluded.created_at,
             token_count = excluded.token_count",
                        params![
        summary.item_id,
        summary.text,
        summary.model,
        summary.created_at,
        summary.token_count as i64
    ],
    )?;
    transaction.execute("UPDATE items_fts SET summary_text = ?1 WHERE item_id = ?2",
                        params![summary.text, summary.item_id])?;
    transaction.commit()
}

pub(super) fn get(conn: &Connection, item_id: &str) -> rusqlite::Result<Option<ContentSummary>> {
    match conn.query_row(
                         "SELECT item_id, text, model, created_at, token_count
         FROM summaries WHERE item_id = ?1",
                         [item_id],
                         |row| {
                             Ok(ContentSummary { item_id:     row.get(0)?,
                                                 text:        row.get(1)?,
                                                 model:       row.get(2)?,
                                                 created_at:  row.get(3)?,
                                                 token_count: row.get::<_, i64>(4)? as usize, })
                         },
    ) {
        Ok(summary) => Ok(Some(summary)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error),
    }
}
