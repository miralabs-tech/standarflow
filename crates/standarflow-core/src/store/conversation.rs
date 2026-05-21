use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::util::now_unix;

/// An AI chat, identified by the provider's stable conversation id. Survives
/// process restarts; the PID columns are diagnostic only.
#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: i64,
    pub provider: String,
    pub provider_conversation_id: String,
    pub client_label: Option<String>,
    pub workspace_path: Option<String>,
    pub transcript_path: Option<String>,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub ended_at: Option<i64>,
    pub last_pid: Option<i64>,
    pub last_conversation_pid: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationUpsert<'a> {
    pub provider: &'a str,
    pub provider_conversation_id: &'a str,
    pub client_label: Option<&'a str>,
    pub workspace_path: Option<&'a str>,
    pub transcript_path: Option<&'a str>,
    pub last_pid: Option<i64>,
    pub last_conversation_pid: Option<i64>,
}

const SELECT_COLS: &str = "id, provider, provider_conversation_id, client_label, \
                           workspace_path, transcript_path, first_seen_at, last_seen_at, \
                           ended_at, last_pid, last_conversation_pid";

/// Insert a conversation, or refresh `last_seen_at` and the optional metadata
/// of an existing one. Metadata fields only overwrite when the new value is
/// `Some` — a later event missing `workspace_path` won't wipe a known one.
/// A fresh event also clears `ended_at`: a conversation producing events is
/// live again (the `SessionEnd` handler re-sets it afterwards via `mark_ended`).
pub fn upsert(conn: &Connection, up: &ConversationUpsert<'_>) -> Result<i64> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO conversations
           (provider, provider_conversation_id, client_label, workspace_path,
            transcript_path, first_seen_at, last_seen_at, last_pid, last_conversation_pid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8)
         ON CONFLICT(provider, provider_conversation_id) DO UPDATE SET
            last_seen_at          = excluded.last_seen_at,
            ended_at              = NULL,
            client_label          = COALESCE(excluded.client_label, conversations.client_label),
            workspace_path        = COALESCE(excluded.workspace_path, conversations.workspace_path),
            transcript_path       = COALESCE(excluded.transcript_path, conversations.transcript_path),
            last_pid              = COALESCE(excluded.last_pid, conversations.last_pid),
            last_conversation_pid = COALESCE(excluded.last_conversation_pid, conversations.last_conversation_pid)",
        params![
            up.provider,
            up.provider_conversation_id,
            up.client_label,
            up.workspace_path,
            up.transcript_path,
            now,
            up.last_pid,
            up.last_conversation_pid
        ],
    )?;
    let id = conn.query_row(
        "SELECT id FROM conversations WHERE provider = ?1 AND provider_conversation_id = ?2",
        params![up.provider, up.provider_conversation_id],
        |r| r.get::<_, i64>(0),
    )?;
    Ok(id)
}

pub fn get(conn: &Connection, id: i64) -> Result<Conversation> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM conversations WHERE id = ?1"),
        params![id],
        map_row,
    )
    .map_err(Error::from_lookup)
}

pub fn find(
    conn: &Connection,
    provider: &str,
    provider_conversation_id: &str,
) -> Result<Option<Conversation>> {
    conn.query_row(
        &format!(
            "SELECT {SELECT_COLS} FROM conversations
             WHERE provider = ?1 AND provider_conversation_id = ?2"
        ),
        params![provider, provider_conversation_id],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

/// Find the conversation most recently associated with an agent root PID.
/// This is the live-correlation lookup: a process walks its tree to the agent
/// root PID, then resolves which conversation that PID currently serves.
pub fn find_by_agent_pid(conn: &Connection, pid: i64) -> Result<Option<Conversation>> {
    conn.query_row(
        &format!(
            "SELECT {SELECT_COLS} FROM conversations
             WHERE last_conversation_pid = ?1
             ORDER BY last_seen_at DESC LIMIT 1"
        ),
        params![pid],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

/// List conversations, newest activity first. When `active_since` is set, only
/// conversations seen at or after that unix timestamp are returned.
pub fn list(conn: &Connection, active_since: Option<i64>) -> Result<Vec<Conversation>> {
    let rows = match active_since {
        Some(since) => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM conversations
                 WHERE last_seen_at >= ?1 ORDER BY last_seen_at DESC"
            ))?
            .query_map(params![since], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        None => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM conversations ORDER BY last_seen_at DESC"
            ))?
            .query_map([], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

pub fn mark_ended(conn: &Connection, id: i64) -> Result<()> {
    let now = now_unix();
    let n = conn.execute(
        "UPDATE conversations SET ended_at = ?1, last_seen_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

/// Set (or clear, with `None`) the human-friendly label of a conversation.
pub fn set_label(conn: &Connection, id: i64, label: Option<&str>) -> Result<()> {
    let n = conn.execute(
        "UPDATE conversations SET client_label = ?1 WHERE id = ?2",
        params![label, id],
    )?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        provider: row.get(1)?,
        provider_conversation_id: row.get(2)?,
        client_label: row.get(3)?,
        workspace_path: row.get(4)?,
        transcript_path: row.get(5)?,
        first_seen_at: row.get(6)?,
        last_seen_at: row.get(7)?,
        ended_at: row.get(8)?,
        last_pid: row.get(9)?,
        last_conversation_pid: row.get(10)?,
    })
}
