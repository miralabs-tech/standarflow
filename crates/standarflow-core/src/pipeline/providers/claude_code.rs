use super::ProviderAdapter;
use crate::error::{Error, Result};
use crate::pipeline::event::{EventKind, NormalizedEvent};
use crate::util::now_unix;

pub const PROVIDER: &str = "claude-code";

/// Adapter for Claude Code hook payloads. Hook field names are `snake_case`
/// (verified empirically); camelCase is accepted as a fallback.
pub struct ClaudeCodeAdapter;

impl ProviderAdapter for ClaudeCodeAdapter {
    fn name(&self) -> &'static str {
        PROVIDER
    }

    fn normalize(&self, raw: &serde_json::Value) -> Result<NormalizedEvent> {
        let hook = raw
            .get("hook_event_name")
            .or_else(|| raw.get("hookEventName"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        let provider_conversation_id = raw
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Invalid("claude-code event missing session_id".into()))?
            .to_string();

        let workspace_path = raw
            .get("cwd")
            .and_then(serde_json::Value::as_str)
            .map(String::from);
        let transcript_path = raw
            .get("transcript_path")
            .and_then(serde_json::Value::as_str)
            .map(String::from);
        let ts = raw
            .get("ts")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_else(now_unix);

        let kind = match hook {
            "SessionStart" => EventKind::SessionStart,
            "UserPromptSubmit" => EventKind::UserPrompt,
            "PreToolUse" => EventKind::ToolPre {
                tool: tool_name(raw),
            },
            "PostToolUse" => EventKind::ToolPost {
                tool: tool_name(raw),
                file_path: tool_file_path(raw),
            },
            "Stop" => EventKind::Stop,
            "SessionEnd" => EventKind::SessionEnd,
            other => EventKind::Other {
                name: other.to_string(),
            },
        };

        Ok(NormalizedEvent {
            provider: PROVIDER.to_string(),
            provider_conversation_id,
            kind,
            workspace_path,
            transcript_path,
            client_label: None,
            conversation_pid: None,
            ts,
            raw: raw.clone(),
        })
    }
}

fn tool_name(raw: &serde_json::Value) -> String {
    raw.get("tool_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tool")
        .to_string()
}

/// The `file_path` from `tool_input`, when the tool carries one — Write, Edit
/// and Read all do. This is a plain extractor; whether the path was actually
/// *mutated* (vs merely read) is decided downstream in `tail::ingest_event`.
/// Tools with no single path (Bash, Glob, …) yield `None`.
fn tool_file_path(raw: &serde_json::Value) -> Option<String> {
    raw.get("tool_input")
        .and_then(|ti| ti.get("file_path"))
        .and_then(serde_json::Value::as_str)
        .map(String::from)
}
