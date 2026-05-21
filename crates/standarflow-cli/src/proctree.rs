//! Process-tree based conversation scoping.
//!
//! Claude Code (and similar agent harnesses) exposes no env-level identifier
//! per conversation. The only stable per-conversation discriminator is the
//! process tree itself: each chat spawns its own `claude.exe` (or `cursor`,
//! …) which spawns the standarflow MCP server and the `PostToolUse` hook
//! subprocesses. By walking up `parent_pid` from `std::process::id()` until
//! we find an agent-named process, we get a stable per-conversation id.
//!
//! See `docs/automation.md` (section "Conversation scoping") for the broader
//! rationale; this module is the implementation.

use std::collections::HashSet;
use std::sync::OnceLock;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Process names (lowercased, with `.exe` stripped) we consider to be agent
/// roots. Walking the parent chain stops at the first one found.
const AGENT_ROOT_NAMES: &[&str] = &["claude", "cursor"];

const MAX_PARENT_HOPS: u32 = 32;

static CACHED_AGENT_ROOT_PID: OnceLock<Option<u32>> = OnceLock::new();

fn normalize_name(raw: &str) -> String {
    let lower = raw.to_lowercase();
    lower
        .strip_suffix(".exe")
        .map(str::to_string)
        .unwrap_or(lower)
}

fn is_agent_root(name: &str) -> bool {
    let norm = normalize_name(name);
    AGENT_ROOT_NAMES.contains(&norm.as_str())
}

/// Walk up from `std::process::id()` and return the PID of the first agent
/// root (`claude*` / `cursor*`) found. Returns `None` when no agent ancestor
/// exists (CLI invoked from a plain shell, `VSCode` extension MCP server, …).
///
/// Result is cached for the lifetime of the process: parents don't migrate
/// while we're alive, and walking the tree on every focus tool call would be
/// wasted work.
pub fn agent_root_pid() -> Option<u32> {
    *CACHED_AGENT_ROOT_PID.get_or_init(|| {
        let mut sys = System::new();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::new(),
        );

        let mut pid = std::process::id();
        for _ in 0..MAX_PARENT_HOPS {
            let p = sys.process(Pid::from_u32(pid))?;
            if is_agent_root(p.name().to_string_lossy().as_ref()) {
                return Some(pid);
            }
            match p.parent() {
                Some(parent) => pid = parent.as_u32(),
                None => return None,
            }
        }
        None
    })
}

/// PID we use to scope focus rows: `agent_root_pid` when running under an
/// agent, `0` otherwise (global / per-client scope).
pub fn conversation_pid() -> i64 {
    agent_root_pid().map_or(0, i64::from)
}

/// Walk up the parent chain and return the chain as a list of
/// `(pid, name)` pairs from self to root. Used by the `debug env` command
/// to make diagnostics actionable.
pub fn parent_chain() -> Vec<(u32, String)> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new(),
    );

    let mut chain = Vec::new();
    let mut pid = std::process::id();
    for _ in 0..MAX_PARENT_HOPS {
        let Some(p) = sys.process(Pid::from_u32(pid)) else {
            break;
        };
        chain.push((pid, p.name().to_string_lossy().to_string()));
        match p.parent() {
            Some(parent) => pid = parent.as_u32(),
            None => break,
        }
    }
    chain
}

/// PIDs of all live agent-root processes (`claude*` / `cursor*`). A
/// conversation is live iff its `last_conversation_pid` is in this set — a
/// closed chat's process is gone. Unlike `agent_root_pid` this is not cached:
/// liveness changes over the server's lifetime.
pub fn live_agent_pids() -> HashSet<u32> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new(),
    );
    sys.processes()
        .iter()
        .filter(|(_, p)| is_agent_root(p.name().to_string_lossy().as_ref()))
        .map(|(pid, _)| pid.as_u32())
        .collect()
}
