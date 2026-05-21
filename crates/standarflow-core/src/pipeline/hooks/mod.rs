use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

const CLAUDE_CODE_TEMPLATE: &str = include_str!("templates/claude-code.json");
const GENERIC_TEMPLATE: &str = include_str!("templates/generic.json");

/// A provider's hook installation recipe, loaded from an embedded template.
#[derive(Debug, Clone, Deserialize)]
pub struct HookTemplate {
    pub provider: String,
    /// User-scope settings file to patch, `~`-relative. `None` when the
    /// provider has no settings file (the generic provider).
    pub settings_file: Option<String>,
    /// Workspace-relative settings file for the `project-local` scope. `None`
    /// when the provider has no per-project settings.
    #[serde(default)]
    pub local_settings_file: Option<String>,
    /// Hook event names to register.
    pub events: Vec<String>,
    #[serde(default)]
    pub instructions: Option<String>,
}

/// Which settings file the ingest hooks land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// `~/.claude/settings.json` — captures every workspace on this machine.
    User,
    /// `<root>/.claude/settings.local.json` — this workspace only. Git-ignored
    /// by Claude Code convention, so the machine-specific binary path it
    /// embeds never reaches the repo.
    ProjectLocal,
}

impl Scope {
    /// Every scope, in display order.
    pub const ALL: [Scope; 2] = [Scope::User, Scope::ProjectLocal];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::ProjectLocal => "project-local",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Scope> {
        match s {
            "user" => Some(Scope::User),
            "project-local" => Some(Scope::ProjectLocal),
            _ => None,
        }
    }

    /// Resolve the settings file this scope patches. `root` is the workspace
    /// root, consulted only for `ProjectLocal`. `None` when the provider has
    /// no settings file for this scope.
    fn settings_path(self, tpl: &HookTemplate, root: &Path) -> Result<Option<PathBuf>> {
        match self {
            Scope::User => match &tpl.settings_file {
                Some(rel) => Ok(Some(expand_home(rel)?)),
                None => Ok(None),
            },
            Scope::ProjectLocal => Ok(tpl.local_settings_file.as_deref().map(|rel| root.join(rel))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    pub provider: String,
    pub scope: Scope,
    pub settings_file: Option<PathBuf>,
    pub events_added: Vec<String>,
    pub events_already_present: Vec<String>,
    pub backup_path: Option<PathBuf>,
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UninstallReport {
    pub provider: String,
    pub scope: Scope,
    pub settings_file: Option<PathBuf>,
    pub events_removed: Vec<String>,
    pub backup_path: Option<PathBuf>,
}

/// One scope's slice of a `status` report.
#[derive(Debug, Clone, Serialize)]
pub struct ScopeStatus {
    pub scope: Scope,
    pub settings_file: Option<PathBuf>,
    pub installed_events: Vec<String>,
    pub missing_events: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HookStatus {
    pub provider: String,
    /// One entry per scope, in `Scope::ALL` order.
    pub scopes: Vec<ScopeStatus>,
}

/// Load the embedded hook template for a provider.
#[must_use]
pub fn template_for(provider: &str) -> Option<HookTemplate> {
    let raw = match provider {
        "claude-code" => CLAUDE_CODE_TEMPLATE,
        "generic" => GENERIC_TEMPLATE,
        _ => return None,
    };
    serde_json::from_str(raw).ok()
}

/// Load the embedded template, erroring uniformly when the provider is unknown.
fn require_template(provider: &str) -> Result<HookTemplate> {
    template_for(provider).ok_or_else(|| Error::Invalid(format!("unknown provider: {provider}")))
}

/// Install standarflow ingest hooks for a provider by patching the settings
/// file for `scope`. Idempotent: events already wired are left alone. Backs
/// the file up to `<name>.json.bak` before writing, and only writes when
/// something changed. `exe` is the absolute path of the standarflow binary;
/// `root` is the workspace root, used only for the `project-local` scope.
pub fn install(provider: &str, exe: &Path, scope: Scope, root: &Path) -> Result<InstallReport> {
    let tpl = require_template(provider)?;

    let Some(settings_path) = scope.settings_path(&tpl, root)? else {
        return Ok(InstallReport {
            provider: tpl.provider,
            scope,
            settings_file: None,
            events_added: Vec::new(),
            events_already_present: Vec::new(),
            backup_path: None,
            instructions: tpl.instructions,
        });
    };

    let command = build_command(exe, &tpl.provider);
    let marker = command_marker(&tpl.provider);

    let mut doc = read_json_or_empty(&settings_path)?;
    let mut added = Vec::new();
    let mut present = Vec::new();
    {
        let obj = doc
            .as_object_mut()
            .ok_or_else(|| Error::Invalid("settings.json root is not an object".into()))?;
        let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
        let hooks_obj = hooks
            .as_object_mut()
            .ok_or_else(|| Error::Invalid("settings.json `hooks` is not an object".into()))?;

        for event in &tpl.events {
            let entry = hooks_obj
                .entry(event.clone())
                .or_insert_with(|| Value::Array(Vec::new()));
            let arr = entry.as_array_mut().ok_or_else(|| {
                Error::Invalid(format!("settings.json hooks.{event} is not an array"))
            })?;
            if event_has_marker(arr, &marker) {
                present.push(event.clone());
            } else {
                arr.push(new_hook_group(&command));
                added.push(event.clone());
            }
        }
    }

    let backup_path = commit_settings(&settings_path, &doc, !added.is_empty())?;

    Ok(InstallReport {
        provider: tpl.provider,
        scope,
        settings_file: Some(settings_path),
        events_added: added,
        events_already_present: present,
        backup_path,
        instructions: tpl.instructions,
    })
}

/// Remove standarflow ingest hooks for a provider from the `scope` settings
/// file, leaving any other hooks the user configured untouched.
pub fn uninstall(provider: &str, scope: Scope, root: &Path) -> Result<UninstallReport> {
    let tpl = require_template(provider)?;

    let Some(settings_path) = scope.settings_path(&tpl, root)? else {
        return Ok(UninstallReport {
            provider: tpl.provider,
            scope,
            settings_file: None,
            events_removed: Vec::new(),
            backup_path: None,
        });
    };

    if !settings_path.exists() {
        return Ok(UninstallReport {
            provider: tpl.provider,
            scope,
            settings_file: Some(settings_path),
            events_removed: Vec::new(),
            backup_path: None,
        });
    }

    let marker = command_marker(&tpl.provider);
    let mut doc = read_json_or_empty(&settings_path)?;
    let mut removed = Vec::new();

    if let Some(hooks_obj) = doc.get_mut("hooks").and_then(Value::as_object_mut) {
        let event_names: Vec<String> = hooks_obj.keys().cloned().collect();
        for event in event_names {
            let Some(arr) = hooks_obj.get_mut(&event).and_then(Value::as_array_mut) else {
                continue;
            };
            if event_has_marker(arr, &marker) {
                removed.push(event.clone());
            }
            for group in arr.iter_mut() {
                if let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                    hooks.retain(|h| !hook_matches(h, &marker));
                }
            }
            arr.retain(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_none_or(|h| !h.is_empty())
            });
        }
        hooks_obj.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
    }

    let backup_path = commit_settings(&settings_path, &doc, !removed.is_empty())?;

    Ok(UninstallReport {
        provider: tpl.provider,
        scope,
        settings_file: Some(settings_path),
        events_removed: removed,
        backup_path,
    })
}

/// Report which hook events are wired for a provider, across every scope.
pub fn status(provider: &str, root: &Path) -> Result<HookStatus> {
    let tpl = require_template(provider)?;

    let scopes = Scope::ALL
        .iter()
        .map(|&scope| scope_status(&tpl, scope, root))
        .collect::<Result<Vec<_>>>()?;

    Ok(HookStatus {
        provider: tpl.provider,
        scopes,
    })
}

/// Inspect a single scope's settings file for the provider's ingest marker.
fn scope_status(tpl: &HookTemplate, scope: Scope, root: &Path) -> Result<ScopeStatus> {
    let settings_file = scope.settings_path(tpl, root)?;
    let marker = command_marker(&tpl.provider);
    let doc = match &settings_file {
        Some(path) => read_json_or_empty(path)?,
        None => serde_json::json!({}),
    };
    let hooks = doc.get("hooks").and_then(Value::as_object);

    let mut installed = Vec::new();
    let mut missing = Vec::new();
    for event in &tpl.events {
        let present = hooks
            .and_then(|h| h.get(event))
            .and_then(Value::as_array)
            .is_some_and(|arr| event_has_marker(arr, &marker));
        if present {
            installed.push(event.clone());
        } else {
            missing.push(event.clone());
        }
    }

    Ok(ScopeStatus {
        scope,
        settings_file,
        installed_events: installed,
        missing_events: missing,
    })
}

fn command_marker(provider: &str) -> String {
    format!("ingest --provider {provider}")
}

fn build_command(exe: &Path, provider: &str) -> String {
    // Always quote: Claude Code runs hook commands through a POSIX shell, which
    // strips the backslashes from an unquoted Windows path.
    let exe_str = exe.display().to_string();
    format!("\"{exe_str}\" ingest --provider {provider}")
}

fn new_hook_group(command: &str) -> Value {
    serde_json::json!({
        "matcher": "*",
        "hooks": [
            { "type": "command", "command": command }
        ]
    })
}

fn hook_matches(hook: &Value, marker: &str) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains(marker))
}

fn event_has_marker(groups: &[Value], marker: &str) -> bool {
    groups.iter().any(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| hooks.iter().any(|h| hook_matches(h, marker)))
    })
}

fn read_json_or_empty(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(serde_json::json!({})),
        Ok(s) => Ok(serde_json::from_str(&s)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(e) => Err(e.into()),
    }
}

fn backup_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let bak = path.with_extension("json.bak");
    std::fs::copy(path, &bak)?;
    Ok(Some(bak))
}

fn write_json(path: &Path, v: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(v)?)?;
    Ok(())
}

/// Back the settings file up and write the mutated document — but only when
/// `changed` is true. Returns the backup path, or `None` when nothing changed.
fn commit_settings(path: &Path, doc: &Value, changed: bool) -> Result<Option<PathBuf>> {
    if !changed {
        return Ok(None);
    }
    let bak = backup_file(path)?;
    write_json(path, doc)?;
    Ok(bak)
}

fn expand_home(p: &str) -> Result<PathBuf> {
    if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        let home = crate::util::home_dir()
            .ok_or_else(|| Error::Invalid("cannot resolve home directory".into()))?;
        Ok(home.join(rest))
    } else {
        Ok(PathBuf::from(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn build_command_always_quotes_the_exe_path() {
        let cmd = build_command(Path::new(r"c:\Users\me\bin\standarflow.exe"), "claude-code");
        assert_eq!(
            cmd,
            r#""c:\Users\me\bin\standarflow.exe" ingest --provider claude-code"#
        );

        let spaced = build_command(
            Path::new(r"c:\Program Files\sf\standarflow.exe"),
            "claude-code",
        );
        assert!(spaced.starts_with('"'));
        assert!(spaced.contains(r#"standarflow.exe" ingest --provider claude-code"#));
    }

    #[test]
    fn build_command_embeds_the_detection_marker() {
        let cmd = build_command(Path::new("/usr/local/bin/standarflow"), "claude-code");
        assert!(cmd.contains(&command_marker("claude-code")));
    }

    #[test]
    fn new_hook_group_carries_a_matcher() {
        let group = new_hook_group("standarflow.exe ingest --provider claude-code");
        assert_eq!(group["matcher"], "*");
        let hooks = group["hooks"].as_array().expect("hooks array");
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0]["type"], "command");
        assert_eq!(
            hooks[0]["command"],
            "standarflow.exe ingest --provider claude-code"
        );
    }

    #[test]
    fn event_has_marker_detects_an_installed_group() {
        let marker = command_marker("claude-code");
        let cmd = build_command(Path::new(r"c:\bin\standarflow.exe"), "claude-code");
        let groups = vec![new_hook_group(&cmd)];
        assert!(event_has_marker(&groups, &marker));
        assert!(!event_has_marker(&groups, "ingest --provider codex"));
    }

    #[test]
    fn scope_round_trips_through_str() {
        assert_eq!(Scope::parse("user"), Some(Scope::User));
        assert_eq!(Scope::parse("project-local"), Some(Scope::ProjectLocal));
        assert_eq!(Scope::parse("project"), None);
        assert_eq!(Scope::User.as_str(), "user");
        assert_eq!(Scope::ProjectLocal.as_str(), "project-local");
    }

    #[test]
    fn project_local_scope_resolves_under_the_workspace_root() {
        let tpl = template_for("claude-code").expect("claude-code template");
        let root = Path::new("/work/proj");
        let path = Scope::ProjectLocal
            .settings_path(&tpl, root)
            .expect("resolve ok")
            .expect("path present");
        assert!(path.starts_with(root));
        assert!(path.ends_with("settings.local.json"));
    }
}
