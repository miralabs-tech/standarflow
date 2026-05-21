pub mod claude_code;
pub mod generic;

use crate::error::Result;
use crate::pipeline::event::NormalizedEvent;

/// Translates a provider's raw hook payload into the canonical
/// [`NormalizedEvent`]. Adding a new provider means implementing this trait
/// and registering it in [`adapter_for`].
pub trait ProviderAdapter {
    /// Stable provider identifier, e.g. `claude-code`.
    fn name(&self) -> &'static str;

    /// Translate one raw hook payload into a [`NormalizedEvent`].
    fn normalize(&self, raw: &serde_json::Value) -> Result<NormalizedEvent>;
}

/// Resolve an adapter by provider name. Returns `None` for unknown providers.
#[must_use]
pub fn adapter_for(name: &str) -> Option<Box<dyn ProviderAdapter>> {
    match name {
        "claude-code" => Some(Box::new(claude_code::ClaudeCodeAdapter)),
        "generic" => Some(Box::new(generic::GenericAdapter)),
        _ => None,
    }
}
