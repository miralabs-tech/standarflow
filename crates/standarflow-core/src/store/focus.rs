use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::store::meta;
use crate::util::now_unix;

/// The focused session for a conversation. `pending_session_id` holds a focus
/// request not yet confirmed by the conversation.
#[derive(Debug, Clone)]
pub struct Focus {
    pub conversation_id: i64,
    pub session_id: i64,
    pub pending_session_id: Option<i64>,
    pub focused_at: i64,
    pub last_touched_at: i64,
}

/// Set (or replace) the focused session for a conversation. Clears any pending
/// request, and updates the workspace's current-session pointer so a new
/// conversation can inherit it.
pub fn set(conn: &Connection, conversation_id: i64, session_id: i64) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO session_focus
           (conversation_id, session_id, pending_session_id, focused_at, last_touched_at)
         VALUES (?1, ?2, NULL, ?3, ?3)
         ON CONFLICT(conversation_id) DO UPDATE SET
            session_id         = excluded.session_id,
            pending_session_id = NULL,
            focused_at         = excluded.focused_at,
            last_touched_at    = excluded.last_touched_at",
        params![conversation_id, session_id, now],
    )?;
    meta::set_i64(conn, meta::KEY_CURRENT_SESSION_ID, session_id)?;
    Ok(())
}

/// Record a pending focus request for a conversation without an existing row.
/// When the conversation already has a focus, this only updates the pending
/// pointer.
pub fn set_pending(
    conn: &Connection,
    conversation_id: i64,
    pending_session_id: i64,
) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO session_focus
           (conversation_id, session_id, pending_session_id, focused_at, last_touched_at)
         VALUES (?1, ?2, ?2, ?3, ?3)
         ON CONFLICT(conversation_id) DO UPDATE SET
            pending_session_id = ?2,
            last_touched_at    = ?3",
        params![conversation_id, pending_session_id, now],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, conversation_id: i64) -> Result<Option<Focus>> {
    conn.query_row(
        "SELECT conversation_id, session_id, pending_session_id, focused_at, last_touched_at
         FROM session_focus WHERE conversation_id = ?1",
        params![conversation_id],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

pub fn required(conn: &Connection, conversation_id: i64) -> Result<Focus> {
    get(conn, conversation_id)?.ok_or(Error::NotFound)
}

pub fn clear(conn: &Connection, conversation_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM session_focus WHERE conversation_id = ?1",
        params![conversation_id],
    )?;
    Ok(())
}

/// Bump `last_touched_at` for a conversation's focus row, if it has one.
pub fn touch(conn: &Connection, conversation_id: i64) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE session_focus SET last_touched_at = ?1 WHERE conversation_id = ?2",
        params![now, conversation_id],
    )?;
    Ok(())
}

/// Every focus row, most recently touched first.
pub fn list_all(conn: &Connection) -> Result<Vec<Focus>> {
    let rows = conn
        .prepare(
            "SELECT conversation_id, session_id, pending_session_id, focused_at, last_touched_at
             FROM session_focus ORDER BY last_touched_at DESC",
        )?
        .query_map([], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Cold-start back-fill for the workspace current-session pointer. A DB that
/// predates the pointer — or one with focus rows but no `set` since — has the
/// `KEY_CURRENT_SESSION_ID` meta key empty. Seed it from the most recently
/// touched focus row. Idempotent: a no-op once the pointer exists. Returns the
/// session id it back-filled, if any.
pub fn backfill_current_session(conn: &Connection) -> Result<Option<i64>> {
    if meta::get_i64(conn, meta::KEY_CURRENT_SESSION_ID)?.is_some() {
        return Ok(None);
    }
    let latest: Option<i64> = conn
        .query_row(
            "SELECT session_id FROM session_focus
             ORDER BY last_touched_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(session_id) = latest {
        meta::set_i64(conn, meta::KEY_CURRENT_SESSION_ID, session_id)?;
    }
    Ok(latest)
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Focus> {
    Ok(Focus {
        conversation_id: row.get(0)?,
        session_id: row.get(1)?,
        pending_session_id: row.get(2)?,
        focused_at: row.get(3)?,
        last_touched_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{mem_db, seed_conversation, seed_group, seed_session};

    fn seed_focus(conn: &Connection, conv_id: i64, session_id: i64, touched_at: i64) {
        conn.execute(
            "INSERT INTO session_focus
               (conversation_id, session_id, focused_at, last_touched_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![conv_id, session_id, touched_at],
        )
        .expect("seed focus row");
    }

    #[test]
    fn backfill_seeds_the_pointer_from_the_latest_focus_row() {
        let conn = mem_db();
        let g = seed_group(&conn, "g");
        let s_old = seed_session(&conn, g, "old");
        let s_new = seed_session(&conn, g, "new");
        let c1 = seed_conversation(&conn, "conv-1");
        let c2 = seed_conversation(&conn, "conv-2");

        seed_focus(&conn, c1, s_old, 10);
        seed_focus(&conn, c2, s_new, 20);

        assert_eq!(meta::get_i64(&conn, meta::KEY_CURRENT_SESSION_ID).unwrap(), None);
        assert_eq!(backfill_current_session(&conn).expect("backfill"), Some(s_new));
        assert_eq!(
            meta::get_i64(&conn, meta::KEY_CURRENT_SESSION_ID).unwrap(),
            Some(s_new)
        );
    }

    #[test]
    fn backfill_is_a_noop_when_the_pointer_already_exists() {
        let conn = mem_db();
        let g = seed_group(&conn, "g");
        let s1 = seed_session(&conn, g, "s1");
        let s2 = seed_session(&conn, g, "s2");
        let c = seed_conversation(&conn, "conv-1");

        meta::set_i64(&conn, meta::KEY_CURRENT_SESSION_ID, s1).unwrap();
        seed_focus(&conn, c, s2, 99);

        assert_eq!(backfill_current_session(&conn).expect("backfill"), None);
        assert_eq!(
            meta::get_i64(&conn, meta::KEY_CURRENT_SESSION_ID).unwrap(),
            Some(s1)
        );
    }

    #[test]
    fn backfill_is_a_noop_with_no_focus_rows() {
        let conn = mem_db();

        assert_eq!(backfill_current_session(&conn).expect("backfill"), None);
        assert_eq!(meta::get_i64(&conn, meta::KEY_CURRENT_SESSION_ID).unwrap(), None);
    }
}
