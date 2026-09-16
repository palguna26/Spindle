use std::env;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{json, Value};

const CODEX_HOOK_ASSET: &str = include_str!("integration/assets/codex-agent-state.ps1");
const CODEX_HOOK_NAME: &str = "spindle-agent-state.ps1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Codex,
    Opencode,
}

impl Target {
    pub(crate) const ALL: [Self; 2] = [Self::Codex, Self::Opencode];

    fn label(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }

    fn command(self) -> &'static str {
        self.label()
    }

    fn path(self) -> PathBuf {
        match self {
            Self::Codex => codex_dir().join(CODEX_HOOK_NAME),
            Self::Opencode => home_dir()
                .join(".config")
                .join("opencode")
                .join("plugins")
                .join("spindle-agent-state.js"),
        }
    }

    fn installed(self) -> bool {
        self.path().is_file()
    }

    fn available(self) -> bool {
        command_available(self.command()) || self.path().parent().is_some_and(|path| path.is_dir())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Status {
    pub target: &'static str,
    pub command: &'static str,
    pub available: bool,
    pub installed: bool,
    pub path: String,
}

pub(crate) fn statuses() -> Vec<Status> {
    Target::ALL
        .into_iter()
        .map(|target| Status {
            target: target.label(),
            command: target.command(),
            available: target.available(),
            installed: target.installed(),
            path: target.path().display().to_string(),
        })
        .collect()
}

fn home_dir() -> PathBuf {
    if cfg!(windows) {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn command_available(command: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|dir| {
        let base = dir.join(command);
        if base.is_file() {
            return true;
        }
        if !cfg!(windows) {
            return false;
        }
        [".exe", ".cmd", ".bat", ".ps1"]
            .into_iter()
            .any(|extension| dir.join(format!("{command}{extension}")).is_file())
    })
}

pub(crate) fn run_status(json: bool) -> std::io::Result<()> {
    let statuses = statuses();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&statuses).map_err(std::io::Error::other)?
        );
        return Ok(());
    }

    for status in statuses {
        let state = if status.installed {
            "installed"
        } else {
            "not installed"
        };
        println!(
            "{}: {state} (available: {}, {})",
            status.target, status.available, status.path
        );
    }
    Ok(())
}

pub(crate) fn install_codex() -> std::io::Result<Vec<String>> {
    let dir = codex_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "codex config directory not found at {}. install codex first",
            dir.display()
        )));
    }

    let hook_path = dir.join(CODEX_HOOK_NAME);
    std::fs::write(&hook_path, CODEX_HOOK_ASSET)?;
    let hooks_path = dir.join("hooks.json");
    let mut config = if hooks_path.is_file() {
        serde_json::from_str::<Value>(&std::fs::read_to_string(&hooks_path)?).map_err(|error| {
            std::io::Error::other(format!("failed to parse {}: {error}", hooks_path.display()))
        })?
    } else {
        json!({})
    };
    ensure_codex_hook(&mut config, &hooks_path, &hook_path)?;
    std::fs::write(&hooks_path, serde_json::to_string_pretty(&config)?)?;
    Ok(vec![
        format!(
            "installed codex integration hook to {}",
            hook_path.display()
        ),
        format!("ensured codex hooks at {}", hooks_path.display()),
    ])
}

pub(crate) fn uninstall_codex() -> std::io::Result<Vec<String>> {
    let dir = codex_dir();
    let hook_path = dir.join(CODEX_HOOK_NAME);
    let hooks_path = dir.join("hooks.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if hooks_path.is_file() {
        let mut config = serde_json::from_str::<Value>(&std::fs::read_to_string(&hooks_path)?)
            .map_err(|error| {
                std::io::Error::other(format!("failed to parse {}: {error}", hooks_path.display()))
            })?;
        changed = remove_codex_hook(&mut config, &hooks_path, &hook_path)?;
        if changed {
            std::fs::write(&hooks_path, serde_json::to_string_pretty(&config)?)?;
        }
    }
    Ok(vec![format!(
        "{} codex integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

fn codex_dir() -> PathBuf {
    env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex"))
}

fn ensure_codex_hook(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<()> {
    let root = config.as_object_mut().ok_or_else(|| {
        std::io::Error::other(format!(
            "codex hooks file at {} must be a JSON object",
            path.display()
        ))
    })?;
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("codex hooks must be a JSON object"))?;
    let entries = hooks.entry("SessionStart").or_insert_with(|| json!([]));
    let entries = entries
        .as_array_mut()
        .ok_or_else(|| std::io::Error::other("codex SessionStart hooks must be an array"))?;
    let command = codex_hook_command(hook_path);
    if entries.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("command").and_then(Value::as_str) == Some(command.as_str())
                })
            })
    }) {
        return Ok(());
    }
    remove_matching_codex_commands(entries, &command);
    entries.push(json!({"hooks": [{"type": "command", "command": command, "timeout": 10}]}));
    Ok(())
}

fn remove_codex_hook(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<bool> {
    let Some(root) = config.as_object_mut() else {
        return Err(std::io::Error::other(format!(
            "codex hooks file at {} must be a JSON object",
            path.display()
        )));
    };
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let Some(entries) = hooks.get_mut("SessionStart").and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let command = codex_hook_command(hook_path);
    let changed = remove_matching_codex_commands(entries, &command);
    entries.retain(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|items| !items.is_empty())
    });
    Ok(changed)
}

fn remove_matching_codex_commands(entries: &mut Vec<Value>, command: &str) -> bool {
    let mut changed = false;
    for entry in entries.iter_mut() {
        if let Some(items) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
            let before = items.len();
            items.retain(|item| item.get("command").and_then(Value::as_str) != Some(command));
            changed |= before != items.len();
        }
    }
    entries.retain(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|items| !items.is_empty())
    });
    changed
}

fn codex_hook_command(path: &std::path::Path) -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" session",
        path.display().to_string().replace('"', "\\\"")
    )
}

fn remove_file_if_exists(path: &std::path::Path) -> std::io::Result<bool> {
    if path.is_file() {
        std::fs::remove_file(path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::Target;

    #[test]
    fn targets_match_herdr_names() {
        assert_eq!(Target::Codex.label(), "codex");
        assert_eq!(Target::Opencode.label(), "opencode");
    }

    #[test]
    fn target_paths_match_agent_layouts() {
        assert!(Target::Codex.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Opencode.path().ends_with("spindle-agent-state.js"));
    }

    #[test]
    fn codex_hook_edit_preserves_unrelated_hooks_and_is_idempotent() {
        let hook_path = std::path::Path::new("C:\\Users\\test\\.codex\\spindle-agent-state.ps1");
        let command = super::codex_hook_command(hook_path);
        let mut config = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "*",
                    "hooks": [{"type": "command", "command": "custom-hook"},
                              {"type": "command", "command": command}]
                }]
            },
            "other": true
        });

        super::ensure_codex_hook(&mut config, hook_path, hook_path).unwrap();
        let entries = &config["hooks"]["SessionStart"];
        assert_eq!(entries[0]["hooks"].as_array().unwrap().len(), 2);
        assert_eq!(entries[0]["hooks"][0]["command"], "custom-hook");
        assert_eq!(entries[0]["hooks"][1]["command"], command);
        assert!(config["other"].as_bool().unwrap());
    }
}
