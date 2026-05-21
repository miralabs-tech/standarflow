use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::util::now_unix;

pub const KIND_SESSION: &str = "session";

#[derive(Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub group_id: i64,
    pub parent_session_id: Option<i64>,
    pub slug: String,
    pub kind: String,
    pub status: String,
    pub title: Option<String>,
    pub body_md: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: String,
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewSession<'a> {
    pub group_id: i64,
    pub parent_session_id: Option<i64>,
    pub slug: &'a str,
    pub kind: &'a str,
    pub title: Option<&'a str>,
    pub body_md: &'a str,
    pub created_by: &'a str,
}

/// Patch for `update`. Each field is `None` when it should be left untouched.
///
/// `parent_session_id` and `title` are doubly wrapped to differentiate "leave
/// alone" (`None`) from "clear to NULL" (`Some(None)`) and "set" (`Some(Some)`).
#[derive(Debug, Clone, Default)]
pub struct SessionPatch<'a> {
    pub body_md: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub status: Option<&'a str>,
    pub title: Option<Option<&'a str>>,
    pub parent_session_id: Option<Option<i64>>,
    pub new_group_id: Option<i64>,
    pub new_slug: Option<&'a str>,
    pub updated_by: Option<&'a str>,
}

const SELECT_COLS: &str = "id, group_id, parent_session_id, slug, kind, status, title, \
                           body_md, created_at, updated_at, created_by, updated_by";

pub fn create(conn: &Connection, new: &NewSession<'_>) -> Result<i64> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO sessions
         (group_id, parent_session_id, slug, kind, status, title, body_md,
          created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?7, ?8, NULL)",
        params![
            new.group_id,
            new.parent_session_id,
            new.slug,
            new.kind,
            new.title,
            new.body_md,
            now,
            new.created_by
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> Result<Session> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM sessions WHERE id = ?1"),
        params![id],
        map_row,
    )
    .map_err(Error::from_lookup)
}

pub fn find_by_slug(conn: &Connection, group_id: i64, slug: &str) -> Result<Option<Session>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM sessions WHERE group_id = ?1 AND slug = ?2"),
        params![group_id, slug],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

pub fn latest_in_group(conn: &Connection, group_id: i64, kind: &str) -> Result<Option<Session>> {
    conn.query_row(
        &format!(
            "SELECT {SELECT_COLS} FROM sessions
             WHERE group_id = ?1 AND kind = ?2 AND status = 'active'
             ORDER BY created_at DESC LIMIT 1"
        ),
        params![group_id, kind],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

pub fn list_in_group(conn: &Connection, group_id: i64) -> Result<Vec<Session>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM sessions
             WHERE group_id = ?1 ORDER BY created_at DESC"
        ))?
        .query_map(params![group_id], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find_by_pattern(conn: &Connection, group_id: i64, pattern: &str) -> Result<Vec<Session>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM sessions
             WHERE group_id = ?1 AND (slug GLOB ?2 OR kind GLOB ?2)
             ORDER BY created_at DESC"
        ))?
        .query_map(params![group_id, pattern], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_children(conn: &Connection, parent_session_id: i64) -> Result<Vec<Session>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM sessions
             WHERE parent_session_id = ?1 ORDER BY created_at"
        ))?
        .query_map(params![parent_session_id], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every session in the database, ordered by group then creation time. Used by
/// the export pipeline.
pub fn list_all(conn: &Connection) -> Result<Vec<Session>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM sessions ORDER BY group_id, created_at"
        ))?
        .query_map([], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_status(
    conn: &Connection,
    id: i64,
    status: &str,
    updated_by: Option<&str>,
) -> Result<()> {
    let now = now_unix();
    let n = conn.execute(
        "UPDATE sessions SET status = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
        params![status, now, updated_by, id],
    )?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

/// Apply a partial update to a session. Untouched fields keep their values.
/// `updated_at` is always set to now.
pub fn update(conn: &Connection, id: i64, patch: &SessionPatch<'_>) -> Result<()> {
    let _existing = get(conn, id)?;

    let now = now_unix();
    let mut sets: Vec<&str> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(b) = patch.body_md {
        sets.push("body_md = ?");
        values.push(Box::new(b.to_string()));
    }
    if let Some(k) = patch.kind {
        sets.push("kind = ?");
        values.push(Box::new(k.to_string()));
    }
    if let Some(s) = patch.status {
        sets.push("status = ?");
        values.push(Box::new(s.to_string()));
    }
    if let Some(t) = patch.title {
        sets.push("title = ?");
        values.push(Box::new(t.map(std::string::ToString::to_string)));
    }
    if let Some(p) = patch.parent_session_id {
        sets.push("parent_session_id = ?");
        values.push(Box::new(p));
    }
    if let Some(g) = patch.new_group_id {
        sets.push("group_id = ?");
        values.push(Box::new(g));
    }
    if let Some(s) = patch.new_slug {
        sets.push("slug = ?");
        values.push(Box::new(s.to_string()));
    }

    sets.push("updated_at = ?");
    values.push(Box::new(now));
    sets.push("updated_by = ?");
    values.push(Box::new(
        patch.updated_by.map(std::string::ToString::to_string),
    ));

    let sql = format!("UPDATE sessions SET {} WHERE id = ?", sets.join(", "));
    values.push(Box::new(id));

    let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(std::convert::AsRef::as_ref).collect();
    let n = conn.execute(&sql, refs.as_slice())?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        group_id: row.get(1)?,
        parent_session_id: row.get(2)?,
        slug: row.get(3)?,
        kind: row.get(4)?,
        status: row.get(5)?,
        title: row.get(6)?,
        body_md: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        created_by: row.get(10)?,
        updated_by: row.get(11)?,
    })
}
