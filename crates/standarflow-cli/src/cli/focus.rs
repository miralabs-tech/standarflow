use anyhow::{anyhow, Context};
use clap::Subcommand;
use standarflow_core::{
    store::{conversation, focus, group, session},
    Connection,
};

use crate::common::{resolve_group, resolve_session};
use crate::proctree;

#[derive(Subcommand)]
pub(crate) enum FocusCmd {
    Set {
        #[arg(long)]
        group: String,
        #[arg(long)]
        slug: String,
        /// Conversation id to focus. Defaults to the conversation resolved
        /// from this process's agent ancestor.
        #[arg(long)]
        conversation: Option<i64>,
    },
    Clear {
        #[arg(long)]
        conversation: Option<i64>,
    },
    Current {
        #[arg(long)]
        conversation: Option<i64>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ConversationCmd {
    List,
    Get { id: i64 },
}

/// Resolve which conversation a focus operation targets: an explicit id, or
/// the conversation bound to this process's agent ancestor.
fn resolve_conversation_id(conn: &Connection, explicit: Option<i64>) -> anyhow::Result<i64> {
    if let Some(id) = explicit {
        conversation::get(conn, id).with_context(|| format!("no conversation#{id}"))?;
        return Ok(id);
    }
    let pid = proctree::agent_root_pid().ok_or_else(|| {
        anyhow!("no conversation context — run under an agent or pass --conversation <id>")
    })?;
    let c = conversation::find_by_agent_pid(conn, i64::from(pid))?.ok_or_else(|| {
        anyhow!("no conversation resolved for agent pid {pid} — pass --conversation <id>")
    })?;
    Ok(c.id)
}

pub(crate) fn handle_focus(conn: &Connection, action: FocusCmd) -> anyhow::Result<()> {
    match action {
        FocusCmd::Set {
            group,
            slug,
            conversation,
        } => {
            let conv_id = resolve_conversation_id(conn, conversation)?;
            let group_id = resolve_group(conn, &group)?;
            let session_id = resolve_session(conn, group_id, &slug)?;
            focus::set(conn, conv_id, session_id)?;
            println!("focused conversation#{conv_id} → {group}/{slug} (session#{session_id})");
        }
        FocusCmd::Clear { conversation } => {
            let conv_id = resolve_conversation_id(conn, conversation)?;
            focus::clear(conn, conv_id)?;
            println!("cleared focus for conversation#{conv_id}");
        }
        FocusCmd::Current { conversation } => {
            let conv_id = resolve_conversation_id(conn, conversation)?;
            if let Some(f) = focus::get(conn, conv_id)? {
                let s = session::get(conn, f.session_id)?;
                let g = group::get(conn, s.group_id)?;
                println!("{}\t{}", g.slug, s.slug);
            }
        }
    }
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn handle_conversation(
    conn: &Connection,
    action: ConversationCmd,
) -> anyhow::Result<()> {
    match action {
        ConversationCmd::List => {
            for c in conversation::list(conn, None)? {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    c.id,
                    c.provider,
                    c.provider_conversation_id,
                    c.workspace_path.as_deref().unwrap_or(""),
                    c.last_seen_at
                );
            }
        }
        ConversationCmd::Get { id } => {
            let c = conversation::get(conn, id)?;
            println!("conversation#{}", c.id);
            println!("provider: {}", c.provider);
            println!("provider_conversation_id: {}", c.provider_conversation_id);
            println!("workspace: {}", c.workspace_path.as_deref().unwrap_or(""));
            println!("transcript: {}", c.transcript_path.as_deref().unwrap_or(""));
            println!(
                "first_seen: {}  last_seen: {}",
                c.first_seen_at, c.last_seen_at
            );
            if let Some(e) = c.ended_at {
                println!("ended: {e}");
            }
        }
    }
    Ok(())
}
