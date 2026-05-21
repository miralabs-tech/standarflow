PRAGMA foreign_keys = ON;

-- ───────────────────────────────────────────────────────────────────────────
-- Schema metadata — app-level key/value (schema version mirror, seed time).
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- ───────────────────────────────────────────────────────────────────────────
-- Conversations — the AI chat itself, identified by the provider's stable id
-- (Claude Code session UUID, etc.). Survives process restarts; PID columns are
-- diagnostic only and must never be used as an identity key.
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE conversations (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    provider                 TEXT    NOT NULL,
    provider_conversation_id TEXT    NOT NULL,
    client_label             TEXT,
    workspace_path           TEXT,
    transcript_path          TEXT,
    first_seen_at            INTEGER NOT NULL,
    last_seen_at             INTEGER NOT NULL,
    ended_at                 INTEGER,
    last_pid                 INTEGER,
    last_conversation_pid    INTEGER,
    UNIQUE (provider, provider_conversation_id)
);
CREATE INDEX idx_conversations_last_seen  ON conversations(last_seen_at);
CREATE INDEX idx_conversations_provider   ON conversations(provider);
CREATE INDEX idx_conversations_workspace  ON conversations(workspace_path)
    WHERE workspace_path IS NOT NULL;

-- ───────────────────────────────────────────────────────────────────────────
-- Groups — nestable namespaces for sessions. Slug is unique among root groups
-- and among siblings of the same parent.
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id   INTEGER REFERENCES groups(id) ON DELETE CASCADE,
    slug        TEXT    NOT NULL,
    title       TEXT,
    description TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    created_by  TEXT    NOT NULL,
    updated_by  TEXT
);
CREATE UNIQUE INDEX idx_groups_root_slug
    ON groups(slug) WHERE parent_id IS NULL;
CREATE UNIQUE INDEX idx_groups_child_slug
    ON groups(parent_id, slug) WHERE parent_id IS NOT NULL;
CREATE INDEX idx_groups_parent
    ON groups(parent_id) WHERE parent_id IS NOT NULL;

-- ───────────────────────────────────────────────────────────────────────────
-- Sessions — temporal containers (kind = session/adr/note/memory/…), nestable
-- via parent_session_id. body_md holds the markdown payload.
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE sessions (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id          INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    parent_session_id INTEGER REFERENCES sessions(id) ON DELETE CASCADE,
    slug              TEXT    NOT NULL,
    kind              TEXT    NOT NULL DEFAULT 'session',
    status            TEXT    NOT NULL DEFAULT 'active'
                          CHECK (status IN ('active', 'completed', 'superseded',
                                            'archived', 'paused')),
    title             TEXT,
    body_md           TEXT    NOT NULL DEFAULT '',
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    created_by        TEXT    NOT NULL,
    updated_by        TEXT,
    UNIQUE (group_id, slug)
);
CREATE INDEX idx_sessions_group       ON sessions(group_id);
CREATE INDEX idx_sessions_parent      ON sessions(parent_session_id)
    WHERE parent_session_id IS NOT NULL;
CREATE INDEX idx_sessions_kind        ON sessions(kind);
CREATE INDEX idx_sessions_status      ON sessions(status);
CREATE INDEX idx_sessions_created_at  ON sessions(created_at);
CREATE INDEX idx_sessions_updated_at  ON sessions(updated_at);

-- ───────────────────────────────────────────────────────────────────────────
-- Focus — one active session per conversation. pending_session_id holds a
-- focus request not yet confirmed by the conversation.
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE session_focus (
    conversation_id    INTEGER PRIMARY KEY
                           REFERENCES conversations(id) ON DELETE CASCADE,
    session_id         INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    pending_session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    focused_at         INTEGER NOT NULL,
    last_touched_at    INTEGER NOT NULL
);
CREATE INDEX idx_session_focus_session ON session_focus(session_id);

-- ───────────────────────────────────────────────────────────────────────────
-- File refs — links a session to a workspace file. role = semantic purpose,
-- source = how the ref was created.
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE file_refs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    role        TEXT    NOT NULL DEFAULT 'attachment',
    source      TEXT    NOT NULL DEFAULT 'manual'
                    CHECK (source IN ('manual', 'hook', 'memory_import')),
    description TEXT,
    created_at  INTEGER NOT NULL,
    created_by  TEXT    NOT NULL,
    UNIQUE (session_id, path)
);
CREATE INDEX idx_file_refs_session ON file_refs(session_id);
CREATE INDEX idx_file_refs_path    ON file_refs(path);
CREATE INDEX idx_file_refs_role    ON file_refs(role);

-- ───────────────────────────────────────────────────────────────────────────
-- Session links — typed session → session relations (continues, supersedes…).
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE session_links (
    from_id    INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    to_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    relation   TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    created_by TEXT    NOT NULL,
    PRIMARY KEY (from_id, to_id, relation)
);
CREATE INDEX idx_session_links_to       ON session_links(to_id);
CREATE INDEX idx_session_links_relation ON session_links(relation);

-- ───────────────────────────────────────────────────────────────────────────
-- Session participants — audit: which conversation touched which session.
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE session_participants (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    conversation_id INTEGER NOT NULL
                        REFERENCES conversations(id) ON DELETE CASCADE,
    first_touch_at  INTEGER NOT NULL,
    last_touch_at   INTEGER NOT NULL,
    touch_count     INTEGER NOT NULL DEFAULT 1,
    UNIQUE (session_id, conversation_id)
);
CREATE INDEX idx_participants_session ON session_participants(session_id);
CREATE INDEX idx_participants_conv    ON session_participants(conversation_id);

-- ───────────────────────────────────────────────────────────────────────────
-- Session file changes — audit: file mutations attributed to a session,
-- populated automatically from provider hooks (PostToolUse Edit/Write/…).
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE session_file_changes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    conversation_id INTEGER NOT NULL
                        REFERENCES conversations(id) ON DELETE CASCADE,
    file_path       TEXT    NOT NULL,
    op              TEXT    NOT NULL
                        CHECK (op IN ('create', 'edit', 'delete',
                                      'attach', 'detach')),
    kind            TEXT,
    tool_name       TEXT,
    ts              INTEGER NOT NULL
);
CREATE INDEX idx_file_changes_session ON session_file_changes(session_id);
CREATE INDEX idx_file_changes_conv    ON session_file_changes(conversation_id);
CREATE INDEX idx_file_changes_file    ON session_file_changes(file_path);
CREATE INDEX idx_file_changes_ts      ON session_file_changes(ts);

-- ───────────────────────────────────────────────────────────────────────────
-- Events — raw normalized event log from provider hooks, kept for replay and
-- debugging. payload_json holds the full NormalizedEvent blob.
-- ───────────────────────────────────────────────────────────────────────────
CREATE TABLE events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER REFERENCES conversations(id) ON DELETE SET NULL,
    provider        TEXT    NOT NULL,
    event_kind      TEXT    NOT NULL,
    ts              INTEGER NOT NULL,
    payload_json    TEXT    NOT NULL
);
CREATE INDEX idx_events_conv ON events(conversation_id, ts);
CREATE INDEX idx_events_kind ON events(event_kind, ts);

INSERT INTO schema_meta (key, value) VALUES ('schema_version', '1');
