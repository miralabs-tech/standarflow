use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorData as McpError};
use rmcp::{tool, tool_router};
use standarflow_core::store::{conversation, focus, meta, session};

use super::helpers::{
    group_path_for, json_result, pid_is_live, require_conversation, text_result,
    touch_participant,
};
use super::out::{ConversationOut, FocusEntryOut, SessionFocusedOut};
use super::req::{
    ConversationGetReq, ConversationListReq, ConversationSetLabelReq, SessionFocusReq,
    SessionFocusedReq, SessionUnfocusReq,
};
use super::StandarflowMcp;
use crate::common::{resolve_group, resolve_session};
use crate::proctree;

#[tool_router(router = focus_router, vis = "pub(crate)")]
impl StandarflowMcp {
    #[tool(description = "Pin a session as the focused session for THIS conversation. Focus is keyed on the conversation's stable id (resolved via the agent process tree), so it survives MCP server restarts. Subsequent hook-driven file changes are attributed to this session.")]
    async fn session_focus(
        &self,
        Parameters(req): Parameters<SessionFocusReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            let conv = match req.conversation_id {
                Some(id) => conversation::get(&conn, id)?,
                None => require_conversation(&conn)?,
            };
            let group_id = resolve_group(&conn, &req.group_path)?;
            let session_id = resolve_session(&conn, group_id, &req.slug)?;
            focus::set(&conn, conv.id, session_id)?;
            touch_participant(&conn, session_id);
            Ok(format!(
                "focused conversation#{} → {}/{} (session#{session_id})",
                conv.id, req.group_path, req.slug
            ))
        })
    }

    #[tool(description = "Clear the focused session for a conversation. Defaults to the conversation this MCP server is bound to; pass conversation_id to clear another conversation's focus.")]
    async fn session_unfocus(
        &self,
        Parameters(req): Parameters<SessionUnfocusReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            let conv = match req.conversation_id {
                Some(id) => conversation::get(&conn, id)?,
                None => require_conversation(&conn)?,
            };
            focus::clear(&conn, conv.id)?;
            Ok(format!("cleared focus for conversation#{}", conv.id))
        })
    }

    #[tool(description = "Return the focused session for this conversation (or an explicit conversation_id). With a focus row, confirmed=true. With none, falls back to the workspace's current session as an unconfirmed suggestion (confirmed=false) the conversation can adopt via focus_adopt. null only when there is nothing to suggest.")]
    async fn session_focused(
        &self,
        Parameters(req): Parameters<SessionFocusedReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Option<SessionFocusedOut>> {
            let conv = match req.conversation_id {
                Some(id) => conversation::get(&conn, id)?,
                None => require_conversation(&conn)?,
            };
            let mut cache: HashMap<i64, String> = HashMap::new();
            if let Some(f) = focus::get(&conn, conv.id)? {
                let s = session::get(&conn, f.session_id)?;
                let group_path = group_path_for(&conn, s.group_id, &mut cache)?;
                return Ok(Some(SessionFocusedOut {
                    conversation_id: conv.id,
                    provider: conv.provider,
                    provider_conversation_id: conv.provider_conversation_id,
                    group_path,
                    session_id: s.id,
                    session_slug: s.slug,
                    session_kind: s.kind,
                    session_status: s.status,
                    focused_at: f.focused_at,
                    pending_session_id: f.pending_session_id,
                    confirmed: true,
                }));
            }
            // No focus row — surface the workspace current session as an
            // unconfirmed suggestion the bootstrap can adopt via focus_adopt.
            let Some(sid) = meta::get_i64(&conn, meta::KEY_CURRENT_SESSION_ID)? else {
                return Ok(None);
            };
            let Ok(s) = session::get(&conn, sid) else {
                return Ok(None);
            };
            let group_path = group_path_for(&conn, s.group_id, &mut cache)?;
            Ok(Some(SessionFocusedOut {
                conversation_id: conv.id,
                provider: conv.provider,
                provider_conversation_id: conv.provider_conversation_id,
                group_path,
                session_id: s.id,
                session_slug: s.slug,
                session_kind: s.kind,
                session_status: s.status,
                focused_at: 0,
                pending_session_id: Some(sid),
                confirmed: false,
            }))
        })
    }

    #[tool(description = "Adopt the workspace's current session as THIS conversation's focus, when it has none yet. Idempotent: a conversation that already has a focus keeps it. Meant for the new-chat bootstrap so focus carries over without naming a session.")]
    async fn focus_adopt(&self) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            let conv = require_conversation(&conn)?;
            if let Some(f) = focus::get(&conn, conv.id)? {
                let s = session::get(&conn, f.session_id)?;
                return Ok(format!(
                    "conversation#{} already focused → session#{} {}",
                    conv.id, s.id, s.slug
                ));
            }
            let Some(sid) = meta::get_i64(&conn, meta::KEY_CURRENT_SESSION_ID)? else {
                return Ok(
                    "no workspace current session to adopt — focus one explicitly".into(),
                );
            };
            let Ok(s) = session::get(&conn, sid) else {
                return Ok(
                    "workspace current session no longer exists — focus one explicitly".into(),
                );
            };
            if s.status != "active" {
                return Ok(format!(
                    "workspace current session#{} {} is '{}', not active — not adopted",
                    s.id, s.slug, s.status
                ));
            }
            focus::set(&conn, conv.id, s.id)?;
            touch_participant(&conn, s.id);
            Ok(format!(
                "conversation#{} adopted → session#{} {}",
                conv.id, s.id, s.slug
            ))
        })
    }

    #[tool(description = "List every conversation that has a focused session — one entry per conversation. Lets a client that is not itself a conversation (e.g. an editor extension) see the whole focus map.")]
    async fn focus_list(&self) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Vec<FocusEntryOut>> {
            let mut cache: HashMap<i64, String> = HashMap::new();
            let live = proctree::live_agent_pids();
            let mut out = Vec::new();
            for f in focus::list_all(&conn)? {
                let conv = conversation::get(&conn, f.conversation_id)?;
                let s = session::get(&conn, f.session_id)?;
                let group_path = group_path_for(&conn, s.group_id, &mut cache)?;
                let is_live = pid_is_live(conv.last_conversation_pid, &live);
                out.push(FocusEntryOut {
                    conversation_id: conv.id,
                    provider: conv.provider,
                    provider_conversation_id: conv.provider_conversation_id,
                    client_label: conv.client_label,
                    workspace_path: conv.workspace_path,
                    last_seen_at: conv.last_seen_at,
                    ended_at: conv.ended_at,
                    group_path,
                    session_id: s.id,
                    session_slug: s.slug,
                    session_kind: s.kind,
                    session_status: s.status,
                    focused_at: f.focused_at,
                    is_live,
                });
            }
            Ok(out)
        })
    }

    #[tool(description = "Set or clear the human-friendly label of a conversation. Pass label=null (or omit it) to clear and fall back to the derived label.")]
    async fn conversation_set_label(
        &self,
        Parameters(req): Parameters<ConversationSetLabelReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<conversation::Conversation> {
            conversation::set_label(&conn, req.conversation_id, req.label.as_deref())?;
            Ok(conversation::get(&conn, req.conversation_id)?)
        })
    }

    #[tool(description = "Get a conversation by id, or the conversation this MCP server is bound to when id is omitted.")]
    async fn conversation_get(
        &self,
        Parameters(req): Parameters<ConversationGetReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<conversation::Conversation> {
            match req.conversation_id {
                Some(id) => Ok(conversation::get(&conn, id)?),
                None => require_conversation(&conn),
            }
        })
    }

    #[tool(description = "List conversations (chats) known to this workspace, newest activity first. Each carries is_live (its agent process is still running). Optional active_since unix timestamp filters to recent ones.")]
    async fn conversation_list(
        &self,
        Parameters(req): Parameters<ConversationListReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Vec<ConversationOut>> {
            let live = proctree::live_agent_pids();
            Ok(conversation::list(&conn, req.active_since)?
                .into_iter()
                .map(|c| ConversationOut {
                    is_live: pid_is_live(c.last_conversation_pid, &live),
                    conversation: c,
                })
                .collect())
        })
    }
}
