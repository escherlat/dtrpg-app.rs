//! Tag read/write against the library index.
//!
//! Tags are free text, unique ignoring case. The first spelling stored for a
//! tag is the one kept, so adding "Solo" when "solo" exists tags the item with
//! "solo". A tag is only listed while at least one item in the catalog has it.
//!
//! Tag assignments are not tied to the catalog rows by a foreign key, so
//! rewriting the catalog keeps them, and a tag on an item that is briefly
//! missing from a partial catalog is there again when the item returns. An
//! item's `tags` column in the full-text table is refreshed in the same
//! transaction as every change.

use rusqlite::{Connection, params};

use crate::data::index::IndexError;

/// Longest tag, in characters.
const MAX_TAG_CHARS: usize = 64;

/// Trims `raw` and checks that it can be stored as a tag.
///
/// A comma is rejected because Linux stores a file's tags as a comma-separated
/// list, so a tag containing one could not be written to a file.
fn normalize(raw: &str) -> Result<String, IndexError> {
    let tag = raw.trim();
    let reason = if tag.is_empty() {
        Some("a tag cannot be empty")
    }
    else if tag.chars().count() > MAX_TAG_CHARS {
        Some("a tag cannot be longer than 64 characters")
    }
    else if tag.chars().any(|c| c == ',' || c.is_control()) {
        Some("a tag cannot contain a comma or a control character")
    }
    else {
        None
    };
    match reason {
        Some(reason) => Err(IndexError::InvalidTag { tag: raw.to_owned(),
                                                     reason }),
        None => Ok(tag.to_owned()),
    }
}

/// Returns the id of the tag spelled `name`, ignoring case.
fn find_tag_id(conn: &Connection, name: &str) -> rusqlite::Result<Option<i64>> {
    let wanted = name.to_lowercase();
    let mut stmt = conn.prepare("SELECT id, name FROM tags")?;
    let rows = stmt.query_map([], |row| {
                       Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                   })?;
    for row in rows {
        let (id, stored) = row?;
        if stored.to_lowercase() == wanted {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// Returns the names of `item_id`'s tags, ordered ignoring case.
fn names_for_item(conn: &Connection, item_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT t.name FROM item_tags it
                                 JOIN tags t ON t.id = it.tag_id
                                 WHERE it.item_id = ?1")?;
    let mut names = stmt.query_map(params![item_id], |row| row.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
    names.sort_by_key(|name| name.to_lowercase());
    Ok(names)
}

/// Returns `item_id`'s tags joined by spaces, for the full-text table.
pub(super) fn joined_for_item(conn: &Connection, item_id: &str) -> rusqlite::Result<String> {
    Ok(names_for_item(conn, item_id)?.join(" "))
}

/// Rewrites `item_id`'s `tags` column in the full-text table.
fn refresh_search_text(conn: &Connection, item_id: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE items_fts SET tags = ?1 WHERE item_id = ?2",
                 params![joined_for_item(conn, item_id)?, item_id])?;
    Ok(())
}

/// Returns the ids of the items that have the tag `tag_id`.
fn items_with_tag_id(conn: &Connection, tag_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT item_id FROM item_tags WHERE tag_id = ?1")?;
    stmt.query_map(params![tag_id], |row| row.get::<_, String>(0))?
        .collect()
}

/// Deletes the tag `tag_id` if no assignment uses it any more.
fn delete_tag_if_unused(conn: &Connection, tag_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM tags WHERE id = ?1
                  AND NOT EXISTS (SELECT 1 FROM item_tags WHERE tag_id = ?1)",
                 params![tag_id])?;
    Ok(())
}

/// Returns every tag that some item in the catalog has, ordered ignoring case.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn list_tags(conn: &Connection) -> Result<Vec<String>, IndexError> {
    let mut stmt = conn.prepare("SELECT DISTINCT t.name FROM tags t
                                 JOIN item_tags it ON it.tag_id = t.id
                                 JOIN items i ON i.id = it.item_id")?;
    let mut names = stmt.query_map([], |row| row.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
    names.sort_by_key(|name| name.to_lowercase());
    Ok(names)
}

/// Returns the tags of `item_id`, ordered ignoring case.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn tags_for_item(conn: &Connection, item_id: &str) -> Result<Vec<String>, IndexError> {
    Ok(names_for_item(conn, item_id)?)
}

/// Tags `item_id` with `tag`. Returns `false` if the item already had it.
///
/// # Errors
///
/// Returns [`IndexError::InvalidTag`] if `tag` cannot be stored,
/// [`IndexError::NoSuchItem`] if `item_id` is not in the catalog, and
/// [`IndexError::Sqlite`] if a statement fails; nothing is changed on error.
pub(super) fn add_tag(conn: &mut Connection, item_id: &str, tag: &str) -> Result<bool, IndexError> {
    let name = normalize(tag)?;
    let tx = conn.transaction()?;
    let item_exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
                                         params![item_id],
                                         |row| row.get(0))?;
    if !item_exists {
        return Err(IndexError::NoSuchItem(item_id.to_owned()));
    }
    let tag_id = match find_tag_id(&tx, &name)? {
        Some(id) => id,
        None => {
            tx.execute("INSERT INTO tags (name) VALUES (?1)", params![name])?;
            tx.last_insert_rowid()
        }
    };
    let added = tx.execute("INSERT OR IGNORE INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
                           params![item_id, tag_id])?
                > 0;
    if added {
        refresh_search_text(&tx, item_id)?;
    }
    tx.commit()?;
    Ok(added)
}

/// Removes `tag` from `item_id`. Returns `false` if the item did not have it.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if a statement fails; nothing is changed on
/// error.
pub(super) fn remove_tag(conn: &mut Connection, item_id: &str, tag: &str)
                         -> Result<bool, IndexError> {
    let tx = conn.transaction()?;
    let Some(tag_id) = find_tag_id(&tx, tag.trim())?
    else {
        return Ok(false);
    };
    let removed = tx.execute("DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
                             params![item_id, tag_id])?
                  > 0;
    if removed {
        refresh_search_text(&tx, item_id)?;
        delete_tag_if_unused(&tx, tag_id)?;
    }
    tx.commit()?;
    Ok(removed)
}

/// Renames the tag `from` to `to` on every item that has it, and returns how
/// many items that was. If `to` is already another tag, the two are merged.
/// Renaming a tag that does not exist changes nothing and returns 0.
///
/// # Errors
///
/// Returns [`IndexError::InvalidTag`] if `to` cannot be stored, and
/// [`IndexError::Sqlite`] if a statement fails; nothing is changed on error.
pub(super) fn rename_tag(conn: &mut Connection, from: &str, to: &str) -> Result<usize, IndexError> {
    let new_name = normalize(to)?;
    let tx = conn.transaction()?;
    let Some(from_id) = find_tag_id(&tx, from.trim())?
    else {
        return Ok(0);
    };
    let items = items_with_tag_id(&tx, from_id)?;
    match find_tag_id(&tx, &new_name)? {
        Some(target_id) if target_id != from_id => {
            tx.execute("INSERT OR IGNORE INTO item_tags (item_id, tag_id)
                        SELECT item_id, ?2 FROM item_tags WHERE tag_id = ?1",
                       params![from_id, target_id])?;
            tx.execute("DELETE FROM item_tags WHERE tag_id = ?1", params![from_id])?;
            tx.execute("DELETE FROM tags WHERE id = ?1", params![from_id])?;
        }
        _ => {
            tx.execute("UPDATE tags SET name = ?1 WHERE id = ?2",
                       params![new_name, from_id])?;
        }
    }
    for item_id in &items {
        refresh_search_text(&tx, item_id)?;
    }
    tx.commit()?;
    Ok(items.len())
}

/// Removes the tag `tag` from every item and returns how many items had it.
/// Deleting a tag that does not exist changes nothing and returns 0.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if a statement fails; nothing is changed on
/// error.
pub(super) fn delete_tag(conn: &mut Connection, tag: &str) -> Result<usize, IndexError> {
    let tx = conn.transaction()?;
    let Some(tag_id) = find_tag_id(&tx, tag.trim())?
    else {
        return Ok(0);
    };
    let items = items_with_tag_id(&tx, tag_id)?;
    tx.execute("DELETE FROM item_tags WHERE tag_id = ?1", params![tag_id])?;
    tx.execute("DELETE FROM tags WHERE id = ?1", params![tag_id])?;
    for item_id in &items {
        refresh_search_text(&tx, item_id)?;
    }
    tx.commit()?;
    Ok(items.len())
}

#[cfg(test)]
mod tests;
