use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use standarflow_core::{db, Connection};

mod cli;
mod common;
mod mcp;
mod proctree;

use cli::admin::{handle_db, handle_debug, handle_export, DbCmd, DebugCmd};
use cli::files::{handle_file, handle_memory, FileCmd, MemoryCmd};
use cli::focus::{handle_conversation, handle_focus, ConversationCmd, FocusCmd};
use cli::hooks::{cmd_ingest, handle_events, handle_hooks, EventsCmd, HooksCmd};
use cli::store::{handle_group, handle_link, handle_session, GroupCmd, LinkCmd, SessionCmd};

#[derive(Parser)]
#[command(
    name = "standarflow",
    version,
    about = "standarflow session & flow store"
)]
struct Cli {
    #[arg(long, env = "STANDARFLOW_DB", global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Group {
        #[command(subcommand)]
        action: GroupCmd,
    },
    Session {
        #[command(subcommand)]
        action: SessionCmd,
    },
    Focus {
        #[command(subcommand)]
        action: FocusCmd,
    },
    Conversation {
        #[command(subcommand)]
        action: ConversationCmd,
    },
    Link {
        #[command(subcommand)]
        action: LinkCmd,
    },
    File {
        #[command(subcommand)]
        action: FileCmd,
    },
    Memory {
        #[command(subcommand)]
        action: MemoryCmd,
    },
    /// Ingest a provider hook event from stdin. Never fails loudly.
    Ingest {
        #[arg(long, default_value = "claude-code")]
        provider: String,
    },
    Hooks {
        #[command(subcommand)]
        action: HooksCmd,
    },
    Events {
        #[command(subcommand)]
        action: EventsCmd,
    },
    /// Snapshot the whole database into a browsable markdown + JSONL tree.
    Export {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Db {
        #[command(subcommand)]
        action: DbCmd,
    },
    Debug {
        #[command(subcommand)]
        action: DebugCmd,
    },
    Mcp,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Ingest { provider } => {
            cmd_ingest(&provider);
            Ok(())
        }
        Command::Hooks { action } => handle_hooks(&action),
        Command::Db { action } => handle_db(&action, cli.db.as_deref()),
        other => {
            let path = match cli.db {
                Some(p) => p,
                None => db::default_path(&std::env::current_dir()?),
            };
            let conn = db::open(&path)?;
            dispatch_db(other, conn, &path)
        }
    }
}

fn dispatch_db(cmd: Command, conn: Connection, path: &Path) -> anyhow::Result<()> {
    match cmd {
        Command::Init => println!("standarflow ready at {}", path.display()),
        Command::Group { action } => handle_group(&conn, action)?,
        Command::Session { action } => handle_session(&conn, action)?,
        Command::Focus { action } => handle_focus(&conn, action)?,
        Command::Conversation { action } => handle_conversation(&conn, action)?,
        Command::Link { action } => handle_link(&conn, action)?,
        Command::File { action } => handle_file(&conn, action)?,
        Command::Memory { action } => handle_memory(&conn, action)?,
        Command::Events { action } => handle_events(&conn, path, action)?,
        Command::Export { out } => handle_export(&conn, out)?,
        Command::Debug { action } => handle_debug(action)?,
        Command::Mcp => {
            let db_path_str = path.display().to_string();
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(mcp::run(conn, db_path_str))?;
        }
        Command::Ingest { .. } | Command::Hooks { .. } | Command::Db { .. } => unreachable!(),
    }
    Ok(())
}
