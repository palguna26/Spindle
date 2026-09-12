//! Best-effort identification of coding agents running inside pane shells.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Codex,
    OpenCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Unknown,
    Idle,
    Working,
    Blocked,
}

impl AgentState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
        }
    }

    pub fn sidebar_marker(self) -> &'static str {
        match self {
            Self::Unknown => "?",
            Self::Idle => "I",
            Self::Working => "W",
            Self::Blocked => "!",
        }
    }
}

impl AgentKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
        }
    }
}

/// Classify only visible, agent-specific signals. Missing signals stay unknown.
/// The rules follow Herdr's Codex/OpenCode manifests and avoid guessing from
/// shell activity alone.
pub(crate) fn detect_state(agent: AgentKind, screen: &str, title: &str) -> AgentState {
    let screen_lower = screen.to_ascii_lowercase();
    let title_lower = title.to_ascii_lowercase();
    let combined = format!("{title_lower}\n{screen_lower}");
    let blocked = combined.contains("action required")
        || combined.contains("permission required")
        || combined.contains("do you trust the contents of this directory?")
        || combined.contains("allow command?")
        || combined.contains("press enter to confirm or esc to cancel")
        || (combined.contains("esc dismiss")
            && (combined.contains("enter confirm")
                || combined.contains("enter submit")
                || combined.contains("enter toggle")));
    if blocked {
        return AgentState::Blocked;
    }

    let working = match agent {
        AgentKind::Codex => {
            title.chars().any(is_codex_spinner)
                || screen.lines().rev().take(3).any(|line| {
                    line.to_ascii_lowercase().contains("working (")
                        && line.to_ascii_lowercase().contains("esc to interrupt")
                })
        }
        AgentKind::OpenCode => {
            [
                "esc to interrupt",
                "ctrl+c to interrupt",
                "press esc to interrupt",
            ]
            .iter()
            .any(|signal| combined.contains(signal))
                || has_progress_bar(screen)
        }
    };
    if working {
        return AgentState::Working;
    }

    if agent == AgentKind::Codex && !title.trim().is_empty() {
        AgentState::Idle
    } else {
        AgentState::Unknown
    }
}

fn is_codex_spinner(character: char) -> bool {
    matches!(
        character,
        '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏'
    )
}

fn has_progress_bar(screen: &str) -> bool {
    screen.lines().any(|line| {
        let mut run = 0;
        for character in line.chars() {
            if matches!(character, '█' | '⬝') {
                run += 1;
                if run >= 4 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    })
}

#[cfg(any(windows, test))]
fn identify_process(name: &str) -> Option<AgentKind> {
    let basename = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let basename = basename.strip_suffix(".exe").unwrap_or(basename);
    match basename.to_ascii_lowercase().as_str() {
        "codex" => Some(AgentKind::Codex),
        "opencode" | "opencode2" | "open-code" => Some(AgentKind::OpenCode),
        _ => None,
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessEntry {
    pid: u32,
    parent_pid: u32,
    name: String,
}

#[cfg(any(windows, test))]
fn identify_descendant(root_pid: u32, processes: &[ProcessEntry]) -> Option<AgentKind> {
    let mut descendants = vec![root_pid];
    let mut index = 0;
    while index < descendants.len() {
        let parent = descendants[index];
        index += 1;
        for process in processes.iter().filter(|entry| entry.parent_pid == parent) {
            if !descendants.contains(&process.pid) {
                descendants.push(process.pid);
                if let Some(agent) = identify_process(&process.name) {
                    return Some(agent);
                }
            }
        }
    }
    None
}

#[cfg(windows)]
pub(crate) fn detect_in_process_tree(root_pid: u32) -> Option<AgentKind> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    struct Snapshot(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for Snapshot {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
    let _snapshot = Snapshot(snapshot);
    // ToolHelp's C struct has no Rust Default implementation; zero-init is its
    // documented initialization pattern before setting dwSize.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut processes = Vec::new();
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while has_entry {
        let name = String::from_utf16_lossy(
            &entry.szExeFile[..entry
                .szExeFile
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(entry.szExeFile.len())],
        );
        processes.push(ProcessEntry {
            pid: entry.th32ProcessID,
            parent_pid: entry.th32ParentProcessID,
            name,
        });
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    identify_descendant(root_pid, &processes)
}

#[cfg(not(windows))]
pub(crate) fn detect_in_process_tree(_root_pid: u32) -> Option<AgentKind> {
    None
}

#[cfg(test)]
mod tests {
    use super::{
        detect_state, identify_descendant, identify_process, AgentKind, AgentState, ProcessEntry,
    };

    #[test]
    fn recognizes_herdr_agent_process_names() {
        assert_eq!(identify_process("codex.exe"), Some(AgentKind::Codex));
        assert_eq!(identify_process("opencode2"), Some(AgentKind::OpenCode));
        assert_eq!(
            identify_process("C:\\tools\\open-code.exe"),
            Some(AgentKind::OpenCode)
        );
        assert_eq!(identify_process("powershell.exe"), None);
        assert_eq!(identify_process("node.exe"), None);
    }

    #[test]
    fn finds_agents_nested_under_the_pane_shell() {
        let processes = vec![
            ProcessEntry {
                pid: 20,
                parent_pid: 10,
                name: "cmd.exe".into(),
            },
            ProcessEntry {
                pid: 30,
                parent_pid: 20,
                name: "codex.exe".into(),
            },
            ProcessEntry {
                pid: 40,
                parent_pid: 1,
                name: "opencode.exe".into(),
            },
        ];
        assert_eq!(identify_descendant(10, &processes), Some(AgentKind::Codex));
        assert_eq!(identify_descendant(40, &processes), None);
    }

    #[test]
    fn follows_herdr_codex_state_signals() {
        assert_eq!(
            detect_state(AgentKind::Codex, "Action Required", "Codex"),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(AgentKind::Codex, "", "Codex ⠋"),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Codex, "", "Codex"),
            AgentState::Idle
        );
        assert_eq!(
            detect_state(AgentKind::Codex, "plain shell", ""),
            AgentState::Unknown
        );
    }

    #[test]
    fn follows_herdr_opencode_blocked_and_working_signals() {
        assert_eq!(
            detect_state(
                AgentKind::OpenCode,
                "△ Permission required\nEsc dismiss · Enter confirm",
                "OpenCode"
            ),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(AgentKind::OpenCode, "Press esc to interrupt", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::OpenCode, "Ready for prompt", ""),
            AgentState::Unknown
        );
    }
}
