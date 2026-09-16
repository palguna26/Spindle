use std::env;
use std::path::PathBuf;

use jsonc_parser::cst::{CstInputValue, CstRootNode};
use jsonc_parser::ParseOptions;
use serde::Serialize;
use serde_json::{json, Value};

mod command;
mod file_ops;
mod paths;
use self::command::*;
use self::file_ops::*;
use self::paths::*;

const CODEX_HOOK_ASSET: &str = include_str!("integration/assets/codex-agent-state.ps1");
const CODEX_HOOK_NAME: &str = "spindle-agent-state.ps1";
const COPILOT_HOOK_ASSET: &str = include_str!("integration/assets/copilot-agent-state.ps1");
const COPILOT_HOOK_NAME: &str = "spindle-agent-state.ps1";
const CURSOR_HOOK_ASSET: &str = include_str!("integration/assets/cursor-agent-state.ps1");
const CURSOR_HOOK_NAME: &str = "spindle-agent-state.ps1";
const DEVIN_HOOK_ASSET: &str = include_str!("integration/assets/devin-agent-state.ps1");
const DEVIN_HOOK_NAME: &str = "spindle-agent-state.ps1";
const DROID_HOOK_ASSET: &str = include_str!("integration/assets/droid-agent-state.ps1");
const DROID_HOOK_NAME: &str = "spindle-agent-state.ps1";
const KIMI_HOOK_ASSET: &str = include_str!("integration/assets/kimi-agent-state.ps1");
const KIMI_HOOK_NAME: &str = "spindle-agent-state.ps1";
const KIMI_BEGIN: &str = "# >>> spindle kimi integration";
const KIMI_END: &str = "# <<< spindle kimi integration";
const QODERCLI_HOOK_ASSET: &str = include_str!("integration/assets/qodercli-agent-state.ps1");
const QODERCLI_HOOK_NAME: &str = "spindle-agent-state.ps1";
const QWEN_HOOK_ASSET: &str = include_str!("integration/assets/qwen-agent-session.ps1");
const QWEN_HOOK_NAME: &str = "spindle-agent-session.ps1";
const GROK_HOOK_ASSET: &str = include_str!("integration/assets/grok-agent-state.ps1");
const GROK_HOOK_NAME: &str = "spindle-agent-state.ps1";
const GROK_CONFIG_NAME: &str = "spindle.json";
const KILO_PLUGIN_ASSET: &str = include_str!("integration/assets/kilo-agent-state.js");
const KILO_PLUGIN_NAME: &str = "spindle-agent-state.js";
const HERMES_PLUGIN_INIT: &str = include_str!("integration/assets/hermes/__init__.py");
const HERMES_PLUGIN_MANIFEST: &str = include_str!("integration/assets/hermes/plugin.yaml");
const HERMES_PLUGIN_NAME: &str = "spindle-agent-state";
const ANTIGRAVITY_CLI_HOOK_ASSET: &str =
    include_str!("integration/assets/antigravity-cli-agent-state.ps1");
const ANTIGRAVITY_CLI_HOOK_NAME: &str = "spindle-agent-state.ps1";
const ANTIGRAVITY_CLI_HOOK_BLOCK_NAME: &str = "spindle";
const CLAUDE_HOOK_ASSET: &str = include_str!("integration/assets/claude-agent-state.ps1");
const CLAUDE_HOOK_NAME: &str = "spindle-agent-state.ps1";
const PI_EXTENSION_ASSET: &str = include_str!("integration/assets/pi-agent-state.ts");
const PI_EXTENSION_NAME: &str = "spindle-agent-state.ts";
const OMP_EXTENSION_ASSET: &str = include_str!("integration/assets/omp-agent-state.ts");
const OMP_EXTENSION_NAME: &str = "spindle-omp-agent-state.ts";
const OPENCODE_PLUGIN_ASSET: &str = include_str!("integration/assets/opencode-agent-state.js");
const OPENCODE_PLUGIN_NAME: &str = "spindle-agent-state.js";
const OPENCODE_TUI_ASSET: &str = include_str!("integration/assets/opencode-tui-session.js");
const OPENCODE_TUI_NAME: &str = "spindle-tui-session.js";
const OPENCODE_TUI_SPEC: &str = "./spindle-tui-session.js";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Pi,
    Omp,
    Claude,
    Codex,
    Opencode,
    Copilot,
    Cursor,
    Devin,
    Droid,
    Kimi,
    Qodercli,
    Qwen,
    Grok,
    Kilo,
    Hermes,
    AntigravityCli,
}

impl Target {
    pub(crate) const ALL: [Self; 16] = [
        Self::Pi,
        Self::Omp,
        Self::Claude,
        Self::Codex,
        Self::Opencode,
        Self::Copilot,
        Self::Cursor,
        Self::Devin,
        Self::Droid,
        Self::Kimi,
        Self::Qodercli,
        Self::Qwen,
        Self::Grok,
        Self::Kilo,
        Self::Hermes,
        Self::AntigravityCli,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::Omp => "omp",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Copilot => "copilot",
            Self::Cursor => "cursor",
            Self::Devin => "devin",
            Self::Droid => "droid",
            Self::Kimi => "kimi",
            Self::Qodercli => "qodercli",
            Self::Qwen => "qwen",
            Self::Grok => "grok",
            Self::Kilo => "kilo",
            Self::Hermes => "hermes",
            Self::AntigravityCli => "antigravity-cli",
        }
    }

    fn command(self) -> &'static str {
        self.label()
    }

    fn path(self) -> PathBuf {
        match self {
            Self::Pi => pi_extension_dir().join(PI_EXTENSION_NAME),
            Self::Omp => omp_extension_dir().join(OMP_EXTENSION_NAME),
            Self::Claude => claude_dir().join("hooks").join(CLAUDE_HOOK_NAME),
            Self::Codex => codex_dir().join(CODEX_HOOK_NAME),
            Self::Opencode => home_dir()
                .join(".config")
                .join("opencode")
                .join("plugins")
                .join(OPENCODE_PLUGIN_NAME),
            Self::Copilot => copilot_dir().join("hooks").join(COPILOT_HOOK_NAME),
            Self::Cursor => cursor_dir().join(CURSOR_HOOK_NAME),
            Self::Devin => devin_dir().join(DEVIN_HOOK_NAME),
            Self::Droid => droid_dir().join("hooks").join(DROID_HOOK_NAME),
            Self::Kimi => kimi_dir().join("hooks").join(KIMI_HOOK_NAME),
            Self::Qodercli => qodercli_dir().join("hooks").join(QODERCLI_HOOK_NAME),
            Self::Qwen => qwen_dir().join("hooks").join(QWEN_HOOK_NAME),
            Self::Grok => grok_dir().join("hooks").join(GROK_HOOK_NAME),
            Self::Kilo => kilo_dir().join("plugin").join(KILO_PLUGIN_NAME),
            Self::Hermes => hermes_dir()
                .join("plugins")
                .join(HERMES_PLUGIN_NAME)
                .join("__init__.py"),
            Self::AntigravityCli => antigravity_cli_dir()
                .join("hooks")
                .join(ANTIGRAVITY_CLI_HOOK_NAME),
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

pub(crate) fn install_copilot() -> std::io::Result<Vec<String>> {
    let dir = copilot_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "copilot config directory not found at {}. install github copilot cli first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(COPILOT_HOOK_NAME);
    std::fs::write(&hook_path, COPILOT_HOOK_ASSET)?;
    let settings_path = dir.join("settings.json");
    let mut settings = read_json_object(&settings_path, "copilot settings")?;
    ensure_copilot_hook(&mut settings, &settings_path, &hook_path)?;
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    Ok(vec![
        format!(
            "installed copilot integration hook to {}",
            hook_path.display()
        ),
        format!("ensured copilot settings at {}", settings_path.display()),
    ])
}

pub(crate) fn uninstall_copilot() -> std::io::Result<Vec<String>> {
    let dir = copilot_dir();
    let hook_path = dir.join("hooks").join(COPILOT_HOOK_NAME);
    let settings_path = dir.join("settings.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if settings_path.is_file() {
        let mut settings = read_json_object(&settings_path, "copilot settings")?;
        changed = remove_copilot_hook(&mut settings, &hook_path)?;
        if changed {
            std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }
    Ok(vec![format!(
        "{} copilot integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

pub(crate) fn install_cursor() -> std::io::Result<Vec<String>> {
    let dir = cursor_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "cursor config directory not found at {}. install cursor agent cli first",
            dir.display()
        )));
    }
    let hook_path = dir.join(CURSOR_HOOK_NAME);
    std::fs::write(&hook_path, CURSOR_HOOK_ASSET)?;
    let hooks_path = dir.join("hooks.json");
    let mut config = read_json_object(&hooks_path, "cursor hooks file")?;
    if config.get("version").is_none() {
        config["version"] = json!(1);
    }
    ensure_cursor_hooks(&mut config, &hooks_path, &hook_path)?;
    std::fs::write(&hooks_path, serde_json::to_string_pretty(&config)?)?;
    Ok(vec![
        format!(
            "installed cursor integration hook to {}",
            hook_path.display()
        ),
        format!("updated cursor hooks at {}", hooks_path.display()),
    ])
}

pub(crate) fn uninstall_cursor() -> std::io::Result<Vec<String>> {
    let dir = cursor_dir();
    let hook_path = dir.join(CURSOR_HOOK_NAME);
    let hooks_path = dir.join("hooks.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if hooks_path.is_file() {
        let mut config = read_json_object(&hooks_path, "cursor hooks file")?;
        changed = remove_cursor_hooks(&mut config, &hook_path)?;
        if changed {
            std::fs::write(&hooks_path, serde_json::to_string_pretty(&config)?)?;
        }
    }
    Ok(vec![format!(
        "{} cursor integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

pub(crate) fn install_devin() -> std::io::Result<Vec<String>> {
    let dir = devin_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "devin config directory not found at {}. install devin cli first",
            dir.display()
        )));
    }
    let hook_path = dir.join(DEVIN_HOOK_NAME);
    std::fs::write(&hook_path, DEVIN_HOOK_ASSET)?;
    let settings_path = dir.join("config.json");
    let mut config = read_json_object(&settings_path, "devin settings")?;
    ensure_devin_hooks(&mut config, &settings_path, &hook_path)?;
    std::fs::write(&settings_path, serde_json::to_string_pretty(&config)?)?;
    Ok(vec![
        format!(
            "installed devin integration hook to {}",
            hook_path.display()
        ),
        format!("ensured devin settings at {}", settings_path.display()),
    ])
}

pub(crate) fn uninstall_devin() -> std::io::Result<Vec<String>> {
    let dir = devin_dir();
    let hook_path = dir.join(DEVIN_HOOK_NAME);
    let settings_path = dir.join("config.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if settings_path.is_file() {
        let mut config = read_json_object(&settings_path, "devin settings")?;
        changed = remove_devin_hooks(&mut config, &hook_path)?;
        if changed {
            std::fs::write(&settings_path, serde_json::to_string_pretty(&config)?)?;
        }
    }
    Ok(vec![format!(
        "{} devin integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

pub(crate) fn install_droid() -> std::io::Result<Vec<String>> {
    let dir = droid_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "droid config directory not found at {}. install droid first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(DROID_HOOK_NAME);
    std::fs::write(&hook_path, DROID_HOOK_ASSET)?;
    let settings_path = dir.join("settings.json");
    let mut settings = read_json_object(&settings_path, "droid settings")?;
    ensure_droid_settings(&mut settings, &settings_path, &hook_path)?;
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    let hooks_path = dir.join("hooks.json");
    if hooks_path.is_file() {
        let mut legacy = read_json_object(&hooks_path, "droid hooks file")?;
        if remove_droid_legacy(&mut legacy, &hook_path)? {
            std::fs::write(&hooks_path, serde_json::to_string_pretty(&legacy)?)?;
        }
    }
    Ok(vec![
        format!(
            "installed droid integration hook to {}",
            hook_path.display()
        ),
        format!("ensured droid settings at {}", settings_path.display()),
    ])
}

pub(crate) fn uninstall_droid() -> std::io::Result<Vec<String>> {
    let dir = droid_dir();
    let hook_path = dir.join("hooks").join(DROID_HOOK_NAME);
    let settings_path = dir.join("settings.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if settings_path.is_file() {
        let mut settings = read_json_object(&settings_path, "droid settings")?;
        changed = remove_droid_settings(&mut settings, &hook_path)?;
        if changed {
            std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }
    let hooks_path = dir.join("hooks.json");
    if hooks_path.is_file() {
        let mut legacy = read_json_object(&hooks_path, "droid hooks file")?;
        changed |= remove_droid_legacy(&mut legacy, &hook_path)?;
        if changed {
            std::fs::write(&hooks_path, serde_json::to_string_pretty(&legacy)?)?;
        }
    }
    Ok(vec![format!(
        "{} droid integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

pub(crate) fn install_kimi() -> std::io::Result<Vec<String>> {
    let dir = kimi_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "kimi code config directory not found at {}. install kimi code first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(KIMI_HOOK_NAME);
    std::fs::write(&hook_path, KIMI_HOOK_ASSET)?;
    let config_path = dir.join("config.toml");
    let existing = if config_path.is_file() {
        std::fs::read_to_string(&config_path)?
    } else {
        String::new()
    };
    let updated = build_kimi_config(&existing, &hook_path);
    if updated != existing {
        std::fs::write(&config_path, updated)?;
    }
    Ok(vec![
        format!("installed kimi integration hook to {}", hook_path.display()),
        format!("updated kimi config at {}", config_path.display()),
    ])
}

pub(crate) fn uninstall_kimi() -> std::io::Result<Vec<String>> {
    let dir = kimi_dir();
    let hook_path = dir.join("hooks").join(KIMI_HOOK_NAME);
    let config_path = dir.join("config.toml");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if config_path.is_file() {
        let existing = std::fs::read_to_string(&config_path)?;
        let updated = remove_kimi_config(&existing);
        changed = updated != existing;
        if changed {
            std::fs::write(&config_path, updated)?;
        }
    }
    Ok(vec![format!(
        "{} kimi integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

pub(crate) fn install_qodercli() -> std::io::Result<Vec<String>> {
    let dir = qodercli_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "qodercli config directory not found at {}. install qodercli first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(QODERCLI_HOOK_NAME);
    std::fs::write(&hook_path, QODERCLI_HOOK_ASSET)?;
    let settings_path = dir.join("settings.json");
    let mut config = read_json_object(&settings_path, "qodercli settings")?;
    ensure_qodercli_hooks(&mut config, &settings_path, &hook_path)?;
    std::fs::write(&settings_path, serde_json::to_string_pretty(&config)?)?;
    Ok(vec![
        format!(
            "installed qodercli integration hook to {}",
            hook_path.display()
        ),
        format!("ensured qodercli settings at {}", settings_path.display()),
    ])
}

pub(crate) fn uninstall_qodercli() -> std::io::Result<Vec<String>> {
    let dir = qodercli_dir();
    let hook_path = dir.join("hooks").join(QODERCLI_HOOK_NAME);
    let settings_path = dir.join("settings.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if settings_path.is_file() {
        let mut config = read_json_object(&settings_path, "qodercli settings")?;
        changed = remove_qodercli_hooks(&mut config, &hook_path)?;
        if changed {
            std::fs::write(&settings_path, serde_json::to_string_pretty(&config)?)?;
        }
    }
    Ok(vec![format!(
        "{} qodercli integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

pub(crate) fn install_qwen() -> std::io::Result<Vec<String>> {
    let dir = qwen_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "qwen code config directory not found at {}. install qwen code first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(QWEN_HOOK_NAME);
    std::fs::write(&hook_path, QWEN_HOOK_ASSET)?;
    let settings_path = dir.join("settings.json");
    let mut config = read_json_object(&settings_path, "qwen settings")?;
    ensure_qwen_hook(&mut config, &settings_path, &hook_path)?;
    std::fs::write(&settings_path, serde_json::to_string_pretty(&config)?)?;
    Ok(vec![
        format!("installed qwen integration hook to {}", hook_path.display()),
        format!("ensured qwen settings at {}", settings_path.display()),
    ])
}

pub(crate) fn uninstall_qwen() -> std::io::Result<Vec<String>> {
    let dir = qwen_dir();
    let hook_path = dir.join("hooks").join(QWEN_HOOK_NAME);
    let settings_path = dir.join("settings.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if settings_path.is_file() {
        let mut config = read_json_object(&settings_path, "qwen settings")?;
        changed = remove_qwen_hook(&mut config, &hook_path)?;
        if changed {
            std::fs::write(&settings_path, serde_json::to_string_pretty(&config)?)?;
        }
    }
    Ok(vec![format!(
        "{} qwen integration hook {}",
        if removed_hook || changed {
            "removed"
        } else {
            "did not find"
        },
        hook_path.display()
    )])
}

pub(crate) fn install_grok() -> std::io::Result<Vec<String>> {
    let dir = grok_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "grok config directory not found at {}. install grok cli first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(GROK_HOOK_NAME);
    let config_path = hooks_dir.join(GROK_CONFIG_NAME);
    std::fs::write(&hook_path, GROK_HOOK_ASSET)?;
    let config = json!({"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":format!("{} session", direct_hook_command(&hook_path)),"timeout":10}]}]}});
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(vec![
        format!("installed grok integration hook to {}", hook_path.display()),
        format!("ensured grok hook config at {}", config_path.display()),
    ])
}

pub(crate) fn uninstall_grok() -> std::io::Result<Vec<String>> {
    let dir = grok_dir();
    let hooks_dir = dir.join("hooks");
    let hook_path = hooks_dir.join(GROK_HOOK_NAME);
    let config_path = hooks_dir.join(GROK_CONFIG_NAME);
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let removed_config = remove_file_if_exists(&config_path)?;
    Ok(vec![format!(
        "{} grok integration files",
        if removed_hook || removed_config {
            "removed"
        } else {
            "did not find"
        }
    )])
}

pub(crate) fn install_kilo() -> std::io::Result<Vec<String>> {
    let dir = kilo_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "kilo config directory not found at {}. install kilo first",
            dir.display()
        )));
    }
    let plugin_dir = dir.join("plugin");
    std::fs::create_dir_all(&plugin_dir)?;
    let path = plugin_dir.join(KILO_PLUGIN_NAME);
    std::fs::write(&path, KILO_PLUGIN_ASSET)?;
    Ok(vec![format!(
        "installed kilo integration plugin to {}",
        path.display()
    )])
}

pub(crate) fn uninstall_kilo() -> std::io::Result<Vec<String>> {
    let path = kilo_dir().join("plugin").join(KILO_PLUGIN_NAME);
    let removed = remove_file_if_exists(&path)?;
    Ok(vec![format!(
        "{} kilo integration plugin {}",
        if removed { "removed" } else { "did not find" },
        path.display()
    )])
}

pub(crate) fn install_hermes() -> std::io::Result<Vec<String>> {
    let dir = hermes_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "hermes config directory not found at {}. install hermes agent first",
            dir.display()
        )));
    }
    let plugin_dir = dir.join("plugins").join(HERMES_PLUGIN_NAME);
    std::fs::create_dir_all(&plugin_dir)?;
    std::fs::write(plugin_dir.join("__init__.py"), HERMES_PLUGIN_INIT)?;
    std::fs::write(plugin_dir.join("plugin.yaml"), HERMES_PLUGIN_MANIFEST)?;
    let config_path = dir.join("config.yaml");
    let existing = if config_path.is_file() {
        std::fs::read_to_string(&config_path)?
    } else {
        String::new()
    };
    let updated = enable_hermes_plugin(&existing);
    if updated != existing {
        std::fs::write(&config_path, updated)?;
    }
    Ok(vec![
        format!(
            "installed hermes integration plugin to {}",
            plugin_dir.display()
        ),
        format!("ensured hermes config at {}", config_path.display()),
    ])
}

pub(crate) fn uninstall_hermes() -> std::io::Result<Vec<String>> {
    let dir = hermes_dir();
    let plugin_dir = dir.join("plugins").join(HERMES_PLUGIN_NAME);
    let config_path = dir.join("config.yaml");
    let removed = remove_dir_if_exists(&plugin_dir)?;
    let mut changed = false;
    if config_path.is_file() {
        let existing = std::fs::read_to_string(&config_path)?;
        let updated = disable_hermes_plugin(&existing);
        changed = updated != existing;
        if changed {
            std::fs::write(&config_path, updated)?;
        }
    }
    Ok(vec![format!(
        "{} hermes integration",
        if removed || changed {
            "removed"
        } else {
            "did not find"
        }
    )])
}

pub(crate) fn install_antigravity_cli() -> std::io::Result<Vec<String>> {
    let dir = antigravity_cli_dir();
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "antigravity cli config directory not found at {}. install antigravity cli first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(ANTIGRAVITY_CLI_HOOK_NAME);
    std::fs::write(&hook_path, ANTIGRAVITY_CLI_HOOK_ASSET)?;
    let config_path = dir.join("hooks.json");
    let mut config = read_json_object(&config_path, "antigravity cli hooks")?;
    ensure_antigravity_cli_hook(&mut config, &hook_path);
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(vec![
        format!(
            "installed antigravity cli integration hook to {}",
            hook_path.display()
        ),
        format!("ensured antigravity cli hooks at {}", config_path.display()),
    ])
}

pub(crate) fn uninstall_antigravity_cli() -> std::io::Result<Vec<String>> {
    let dir = antigravity_cli_dir();
    let hook_path = dir.join("hooks").join(ANTIGRAVITY_CLI_HOOK_NAME);
    let config_path = dir.join("hooks.json");
    let removed_hook = remove_file_if_exists(&hook_path)?;
    let mut changed = false;
    if config_path.is_file() {
        let mut config = read_json_object(&config_path, "antigravity cli hooks")?;
        changed = remove_antigravity_cli_hook(&mut config);
        if changed {
            std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
        }
    }
    Ok(vec![format!(
        "{} antigravity cli integration {}",
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

pub(crate) fn install_pi() -> std::io::Result<Vec<String>> {
    let dir = pi_extension_dir();
    if !dir.parent().is_some_and(|parent| parent.is_dir()) && !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "pi extension directory not found at {}. install pi first",
            dir.display()
        )));
    }
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(PI_EXTENSION_NAME);
    std::fs::write(&path, PI_EXTENSION_ASSET)?;
    Ok(vec![format!(
        "installed pi integration to {}",
        path.display()
    )])
}

pub(crate) fn uninstall_pi() -> std::io::Result<Vec<String>> {
    let path = pi_extension_dir().join(PI_EXTENSION_NAME);
    let removed = remove_file_if_exists(&path)?;
    Ok(vec![format!(
        "{} pi integration {}",
        if removed { "removed" } else { "did not find" },
        path.display()
    )])
}

pub(crate) fn install_omp() -> std::io::Result<Vec<String>> {
    let dir = omp_extension_dir();
    if dir == pi_extension_dir() {
        return Err(std::io::Error::other(format!(
            "Pi and OMP resolve to the same extension directory at {}; configure separate agent directories before installing OMP",
            dir.display()
        )));
    }
    if !dir.parent().is_some_and(|parent| parent.is_dir()) && !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "omp extension directory not found at {}. install omp first",
            dir.display()
        )));
    }
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(OMP_EXTENSION_NAME);
    std::fs::write(&path, OMP_EXTENSION_ASSET)?;
    Ok(vec![format!(
        "installed omp integration to {}",
        path.display()
    )])
}

pub(crate) fn uninstall_omp() -> std::io::Result<Vec<String>> {
    let path = omp_extension_dir().join(OMP_EXTENSION_NAME);
    let removed = remove_file_if_exists(&path)?;
    Ok(vec![format!(
        "{} omp integration {}",
        if removed { "removed" } else { "did not find" },
        path.display()
    )])
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

fn ensure_antigravity_cli_hook(config: &mut Value, hook_path: &std::path::Path) {
    let hooks = config
        .as_object_mut()
        .expect("read_json_object always returns an object")
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("antigravity cli hooks must be an object");
    hooks.insert(
        ANTIGRAVITY_CLI_HOOK_BLOCK_NAME.to_owned(),
        json!({"PreInvocation": [{
            "type": "command",
            "command": format!("{} session", direct_hook_command(hook_path)),
            "timeout": 10
        }]}),
    );
}

fn remove_antigravity_cli_hook(config: &mut Value) -> bool {
    config
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .and_then(|hooks| hooks.remove(ANTIGRAVITY_CLI_HOOK_BLOCK_NAME))
        .is_some()
}

fn enable_hermes_plugin(content: &str) -> String {
    if content
        .lines()
        .any(|line| line.trim() == "- spindle-agent-state")
    {
        return content.to_owned();
    }
    let mut result = content.trim_end_matches(['\r', '\n']).to_owned();
    if !result.is_empty() {
        result.push('\n');
    }
    result.push_str("plugins:\n  enabled:\n    - spindle-agent-state\n");
    result
}
fn disable_hermes_plugin(content: &str) -> String {
    let mut lines = content
        .lines()
        .filter(|line| line.trim() != "- spindle-agent-state")
        .collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let mut result = lines.join("\n");
    if content.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}
fn qwen_command(path: &std::path::Path) -> String {
    format!("{} session", direct_hook_command(path))
}
fn ensure_qwen_hook(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<()> {
    let root = config.as_object_mut().ok_or_else(|| {
        std::io::Error::other(format!(
            "qwen settings at {} must be a JSON object",
            path.display()
        ))
    })?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("qwen settings hooks must be a JSON object"))?;
    let entries = hooks
        .entry("SessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| std::io::Error::other("qwen SessionStart hooks must be an array"))?;
    entries.retain(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|items| {
                !items.iter().any(|item| {
                    item.get("command").and_then(Value::as_str)
                        == Some(qwen_command(hook_path).as_str())
                })
            })
    });
    entries.push(json!({"matcher":"*","hooks":[{"type":"command","command":qwen_command(hook_path),"timeout":10000}]}));
    Ok(())
}
fn remove_qwen_hook(config: &mut Value, hook_path: &std::path::Path) -> std::io::Result<bool> {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let Some(entries) = hooks.get_mut("SessionStart").and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let command = qwen_command(hook_path);
    let before = entries.len();
    entries.retain(|entry| {
        !entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("command").and_then(Value::as_str) == Some(command.as_str())
                })
            })
    });
    Ok(before != entries.len())
}

fn qodercli_events() -> [(&'static str, &'static str); 1] {
    [("SessionStart", "session")]
}
fn qodercli_removed_events() -> [(&'static str, &'static str); 12] {
    [
        ("SessionStart", "idle"),
        ("UserPromptSubmit", "working"),
        ("PreToolUse", "working"),
        ("PostToolUse", "working"),
        ("PostToolUseFailure", "working"),
        ("SubagentStart", "working"),
        ("SubagentStop", "working"),
        ("PreCompact", "working"),
        ("PermissionRequest", "blocked"),
        ("PermissionResult", "working"),
        ("Stop", "idle"),
        ("SessionEnd", "release"),
    ]
}

fn qodercli_command(path: &std::path::Path, action: &str) -> String {
    format!("{} {}", direct_hook_command(path), action)
}
fn ensure_qodercli_hooks(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<()> {
    let root = config.as_object_mut().ok_or_else(|| {
        std::io::Error::other(format!(
            "qodercli settings at {} must be a JSON object",
            path.display()
        ))
    })?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("qodercli settings hooks must be a JSON object"))?;
    for (event, action) in qodercli_removed_events()
        .into_iter()
        .chain(qodercli_events())
    {
        remove_devin_hook(hooks, event, &qodercli_command(hook_path, action))?;
    }
    let entries = hooks
        .entry("SessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| std::io::Error::other("qodercli SessionStart hooks must be an array"))?;
    entries.push(json!({"matcher":"*","hooks":[{"type":"command","command":qodercli_command(hook_path,"session"),"timeout":10}]}));
    Ok(())
}
fn remove_qodercli_hooks(config: &mut Value, hook_path: &std::path::Path) -> std::io::Result<bool> {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let mut changed = false;
    for (event, action) in qodercli_events()
        .into_iter()
        .chain(qodercli_removed_events())
    {
        changed |= remove_devin_hook(hooks, event, &qodercli_command(hook_path, action))?;
    }
    Ok(changed)
}

fn kimi_events() -> [(&'static str, Option<&'static str>, &'static str); 12] {
    [
        ("SessionStart", None, "session"),
        ("UserPromptSubmit", None, "working"),
        ("PreToolUse", Some("^(?!AskUserQuestion$).*$"), "working"),
        ("PreToolUse", Some("^AskUserQuestion$"), "blocked"),
        ("PostToolUse", Some("^AskUserQuestion$"), "working"),
        ("PostToolUseFailure", Some("^AskUserQuestion$"), "working"),
        ("SubagentStart", None, "working"),
        ("PreCompact", None, "working"),
        ("PermissionRequest", None, "blocked"),
        ("PermissionResult", None, "working"),
        ("Stop", None, "idle"),
        ("SessionEnd", None, "release"),
    ]
}

fn toml_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn build_kimi_config(content: &str, hook_path: &std::path::Path) -> String {
    let mut result = remove_kimi_config(content);
    while result.ends_with('\n') {
        result.pop();
    }
    if !result.is_empty() {
        result.push_str("\n\n");
    }
    result.push_str(KIMI_BEGIN);
    result.push('\n');
    for (event, matcher, action) in kimi_events() {
        result.push_str("[[hooks]]\nevent = ");
        result.push_str(&toml_quote(event));
        result.push('\n');
        if let Some(matcher) = matcher {
            result.push_str("matcher = ");
            result.push_str(&toml_quote(matcher));
            result.push('\n');
        }
        result.push_str("command = ");
        result.push_str(&toml_quote(&format!(
            "{} {}",
            direct_hook_command(hook_path),
            action
        )));
        result.push_str("\ntimeout = 10\n\n");
    }
    result.push_str(KIMI_END);
    result.push('\n');
    result
}

fn remove_kimi_config(content: &str) -> String {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == KIMI_BEGIN || trimmed == "# >>> herdr kimi integration" {
            inside = true;
            continue;
        }
        if inside {
            if trimmed == KIMI_END || trimmed == "# <<< herdr kimi integration" {
                inside = false;
            }
            continue;
        }
        lines.push(line);
    }
    let mut result = lines.join("\n");
    if content.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

fn droid_events() -> [(&'static str, &'static str); 1] {
    [("SessionStart", "session")]
}
fn droid_removed_events() -> [(&'static str, &'static str); 9] {
    [
        ("SessionStart", "idle"),
        ("UserPromptSubmit", "working"),
        ("PreToolUse", "working"),
        ("PostToolUse", "working"),
        ("Notification", "blocked"),
        ("Stop", "idle"),
        ("SubagentStop", "working"),
        ("PreCompact", "working"),
        ("SessionEnd", "release"),
    ]
}

fn ensure_droid_settings(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<()> {
    let root = config.as_object_mut().ok_or_else(|| {
        std::io::Error::other(format!(
            "droid settings at {} must be a JSON object",
            path.display()
        ))
    })?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("droid settings hooks must be a JSON object"))?;
    for (event, action) in droid_removed_events().into_iter().chain(droid_events()) {
        remove_devin_hook(hooks, event, &devin_hook_command(hook_path, action))?;
    }
    for (event, action) in droid_events() {
        let entries = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                std::io::Error::other(format!("droid {event} hooks must be an array"))
            })?;
        entries.push(json!({"hooks":[{"type":"command","command":devin_hook_command(hook_path, action),"timeout":10}]}));
    }
    Ok(())
}

fn remove_droid_settings(config: &mut Value, hook_path: &std::path::Path) -> std::io::Result<bool> {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let mut changed = false;
    for (event, action) in droid_events().into_iter().chain(droid_removed_events()) {
        changed |= remove_devin_hook(hooks, event, &devin_hook_command(hook_path, action))?;
    }
    Ok(changed)
}

fn remove_droid_legacy(config: &mut Value, hook_path: &std::path::Path) -> std::io::Result<bool> {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let mut changed = false;
    for (event, action) in droid_events().into_iter().chain(droid_removed_events()) {
        changed |= remove_devin_hook(hooks, event, &devin_hook_command(hook_path, action))?;
    }
    Ok(changed)
}

fn devin_events() -> [(&'static str, &'static str); 6] {
    [
        ("SessionStart", "session"),
        ("UserPromptSubmit", "session"),
        ("PreToolUse", "session"),
        ("PostToolUse", "session"),
        ("PermissionRequest", "session"),
        ("Stop", "session"),
    ]
}
fn devin_removed_events() -> [(&'static str, &'static str); 6] {
    [
        ("UserPromptSubmit", "working"),
        ("PreToolUse", "working"),
        ("PostToolUse", "working"),
        ("PermissionRequest", "blocked"),
        ("Stop", "idle"),
        ("SessionEnd", "release"),
    ]
}

fn devin_hook_command(path: &std::path::Path, action: &str) -> String {
    format!("{} {}", direct_hook_command(path), action)
}

fn ensure_devin_hooks(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<()> {
    let root = config.as_object_mut().ok_or_else(|| {
        std::io::Error::other(format!(
            "devin settings at {} must be a JSON object",
            path.display()
        ))
    })?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("devin settings hooks must be a JSON object"))?;
    for (event, action) in devin_removed_events().into_iter().chain(devin_events()) {
        remove_devin_hook(hooks, event, &devin_hook_command(hook_path, action))?;
    }
    for (event, action) in devin_events() {
        let entries = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                std::io::Error::other(format!("devin {event} hooks must be an array"))
            })?;
        entries.push(json!({"hooks":[{"type":"command","command":devin_hook_command(hook_path, action),"timeout":10}]}));
    }
    Ok(())
}

fn remove_devin_hooks(config: &mut Value, hook_path: &std::path::Path) -> std::io::Result<bool> {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let mut changed = false;
    for (event, action) in devin_events().into_iter().chain(devin_removed_events()) {
        changed |= remove_devin_hook(hooks, event, &devin_hook_command(hook_path, action))?;
    }
    Ok(changed)
}

fn remove_devin_hook(
    hooks: &mut serde_json::Map<String, Value>,
    event: &str,
    command: &str,
) -> std::io::Result<bool> {
    let Some(entries) = hooks.get_mut(event).and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let before = entries.len();
    entries.retain(|entry| {
        !entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.get("command").and_then(Value::as_str) == Some(command))
            })
    });
    Ok(before != entries.len())
}

fn cursor_events() -> [&'static str; 6] {
    [
        "sessionStart",
        "beforeSubmitPrompt",
        "beforeShellExecution",
        "beforeMCPExecution",
        "stop",
        "sessionEnd",
    ]
}

fn cursor_hook_command(path: &std::path::Path) -> String {
    format!("{} session", direct_hook_command(path))
}

fn ensure_cursor_hooks(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<()> {
    let root = config.as_object_mut().ok_or_else(|| {
        std::io::Error::other(format!(
            "cursor hooks file at {} must be a JSON object",
            path.display()
        ))
    })?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("cursor hooks must be a JSON object"))?;
    let command = cursor_hook_command(hook_path);
    for event in cursor_events() {
        remove_simple_cursor_hook(hooks, event, &command)?;
    }
    hooks
        .entry("sessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| std::io::Error::other("cursor sessionStart hooks must be an array"))?
        .push(json!({"command": command}));
    Ok(())
}

fn remove_cursor_hooks(config: &mut Value, hook_path: &std::path::Path) -> std::io::Result<bool> {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let command = cursor_hook_command(hook_path);
    let mut changed = false;
    for event in cursor_events() {
        changed |= remove_simple_cursor_hook(hooks, event, &command)?;
    }
    Ok(changed)
}

fn remove_simple_cursor_hook(
    hooks: &mut serde_json::Map<String, Value>,
    event: &str,
    command: &str,
) -> std::io::Result<bool> {
    let Some(entries) = hooks.get_mut(event).and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let before = entries.len();
    entries.retain(|entry| entry.get("command").and_then(Value::as_str) != Some(command));
    Ok(before != entries.len())
}

fn copilot_events() -> [&'static str; 1] {
    ["SessionStart"]
}
fn copilot_removed_events() -> [&'static str; 9] {
    [
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "Stop",
        "agentStop",
        "SessionEnd",
        "notification",
        "error",
    ]
}

fn ensure_copilot_hook(
    config: &mut Value,
    path: &std::path::Path,
    hook_path: &std::path::Path,
) -> std::io::Result<()> {
    let root = config.as_object_mut().ok_or_else(|| {
        std::io::Error::other(format!(
            "copilot settings at {} must be a JSON object",
            path.display()
        ))
    })?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("copilot settings hooks must be a JSON object"))?;
    let command = direct_hook_command(hook_path);
    for event in copilot_removed_events().into_iter().chain(copilot_events()) {
        remove_direct_hook(hooks, event, &command)?;
    }
    let entries = hooks
        .entry("SessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| std::io::Error::other("copilot SessionStart hooks must be an array"))?;
    if !entries.iter().any(|entry| {
        entry.get("type").and_then(Value::as_str) == Some("command")
            && entry.get("powershell").and_then(Value::as_str) == Some(command.as_str())
    }) {
        entries.push(json!({"type":"command","powershell":command,"timeoutSec":10}));
    }
    Ok(())
}

fn remove_copilot_hook(config: &mut Value, hook_path: &std::path::Path) -> std::io::Result<bool> {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let command = direct_hook_command(hook_path);
    let mut changed = false;
    for event in copilot_removed_events().into_iter().chain(copilot_events()) {
        changed |= remove_direct_hook(hooks, event, &command)?;
    }
    Ok(changed)
}

fn remove_direct_hook(
    hooks: &mut serde_json::Map<String, Value>,
    event: &str,
    command: &str,
) -> std::io::Result<bool> {
    let Some(entries) = hooks.get_mut(event).and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let before = entries.len();
    entries.retain(|entry| {
        !(entry.get("type").and_then(Value::as_str) == Some("command")
            && ["command", "bash", "powershell"]
                .into_iter()
                .any(|field| entry.get(field).and_then(Value::as_str) == Some(command)))
    });
    Ok(before != entries.len())
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

#[cfg(test)]
mod tests {
    use super::{
        Target, OMP_EXTENSION_NAME, OPENCODE_PLUGIN_NAME, OPENCODE_TUI_SPEC, PI_EXTENSION_NAME,
    };

    #[test]
    fn targets_match_herdr_names() {
        assert_eq!(Target::Pi.label(), "pi");
        assert_eq!(Target::Omp.label(), "omp");
        assert_eq!(Target::Codex.label(), "codex");
        assert_eq!(Target::Opencode.label(), "opencode");
        assert_eq!(Target::Copilot.label(), "copilot");
        assert_eq!(Target::Cursor.label(), "cursor");
        assert_eq!(Target::Devin.label(), "devin");
        assert_eq!(Target::Droid.label(), "droid");
        assert_eq!(Target::Kimi.label(), "kimi");
        assert_eq!(Target::Qodercli.label(), "qodercli");
        assert_eq!(Target::Qwen.label(), "qwen");
        assert_eq!(Target::Grok.label(), "grok");
        assert_eq!(Target::Kilo.label(), "kilo");
        assert_eq!(Target::Hermes.label(), "hermes");
        assert_eq!(Target::AntigravityCli.label(), "antigravity-cli");
    }

    #[test]
    fn target_paths_match_agent_layouts() {
        assert!(Target::Pi.path().ends_with(PI_EXTENSION_NAME));
        assert!(Target::Omp.path().ends_with(OMP_EXTENSION_NAME));
        assert!(Target::Codex.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Opencode.path().ends_with(OPENCODE_PLUGIN_NAME));
        assert!(Target::Copilot.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Cursor.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Devin.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Droid.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Kimi.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Qodercli.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Qwen.path().ends_with("spindle-agent-session.ps1"));
        assert!(Target::Grok.path().ends_with("spindle-agent-state.ps1"));
        assert!(Target::Kilo.path().ends_with("spindle-agent-state.js"));
        assert!(Target::Hermes.path().ends_with("__init__.py"));
        assert!(Target::AntigravityCli
            .path()
            .ends_with("spindle-agent-state.ps1"));
    }

    #[test]
    fn antigravity_cli_hook_edit_preserves_named_hooks_and_is_idempotent() {
        let hook_path = std::path::Path::new(
            "C:\\Users\\test\\.gemini\\config\\hooks\\spindle-agent-state.ps1",
        );
        let mut config = serde_json::json!({
            "hooks": {
                "custom": {"PreInvocation": [{"type": "command", "command": "keep"}]},
                "spindle": {"old": true}
            },
            "other": true
        });

        super::ensure_antigravity_cli_hook(&mut config, hook_path);
        super::ensure_antigravity_cli_hook(&mut config, hook_path);
        assert_eq!(
            config["hooks"]["custom"]["PreInvocation"][0]["command"],
            "keep"
        );
        assert_eq!(
            config["hooks"]["spindle"]["PreInvocation"][0]["timeout"],
            10
        );
        assert!(config["other"].as_bool().unwrap());
        assert!(super::remove_antigravity_cli_hook(&mut config));
        assert!(config["hooks"]["custom"].is_object());
        assert!(config["hooks"]["spindle"].is_null());
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
    fn copilot_hook_edit_preserves_unrelated_hooks_and_is_idempotent() {
        let hook_path =
            std::path::Path::new("C:\\Users\\test\\.copilot\\hooks\\spindle-agent-state.ps1");
        let command = super::direct_hook_command(hook_path);
        let mut config = serde_json::json!({
            "hooks": { "SessionStart": [
                {"type": "command", "powershell": "custom-hook"},
                {"type": "command", "powershell": command}
            ]},
            "other": true
        });

        super::ensure_copilot_hook(&mut config, hook_path, hook_path).unwrap();
        assert_eq!(config["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert_eq!(
            config["hooks"]["SessionStart"][0]["powershell"],
            "custom-hook"
        );
        assert!(config["other"].as_bool().unwrap());
        assert!(super::remove_copilot_hook(&mut config, hook_path).unwrap());
        assert_eq!(config["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn devin_hook_edit_preserves_nested_user_hooks_and_is_idempotent() {
        let hook_path = std::path::Path::new(
            "C:\\Users\\test\\AppData\\Roaming\\devin\\spindle-agent-state.ps1",
        );
        let mut config = serde_json::json!({
            "hooks": { "SessionEnd": [{"hooks": [{"type": "command", "command": "keep-me"}]}] },
            "other": true
        });
        super::ensure_devin_hooks(&mut config, hook_path, hook_path).unwrap();
        super::ensure_devin_hooks(&mut config, hook_path, hook_path).unwrap();
        assert_eq!(config["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(
            config["hooks"]["SessionEnd"][0]["hooks"][0]["command"],
            "keep-me"
        );
        assert!(super::remove_devin_hooks(&mut config, hook_path).unwrap());
        assert_eq!(
            config["hooks"]["SessionEnd"][0]["hooks"][0]["command"],
            "keep-me"
        );
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
