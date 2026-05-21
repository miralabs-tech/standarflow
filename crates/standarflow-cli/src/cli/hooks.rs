use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use clap::{Subcommand, ValueEnum};
use standarflow_core::{pipeline::{event, hooks, ingest, tail}, Connection};

use crate::proctree;

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum ScopeArg {
    /// ~/.claude/settings.json — captures every workspace on this machine.
    User,
    /// <root>/.claude/settings.local.json — this workspace only.
    ProjectLocal,
}

impl ScopeArg {
    fn to_core(self) -> hooks::Scope {
        match self {
            ScopeArg::User => hooks::Scope::User,
            ScopeArg::ProjectLocal => hooks::Scope::ProjectLocal,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum HooksCmd {
    /// Wire standarflow ingest hooks into a provider's settings file.
    Install {
        #[arg(long, default_value = "claude-code")]
        provider: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
        /// Workspace root for `--scope project-local`. Defaults to the cwd.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Remove standarflow ingest hooks from a provider's settings file.
    Uninstall {
        #[arg(long, default_value = "claude-code")]
        provider: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Show which hook events are wired, across every scope.
    Status {
        #[arg(long, default_value = "claude-code")]
        provider: String,
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum EventsCmd {
    /// Drain new events from the workspace log into the database.
    Tail,
    /// List recently ingested events.
    List {
        #[arg(long, default_value_t = 30)]
        limit: i64,
    },
}

pub(crate) fn cmd_ingest(provider: &str) {
    // Hot path, run on every hook firing. Never fail loudly — a broken ingest
    // must not block the agent. Swallow everything and exit 0.
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return;
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };
    let pid = proctree::agent_root_pid().map(i64::from);
    let _ = ingest::ingest(provider, &json, pid);
}

pub(crate) fn handle_hooks(action: &HooksCmd) -> anyhow::Result<()> {
    match action {
        HooksCmd::Install {
            provider,
            scope,
            root,
        } => {
            let exe = std::env::current_exe()?;
            let root = hooks_root(root.as_deref())?;
            let r = hooks::install(provider, &exe, scope.to_core(), &root)?;
            println!("provider: {}", r.provider);
            println!("scope:    {}", r.scope.as_str());
            if let Some(f) = &r.settings_file {
                println!("settings: {}", f.display());
            }
            if !r.events_added.is_empty() {
                println!("added: {}", r.events_added.join(", "));
            }
            if !r.events_already_present.is_empty() {
                println!("already present: {}", r.events_already_present.join(", "));
            }
            if let Some(b) = &r.backup_path {
                println!("backup: {}", b.display());
            }
            if let Some(i) = &r.instructions {
                println!("\n{i}");
            }
        }
        HooksCmd::Uninstall {
            provider,
            scope,
            root,
        } => {
            let root = hooks_root(root.as_deref())?;
            let r = hooks::uninstall(provider, scope.to_core(), &root)?;
            println!("provider: {}", r.provider);
            println!("scope:    {}", r.scope.as_str());
            if r.events_removed.is_empty() {
                println!("nothing to remove");
            } else {
                println!("removed: {}", r.events_removed.join(", "));
            }
            if let Some(b) = &r.backup_path {
                println!("backup: {}", b.display());
            }
        }
        HooksCmd::Status { provider, root } => {
            let root = hooks_root(root.as_deref())?;
            let r = hooks::status(provider, &root)?;
            println!("provider: {}", r.provider);
            for s in &r.scopes {
                println!();
                println!("[{}]", s.scope.as_str());
                if let Some(f) = &s.settings_file {
                    println!("  settings:  {}", f.display());
                }
                println!("  installed: {}", s.installed_events.join(", "));
                println!("  missing:   {}", s.missing_events.join(", "));
            }
        }
    }
    Ok(())
}

/// Resolve the workspace root for hook scopes — explicit `--root` or the cwd.
fn hooks_root(root: Option<&Path>) -> anyhow::Result<PathBuf> {
    match root {
        Some(p) => Ok(p.to_path_buf()),
        None => Ok(std::env::current_dir()?),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn handle_events(conn: &Connection, db_path: &Path, action: EventsCmd) -> anyhow::Result<()> {
    match action {
        EventsCmd::Tail => {
            let workspace = db_path
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| anyhow!("cannot derive workspace from db path"))?;
            let r = tail::tail(conn, workspace)?;
            println!(
                "tailed: {} ingested, {} skipped, offset {}",
                r.events_ingested, r.lines_skipped, r.new_offset
            );
        }
        EventsCmd::List { limit } => {
            for e in event::list_recent(conn, limit)? {
                println!(
                    "{}\t{}\t{}\t{}\tconv={}",
                    e.id,
                    e.ts,
                    e.provider,
                    e.event_kind,
                    e.conversation_id
                        .map_or_else(|| "-".to_string(), |i| i.to_string())
                );
            }
        }
    }
    Ok(())
}
