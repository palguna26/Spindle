//! Agent configuration locations.
//!
//! These layouts follow Herdr's `src/integration/env.rs`, with Spindle's
//! environment variable names kept as the public override seam.

use std::env;
use std::path::PathBuf;

pub(super) fn home_dir() -> PathBuf {
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

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub(super) fn codex_dir() -> PathBuf {
    env_path("CODEX_HOME").unwrap_or_else(|| home_dir().join(".codex"))
}

pub(super) fn copilot_dir() -> PathBuf {
    env_path("COPILOT_HOME").unwrap_or_else(|| home_dir().join(".copilot"))
}

pub(super) fn cursor_dir() -> PathBuf {
    env_path("CURSOR_CONFIG_DIR").unwrap_or_else(|| home_dir().join(".cursor"))
}

pub(super) fn devin_dir() -> PathBuf {
    env_path("DEVIN_CONFIG_DIR")
        .or_else(|| env_path("APPDATA").map(|path| path.join("devin")))
        .unwrap_or_else(|| home_dir().join(".config").join("devin"))
}

pub(super) fn droid_dir() -> PathBuf {
    home_dir().join(".factory")
}

pub(super) fn kimi_dir() -> PathBuf {
    env_path("KIMI_CODE_HOME").unwrap_or_else(|| home_dir().join(".kimi-code"))
}

pub(super) fn qodercli_dir() -> PathBuf {
    env_path("QODERCLI_CONFIG_DIR").unwrap_or_else(|| home_dir().join(".qoder"))
}

pub(super) fn qwen_dir() -> PathBuf {
    env_path("QWEN_HOME").unwrap_or_else(|| home_dir().join(".qwen"))
}

pub(super) fn grok_dir() -> PathBuf {
    env_path("GROK_CONFIG_DIR")
        .or_else(|| env_path("GROK_HOME"))
        .unwrap_or_else(|| home_dir().join(".grok"))
}

pub(super) fn kilo_dir() -> PathBuf {
    env_path("KILO_CONFIG_DIR")
        .or_else(|| env_path("KILO_HOME"))
        .unwrap_or_else(|| home_dir().join(".config").join("kilo"))
}

pub(super) fn hermes_dir() -> PathBuf {
    env_path("HERMES_HOME")
        .or_else(|| env_path("LOCALAPPDATA").map(|path| path.join("hermes")))
        .unwrap_or_else(|| home_dir().join(".hermes"))
}

pub(super) fn antigravity_cli_dir() -> PathBuf {
    env_path("ANTIGRAVITY_CLI_CONFIG_DIR")
        .unwrap_or_else(|| home_dir().join(".gemini").join("config"))
}

pub(super) fn opencode_dir() -> PathBuf {
    home_dir().join(".config").join("opencode")
}

pub(super) fn claude_dir() -> PathBuf {
    env_path("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home_dir().join(".claude"))
}

pub(super) fn pi_extension_dir() -> PathBuf {
    env_path("PI_CODING_AGENT_DIR")
        .unwrap_or_else(|| home_dir().join(".pi").join("agent"))
        .join("extensions")
}

pub(super) fn omp_extension_dir() -> PathBuf {
    if let Some(path) = env_path("PI_CODING_AGENT_DIR") {
        return path.join("extensions");
    }
    let config = env_path("PI_CONFIG_DIR").unwrap_or_else(|| PathBuf::from(".omp"));
    home_dir().join(config).join("agent").join("extensions")
}
