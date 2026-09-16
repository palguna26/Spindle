use std::env;
use std::path::PathBuf;

use serde::Serialize;

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
            Self::Codex => home_dir().join(".codex").join("herdr-agent-state.ps1"),
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
        assert!(Target::Codex.path().ends_with("herdr-agent-state.ps1"));
        assert!(Target::Opencode.path().ends_with("spindle-agent-state.js"));
    }
}
