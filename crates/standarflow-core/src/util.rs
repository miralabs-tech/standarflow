use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// The standarflow data directory, relative to a workspace root or the home
/// directory — holds the database, the event log, and exports.
pub(crate) const STANDARFLOW_DIR: &str = ".standarflow";

pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// The current user's home directory, from `USERPROFILE` (Windows) or `HOME`.
pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
