use std::collections::HashMap;

use anyhow::anyhow;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorData as McpError};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_router, RoleServer};
use standarflow_core::store::{file_change, link, participant, session};

use super::helpers::{client_name, json_result, text_result, touch_participant};
use super::out::{resolve_peer, LinkOfOut, LinkOut, SessionLite, SessionOut};
use super::req::{
    LinkAddReq, LinkOfReq, LinkRemoveReq, SessionChildrenReq, SessionDeleteReq,
    SessionFileChangesReq, SessionGetReq, SessionListReq, SessionParticipantsReq, SessionSaveReq,
    SessionUpdateReq,
};
use super::StandarflowMcp;
use crate::common::{resolve_group, resolve_session};

#[tool_router(router = session_router, vis = "pub(crate)")]
impl StandarflowMcp {
    #[tool(
        description = "Save a session (kind defaults to 'session'). Use continues_slug to chain after a previous session (auto-supersedes it). created_by is the MCP client name."
    )]
    async fn session_save(
        &self,
        Parameters(req): Parameters<SessionSaveReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let by = client_name(&ctx);
        let conn = self.locked();
        json_result(|| -> anyhow::Result<SessionOut> {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let parent_session_id = match req.parent_slug.as_deref() {
                Some(p) => Some(resolve_session(&conn, group_id, p)?),
                None => None,
            };
            let kind = req.kind.as_deref().unwrap_or(session::KIND_SESSION);
            let id = session::create(
                &conn,
                &session::NewSession {
                    group_id,
                    parent_session_id,
                    slug: &req.slug,
                    kind,
                    title: req.title.as_deref(),
                    body_md: &req.body_md,
                    created_by: &by,
                },
            )?;
            if let Some(prev_slug) = req.continues_slug.as_deref() {
                let prev_id = resolve_session(&conn, group_id, prev_slug)?;
                link::add(&conn, id, prev_id, link::REL_CONTINUES, &by)?;
                session::set_status(&conn, prev_id, "superseded", Some(&by))?;
            }
            touch_participant(&conn, id);
            Ok(session::get(&conn, id)?.into())
        })
    }

    #[tool(
        description = "Get a session by slug, or the latest active of the given kind (default 'session') in the group."
    )]
    async fn session_get(
        &self,
        Parameters(req): Parameters<SessionGetReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<SessionOut> {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let s = if let Some(s) = req.slug {
                session::find_by_slug(&conn, group_id, &s)?
                    .ok_or_else(|| anyhow!("session not found: {s}"))?
            } else {
                let kind = req.kind.as_deref().unwrap_or(session::KIND_SESSION);
                session::latest_in_group(&conn, group_id, kind)?
                    .ok_or_else(|| anyhow!("no session of kind '{kind}' in group"))?
            };
            touch_participant(&conn, s.id);
            Ok(s.into())
        })
    }

    #[tool(
        description = "List sessions in a group (most recent first). Optional GLOB pattern matches slug or kind."
    )]
    async fn session_list(
        &self,
        Parameters(req): Parameters<SessionListReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Vec<SessionLite>> {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let rows = match req.pattern.as_deref() {
                Some(p) => session::find_by_pattern(&conn, group_id, p)?,
                None => session::list_in_group(&conn, group_id)?,
            };
            Ok(rows.into_iter().map(Into::into).collect())
        })
    }

    #[tool(description = "List artefact sessions whose parent_session_id matches the given id.")]
    async fn session_children(
        &self,
        Parameters(req): Parameters<SessionChildrenReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Vec<SessionLite>> {
            Ok(session::list_children(&conn, req.session_id)?
                .into_iter()
                .map(Into::into)
                .collect())
        })
    }

    #[tool(
        description = "Patch-update a session. Every field is optional: body_md, kind, status, title, clear_title, parent_slug, clear_parent, new_group_path (move), new_slug (rename). Touches updated_at / updated_by."
    )]
    async fn session_update(
        &self,
        Parameters(req): Parameters<SessionUpdateReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let by = client_name(&ctx);
        let conn = self.locked();
        json_result(|| -> anyhow::Result<SessionOut> {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let id = resolve_session(&conn, group_id, &req.slug)?;

            let parent_session_id = match (req.clear_parent, req.parent_slug.as_deref()) {
                (true, Some(_)) => {
                    return Err(anyhow!(
                        "parent_slug and clear_parent=true are mutually exclusive"
                    ))
                }
                (true, None) => Some(None),
                (false, Some(p)) => Some(Some(resolve_session(&conn, group_id, p)?)),
                (false, None) => None,
            };
            let title = match (req.clear_title, req.title.as_deref()) {
                (true, Some(_)) => {
                    return Err(anyhow!("title and clear_title=true are mutually exclusive"))
                }
                (true, None) => Some(None),
                (false, Some(t)) => Some(Some(t)),
                (false, None) => None,
            };
            let new_group_id = match req.new_group_path.as_deref() {
                Some(p) => Some(resolve_group(&conn, p)?),
                None => None,
            };

            session::update(
                &conn,
                id,
                &session::SessionPatch {
                    body_md: req.body_md.as_deref(),
                    kind: req.kind.as_deref(),
                    status: req.status.as_deref(),
                    title,
                    parent_session_id,
                    new_group_id,
                    new_slug: req.new_slug.as_deref(),
                    updated_by: Some(&by),
                },
            )?;
            touch_participant(&conn, id);
            Ok(session::get(&conn, id)?.into())
        })
    }

    #[tool(
        description = "Delete a session by group_path + slug. Cascades to its artefacts, file_refs and links. Files on disk are NOT removed."
    )]
    async fn session_delete(
        &self,
        Parameters(req): Parameters<SessionDeleteReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let id = resolve_session(&conn, group_id, &req.slug)?;
            session::delete(&conn, id)?;
            Ok(format!("deleted session#{id} {}", req.slug))
        })
    }

    #[tool(
        description = "Add a typed link between two sessions (e.g. relation = 'references', 'fixes', 'relates_to')."
    )]
    async fn link_add(
        &self,
        Parameters(req): Parameters<LinkAddReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let by = client_name(&ctx);
        let conn = self.locked();
        text_result(|| {
            link::add(&conn, req.from_id, req.to_id, &req.relation, &by)?;
            Ok(format!(
                "linked {} -[{}]-> {}",
                req.from_id, req.relation, req.to_id
            ))
        })
    }

    #[tool(description = "Remove a typed link between two sessions.")]
    async fn link_remove(
        &self,
        Parameters(req): Parameters<LinkRemoveReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            link::remove(&conn, req.from_id, req.to_id, &req.relation)?;
            Ok(format!(
                "removed {} -[{}]-> {}",
                req.from_id, req.relation, req.to_id
            ))
        })
    }

    #[tool(
        description = "List outgoing and incoming links for a session id. Each row carries a resolved `peer` (id, slug, group_path, kind, status) so callers can navigate without an extra round-trip."
    )]
    async fn link_of(
        &self,
        Parameters(req): Parameters<LinkOfReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<LinkOfOut> {
            let mut cache: HashMap<i64, String> = HashMap::new();
            let mut outgoing: Vec<LinkOut> = Vec::new();
            for l in link::outgoing(&conn, req.session_id, None)? {
                let peer_id = l.to_id;
                let mut row: LinkOut = l.into();
                row.peer = resolve_peer(&conn, peer_id, &mut cache).ok();
                outgoing.push(row);
            }
            let mut incoming: Vec<LinkOut> = Vec::new();
            for l in link::incoming(&conn, req.session_id, None)? {
                let peer_id = l.from_id;
                let mut row: LinkOut = l.into();
                row.peer = resolve_peer(&conn, peer_id, &mut cache).ok();
                incoming.push(row);
            }
            Ok(LinkOfOut { outgoing, incoming })
        })
    }

    #[tool(
        description = "List the conversations that have worked in a session (audit), most recently active first."
    )]
    async fn session_participants(
        &self,
        Parameters(req): Parameters<SessionParticipantsReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| Ok(participant::list_for_session(&conn, req.session_id)?))
    }

    #[tool(
        description = "List file changes attributed to a session (audit), most recent first. Populated automatically from provider hooks."
    )]
    async fn session_file_changes(
        &self,
        Parameters(req): Parameters<SessionFileChangesReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        let limit = req.limit.unwrap_or(200);
        json_result(|| Ok(file_change::list_for_session(&conn, req.session_id, limit)?))
    }
}
