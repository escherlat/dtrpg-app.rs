//! Application-defined SQL functions registered on every index connection.

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;

use super::IndexError;

/// Registers `ulower(text)` — SQLite's built-in `lower()`/`LIKE`/`NOCASE`
/// fold ASCII only, while `SearchQuery`'s matcher folds with Rust's
/// `str::to_lowercase()`. Queries that must match exactly what the Rust
/// matcher matched therefore compare against this function's output, so both
/// sides share one Unicode fold.
///
/// Must be called on every connection the index opens, before any query
/// referencing `ulower` is prepared.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if registration fails.
pub(super) fn register_functions(conn: &Connection) -> Result<(), IndexError> {
    // Deterministic: same text in, same lower-case text out — `ulower` is
    // Rust's `to_lowercase`, character-for-character.
    conn.create_scalar_function("ulower",
                                1,
                                FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
                                |ctx| {
                                    // NULL-in/NULL-out per SQLite convention;
                                    // today every `ulower` call site reads a
                                    // NOT NULL column, but a future nullable
                                    // one must not fail the whole statement.
                                    let text: Option<String> = ctx.get(0)?;
                                    Ok(text.map(|t| t.to_lowercase()))
                                })?;
    Ok(())
}
