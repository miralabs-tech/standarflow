use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use clap::Subcommand;
use standarflow_core::{db, export, Connection};

use super::client_label;
use crate::common::DEBUG_ENV_PREFIXES;
use crate::proctree;

#[derive(Subcommand)]
pub(crate) enum DbCmd {
    /// Wipe the database. Exports a snapshot first unless --confirm-no-export.
    Reset {
        #[arg(long)]
        confirm_no_export: bool,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum DebugCmd {
    /// Dump pid / cwd / exe / args / env as JSON, plus the agent process tree.
    Env {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value = "")]
        tag: String,
    },
}

pub(crate) fn handle_db(action: &DbCmd, db_override: Option<&Path>) -> anyhow::Result<()> {
    match action {
        DbCmd::Reset {
            confirm_no_export,
            out,
        } => {
            let path = match db_override {
                Some(p) => p.to_path_buf(),
                None => db::default_path(&std::env::current_dir()?),
            };
            if !confirm_no_export {
                if path.exists() {
                    let conn = db::open(&path)?;
                    let out_dir = resolve_export_dir(out.clone())?;
                    let r = export::export(&conn, &out_dir)?;
                    drop(conn);
                    println!(
                        "exported {} sessions to {}",
                        r.sessions,
                        r.out_dir.display()
                    );
                } else {
                    println!("no database at {} — nothing to export", path.display());
                }
            }
            delete_db_files(&path)?;
            println!("database reset: {}", path.display());
        }
    }
    Ok(())
}

fn delete_db_files(path: &Path) -> anyhow::Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let p = if suffix.is_empty() {
            path.to_path_buf()
        } else {
            PathBuf::from(format!("{}{}", path.display(), suffix))
        };
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(anyhow::Error::from(e).context(format!("cannot delete {}", p.display())))
            }
        }
    }
    Ok(())
}

fn resolve_export_dir(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match explicit {
        Some(o) => Ok(o),
        None => export::default_export_dir()
            .ok_or_else(|| anyhow!("cannot resolve home directory for export")),
    }
}

pub(crate) fn handle_export(conn: &Connection, out: Option<PathBuf>) -> anyhow::Result<()> {
    let out_dir = resolve_export_dir(out)?;
    let r = export::export(conn, &out_dir)?;
    println!("exported to {}", r.out_dir.display());
    println!(
        "  groups: {}  sessions: {}  conversations: {}  events: {}",
        r.groups, r.sessions, r.conversations, r.events
    );
    Ok(())
}

pub(crate) fn handle_debug(action: DebugCmd) -> anyhow::Result<()> {
    match action {
        DebugCmd::Env { all, out, tag } => {
            let envs: std::collections::BTreeMap<String, String> = std::env::vars()
                .filter(|(k, _)| {
                    if all {
                        return true;
                    }
                    let up = k.to_uppercase();
                    DEBUG_ENV_PREFIXES.iter().any(|p| up.starts_with(p))
                })
                .collect();

            let info = serde_json::json!({
                "tag": if tag.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(tag) },
                "captured_at": now_iso(),
                "pid": std::process::id(),
                "cwd": std::env::current_dir().ok().map(|p| p.display().to_string()),
                "exe": std::env::current_exe().ok().map(|p| p.display().to_string()),
                "args": std::env::args().collect::<Vec<_>>(),
                "effective_client_name": client_label(),
                "agent_root_pid": proctree::agent_root_pid(),
                "conversation_pid": proctree::conversation_pid(),
                "parent_chain": proctree::parent_chain(),
                "env": envs,
            });

            let pretty = serde_json::to_string_pretty(&info)?;
            match out {
                Some(p) => {
                    if let Some(parent) = p.parent() {
                        if !parent.as_os_str().is_empty() {
                            std::fs::create_dir_all(parent).ok();
                        }
                    }
                    let mut f = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&p)
                        .with_context(|| format!("cannot open {}", p.display()))?;
                    writeln!(f, "{pretty}")?;
                    writeln!(f, "---")?;
                }
                None => println!("{pretty}"),
            }
        }
    }
    Ok(())
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    format_unix_iso(secs)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn format_unix_iso(ts: i64) -> String {
    let secs = u64::try_from(ts).unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    let (y, m, d) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}
