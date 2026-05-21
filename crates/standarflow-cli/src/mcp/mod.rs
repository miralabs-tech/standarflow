mod helpers;
mod out;
mod req;
mod tools_debug;
mod tools_file;
mod tools_focus;
mod tools_group;
mod tools_session;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rmcp::transport::stdio;
use rmcp::{tool_handler, ServerHandler, ServiceExt};
use standarflow_core::{pipeline::tail, store::focus, Connection};

#[derive(Clone)]
pub struct StandarflowMcp {
    conn: Arc<Mutex<Connection>>,
    db_path: String,
    workspace: PathBuf,
}

impl StandarflowMcp {
    pub fn new(conn: Connection, db_path: String) -> Self {
        let workspace = Path::new(&db_path)
            .parent()
            .and_then(Path::parent)
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
            workspace,
        }
    }

    /// Lock the connection and drain any new events from the workspace log.
    /// Every tool call goes through this so conversation state stays fresh.
    fn locked(&self) -> std::sync::MutexGuard<'_, Connection> {
        let conn = self.conn.lock().unwrap();
        let _ = tail::tail(&conn, &self.workspace);
        conn
    }
}

pub async fn run(conn: Connection, db_path: String) -> anyhow::Result<()> {
    let svc = StandarflowMcp::new(conn, db_path);
    // Startup catch-up: drain events that arrived while the server was down,
    // then seed the current-session pointer if an older DB never wrote it.
    {
        let conn = svc.conn.lock().unwrap();
        let _ = tail::tail(&conn, &svc.workspace);
        let _ = focus::backfill_current_session(&conn);
    }
    let service = svc.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[tool_handler(
    router = (Self::group_router()
        + Self::session_router()
        + Self::file_router()
        + Self::focus_router()
        + Self::debug_router()),
    instructions = "Standarflow session, artefact, link and file-reference store. Groups namespace sessions per agent/topic. Sessions are temporal containers; artefacts (ADR, note, memory…) live inside a session via parent_session_id. Conversations are the agent chats themselves, identified by a stable provider id and resolved via the process tree; focus is keyed per conversation and survives restarts. Links express typed relations between sessions."
)]
impl ServerHandler for StandarflowMcp {}
