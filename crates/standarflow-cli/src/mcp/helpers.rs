use std::collections::{HashMap, HashSet};

use anyhow::anyhow;
use rmcp::model::{CallToolResult, Content, ErrorData as McpError};
use rmcp::service::RequestContext;
use rmcp::RoleServer;
use serde::Serialize;
use standarflow_core::{store::{conversation, group, participant}, Connection};

use crate::common::strip_unc;
use crate::proctree;

/// The conversation the MCP server serves, resolved by walking the process
/// tree to the agent root PID. `None` outside a known agent.
pub(crate) fn current_conversation(conn: &Connection) -> Option<conversation::Conversation> {
    let pid = proctree::agent_root_pid()?;
    conversation::find_by_agent_pid(conn, i64::from(pid))
        .ok()
        .flatten()
}

pub(crate) fn require_conversation(conn: &Connection) -> anyhow::Result<conversation::Conversation> {
    current_conversation(conn).ok_or_else(|| {
        anyhow!(
            "no conversation resolved — the MCP server is not running under a \
             recognized agent, or no hook event has arrived yet"
        )
    })
}

/// Best-effort participant audit: record that the current conversation touched
/// a session. Silent when no conversation is resolvable.
pub(crate) fn touch_participant(conn: &Connection, session_id: i64) {
    if let Some(c) = current_conversation(conn) {
        let _ = participant::touch(conn, session_id, c.id);
    }
}

pub(crate) fn client_name(ctx: &RequestContext<RoleServer>) -> String {
    ctx.peer
        .peer_info().map_or_else(|| "unknown".to_string(), |info| info.client_info.name.clone())
}

#[allow(clippy::unnecessary_wraps)]
fn ok_text(s: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(s.into())]))
}

pub(crate) fn ok_json<T: Serialize>(v: &T) -> Result<CallToolResult, McpError> {
    let s = serde_json::to_string_pretty(v)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(s)]))
}

#[allow(clippy::unnecessary_wraps, clippy::needless_pass_by_value)]
fn fail(e: anyhow::Error) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
}

/// Run a fallible step and render its value as pretty JSON, or its error as a
/// tool-level failure. Collapses the per-tool result-handling boilerplate.
pub(crate) fn json_result<T: Serialize>(
    f: impl FnOnce() -> anyhow::Result<T>,
) -> Result<CallToolResult, McpError> {
    match f() {
        Ok(v) => ok_json(&v),
        Err(e) => fail(e),
    }
}

/// Run a fallible step and render its `String` as tool text, or its error as a
/// tool-level failure.
pub(crate) fn text_result(
    f: impl FnOnce() -> anyhow::Result<String>,
) -> Result<CallToolResult, McpError> {
    match f() {
        Ok(s) => ok_text(s),
        Err(e) => fail(e),
    }
}

pub(crate) fn group_path_for(
    conn: &Connection,
    group_id: i64,
    cache: &mut HashMap<i64, String>,
) -> anyhow::Result<String> {
    if let Some(p) = cache.get(&group_id) {
        return Ok(p.clone());
    }
    let mut parts: Vec<String> = Vec::new();
    let mut cur = Some(group_id);
    while let Some(id) = cur {
        let g = group::get(conn, id)?;
        parts.push(g.slug);
        cur = g.parent_id;
    }
    parts.reverse();
    let path = parts.join("/");
    cache.insert(group_id, path.clone());
    Ok(path)
}

pub(crate) fn current_bin_path() -> Option<String> {
    let p = std::env::current_exe().ok()?;
    Some(strip_unc(p.to_string_lossy().to_string()))
}

/// True when `pid` (a conversation's `last_conversation_pid`) is in the live
/// agent-process set — i.e. the chat is still open.
pub(crate) fn pid_is_live(pid: Option<i64>, live: &HashSet<u32>) -> bool {
    pid.and_then(|p| u32::try_from(p).ok())
        .is_some_and(|p| live.contains(&p))
}
