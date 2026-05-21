use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;
use crate::util::now_unix;

pub const REL_CONTINUES: &str = "continues";
pub const REL_SUPERSEDES: &str = "supersedes";
pub const REL_REFERENCES: &str = "references";
pub const REL_FIXES: &str = "fixes";
pub const REL_RELATES_TO: &str = "relates_to";

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub from_id: i64,
    pub to_id: i64,
    pub relation: String,
    pub created_at: i64,
    pub created_by: String,
}

pub fn add(
    conn: &Connection,
    from_id: i64,
    to_id: i64,
    relation: &str,
    created_by: &str,
) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "INSERT OR IGNORE INTO session_links (from_id, to_id, relation, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![from_id, to_id, relation, now, created_by],
    )?;
    Ok(())
}

pub fn remove(conn: &Connection, from_id: i64, to_id: i64, relation: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM session_links WHERE from_id = ?1 AND to_id = ?2 AND relation = ?3",
        params![from_id, to_id, relation],
    )?;
    Ok(())
}

pub fn outgoing(conn: &Connection, from_id: i64, relation: Option<&str>) -> Result<Vec<Link>> {
    let rows = match relation {
        Some(r) => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM session_links WHERE from_id = ?1 AND relation = ?2"
            ))?
            .query_map(params![from_id, r], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        None => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM session_links WHERE from_id = ?1"
            ))?
            .query_map(params![from_id], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

pub fn incoming(conn: &Connection, to_id: i64, relation: Option<&str>) -> Result<Vec<Link>> {
    let rows = match relation {
        Some(r) => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM session_links WHERE to_id = ?1 AND relation = ?2"
            ))?
            .query_map(params![to_id, r], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        None => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM session_links WHERE to_id = ?1"
            ))?
            .query_map(params![to_id], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// Every link in the database. Used by the export pipeline.
pub fn list_all(conn: &Connection) -> Result<Vec<Link>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM session_links ORDER BY from_id, to_id"
        ))?
        .query_map([], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

const SELECT_COLS: &str = "from_id, to_id, relation, created_at, created_by";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Link> {
    Ok(Link {
        from_id: row.get(0)?,
        to_id: row.get(1)?,
        relation: row.get(2)?,
        created_at: row.get(3)?,
        created_by: row.get(4)?,
    })
}
