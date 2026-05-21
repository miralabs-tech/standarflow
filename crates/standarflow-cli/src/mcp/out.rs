use std::collections::HashMap;

use serde::Serialize;
use standarflow_core::{
    store::{conversation, file_ref, group, link, session},
    Connection,
};

use super::helpers::group_path_for;

#[derive(Serialize)]
pub(crate) struct GroupOut {
    pub(crate) id: i64,
    pub(crate) parent_id: Option<i64>,
    pub(crate) slug: String,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) created_by: String,
    pub(crate) updated_by: Option<String>,
}

impl From<group::Group> for GroupOut {
    fn from(g: group::Group) -> Self {
        Self {
            id: g.id,
            parent_id: g.parent_id,
            slug: g.slug,
            title: g.title,
            description: g.description,
            created_at: g.created_at,
            updated_at: g.updated_at,
            created_by: g.created_by,
            updated_by: g.updated_by,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SessionOut {
    pub(crate) id: i64,
    pub(crate) group_id: i64,
    pub(crate) parent_session_id: Option<i64>,
    pub(crate) slug: String,
    pub(crate) kind: String,
    pub(crate) status: String,
    pub(crate) title: Option<String>,
    pub(crate) body_md: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) created_by: String,
    pub(crate) updated_by: Option<String>,
}

impl From<session::Session> for SessionOut {
    fn from(s: session::Session) -> Self {
        Self {
            id: s.id,
            group_id: s.group_id,
            parent_session_id: s.parent_session_id,
            slug: s.slug,
            kind: s.kind,
            status: s.status,
            title: s.title,
            body_md: s.body_md,
            created_at: s.created_at,
            updated_at: s.updated_at,
            created_by: s.created_by,
            updated_by: s.updated_by,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SessionLite {
    pub(crate) id: i64,
    pub(crate) parent_session_id: Option<i64>,
    pub(crate) slug: String,
    pub(crate) kind: String,
    pub(crate) status: String,
    pub(crate) title: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) created_by: String,
}

impl From<session::Session> for SessionLite {
    fn from(s: session::Session) -> Self {
        Self {
            id: s.id,
            parent_session_id: s.parent_session_id,
            slug: s.slug,
            kind: s.kind,
            status: s.status,
            title: s.title,
            created_at: s.created_at,
            updated_at: s.updated_at,
            created_by: s.created_by,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct LinkOut {
    pub(crate) from_id: i64,
    pub(crate) to_id: i64,
    pub(crate) relation: String,
    pub(crate) created_at: i64,
    pub(crate) created_by: String,
    pub(crate) peer: Option<LinkPeer>,
}

#[derive(Serialize)]
pub(crate) struct LinkPeer {
    pub(crate) id: i64,
    pub(crate) slug: String,
    pub(crate) group_path: String,
    pub(crate) kind: String,
    pub(crate) status: String,
}

impl From<link::Link> for LinkOut {
    fn from(l: link::Link) -> Self {
        Self {
            from_id: l.from_id,
            to_id: l.to_id,
            relation: l.relation,
            created_at: l.created_at,
            created_by: l.created_by,
            peer: None,
        }
    }
}

pub(crate) fn resolve_peer(
    conn: &Connection,
    peer_session_id: i64,
    cache: &mut HashMap<i64, String>,
) -> anyhow::Result<LinkPeer> {
    let s = session::get(conn, peer_session_id)?;
    let group_path = group_path_for(conn, s.group_id, cache)?;
    Ok(LinkPeer {
        id: s.id,
        slug: s.slug,
        group_path,
        kind: s.kind,
        status: s.status,
    })
}

#[derive(Serialize)]
pub(crate) struct FileRefOut {
    pub(crate) id: i64,
    pub(crate) session_id: i64,
    pub(crate) path: String,
    pub(crate) role: String,
    pub(crate) source: String,
    pub(crate) description: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) created_by: String,
}

impl From<file_ref::FileRef> for FileRefOut {
    fn from(f: file_ref::FileRef) -> Self {
        Self {
            id: f.id,
            session_id: f.session_id,
            path: f.path,
            role: f.role,
            source: f.source,
            description: f.description,
            created_at: f.created_at,
            created_by: f.created_by,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct LinkOfOut {
    pub(crate) outgoing: Vec<LinkOut>,
    pub(crate) incoming: Vec<LinkOut>,
}

#[derive(Serialize)]
pub(crate) struct FileDeleteWithSourceOut {
    pub(crate) file_ref_id: i64,
    pub(crate) path: String,
    pub(crate) file_deleted: bool,
    pub(crate) file_was_missing: bool,
}

#[derive(Serialize)]
pub(crate) struct WorkspaceInfoOut {
    pub(crate) bin_path: Option<String>,
    pub(crate) db_path: String,
    pub(crate) schema_version: i64,
    pub(crate) groups_count: i64,
    pub(crate) sessions_count: i64,
    pub(crate) file_refs_count: i64,
    pub(crate) conversations_count: i64,
    /// The workspace's current session pointer (`meta` `current_session_id`),
    /// or `None` before the first focus on a build carrying the pointer.
    pub(crate) current_session_id: Option<i64>,
    /// The current session's location, resolved from `current_session_id` so a
    /// client can reveal it without a second lookup. Both `None` (alongside a
    /// set `current_session_id`) only if the pointer dangles.
    pub(crate) current_session_group_path: Option<String>,
    pub(crate) current_session_slug: Option<String>,
    pub(crate) first_run: bool,
}

/// What changed since a timestamp — a coarse change-feed a client polls to
/// refresh only the affected parts of its view, instead of reloading wholesale.
#[derive(Serialize)]
pub(crate) struct ChangesSinceOut {
    /// The server's "now" — pass it back as the next `ts`.
    pub(crate) now: i64,
    /// A group row was created or updated.
    pub(crate) groups: bool,
    /// A conversation or focus row was touched (liveness, new chat, focus).
    pub(crate) conversations: bool,
    /// Ids of sessions created or updated.
    pub(crate) sessions: Vec<i64>,
    /// Ids of sessions with new file-change rows.
    pub(crate) file_change_sessions: Vec<i64>,
}

#[derive(Serialize)]
pub(crate) struct SessionFocusedOut {
    pub(crate) conversation_id: i64,
    pub(crate) provider: String,
    pub(crate) provider_conversation_id: String,
    pub(crate) group_path: String,
    pub(crate) session_id: i64,
    pub(crate) session_slug: String,
    pub(crate) session_kind: String,
    pub(crate) session_status: String,
    pub(crate) focused_at: i64,
    pub(crate) pending_session_id: Option<i64>,
    /// `false` when this is the workspace's inherited suggestion (no focus row
    /// yet) rather than a confirmed focus.
    pub(crate) confirmed: bool,
}

#[derive(Serialize)]
pub(crate) struct FocusEntryOut {
    pub(crate) conversation_id: i64,
    pub(crate) provider: String,
    pub(crate) provider_conversation_id: String,
    pub(crate) client_label: Option<String>,
    pub(crate) workspace_path: Option<String>,
    pub(crate) last_seen_at: i64,
    pub(crate) ended_at: Option<i64>,
    pub(crate) group_path: String,
    pub(crate) session_id: i64,
    pub(crate) session_slug: String,
    pub(crate) session_kind: String,
    pub(crate) session_status: String,
    pub(crate) focused_at: i64,
    pub(crate) is_live: bool,
}

/// A conversation row plus computed liveness (its agent process is running).
#[derive(Serialize)]
pub(crate) struct ConversationOut {
    #[serde(flatten)]
    pub(crate) conversation: conversation::Conversation,
    pub(crate) is_live: bool,
}

#[derive(Serialize)]
pub(crate) struct DebugEnvOut {
    pub(crate) pid: u32,
    pub(crate) cwd: Option<String>,
    pub(crate) exe: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) mcp_client_name: Option<String>,
    pub(crate) mcp_client_version: Option<String>,
    pub(crate) agent_root_pid: Option<u32>,
    pub(crate) conversation_pid: i64,
    pub(crate) resolved_conversation_id: Option<i64>,
    pub(crate) parent_chain: Vec<(u32, String)>,
    pub(crate) env: std::collections::BTreeMap<String, String>,
}
