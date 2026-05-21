use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::util::now_unix;

pub const ROLE_MEMORY: &str = "memory";
pub const ROLE_NOTE: &str = "note";
pub const ROLE_ATTACHMENT: &str = "attachment";
pub const ROLE_SOURCE: &str = "source";

pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_HOOK: &str = "hook";
pub const SOURCE_MEMORY_IMPORT: &str = "memory_import";

#[derive(Debug, Clone)]
pub struct FileRef {
    pub id: i64,
    pub session_id: i64,
    pub path: String,
    pub role: String,
    pub source: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Debug, Clone)]
pub struct NewFileRef<'a> {
    pub session_id: i64,
    pub path: &'a str,
    pub role: &'a str,
    pub source: &'a str,
    pub description: Option<&'a str>,
    pub created_by: &'a str,
}

pub fn attach(conn: &Connection, new: &NewFileRef<'_>) -> Result<i64> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO file_refs (session_id, path, role, source, description, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(session_id, path) DO UPDATE SET
            role        = excluded.role,
            source      = excluded.source,
            description = excluded.description,
            created_by  = excluded.created_by",
        params![
            new.session_id,
            new.path,
            new.role,
            new.source,
            new.description,
            now,
            new.created_by
        ],
    )?;
    let id = conn.query_row(
        "SELECT id FROM file_refs WHERE session_id = ?1 AND path = ?2",
        params![new.session_id, new.path],
        |r| r.get::<_, i64>(0),
    )?;
    Ok(id)
}

pub fn detach(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM file_refs WHERE id = ?1", params![id])?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

/// Re-assign the `created_by` of an existing `file_ref`.
pub fn claim(conn: &Connection, id: i64, new_by: &str) -> Result<()> {
    let n = conn.execute(
        "UPDATE file_refs SET created_by = ?1 WHERE id = ?2",
        params![new_by, id],
    )?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

/// Outcome of `delete_with_source` — the `file_ref` is always detached when the
/// call returns `Ok`, but the on-disk file may have already been gone.
#[derive(Debug, Clone)]
pub struct DeleteWithSourceOutcome {
    pub path: String,
    pub file_deleted: bool,
    pub file_was_missing: bool,
}

/// Delete the file on disk AND detach the `file_ref`. If the file doesn't exist,
/// detach still happens and `file_was_missing` is set.
pub fn delete_with_source(conn: &Connection, id: i64) -> Result<DeleteWithSourceOutcome> {
    let f = get(conn, id)?;
    let path = f.path.clone();
    let (file_deleted, file_was_missing) = match std::fs::remove_file(&path) {
        Ok(()) => (true, false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (false, true),
        Err(e) => return Err(e.into()),
    };
    detach(conn, id)?;
    Ok(DeleteWithSourceOutcome {
        path,
        file_deleted,
        file_was_missing,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<FileRef> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM file_refs WHERE id = ?1"),
        params![id],
        map_row,
    )
    .map_err(Error::from_lookup)
}

pub fn list_for_session(conn: &Connection, session_id: i64) -> Result<Vec<FileRef>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM file_refs WHERE session_id = ?1 ORDER BY created_at"
        ))?
        .query_map(params![session_id], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find_by_path(conn: &Connection, path: &str) -> Result<Vec<FileRef>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM file_refs WHERE path = ?1 ORDER BY created_at"
        ))?
        .query_map(params![path], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find_for_session_by_path(
    conn: &Connection,
    session_id: i64,
    path: &str,
) -> Result<Option<FileRef>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM file_refs WHERE session_id = ?1 AND path = ?2"),
        params![session_id, path],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

const SELECT_COLS: &str = "id, session_id, path, role, source, description, created_at, created_by";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<FileRef> {
    Ok(FileRef {
        id: row.get(0)?,
        session_id: row.get(1)?,
        path: row.get(2)?,
        role: row.get(3)?,
        source: row.get(4)?,
        description: row.get(5)?,
        created_at: row.get(6)?,
        created_by: row.get(7)?,
    })
}
