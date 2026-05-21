use std::io::Read;

use anyhow::{anyhow, Context};
use clap::Subcommand;
use standarflow_core::{store::{group, link, session}, Connection};

use super::client_label;
use crate::common::{resolve_group, resolve_session};

#[derive(Subcommand)]
pub(crate) enum GroupCmd {
    Create {
        slug: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        parent: Option<String>,
    },
    List {
        #[arg(long)]
        parent: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum SessionCmd {
    Save {
        #[arg(long)]
        group: String,
        #[arg(long)]
        slug: String,
        #[arg(long, default_value = session::KIND_SESSION)]
        kind: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        continues: Option<String>,
        body: Option<String>,
    },
    Get {
        #[arg(long)]
        group: String,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long, default_value = session::KIND_SESSION)]
        kind: String,
    },
    List {
        #[arg(long)]
        group: String,
        #[arg(long)]
        pattern: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum LinkCmd {
    Add {
        from: i64,
        to: i64,
        relation: String,
    },
    Remove {
        from: i64,
        to: i64,
        relation: String,
    },
    Of {
        id: i64,
    },
}

fn read_body(arg: Option<String>) -> anyhow::Result<String> {
    match arg {
        Some(s) if s != "-" => Ok(s),
        _ => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

pub(crate) fn handle_group(conn: &Connection, action: GroupCmd) -> anyhow::Result<()> {
    match action {
        GroupCmd::Create {
            slug,
            title,
            description,
            parent,
        } => {
            let parent_id = match parent {
                Some(p) => Some(resolve_group(conn, &p)?),
                None => None,
            };
            let by = client_label();
            let id = group::create(
                conn,
                &group::NewGroup {
                    parent_id,
                    slug: &slug,
                    title: title.as_deref(),
                    description: description.as_deref(),
                    created_by: &by,
                },
            )?;
            println!("group#{id} {slug}");
        }
        GroupCmd::List { parent } => {
            let parent_id = match parent {
                Some(p) => Some(resolve_group(conn, &p)?),
                None => None,
            };
            for g in group::list_children(conn, parent_id)? {
                println!(
                    "{}\t{}\t{}\t{}",
                    g.id,
                    g.slug,
                    g.title.as_deref().unwrap_or(""),
                    g.created_by
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_session(conn: &Connection, action: SessionCmd) -> anyhow::Result<()> {
    match action {
        SessionCmd::Save {
            group,
            slug,
            kind,
            title,
            parent,
            continues,
            body,
        } => {
            let group_id = resolve_group(conn, &group)?;
            let parent_session_id = match parent {
                Some(p) => Some(resolve_session(conn, group_id, &p)?),
                None => None,
            };
            let body_md = read_body(body)?;
            let by = client_label();
            let id = session::create(
                conn,
                &session::NewSession {
                    group_id,
                    parent_session_id,
                    slug: &slug,
                    kind: &kind,
                    title: title.as_deref(),
                    body_md: &body_md,
                    created_by: &by,
                },
            )?;
            if let Some(prev_slug) = continues {
                let prev_id = resolve_session(conn, group_id, &prev_slug)?;
                link::add(conn, id, prev_id, link::REL_CONTINUES, &by)?;
                session::set_status(conn, prev_id, "superseded", Some(&by))?;
            }
            println!("session#{id} {slug}");
        }
        SessionCmd::Get { group, slug, kind } => {
            let group_id = resolve_group(conn, &group)?;
            let s = match slug {
                Some(s) => session::find_by_slug(conn, group_id, &s)?
                    .ok_or_else(|| anyhow!("session not found: {s}"))?,
                None => session::latest_in_group(conn, group_id, &kind)?
                    .context("no session in group")?,
            };
            println!(
                "# {} (#{}, kind={}, status={}, by={})",
                s.slug, s.id, s.kind, s.status, s.created_by
            );
            println!();
            println!("{}", s.body_md);
        }
        SessionCmd::List { group, pattern } => {
            let group_id = resolve_group(conn, &group)?;
            let rows = match pattern {
                Some(p) => session::find_by_pattern(conn, group_id, &p)?,
                None => session::list_in_group(conn, group_id)?,
            };
            for s in rows {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    s.id, s.kind, s.status, s.slug, s.created_by, s.created_at
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_link(conn: &Connection, action: LinkCmd) -> anyhow::Result<()> {
    match action {
        LinkCmd::Add { from, to, relation } => {
            link::add(conn, from, to, &relation, &client_label())?;
            println!("linked {from} -[{relation}]-> {to}");
        }
        LinkCmd::Remove { from, to, relation } => {
            link::remove(conn, from, to, &relation)?;
            println!("removed {from} -[{relation}]-> {to}");
        }
        LinkCmd::Of { id } => {
            println!("# outgoing");
            for l in link::outgoing(conn, id, None)? {
                println!("{} -[{}]-> {}", l.from_id, l.relation, l.to_id);
            }
            println!("# incoming");
            for l in link::incoming(conn, id, None)? {
                println!("{} -[{}]-> {}", l.from_id, l.relation, l.to_id);
            }
        }
    }
    Ok(())
}
