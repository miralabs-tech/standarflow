//! Test-only helpers: an in-memory database with migrations applied, plus
//! minimal fixture inserts. Gated behind `#[cfg(test)]` — never shipped.

use rusqlite::{params, Connection};

/// A fresh in-memory database with the schema migrated and foreign keys on.
pub(crate) fn mem_db() -> Connection {
    let mut conn = Connection::open_in_memory().expect("open in-memory database");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("enable foreign keys");
    crate::db::migrate(&mut conn).expect("apply migrations");
    conn
}

/// Insert a root group, returning its id.
pub(crate) fn seed_group(conn: &Connection, slug: &str) -> i64 {
    conn.execute(
        "INSERT INTO groups (slug, created_at, updated_at, created_by)
         VALUES (?1, 0, 0, 'test')",
        [slug],
    )
    .expect("insert group");
    conn.last_insert_rowid()
}

/// Insert a session in `group_id`, returning its id.
pub(crate) fn seed_session(conn: &Connection, group_id: i64, slug: &str) -> i64 {
    conn.execute(
        "INSERT INTO sessions (group_id, slug, created_at, updated_at, created_by)
         VALUES (?1, ?2, 0, 0, 'test')",
        params![group_id, slug],
    )
    .expect("insert session");
    conn.last_insert_rowid()
}

/// Insert a conversation, returning its id.
pub(crate) fn seed_conversation(conn: &Connection, provider_conversation_id: &str) -> i64 {
    conn.execute(
        "INSERT INTO conversations
           (provider, provider_conversation_id, first_seen_at, last_seen_at)
         VALUES ('claude-code', ?1, 0, 0)",
        [provider_conversation_id],
    )
    .expect("insert conversation");
    conn.last_insert_rowid()
}
