use rmcp::schemars;

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GroupCreateReq {
    pub slug: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_path: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GroupListReq {
    #[serde(default)]
    pub parent_path: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GroupDeleteReq {
    pub group_path: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionSaveReq {
    pub group_path: String,
    pub slug: String,
    pub body_md: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub parent_slug: Option<String>,
    #[serde(default)]
    pub continues_slug: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionGetReq {
    pub group_path: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionListReq {
    pub group_path: String,
    #[serde(default)]
    pub pattern: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionChildrenReq {
    pub session_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionUpdateReq {
    pub group_path: String,
    pub slug: String,
    #[serde(default)]
    pub body_md: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub clear_title: bool,
    #[serde(default)]
    pub parent_slug: Option<String>,
    #[serde(default)]
    pub clear_parent: bool,
    #[serde(default)]
    pub new_group_path: Option<String>,
    #[serde(default)]
    pub new_slug: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionDeleteReq {
    pub group_path: String,
    pub slug: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct LinkAddReq {
    pub from_id: i64,
    pub to_id: i64,
    pub relation: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct LinkRemoveReq {
    pub from_id: i64,
    pub to_id: i64,
    pub relation: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct LinkOfReq {
    pub session_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FileAttachReq {
    pub group_path: String,
    pub session_slug: String,
    pub path: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FileListReq {
    pub group_path: String,
    pub session_slug: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FileReadReq {
    pub file_ref_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FileRemoveReq {
    pub file_ref_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FileClaimReq {
    pub file_ref_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FileDeleteWithSourceReq {
    pub file_ref_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionFocusReq {
    pub group_path: String,
    pub slug: String,
    /// Focus on behalf of an explicit conversation. Defaults to the
    /// conversation this MCP server is bound to (agent-side focus).
    #[serde(default)]
    pub conversation_id: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionUnfocusReq {
    /// Clear focus for an explicit conversation. Defaults to the conversation
    /// this MCP server is bound to.
    #[serde(default)]
    pub conversation_id: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionFocusedReq {
    /// Look up a specific conversation's focus. Defaults to the conversation
    /// this MCP server is bound to.
    #[serde(default)]
    pub conversation_id: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ConversationGetReq {
    /// Conversation id. Defaults to the conversation this MCP server serves.
    #[serde(default)]
    pub conversation_id: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ConversationListReq {
    /// Only conversations seen at or after this unix timestamp.
    #[serde(default)]
    pub active_since: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ConversationSetLabelReq {
    pub conversation_id: i64,
    /// New label. Omit or pass null to clear it and fall back to the
    /// derived label.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionParticipantsReq {
    pub session_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionFileChangesReq {
    pub session_id: i64,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct MemoryImportReq {
    pub group_path: String,
    pub session_slug: String,
    pub dir_path: String,
    #[serde(default)]
    pub ext: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DebugEnvReq {
    #[serde(default)]
    pub all: bool,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ChangesSinceReq {
    /// Report rows changed strictly after this unix timestamp (seconds).
    pub ts: i64,
}
