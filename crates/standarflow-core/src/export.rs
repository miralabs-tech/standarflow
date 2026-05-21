#![allow(clippy::format_push_string)]

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::Result;
use crate::pipeline::event;
use crate::store::{conversation, file_change, file_ref, group, link, participant, session};
use crate::util::{home_dir, now_unix, STANDARFLOW_DIR};

#[derive(Debug, Clone)]
pub struct ExportReport {
    pub out_dir: PathBuf,
    pub groups: usize,
    pub sessions: usize,
    pub conversations: usize,
    pub events: usize,
}

/// Default export directory: `<home>/.standarflow/exports/export-<unix>`.
#[must_use]
pub fn default_export_dir() -> Option<PathBuf> {
    home_dir().map(|h| {
        h.join(STANDARFLOW_DIR)
            .join("exports")
            .join(format!("export-{}", now_unix()))
    })
}

/// Snapshot the whole database into a browsable directory tree: one markdown
/// file per session (YAML frontmatter + body), per-session file-change logs,
/// and global JSONL bundles for conversations / events / links.
pub fn export(conn: &Connection, out_dir: &Path) -> Result<ExportReport> {
    std::fs::create_dir_all(out_dir)?;

    let groups = group::list_all(conn)?;
    let sessions = session::list_all(conn)?;
    let conversations = conversation::list(conn, None)?;
    let events = event::list_all(conn)?;
    let links = link::list_all(conn)?;

    let path_of: HashMap<i64, String> = groups
        .iter()
        .map(|g| (g.id, group_path(&groups, g.id)))
        .collect();

    let groups_root = out_dir.join("groups");
    for g in &groups {
        let dir = groups_root.join(path_of.get(&g.id).cloned().unwrap_or_default());
        std::fs::create_dir_all(&dir)?;
        write_group_md(&dir, g)?;
    }

    let mut children: HashMap<Option<i64>, Vec<&session::Session>> = HashMap::new();
    for s in &sessions {
        children.entry(s.parent_session_id).or_default().push(s);
    }

    for g in &groups {
        let dir = groups_root.join(path_of.get(&g.id).cloned().unwrap_or_default());
        for s in sessions
            .iter()
            .filter(|s| s.group_id == g.id && s.parent_session_id.is_none())
        {
            write_session_tree(conn, &dir, s, &children, &path_of)?;
        }
    }

    write_jsonl(&out_dir.join("conversations.jsonl"), &conversations)?;
    write_jsonl(&out_dir.join("links.jsonl"), &links)?;
    {
        // events.payload_json is already a JSON line.
        let mut f = std::fs::File::create(out_dir.join("events.jsonl"))?;
        for e in &events {
            writeln!(f, "{}", e.payload_json)?;
        }
    }

    write_manifest(
        out_dir,
        groups.len(),
        sessions.len(),
        conversations.len(),
        events.len(),
    )?;

    Ok(ExportReport {
        out_dir: out_dir.to_path_buf(),
        groups: groups.len(),
        sessions: sessions.len(),
        conversations: conversations.len(),
        events: events.len(),
    })
}

fn group_path(groups: &[group::Group], id: i64) -> String {
    let mut parts = Vec::new();
    let mut cur = Some(id);
    while let Some(cid) = cur {
        let Some(g) = groups.iter().find(|g| g.id == cid) else {
            break;
        };
        parts.push(g.slug.clone());
        cur = g.parent_id;
    }
    parts.reverse();
    parts.join("/")
}

fn write_session_tree(
    conn: &Connection,
    dir: &Path,
    s: &session::Session,
    children: &HashMap<Option<i64>, Vec<&session::Session>>,
    path_of: &HashMap<i64, String>,
) -> Result<()> {
    write_session_md(conn, dir, s, path_of)?;
    if let Some(kids) = children.get(&Some(s.id)) {
        if !kids.is_empty() {
            let sub = dir.join(&s.slug);
            std::fs::create_dir_all(&sub)?;
            for k in kids {
                write_session_tree(conn, &sub, k, children, path_of)?;
            }
        }
    }
    Ok(())
}

fn write_session_md(
    conn: &Connection,
    dir: &Path,
    s: &session::Session,
    path_of: &HashMap<i64, String>,
) -> Result<()> {
    let file_refs = file_ref::list_for_session(conn, s.id)?;
    let participants = participant::list_for_session(conn, s.id)?;
    let out_links = link::outgoing(conn, s.id, None)?;
    let in_links = link::incoming(conn, s.id, None)?;
    let changes = file_change::list_all_for_session(conn, s.id)?;

    let mut fm = String::new();
    fm.push_str("---\n");
    fm.push_str(&format!("id: {}\n", s.id));
    fm.push_str(&format!(
        "group: {}\n",
        yaml_str(path_of.get(&s.group_id).map_or("", String::as_str))
    ));
    fm.push_str(&format!("slug: {}\n", yaml_str(&s.slug)));
    fm.push_str(&format!("kind: {}\n", yaml_str(&s.kind)));
    fm.push_str(&format!("status: {}\n", yaml_str(&s.status)));
    fm.push_str(&format!("title: {}\n", opt_yaml_str(s.title.as_deref())));
    fm.push_str(&format!(
        "parent_session_id: {}\n",
        opt_i64(s.parent_session_id)
    ));
    fm.push_str(&format!("created_at: {}\n", s.created_at));
    fm.push_str(&format!("updated_at: {}\n", s.updated_at));
    fm.push_str(&format!("created_by: {}\n", yaml_str(&s.created_by)));
    fm.push_str(&format!(
        "updated_by: {}\n",
        opt_yaml_str(s.updated_by.as_deref())
    ));

    fm.push_str("file_refs:\n");
    for f in &file_refs {
        fm.push_str(&format!("  - path: {}\n", yaml_str(&f.path)));
        fm.push_str(&format!("    role: {}\n", yaml_str(&f.role)));
        fm.push_str(&format!("    source: {}\n", yaml_str(&f.source)));
    }
    fm.push_str("participants:\n");
    for p in &participants {
        fm.push_str(&format!("  - conversation_id: {}\n", p.conversation_id));
        fm.push_str(&format!("    touch_count: {}\n", p.touch_count));
        fm.push_str(&format!("    first_touch_at: {}\n", p.first_touch_at));
        fm.push_str(&format!("    last_touch_at: {}\n", p.last_touch_at));
    }
    fm.push_str("links_outgoing:\n");
    for l in &out_links {
        fm.push_str(&format!("  - to_id: {}\n", l.to_id));
        fm.push_str(&format!("    relation: {}\n", yaml_str(&l.relation)));
    }
    fm.push_str("links_incoming:\n");
    for l in &in_links {
        fm.push_str(&format!("  - from_id: {}\n", l.from_id));
        fm.push_str(&format!("    relation: {}\n", yaml_str(&l.relation)));
    }
    fm.push_str(&format!("file_changes_count: {}\n", changes.len()));
    fm.push_str("---\n\n");
    fm.push_str(&s.body_md);
    if !s.body_md.ends_with('\n') {
        fm.push('\n');
    }

    std::fs::write(dir.join(format!("{}.md", s.slug)), fm)?;

    if !changes.is_empty() {
        write_jsonl(
            &dir.join(format!("{}.file-changes.jsonl", s.slug)),
            &changes,
        )?;
    }
    Ok(())
}

fn write_group_md(dir: &Path, g: &group::Group) -> Result<()> {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("id: {}\n", g.id));
    s.push_str(&format!("slug: {}\n", yaml_str(&g.slug)));
    s.push_str(&format!("title: {}\n", opt_yaml_str(g.title.as_deref())));
    s.push_str(&format!(
        "description: {}\n",
        opt_yaml_str(g.description.as_deref())
    ));
    s.push_str(&format!("parent_id: {}\n", opt_i64(g.parent_id)));
    s.push_str(&format!("created_at: {}\n", g.created_at));
    s.push_str(&format!("created_by: {}\n", yaml_str(&g.created_by)));
    s.push_str("---\n");
    std::fs::write(dir.join("_group.md"), s)?;
    Ok(())
}

fn write_manifest(
    out: &Path,
    groups: usize,
    sessions: usize,
    conversations: usize,
    events: usize,
) -> Result<()> {
    let mut m = String::new();
    m.push_str("# standarflow export\n\n");
    m.push_str("- schema_version: 1\n");
    m.push_str(&format!("- exported_at_unix: {}\n", now_unix()));
    m.push_str(&format!("- groups: {groups}\n"));
    m.push_str(&format!("- sessions: {sessions}\n"));
    m.push_str(&format!("- conversations: {conversations}\n"));
    m.push_str(&format!("- events: {events}\n\n"));
    m.push_str("## Layout\n\n");
    m.push_str("- `groups/<path>/_group.md` — group metadata\n");
    m.push_str("- `groups/<path>/<slug>.md` — session (YAML frontmatter + body)\n");
    m.push_str("- `groups/<path>/<slug>/` — nested sub-sessions\n");
    m.push_str("- `groups/<path>/<slug>.file-changes.jsonl` — per-session change audit\n");
    m.push_str("- `conversations.jsonl` / `events.jsonl` / `links.jsonl` — global bundles\n\n");
    m.push_str("## Schema (V1)\n\n```sql\n");
    m.push_str(include_str!("../migrations/V1__init.sql"));
    m.push_str("```\n");
    std::fs::write(out.join("MANIFEST.md"), m)?;
    Ok(())
}

fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    let mut f = std::fs::File::create(path)?;
    for r in rows {
        writeln!(f, "{}", serde_json::to_string(r)?)?;
    }
    Ok(())
}

/// Render a string as a double-quoted scalar (valid in both JSON and YAML).
fn yaml_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn opt_yaml_str(s: Option<&str>) -> String {
    s.map_or_else(|| "null".to_string(), yaml_str)
}

fn opt_i64(v: Option<i64>) -> String {
    v.map_or_else(|| "null".to_string(), |n| n.to_string())
}
