use super::ProviderAdapter;
use crate::error::{Error, Result};
use crate::pipeline::event::{EventKind, NormalizedEvent};
use crate::util::now_unix;

pub const PROVIDER: &str = "generic";

/// Adapter for tools without a built-in provider. The raw payload is expected
/// to already resemble a [`NormalizedEvent`]: a `conversation_id` and a `kind`
/// (either a discriminant string, or the full `EventKind` object).
pub struct GenericAdapter;

impl ProviderAdapter for GenericAdapter {
    fn name(&self) -> &'static str {
        PROVIDER
    }

    fn normalize(&self, raw: &serde_json::Value) -> Result<NormalizedEvent> {
        let provider = raw
            .get("provider")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(PROVIDER)
            .to_string();

        let provider_conversation_id = raw
            .get("conversation_id")
            .or_else(|| raw.get("provider_conversation_id"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Invalid("generic event missing conversation_id".into()))?
            .to_string();

        let kind = parse_kind(raw)?;
        let ts = raw
            .get("ts")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_else(now_unix);

        Ok(NormalizedEvent {
            provider,
            provider_conversation_id,
            kind,
            workspace_path: str_field(raw, "workspace_path"),
            transcript_path: str_field(raw, "transcript_path"),
            client_label: str_field(raw, "client_label"),
            conversation_pid: raw
                .get("conversation_pid")
                .and_then(serde_json::Value::as_i64),
            ts,
            raw: raw.clone(),
        })
    }
}

fn str_field(raw: &serde_json::Value, key: &str) -> Option<String> {
    raw.get(key)
        .and_then(serde_json::Value::as_str)
        .map(String::from)
}

fn parse_kind(raw: &serde_json::Value) -> Result<EventKind> {
    let kind = raw
        .get("kind")
        .ok_or_else(|| Error::Invalid("generic event missing kind".into()))?;

    // Object form: deserialize the EventKind directly.
    if kind.is_object() {
        return serde_json::from_value(kind.clone()).map_err(Into::into);
    }

    // String form: a bare discriminant enriched with sibling fields.
    let disc = kind
        .as_str()
        .ok_or_else(|| Error::Invalid("generic event kind must be a string or object".into()))?;
    let tool = || str_field(raw, "tool").unwrap_or_else(|| "tool".to_string());
    Ok(match disc {
        "session_start" => EventKind::SessionStart,
        "user_prompt" => EventKind::UserPrompt,
        "tool_pre" => EventKind::ToolPre { tool: tool() },
        "tool_post" => EventKind::ToolPost {
            tool: tool(),
            file_path: str_field(raw, "file_path"),
        },
        "stop" => EventKind::Stop,
        "session_end" => EventKind::SessionEnd,
        other => EventKind::Other {
            name: other.to_string(),
        },
    })
}
