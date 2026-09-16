use std::env;
use std::path::PathBuf;

use jsonc_parser::cst::{CstInputValue, CstRootNode};
use jsonc_parser::ParseOptions;
use serde::Serialize;
use serde_json::{json, Value};

const CODEX_HOOK_ASSET: &str = include_str!("integration/assets/codex-agent-state.ps1");
const CODEX_HOOK_NAME: &str = "spindle-agent-state.ps1";
const CLAUDE_HOOK_ASSET: &str = include_str!("integration/assets/claude-agent-state.ps1");
const CLAUDE_HOOK_NAME: &str = "spindle-agent-state.ps1";
const OPENCODE_PLUGIN_ASSET: &str = include_str!("integration/assets/opencode-agent-state.js");
const OPENCODE_PLUGIN_NAME: &str = "spindle-agent-state.js";
const OPENCODE_TUI_ASSET: &str = include_str!("integration/assets/opencode-tui-session.js");
const OPENCODE_TUI_NAME: &str = "spindle-tui-session.js";
const OPENCODE_TUI_SPEC: &str = "./spindle-tui-session.js";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Claude,
    Codex,
    Opencode,
}

impl Target {
    pub(crate) const ALL: [Self; 3] = [Self::Claude, Self::Codex, Self::Opencode];

    fn label(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }

    fn command(self) -> &'static str {
        self.label()
    }

    fn path(self) -> PathBuf {
        match self {
            Self::Claude => claude_dir().join("hooks").join(CLAUDE_HOOK_NAME),
            Self::Codex => codex_dir().join(CODEX_HOOK_NAME),
            Self::Opencode => home_dir()
                .join(".config")
                .join("opencode")
                .join("plugins")
                .join(OPENCODE_PLUGIN_NAME),
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

pub(crate) fn install_opencode() -> std::io::Result<Vec<String>> {
    let dir = opencode_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "opencode config directory not found at {}. install opencode first",
            dir.display()
        )));
    }
    let plugins = dir.join("plugins");
    std::fs::create_dir_all(&plugins)?;
    let plugin_path = plugins.join(OPENCODE_PLUGIN_NAME);
    std::fs::write(&plugin_path, OPENCODE_PLUGIN_ASSET)?;
    let tui_path = dir.join(OPENCODE_TUI_NAME);
    std::fs::write(&tui_path, OPENCODE_TUI_ASSET)?;
    let tui_config = add_tui_plugin(&dir, OPENCODE_TUI_SPEC)?;
    Ok(vec![
        format!(
            "installed opencode integration plugin to {}",
            plugin_path.display()
        ),
        format!("installed opencode TUI plugin to {}", tui_path.display()),
        format!("ensured opencode TUI config at {}", tui_config.display()),
    ])
}

pub(crate) fn uninstall_opencode() -> std::io::Result<Vec<String>> {
    let dir = opencode_dir();
    let plugin_path = dir.join("plugins").join(OPENCODE_PLUGIN_NAME);
    let tui_path = dir.join(OPENCODE_TUI_NAME);
    let removed_plugin = remove_file_if_exists(&plugin_path)?;
    let removed_tui = remove_file_if_exists(&tui_path)?;
    let removed_config = remove_tui_plugin(&dir, OPENCODE_TUI_SPEC)?;
    Ok(vec![
        format!(
            "{} opencode integration plugin {}",
            if removed_plugin {
                "removed"
            } else {
                "did not find"
            },
            plugin_path.display()
        ),
        format!(
            "{} opencode TUI plugin {}",
            if removed_tui {
                "removed"
            } else {
                "did not find"
            },
            tui_path.display()
        ),
        format!(
            "{} opencode TUI config entry",
            if removed_config {
                "removed"
            } else {
                "did not find"
            }
        ),
    ])
}

pub(crate) fn install_claude() -> std::io::Result<Vec<String>> {
    let dir = claude_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "claude config directory not found at {}. install claude code first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(CLAUDE_HOOK_NAME);
    std::fs::write(&hook_path, CLAUDE_HOOK_ASSET)?;
    let settings_path = dir.join("settings.json");
    let content = if settings_path.is_file() {
        std::fs::read_to_string(&settings_path)?
    } else {
        "{}\n".into()
    };
    let updated = add_claude_hook(&content, &settings_path, &hook_path)?;
    if updated != content {
        std::fs::write(&settings_path, updated)?;
    }
    Ok(vec![
        format!(
            "installed claude integration hook to {}",
            hook_path.display()
        ),
        format!("ensured claude settings at {}", settings_path.display()),
    ])
}

pub(crate) fn uninstall_claude() -> std::io::Result<Vec<String>> {
    let dir = claude_dir();
    let hook_path = dir.join("hooks").join(CLAUDE_HOOK_NAME);
    let settings_path = dir.join("settings.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if settings_path.is_file() {
        let content = std::fs::read_to_string(&settings_path)?;
        let updated = remove_claude_hook(&content, &settings_path, &hook_path, &mut changed)?;
        if changed {
            std::fs::write(&settings_path, updated)?;
        }
    }
    Ok(vec![format!(
        "{} claude integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

fn claude_dir() -> PathBuf {
    env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".claude"))
}

fn add_claude_hook(
    content: &str,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<String> {
    let root = parse_jsonc_root(content, path)?;
    let object = root_object(&root, path)?;
    let hooks = object
        .object_value_or_create("hooks")
        .ok_or_else(|| std::io::Error::other("Claude hooks must be a JSON object"))?;
    let session = match hooks.get("SessionStart") {
        Some(property) => property
            .array_value()
            .ok_or_else(|| std::io::Error::other("Claude SessionStart hooks must be an array"))?,
        None => {
            hooks.append("SessionStart", CstInputValue::Array(Vec::new()));
            hooks
                .get("SessionStart")
                .and_then(|property| property.array_value())
                .ok_or_else(|| {
                    std::io::Error::other("failed to create Claude SessionStart hooks")
                })?
        }
    };
    let command = claude_hook_command(hook_path);
    if session.elements().iter().any(|entry| {
        entry
            .as_object()
            .and_then(|object| object.get("hooks"))
            .and_then(|property| property.array_value())
            .is_some_and(|hooks| {
                hooks.elements().iter().any(|hook| {
                    hook.to_serde_value()
                        .is_some_and(|value| value["command"].as_str() == Some(command.as_str()))
                })
            })
    }) {
        return Ok(content.to_owned());
    }
    session.append(CstInputValue::Object(vec![
        ("matcher".into(), CstInputValue::String("*".into())),
        (
            "hooks".into(),
            CstInputValue::Array(vec![CstInputValue::Object(vec![
                ("type".into(), CstInputValue::String("command".into())),
                ("command".into(), CstInputValue::String(command)),
                ("timeout".into(), CstInputValue::Number("10".into())),
            ])]),
        ),
    ]));
    Ok(root.to_string())
}

fn remove_claude_hook(
    content: &str,
    path: &std::path::Path,
    hook_path: &std::path::Path,
    changed: &mut bool,
) -> std::io::Result<String> {
    let root = parse_jsonc_root(content, path)?;
    let object = root_object(&root, path)?;
    let Some(hooks) = object.object_value("hooks") else {
        return Ok(content.to_owned());
    };
    let Some(session) = hooks
        .get("SessionStart")
        .and_then(|property| property.array_value())
    else {
        return Ok(content.to_owned());
    };
    let command = claude_hook_command(hook_path);
    for entry in session.elements() {
        let Some(entry_object) = entry.as_object() else {
            continue;
        };
        let Some(command_hooks) = entry_object
            .get("hooks")
            .and_then(|property| property.array_value())
        else {
            continue;
        };
        for hook in command_hooks.elements() {
            if hook
                .to_serde_value()
                .is_some_and(|value| value["command"].as_str() == Some(command.as_str()))
            {
                hook.remove();
                *changed = true;
            }
        }
        if command_hooks.elements().is_empty() {
            entry.remove();
        }
    }
    Ok(if *changed {
        root.to_string()
    } else {
        content.to_owned()
    })
}

fn claude_hook_command(path: &std::path::Path) -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" session",
        path.display().to_string().replace('"', "\\\"")
    )
}

fn add_tui_plugin(dir: &std::path::Path, plugin_spec: &str) -> std::io::Result<PathBuf> {
    let path = dir.join("tui.jsonc");
    let content = if path.is_file() {
        std::fs::read_to_string(&path)?
    } else {
        "{}\n".into()
    };
    let root = parse_jsonc_root(&content, &path)?;
    let object = root_object(&root, &path)?;
    if let Some(property) = object.get("plugin") {
        let plugins = property
            .array_value()
            .ok_or_else(|| std::io::Error::other("OpenCode TUI plugin list must be an array"))?;
        if !plugins.elements().iter().any(|entry| {
            entry
                .to_serde_value()
                .is_some_and(|value| plugin_entry_matches(&value, plugin_spec))
        }) {
            plugins.append(CstInputValue::String(plugin_spec.to_owned()));
        }
    } else {
        object.append(
            "plugin",
            CstInputValue::Array(vec![CstInputValue::String(plugin_spec.to_owned())]),
        );
    }
    std::fs::write(&path, root.to_string())?;
    Ok(path)
}

fn remove_tui_plugin(dir: &std::path::Path, plugin_spec: &str) -> std::io::Result<bool> {
    let path = dir.join("tui.jsonc");
    if !path.is_file() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&path)?;
    let root = parse_jsonc_root(&content, &path)?;
    let object = root_object(&root, &path)?;
    let Some(property) = object.get("plugin") else {
        return Ok(false);
    };
    let plugins = property
        .array_value()
        .ok_or_else(|| std::io::Error::other("OpenCode TUI plugin list must be an array"))?;
    let mut removed = false;
    for entry in plugins.elements() {
        if entry
            .to_serde_value()
            .is_some_and(|value| plugin_entry_matches(&value, plugin_spec))
        {
            entry.remove();
            removed = true;
        }
    }
    if removed {
        if plugins.elements().is_empty() {
            property.remove();
        }
        std::fs::write(&path, root.to_string())?;
    }
    Ok(removed)
}

fn parse_jsonc_root(content: &str, path: &std::path::Path) -> std::io::Result<CstRootNode> {
    CstRootNode::parse(
        content,
        &ParseOptions {
            allow_comments: true,
            allow_loose_object_property_names: false,
            allow_trailing_commas: true,
            allow_missing_commas: false,
            allow_single_quoted_strings: false,
            allow_hexadecimal_numbers: false,
            allow_unary_plus_numbers: false,
        },
    )
    .map_err(|error| {
        std::io::Error::other(format!(
            "failed to parse OpenCode TUI config at {}: {error}",
            path.display()
        ))
    })
}

fn root_object(
    root: &CstRootNode,
    path: &std::path::Path,
) -> std::io::Result<jsonc_parser::cst::CstObject> {
    root.value()
        .and_then(|value| value.as_object())
        .ok_or_else(|| {
            std::io::Error::other(format!(
                "OpenCode TUI config at {} must be a JSON object",
                path.display()
            ))
        })
}

fn plugin_entry_matches(value: &Value, plugin_spec: &str) -> bool {
    value.as_str() == Some(plugin_spec)
        || value
            .as_array()
            .and_then(|parts| parts.first())
            .and_then(Value::as_str)
            == Some(plugin_spec)
}

fn codex_dir() -> PathBuf {
    env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex"))
}

fn opencode_dir() -> PathBuf {
    home_dir().join(".config").join("opencode")
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
    use super::{Target, OPENCODE_PLUGIN_NAME, OPENCODE_TUI_SPEC};

    #[test]
    fn targets_match_herdr_names() {
        assert_eq!(Target::Codex.label(), "codex");
        assert_eq!(Target::Opencode.label(), "opencode");
    }

    #[test]
    fn target_paths_match_agent_layouts() {
        assert!(Target::Codex.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Opencode.path().ends_with(OPENCODE_PLUGIN_NAME));
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

    #[test]
    fn opencode_tui_registration_preserves_jsonc_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!(
            "spindle-opencode-tui-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tui.jsonc");
        std::fs::write(
            &path,
            "{\n  // Keep this comment.\n  \"theme\": \"system\",\n  \"plugin\": [\"custom\",],\n}\n",
        )
        .unwrap();

        super::add_tui_plugin(&dir, OPENCODE_TUI_SPEC).unwrap();
        super::add_tui_plugin(&dir, OPENCODE_TUI_SPEC).unwrap();
        let installed = std::fs::read_to_string(&path).unwrap();
        assert!(installed.contains("// Keep this comment."));
        let root = super::parse_jsonc_root(&installed, &path).unwrap();
        let value = root.value().unwrap().to_serde_value().unwrap();
        assert_eq!(
            value["plugin"],
            serde_json::json!(["custom", OPENCODE_TUI_SPEC])
        );

        assert!(super::remove_tui_plugin(&dir, OPENCODE_TUI_SPEC).unwrap());
        let removed = std::fs::read_to_string(&path).unwrap();
        assert!(removed.contains("// Keep this comment."));
        let root = super::parse_jsonc_root(&removed, &path).unwrap();
        let value = root.value().unwrap().to_serde_value().unwrap();
        assert_eq!(value["plugin"], serde_json::json!(["custom"]));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
