use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use crate::error::Result;

pub const DEFAULT_DB_FILE: &str = "standarflow.db";

#[must_use] 
pub fn default_path(workspace: &Path) -> PathBuf {
    workspace.join(crate::util::STANDARFLOW_DIR).join(DEFAULT_DB_FILE)
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&mut conn)?;
    Ok(conn)
}

pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let migrations = Migrations::new(vec![M::up(include_str!("../migrations/V1__init.sql"))]);
    migrations.to_latest(conn)?;
    Ok(())
}
