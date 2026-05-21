use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;
use crate::pipeline::event::{self, EventKind, NormalizedEvent};
use crate::pipeline::ingest::events_jsonl_path;
use crate::store::conversation::{self, ConversationUpsert};
use crate::store::file_change::{self, NewFileChange};
use crate::store::focus;
use crate::store::meta;
use crate::store::participant;

#[derive(Debug, Clone)]
pub struct TailReport {
    pub events_ingested: usize,
    pub lines_skipped: usize,
    pub new_offset: u64,
}

/// Drain new lines from the workspace event log into the database. Resumes
/// from the byte offset stored in `schema_meta`; safe to call repeatedly.
/// A partial trailing line (the log being appended to concurrently) is left
/// untouched for the next call.
pub fn tail(conn: &Connection, workspace: &Path) -> Result<TailReport> {
    let path = events_jsonl_path(workspace);
    let stored = meta::get_i64(conn, meta::KEY_EVENTS_LOG_OFFSET)?.unwrap_or(0);
    let mut offset = u64::try_from(stored).unwrap_or(0);

    if !path.exists() {
        return Ok(TailReport {
            events_ingested: 0,
            lines_skipped: 0,
            new_offset: offset,
        });
    }

    let len = std::fs::metadata(&path)?.len();
    if len < offset {
        // Log truncated or rotated — restart from the top.
        offset = 0;
    }
    if len == offset {
        return Ok(TailReport {
            events_ingested: 0,
            lines_skipped: 0,
            new_offset: offset,
        });
    }

    let mut f = File::open(&path)?;
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;

    let mut ingested = 0usize;
    let mut skipped = 0usize;
    let mut consumed: u64 = 0;
    for line in buf.split_inclusive('\n') {
        if !line.ends_with('\n') {
            break; // partial trailing line — leave for the next tail
        }
        consumed += line.len() as u64;
        let trimmed = line.trim_start_matches('\u{FEFF}').trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<NormalizedEvent>(trimmed) {
            Ok(ev) => {
                ingest_event(conn, &ev)?;
                ingested += 1;
            }
            Err(_) => skipped += 1,
        }
    }

    let new_offset = offset + consumed;
    meta::set_i64(
        conn,
        meta::KEY_EVENTS_LOG_OFFSET,
        i64::try_from(new_offset).unwrap_or(i64::MAX),
    )?;
    Ok(TailReport {
        events_ingested: ingested,
        lines_skipped: skipped,
        new_offset,
    })
}

fn ingest_event(conn: &Connection, ev: &NormalizedEvent) -> Result<()> {
    let conv_id = conversation::upsert(
        conn,
        &ConversationUpsert {
            provider: &ev.provider,
            provider_conversation_id: &ev.provider_conversation_id,
            client_label: ev.client_label.as_deref(),
            workspace_path: ev.workspace_path.as_deref(),
            transcript_path: ev.transcript_path.as_deref(),
            last_pid: None,
            last_conversation_pid: ev.conversation_pid,
        },
    )?;
    event::insert(conn, Some(conv_id), ev)?;

    match &ev.kind {
        EventKind::SessionEnd => {
            conversation::mark_ended(conn, conv_id)?;
        }
        EventKind::ToolPost { tool, file_path } => {
            // File work while the conversation has a focused session is
            // attributed to that session — and proves it worked there.
            if let Some(f) = focus::get(conn, conv_id)? {
                let mut logged = false;
                // Read & friends carry a `file_path` too but never mutate —
                // only genuine write tools yield a create/edit row.
                let op = match tool.as_str() {
                    "Write" => Some("create"),
                    "Edit" | "MultiEdit" => Some("edit"),
                    _ => None,
                };
                if let (Some(path), Some(op)) = (file_path, op) {
                    file_change::log(
                        conn,
                        &NewFileChange {
                            session_id: f.session_id,
                            conversation_id: conv_id,
                            file_path: path.as_str(),
                            op,
                            kind: Some(file_change::classify_kind(path)),
                            tool_name: Some(tool.as_str()),
                            ts: ev.ts,
                        },
                    )?;
                    logged = true;
                }
                // Claude Code has no Delete tool — deletions always go through
                // a shell. After a shell command, reconcile the session's
                // tracked paths against the filesystem.
                if matches!(tool.as_str(), "Bash" | "PowerShell") {
                    logged |= reconcile_deletes(conn, f.session_id, conv_id, ev.ts)? > 0;
                }
                if logged {
                    participant::touch(conn, f.session_id, conv_id)?;
                }
            }
        }
        EventKind::Stop => {
            // Safety net for a deletion made just before the turn ended,
            // with no later shell command to trigger reconciliation.
            if let Some(f) = focus::get(conn, conv_id)? {
                if reconcile_deletes(conn, f.session_id, conv_id, ev.ts)? > 0 {
                    participant::touch(conn, f.session_id, conv_id)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Reconcile deletions for a focused session: log a `delete` change for every
/// tracked path that has since vanished from disk. Claude Code exposes no
/// Delete tool, so there is no hook payload to read — the filesystem is the
/// source of truth. Returns how many deletions were recorded.
fn reconcile_deletes(
    conn: &Connection,
    session_id: i64,
    conv_id: i64,
    ts: i64,
) -> Result<usize> {
    let mut deleted = 0;
    for path in file_change::live_paths_for_session(conn, session_id)? {
        if Path::new(&path).exists() {
            continue;
        }
        file_change::log(
            conn,
            &NewFileChange {
                session_id,
                conversation_id: conv_id,
                file_path: path.as_str(),
                op: "delete",
                kind: Some(file_change::classify_kind(&path)),
                tool_name: None,
                ts,
            },
        )?;
        deleted += 1;
    }
    Ok(deleted)
}
