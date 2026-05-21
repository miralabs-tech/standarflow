use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;

/// Audit row: a file mutation attributed to a session, derived from a provider
/// hook (`PostToolUse` Edit/Write/…).
#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub id: i64,
    pub session_id: i64,
    pub conversation_id: i64,
    pub file_path: String,
    pub op: String,
    pub kind: Option<String>,
    pub tool_name: Option<String>,
    pub ts: i64,
}

#[derive(Debug, Clone)]
pub struct NewFileChange<'a> {
    pub session_id: i64,
    pub conversation_id: i64,
    pub file_path: &'a str,
    pub op: &'a str,
    pub kind: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub ts: i64,
}

pub fn log(conn: &Connection, new: &NewFileChange<'_>) -> Result<i64> {
    conn.execute(
        "INSERT INTO session_file_changes
           (session_id, conversation_id, file_path, op, kind, tool_name, ts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            new.session_id,
            new.conversation_id,
            new.file_path,
            new.op,
            new.kind,
            new.tool_name,
            new.ts
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_for_session(
    conn: &Connection,
    session_id: i64,
    limit: i64,
) -> Result<Vec<FileChange>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM session_file_changes
             WHERE session_id = ?1 ORDER BY ts DESC, id DESC LIMIT ?2"
        ))?
        .query_map(params![session_id, limit], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_for_conversation(
    conn: &Connection,
    conversation_id: i64,
    limit: i64,
) -> Result<Vec<FileChange>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM session_file_changes
             WHERE conversation_id = ?1 ORDER BY ts DESC, id DESC LIMIT ?2"
        ))?
        .query_map(params![conversation_id, limit], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every change row for a session, oldest first. Used by the export pipeline.
pub fn list_all_for_session(conn: &Connection, session_id: i64) -> Result<Vec<FileChange>> {
    let rows = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM session_file_changes
             WHERE session_id = ?1 ORDER BY ts, id"
        ))?
        .query_map(params![session_id], map_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Distinct file paths the session has touched whose most recent change is
/// not a `delete` — i.e. paths expected to still exist on disk. The tail uses
/// this to reconcile deletions: a tracked path that has since vanished.
pub fn live_paths_for_session(conn: &Connection, session_id: i64) -> Result<Vec<String>> {
    let rows = conn
        .prepare(
            "SELECT file_path FROM session_file_changes
             WHERE id IN (
               SELECT MAX(id) FROM session_file_changes
               WHERE session_id = ?1 GROUP BY file_path
             )
             AND op != 'delete'",
        )?
        .query_map(params![session_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Classify a file path into a coarse `kind` from its extension.
#[must_use] 
pub fn classify_kind(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "md" | "mdx" | "markdown" => "md",
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h"
        | "hpp" | "lua" | "rb" | "sh" | "ps1" | "css" | "scss" | "html" | "vue" | "svelte" => {
            "code"
        }
        "json" | "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "lock" | "env" => "config",
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => "asset",
        _ => "other",
    }
}

const SELECT_COLS: &str =
    "id, session_id, conversation_id, file_path, op, kind, tool_name, ts";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<FileChange> {
    Ok(FileChange {
        id: row.get(0)?,
        session_id: row.get(1)?,
        conversation_id: row.get(2)?,
        file_path: row.get(3)?,
        op: row.get(4)?,
        kind: row.get(5)?,
        tool_name: row.get(6)?,
        ts: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{mem_db, seed_conversation, seed_group, seed_session};

    fn log_change(conn: &Connection, session_id: i64, conv_id: i64, path: &str, op: &str, ts: i64) {
        log(
            conn,
            &NewFileChange {
                session_id,
                conversation_id: conv_id,
                file_path: path,
                op,
                kind: Some(classify_kind(path)),
                tool_name: None,
                ts,
            },
        )
        .expect("log file change");
    }

    #[test]
    fn live_paths_excludes_a_path_whose_latest_change_is_a_delete() {
        let conn = mem_db();
        let g = seed_group(&conn, "g");
        let s = seed_session(&conn, g, "s");
        let c = seed_conversation(&conn, "conv-1");

        log_change(&conn, s, c, "src/keep.rs", "create", 1);
        log_change(&conn, s, c, "src/keep.rs", "edit", 2);
        log_change(&conn, s, c, "src/gone.rs", "create", 3);
        log_change(&conn, s, c, "src/gone.rs", "delete", 4);

        let live = live_paths_for_session(&conn, s).expect("live paths");
        assert_eq!(live, vec!["src/keep.rs".to_string()]);
    }

    #[test]
    fn live_paths_includes_a_path_recreated_after_a_delete() {
        let conn = mem_db();
        let g = seed_group(&conn, "g");
        let s = seed_session(&conn, g, "s");
        let c = seed_conversation(&conn, "conv-1");

        log_change(&conn, s, c, "src/phoenix.rs", "create", 1);
        log_change(&conn, s, c, "src/phoenix.rs", "delete", 2);
        log_change(&conn, s, c, "src/phoenix.rs", "create", 3);

        let live = live_paths_for_session(&conn, s).expect("live paths");
        assert_eq!(live, vec!["src/phoenix.rs".to_string()]);
    }

    #[test]
    fn live_paths_are_scoped_to_their_session() {
        let conn = mem_db();
        let g = seed_group(&conn, "g");
        let s1 = seed_session(&conn, g, "s1");
        let s2 = seed_session(&conn, g, "s2");
        let c = seed_conversation(&conn, "conv-1");

        log_change(&conn, s1, c, "a.rs", "create", 1);
        log_change(&conn, s2, c, "b.rs", "create", 2);

        let live = live_paths_for_session(&conn, s1).expect("live paths");
        assert_eq!(live, vec!["a.rs".to_string()]);
    }
}
