use std::path::Path;

use anyhow::{anyhow, Context};
use standarflow_core::{
    store::{group, session},
    Connection,
};

/// Environment-variable name prefixes surfaced by the `debug env` diagnostic —
/// the CLI `debug env` command and the `debug_env` MCP tool. One canonical set
/// so both surfaces report the same thing.
pub(crate) const DEBUG_ENV_PREFIXES: &[&str] = &[
    "CLAUDE",
    "ANTHROPIC",
    "MCP",
    "STANDARFLOW",
    "CURSOR",
    "VSCODE",
    "TERM_PROGRAM",
    "TERM_SESSION_ID",
    "WT_SESSION",
    "WT_PROFILE_ID",
    "SESSIONNAME",
    "USERPROFILE",
    "HOME",
    "PWD",
    "OLDPWD",
];

pub(crate) fn resolve_group(conn: &Connection, slug_path: &str) -> anyhow::Result<i64> {
    let mut parent: Option<i64> = None;
    for part in slug_path.split('/') {
        let g = group::find_by_slug(conn, part, parent)?
            .ok_or_else(|| anyhow!("group not found: {part}"))?;
        parent = Some(g.id);
    }
    parent.ok_or_else(|| anyhow!("empty group path"))
}

pub(crate) fn resolve_session(conn: &Connection, group_id: i64, slug: &str) -> anyhow::Result<i64> {
    Ok(session::find_by_slug(conn, group_id, slug)?
        .ok_or_else(|| anyhow!("session not found: {slug}"))?
        .id)
}

/// Strip the Windows `\\?\` verbatim-path prefix so paths render canonically.
pub(crate) fn strip_unc(s: String) -> String {
    if cfg!(windows) {
        s.strip_prefix(r"\\?\").map(String::from).unwrap_or(s)
    } else {
        s
    }
}

pub(crate) fn canonical_path(path: &Path) -> anyhow::Result<String> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("cannot canonicalize {}", path.display()))?;
    Ok(strip_unc(canonical.to_string_lossy().to_string()))
}
