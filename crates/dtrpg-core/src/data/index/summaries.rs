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

/// Sums `token_count` across every summary created at or after
/// `since_unix_secs`, for enforcing a rolling monthly token ceiling.
///
/// Computed live from the stored summaries rather than a separately
/// maintained running counter, so there is no reset bookkeeping to drift:
/// the caller just passes the start of the current calendar month (in the
/// user's local time) each time, and the figure is always exactly "what the
/// summaries table says was spent since then."
pub(super) fn tokens_used_since(conn: &Connection, since_unix_secs: i64) -> rusqlite::Result<u64> {
    let total: i64 = conn.query_row(
                                    "SELECT COALESCE(SUM(token_count), 0) FROM summaries
         WHERE created_at >= ?1",
                                    params![since_unix_secs],
                                    |row| row.get(0),
    )?;
    Ok(total as u64)
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
