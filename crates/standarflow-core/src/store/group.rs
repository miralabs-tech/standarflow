use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::util::now_unix;

#[derive(Debug, Clone)]
pub struct Group {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub slug: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: String,
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewGroup<'a> {
    pub parent_id: Option<i64>,
    pub slug: &'a str,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub created_by: &'a str,
}

/// Patch for `update`. `None` leaves a field untouched; `Some(None)` clears it
/// to NULL; `Some(Some(x))` sets it.
#[derive(Debug, Clone, Default)]
pub struct GroupPatch<'a> {
    pub title: Option<Option<&'a str>>,
    pub description: Option<Option<&'a str>>,
    pub updated_by: Option<&'a str>,
}

const SELECT_COLS: &str =
    "id, parent_id, slug, title, description, created_at, updated_at, created_by, updated_by";

pub fn create(conn: &Connection, new: &NewGroup<'_>) -> Result<i64> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO groups
           (parent_id, slug, title, description, created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, NULL)",
        params![
            new.parent_id,
            new.slug,
            new.title,
            new.description,
            now,
            new.created_by
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> Result<Group> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM groups WHERE id = ?1"),
        params![id],
        map_row,
    )
    .map_err(Error::from_lookup)
}

pub fn find_by_slug(
    conn: &Connection,
    slug: &str,
    parent_id: Option<i64>,
) -> Result<Option<Group>> {
    let row = match parent_id {
        Some(pid) => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM groups WHERE slug = ?1 AND parent_id = ?2"
            ))?
            .query_row(params![slug, pid], map_row)
            .optional()?,
        None => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM groups WHERE slug = ?1 AND parent_id IS NULL"
            ))?
            .query_row(params![slug], map_row)
            .optional()?,
    };
    Ok(row)
}

pub fn list_children(conn: &Connection, parent_id: Option<i64>) -> Result<Vec<Group>> {
    let rows = match parent_id {
        Some(pid) => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM groups WHERE parent_id = ?1 ORDER BY slug"
            ))?
            .query_map(params![pid], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        None => conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM groups WHERE parent_id IS NULL ORDER BY slug"
            ))?
            .query_map([], map_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

pub fn update(conn: &Connection, id: i64, patch: &GroupPatch<'_>) -> Result<()> {
    let _existing = get(conn, id)?;

    let now = now_unix();
    let mut sets: Vec<&str> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(t) = patch.title {
        sets.push("title = ?");
        values.push(Box::new(t.map(std::string::ToString::to_string)));
    }
    if let Some(d) = patch.description {
        sets.push("description = ?");
        values.push(Box::new(d.map(std::string::ToString::to_string)));
    }

    sets.push("updated_at = ?");
    values.push(Box::new(now));
    sets.push("updated_by = ?");
    values.push(Box::new(
        patch.updated_by.map(std::string::ToString::to_string),
    ));

    let sql = format!("UPDATE groups SET {} WHERE id = ?", sets.join(", "));
    values.push(Box::new(id));

    let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(std::convert::AsRef::as_ref).collect();
    let n = conn.execute(&sql, refs.as_slice())?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

/// Every group in the database, ordered by id. Used by the export pipeline.
pub fn list_all(conn: &Connection) -> Result<Vec<Group>> {
    let rows = conn
        .prepare(&format!("SELECT {SELECT_COLS} FROM groups ORDER BY id"))?
        .query_map([], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
    if n == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Group> {
    Ok(Group {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        slug: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        created_by: row.get(7)?,
        updated_by: row.get(8)?,
    })
}
