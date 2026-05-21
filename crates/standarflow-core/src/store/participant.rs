use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;
use crate::util::now_unix;

/// Audit row: a conversation that has worked inside a session.
#[derive(Debug, Clone, Serialize)]
pub struct Participant {
    pub id: i64,
    pub session_id: i64,
    pub conversation_id: i64,
    pub first_touch_at: i64,
    pub last_touch_at: i64,
    pub touch_count: i64,
}

/// Record that a conversation touched a session: inserts a new participant row
/// or bumps `last_touch_at` and `touch_count` on the existing one.
pub fn touch(conn: &Connection, session_id: i64, conversation_id: i64) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO session_participants
           (session_id, conversation_id, first_touch_at, last_touch_at, touch_count)
         VALUES (?1, ?2, ?3, ?3, 1)
         ON CONFLICT(session_id, conversation_id) DO UPDATE SET
            last_touch_at = ?3,
            touch_count   = touch_count + 1",
        params![session_id, conversation_id, now],
    )?;
    Ok(())
}

pub fn list_for_session(conn: &Connection, session_id: i64) -> Result<Vec<Participant>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM session_participants
             WHERE session_id = ?1 ORDER BY last_touch_at DESC"
        ))?
        .query_map(params![session_id], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_for_conversation(
    conn: &Connection,
    conversation_id: i64,
) -> Result<Vec<Participant>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM session_participants
             WHERE conversation_id = ?1 ORDER BY last_touch_at DESC"
        ))?
        .query_map(params![conversation_id], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

const SELECT_COLS: &str =
    "id, session_id, conversation_id, first_touch_at, last_touch_at, touch_count";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Participant> {
    Ok(Participant {
        id: row.get(0)?,
        session_id: row.get(1)?,
        conversation_id: row.get(2)?,
        first_touch_at: row.get(3)?,
        last_touch_at: row.get(4)?,
        touch_count: row.get(5)?,
    })
}
