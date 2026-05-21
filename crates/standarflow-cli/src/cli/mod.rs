pub(crate) mod admin;
pub(crate) mod files;
pub(crate) mod focus;
pub(crate) mod hooks;
pub(crate) mod store;

/// Identity recorded as `created_by` for CLI-originated rows.
pub(crate) fn client_label() -> String {
    std::env::var("STANDARFLOW_CLIENT").unwrap_or_else(|_| "cli".to_string())
}
