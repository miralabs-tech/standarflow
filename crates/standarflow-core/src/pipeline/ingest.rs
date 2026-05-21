use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::pipeline::event::NormalizedEvent;
use crate::pipeline::providers::adapter_for;

pub const EVENTS_FILE: &str = "events.jsonl";
pub const UNROUTED_FILE: &str = "events-unrouted.jsonl";

/// Per-workspace event log: `<workspace>/.standarflow/events.jsonl`.
#[must_use] 
pub fn events_jsonl_path(workspace: &Path) -> PathBuf {
    workspace.join(crate::util::STANDARFLOW_DIR).join(EVENTS_FILE)
}

/// Global dead-letter log for events whose workspace can't be resolved:
/// `<home>/.standarflow/events-unrouted.jsonl`.
#[must_use] 
pub fn unrouted_jsonl_path() -> Option<PathBuf> {
    crate::util::home_dir().map(|h| h.join(crate::util::STANDARFLOW_DIR).join(UNROUTED_FILE))
}

/// Where `ingest` put a normalized event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routing {
    /// Appended to the workspace's `events.jsonl`.
    Workspace,
    /// The workspace has no `standarflow.db` — nothing was written.
    Skipped,
    /// The workspace could not be resolved — appended to the global
    /// dead-letter log.
    Unrouted,
}

#[derive(Debug, Clone)]
pub struct IngestOutcome {
    pub event: NormalizedEvent,
    /// `None` when the event was skipped — no file was created.
    pub log_path: Option<PathBuf>,
    pub routing: Routing,
}

/// Normalize a raw provider payload and append it as one JSON line to the
/// per-workspace event log `<workspace>/.standarflow/events.jsonl`. This is
/// the hot path run on every hook firing: it never opens the database — it
/// only stats for one.
///
/// A workspace with no `standarflow.db` is skipped entirely: nothing is
/// written and no directory is created, so a machine-wide hook does not
/// scatter orphan `.standarflow/` dirs into every directory it fires in.
/// Events whose workspace can't be resolved at all go to the global
/// dead-letter log instead.
///
/// `conversation_pid` is the agent root PID (`claude.exe`, …) the caller
/// resolved by walking its own process tree — the live correlation key.
pub fn ingest(
    provider: &str,
    raw: &serde_json::Value,
    conversation_pid: Option<i64>,
) -> Result<IngestOutcome> {
    let adapter = adapter_for(provider)
        .ok_or_else(|| Error::Invalid(format!("unknown provider: {provider}")))?;
    let mut event = adapter.normalize(raw)?;
    if conversation_pid.is_some() {
        event.conversation_pid = conversation_pid;
    }

    let (log_path, routing) = match event.workspace_path.as_deref() {
        Some(ws) if crate::db::default_path(Path::new(ws)).exists() => {
            (events_jsonl_path(Path::new(ws)), Routing::Workspace)
        }
        // A hook fired in a directory never initialised for standarflow.
        // Capturing there would leave an orphan `events.jsonl` no tail
        // ever drains — skip it, touch nothing.
        Some(_) => {
            return Ok(IngestOutcome {
                event,
                log_path: None,
                routing: Routing::Skipped,
            });
        }
        None => (
            unrouted_jsonl_path()
                .ok_or_else(|| Error::Invalid("cannot resolve home directory".into()))?,
            Routing::Unrouted,
        ),
    };

    append_line(&log_path, &event)?;
    Ok(IngestOutcome {
        event,
        log_path: Some(log_path),
        routing,
    })
}

fn append_line(path: &Path, event: &NormalizedEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    Ok(())
}
