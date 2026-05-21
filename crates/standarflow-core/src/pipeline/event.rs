use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// The canonical, provider-agnostic event shape. Every provider adapter
/// produces this; the ingest pipeline and the overlay only ever see this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEvent {
    /// Provider identifier, e.g. `claude-code`.
    pub provider: String,
    /// The provider's stable conversation id (Claude session UUID, …).
    pub provider_conversation_id: String,
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_label: Option<String>,
    /// PID of the agent root process (e.g. `claude.exe`) the hook ran under.
    /// Volatile — used only as a live correlation key, never as identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_pid: Option<i64>,
    /// Unix seconds.
    pub ts: i64,
    /// The untouched provider payload, kept for audit and replay.
    pub raw: serde_json::Value,
}

/// Lifecycle event, normalized across providers. `tag = "kind"` so the JSON
/// shape is flat: `{"kind": "tool_post", "tool": "Edit", "file_path": "…"}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    SessionStart,
    UserPrompt,
    ToolPre {
        tool: String,
    },
    ToolPost {
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_path: Option<String>,
    },
    Stop,
    SessionEnd,
    Other {
        name: String,
    },
}

impl EventKind {
    /// Stable string stored in the `events.event_kind` column.
    #[must_use] 
    pub fn discriminant(&self) -> &'static str {
        match self {
            EventKind::SessionStart => "session_start",
            EventKind::UserPrompt => "user_prompt",
            EventKind::ToolPre { .. } => "tool_pre",
            EventKind::ToolPost { .. } => "tool_post",
            EventKind::Stop => "stop",
            EventKind::SessionEnd => "session_end",
            EventKind::Other { .. } => "other",
        }
    }
}

/// A persisted event row from the `events` table.
#[derive(Debug, Clone)]
pub struct EventRow {
    pub id: i64,
    pub conversation_id: Option<i64>,
    pub provider: String,
    pub event_kind: String,
    pub ts: i64,
    pub payload_json: String,
}

impl EventRow {
    /// Deserialize the stored payload back into a [`NormalizedEvent`].
    pub fn normalized(&self) -> Result<NormalizedEvent> {
        Ok(serde_json::from_str(&self.payload_json)?)
    }
}

/// Insert a normalized event. `conversation_id` is the DB row id of the
/// resolved conversation, or `None` when it could not be resolved.
pub fn insert(
    conn: &Connection,
    conversation_id: Option<i64>,
    event: &NormalizedEvent,
) -> Result<i64> {
    let payload = serde_json::to_string(event)?;
    conn.execute(
        "INSERT INTO events (conversation_id, provider, event_kind, ts, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            conversation_id,
            event.provider,
            event.kind.discriminant(),
            event.ts,
            payload
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_for_conversation(
    conn: &Connection,
    conversation_id: i64,
    limit: i64,
) -> Result<Vec<EventRow>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM events
             WHERE conversation_id = ?1 ORDER BY ts DESC, id DESC LIMIT ?2"
        ))?
        .query_map(params![conversation_id, limit], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_recent(conn: &Connection, limit: i64) -> Result<Vec<EventRow>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM events ORDER BY ts DESC, id DESC LIMIT ?1"
        ))?
        .query_map(params![limit], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every event row, oldest first. Used by the export pipeline.
pub fn list_all(conn: &Connection) -> Result<Vec<EventRow>> {
    let rows = conn
        .prepare(&format!("SELECT {SELECT_COLS} FROM events ORDER BY ts, id"))?
        .query_map([], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

const SELECT_COLS: &str = "id, conversation_id, provider, event_kind, ts, payload_json";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        provider: row.get(2)?,
        event_kind: row.get(3)?,
        ts: row.get(4)?,
        payload_json: row.get(5)?,
    })
}
