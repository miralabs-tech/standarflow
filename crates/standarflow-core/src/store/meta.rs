use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;

/// `schema_meta` key holding the byte offset the event-log tail has consumed.
pub const KEY_EVENTS_LOG_OFFSET: &str = "events_log_offset";

/// `schema_meta` key holding the workspace's current session id — the session
/// a new conversation inherits as its focus. Updated on every `focus::set`.
pub const KEY_CURRENT_SESSION_ID: &str = "current_session_id";

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM schema_meta WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO schema_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_i64(conn: &Connection, key: &str) -> Result<Option<i64>> {
    Ok(get(conn, key)?.and_then(|v| v.parse().ok()))
}

pub fn set_i64(conn: &Connection, key: &str, value: i64) -> Result<()> {
    set(conn, key, &value.to_string())
}
