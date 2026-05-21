use std::path::PathBuf;

use anyhow::{anyhow, Context};
use clap::Subcommand;
use standarflow_core::{store::file_ref, Connection};

use super::client_label;
use crate::common::{canonical_path, resolve_group, resolve_session};

#[derive(Subcommand)]
pub(crate) enum FileCmd {
    Attach {
        #[arg(long)]
        group: String,
        #[arg(long)]
        session: String,
        #[arg(long, default_value = file_ref::ROLE_ATTACHMENT)]
        role: String,
        #[arg(long)]
        desc: Option<String>,
        path: PathBuf,
    },
    List {
        #[arg(long)]
        group: String,
        #[arg(long)]
        session: String,
    },
    Read {
        id: i64,
    },
    Remove {
        id: i64,
    },
}

#[derive(Subcommand)]
pub(crate) enum MemoryCmd {
    Import {
        #[arg(long)]
        group: String,
        #[arg(long)]
        session: String,
        #[arg(long, default_value = "md")]
        ext: String,
        #[arg(long, default_value = file_ref::ROLE_MEMORY)]
        role: String,
        path: PathBuf,
    },
}

pub(crate) fn handle_file(conn: &Connection, action: FileCmd) -> anyhow::Result<()> {
    match action {
        FileCmd::Attach {
            group,
            session,
            role,
            desc,
            path,
        } => {
            let group_id = resolve_group(conn, &group)?;
            let session_id = resolve_session(conn, group_id, &session)?;
            let p = canonical_path(&path)?;
            let by = client_label();
            let id = file_ref::attach(
                conn,
                &file_ref::NewFileRef {
                    session_id,
                    path: &p,
                    role: &role,
                    source: file_ref::SOURCE_MANUAL,
                    description: desc.as_deref(),
                    created_by: &by,
                },
            )?;
            println!("file_ref#{id} {p}");
        }
        FileCmd::List { group, session } => {
            let group_id = resolve_group(conn, &group)?;
            let session_id = resolve_session(conn, group_id, &session)?;
            for f in file_ref::list_for_session(conn, session_id)? {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    f.id,
                    f.role,
                    f.path,
                    f.created_by,
                    f.description.as_deref().unwrap_or("")
                );
            }
        }
        FileCmd::Read { id } => {
            let f = file_ref::get(conn, id)?;
            let content = std::fs::read_to_string(&f.path)
                .with_context(|| format!("cannot read {}", f.path))?;
            println!("# file_ref#{} {} ({})", f.id, f.path, f.role);
            println!();
            print!("{content}");
        }
        FileCmd::Remove { id } => {
            file_ref::detach(conn, id)?;
            println!("detached file_ref#{id}");
        }
    }
    Ok(())
}

pub(crate) fn handle_memory(conn: &Connection, action: MemoryCmd) -> anyhow::Result<()> {
    match action {
        MemoryCmd::Import {
            group,
            session,
            ext,
            role,
            path,
        } => {
            let group_id = resolve_group(conn, &group)?;
            let session_id = resolve_session(conn, group_id, &session)?;
            let meta = std::fs::metadata(&path)
                .with_context(|| format!("cannot stat {}", path.display()))?;
            if !meta.is_dir() {
                return Err(anyhow!("path is not a directory: {}", path.display()));
            }
            let want = ext.trim_start_matches('.');
            let by = client_label();
            let mut count = 0u64;
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                if p.extension().and_then(|e| e.to_str()) != Some(want) {
                    continue;
                }
                let s = canonical_path(&p)?;
                file_ref::attach(
                    conn,
                    &file_ref::NewFileRef {
                        session_id,
                        path: &s,
                        role: &role,
                        source: file_ref::SOURCE_MEMORY_IMPORT,
                        description: None,
                        created_by: &by,
                    },
                )?;
                count += 1;
                println!("+ {s}");
            }
            println!("imported {count} file(s) into session#{session_id} ({role})");
        }
    }
    Ok(())
}
