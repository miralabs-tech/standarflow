use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorData as McpError};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_router, RoleServer};
use standarflow_core::store::{group, meta, session};

use super::helpers::{client_name, current_bin_path, group_path_for, json_result, text_result};
use super::out::{ChangesSinceOut, GroupOut, WorkspaceInfoOut};
use super::req::{ChangesSinceReq, GroupCreateReq, GroupDeleteReq, GroupListReq};
use super::StandarflowMcp;
use crate::common::resolve_group;

#[tool_router(router = group_router, vis = "pub(crate)")]
impl StandarflowMcp {
    #[tool(
        description = "Return workspace state: db_path, schema_version, row counts, current_session_id, and first_run (true when no groups exist yet — extension should propose onboarding)."
    )]
    async fn workspace_info(&self) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<WorkspaceInfoOut> {
            let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            let groups_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))?;
            let sessions_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
            let file_refs_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM file_refs", [], |r| r.get(0))?;
            let conversations_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))?;
            let current_session_id = meta::get_i64(&conn, meta::KEY_CURRENT_SESSION_ID)?;
            let (current_session_group_path, current_session_slug) = match current_session_id {
                Some(id) => match session::get(&conn, id) {
                    Ok(s) => {
                        let mut cache: HashMap<i64, String> = HashMap::new();
                        (
                            group_path_for(&conn, s.group_id, &mut cache).ok(),
                            Some(s.slug),
                        )
                    }
                    Err(_) => (None, None),
                },
                None => (None, None),
            };
            Ok(WorkspaceInfoOut {
                bin_path: current_bin_path(),
                db_path: self.db_path.clone(),
                schema_version,
                groups_count,
                sessions_count,
                file_refs_count,
                conversations_count,
                current_session_id,
                current_session_group_path,
                current_session_slug,
                first_run: groups_count == 0,
            })
        })
    }

    #[tool(
        description = "Coarse change-feed: report what DB rows changed strictly after `ts` (unix seconds). Returns `now` to pass back as the next `ts`, plus per-area flags and the session ids touched — so a client refreshes only the affected parts of its view instead of reloading wholesale."
    )]
    async fn changes_since(
        &self,
        Parameters(req): Parameters<ChangesSinceReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<ChangesSinceOut> {
            let ts = req.ts;
            let groups: bool = conn.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM groups WHERE updated_at > ?1 OR created_at > ?1)",
                [ts],
                |r| r.get(0),
            )?;
            let conversations: bool = conn.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM conversations
                   WHERE last_seen_at > ?1 OR first_seen_at > ?1 OR ended_at > ?1)
                 OR EXISTS(
                   SELECT 1 FROM session_focus
                   WHERE last_touched_at > ?1 OR focused_at > ?1)",
                [ts],
                |r| r.get(0),
            )?;
            let sessions: Vec<i64> = conn
                .prepare("SELECT id FROM sessions WHERE updated_at > ?1 OR created_at > ?1")?
                .query_map([ts], |r| r.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let file_change_sessions: Vec<i64> = conn
                .prepare("SELECT DISTINCT session_id FROM session_file_changes WHERE ts > ?1")?
                .query_map([ts], |r| r.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            Ok(ChangesSinceOut {
                now,
                groups,
                conversations,
                sessions,
                file_change_sessions,
            })
        })
    }

    #[tool(
        description = "Create a group. Use parent_path for nesting (e.g. \"backend/auth\"). created_by is taken from the MCP client name."
    )]
    async fn group_create(
        &self,
        Parameters(req): Parameters<GroupCreateReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let by = client_name(&ctx);
        let conn = self.locked();
        json_result(|| -> anyhow::Result<GroupOut> {
            let parent_id = match req.parent_path.as_deref() {
                Some(p) => Some(resolve_group(&conn, p)?),
                None => None,
            };
            let id = group::create(
                &conn,
                &group::NewGroup {
                    parent_id,
                    slug: &req.slug,
                    title: req.title.as_deref(),
                    description: req.description.as_deref(),
                    created_by: &by,
                },
            )?;
            Ok(group::get(&conn, id)?.into())
        })
    }

    #[tool(
        description = "List child groups under an optional parent_path (root groups if omitted)."
    )]
    async fn group_list(
        &self,
        Parameters(req): Parameters<GroupListReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Vec<GroupOut>> {
            let parent_id = match req.parent_path.as_deref() {
                Some(p) => Some(resolve_group(&conn, p)?),
                None => None,
            };
            Ok(group::list_children(&conn, parent_id)?
                .into_iter()
                .map(Into::into)
                .collect())
        })
    }

    #[tool(
        description = "Delete a group (cascades to its sessions, artefacts, file_refs and links). Files on disk are NOT removed."
    )]
    async fn group_delete(
        &self,
        Parameters(req): Parameters<GroupDeleteReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            let id = resolve_group(&conn, &req.group_path)?;
            group::delete(&conn, id)?;
            Ok(format!("deleted group#{id} {}", req.group_path))
        })
    }
}
