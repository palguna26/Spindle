//! Best-effort identification of coding agents running inside pane shells.

use serde::{Deserialize, Serialize};

#[path = "detect/agents/mod.rs"]
mod agents;
use agents::{cline_permission_required, qodercli_is_working, qodercli_permission_required};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Pi,
    QoderCli,
    Droid,
    Kiro,
    Cline,
    Claude,
    Codex,
    Gemini,
    OpenCode,
    GithubCopilot,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDisplayState {
    Unknown,
    Idle,
    Working,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentProcessScan {
    Found(AgentKind),
    Absent,
    Unavailable,
}

impl AgentDisplayState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }

    pub fn sidebar_marker(self) -> &'static str {
        match self {
            Self::Unknown => "?",
            Self::Idle => "I",
            Self::Working => "W",
            Self::Blocked => "!",
            Self::Done => "✓",
        }
    }
}

impl AgentKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pi => "Pi",
            Self::QoderCli => "Qoder CLI",
            Self::Droid => "Droid",
            Self::Kiro => "Kiro",
            Self::Cline => "Cline",
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::OpenCode => "OpenCode",
            Self::GithubCopilot => "GitHub Copilot",
        }
    }
}

/// Classify only visible, agent-specific signals. Missing signals stay unknown.
/// The rules follow high-confidence signals from Herdr's agent manifests and
/// avoid guessing from shell activity alone.
#[cfg(test)]
pub(crate) fn detect_state(agent: AgentKind, screen: &str, title: &str) -> AgentState {
    detect_state_with_osc(agent, screen, title, "")
}

pub(crate) fn detect_state_with_osc(
    agent: AgentKind,
    screen: &str,
    title: &str,
    osc_progress: &str,
) -> AgentState {
    let screen_lower = screen.to_ascii_lowercase();
    let title_lower = title.to_ascii_lowercase();
    let combined = format!("{title_lower}\n{screen_lower}");
    let recent = recent_nonempty_lines(screen, 20).to_ascii_lowercase();
    let bottom_three = recent_nonempty_lines(screen, 3).to_ascii_lowercase();
    let bottom_twelve = recent_nonempty_lines(screen, 12).to_ascii_lowercase();
    let bottom_five = recent_nonempty_lines(screen, 5).to_ascii_lowercase();
    let bottom_eight = recent_nonempty_lines(screen, 8).to_ascii_lowercase();
    let blocked = match agent {
        AgentKind::Pi => false,
        AgentKind::QoderCli => qodercli_permission_required(&recent),
        AgentKind::Droid => droid_permission_required(&recent, &bottom_eight),
        AgentKind::Kiro => kiro_permission_required(&recent),
        AgentKind::Cline => cline_permission_required(&recent),
        AgentKind::Codex => {
            combined.contains("action required")
                || codex_trust_directory_prompt(
                    &top_nonempty_lines(screen, 20).to_ascii_lowercase(),
                )
                || combined.contains("allow command?")
                || combined.contains("press enter to confirm or esc to cancel")
                || codex_recent_blocker(&recent)
        }
        AgentKind::OpenCode => opencode_permission_required(&recent),
        AgentKind::Claude => claude_permission_required(&recent),
        AgentKind::Gemini => gemini_permission_required(&recent),
        AgentKind::GithubCopilot => copilot_permission_required(&recent),
    };
    if blocked {
        return AgentState::Blocked;
    }

    let working = match agent {
        AgentKind::Pi => recent.contains("working..."),
        AgentKind::QoderCli => qodercli_is_working(&recent),
        AgentKind::Droid => recent.contains("esc to stop"),
        AgentKind::Kiro => kiro_is_working(&recent),
        AgentKind::Cline => !recent.is_empty(),
        AgentKind::Codex => {
            title.chars().any(is_codex_spinner)
                || (!bottom_three.contains("conversation interrupted")
                    && bottom_three.lines().any(|line| {
                        line.contains("working (") && line.contains("esc to interrupt")
                    }))
        }
        AgentKind::OpenCode => {
            [
                "esc to interrupt",
                "ctrl+c to interrupt",
                "press esc to interrupt",
            ]
            .iter()
            .any(|signal| combined.contains(signal))
                || combined.lines().any(|line| {
                    line.contains("opencode")
                        && (line.contains("esc to interrupt")
                            || line.contains("esc again to interrupt"))
                })
                || has_progress_bar(screen)
        }
        AgentKind::Claude => claude_is_working(&bottom_twelve, &bottom_five, title),
        AgentKind::Gemini => recent.contains("esc to cancel"),
        AgentKind::GithubCopilot => {
            copilot_has_cancel_hint(&recent) || copilot_background_agents_working(&recent)
        }
    };
    if working {
        return AgentState::Working;
    }

    if has_visible_idle_signal(agent, &bottom_five, title, osc_progress) {
        AgentState::Idle
    } else {
        AgentState::Unknown
    }
}

pub(crate) fn has_visible_idle_signal(
    agent: AgentKind,
    screen: &str,
    title: &str,
    osc_progress: &str,
) -> bool {
    match agent {
        AgentKind::Pi => false,
        AgentKind::QoderCli => false,
        AgentKind::Droid => false,
        AgentKind::Kiro => {
            screen.contains("ask a question or describe a task")
                && screen.contains("/copy to clipboard")
                && !screen.contains("kiro is working")
                && !screen.contains("esc to cancel")
        }
        AgentKind::Cline => false,
        AgentKind::Codex => !title.trim().is_empty(),
        AgentKind::Claude => {
            title.starts_with("\u{2733} ")
                || osc_progress.starts_with("4;0")
                || recent_nonempty_lines(screen, 3)
                    .lines()
                    .any(|line| line.trim_start().starts_with('\u{276f}'))
        }
        AgentKind::OpenCode => false,
        AgentKind::Gemini => false,
        AgentKind::GithubCopilot => false,
    }
}

fn claude_permission_required(recent: &str) -> bool {
    recent.contains("waiting for permission")
        || recent.contains("do you want to allow this connection?")
        || recent.contains("review your answers")
        || (recent.contains("esc to cancel")
            && (recent.contains("do you want to proceed?")
                || recent.contains("enter to confirm")
                || recent.contains("enter to select")
                || recent.contains("arrow keys to navigate")
                || recent.contains("tab/arrow keys to navigate")))
}

fn droid_permission_required(recent: &str, bottom_eight: &str) -> bool {
    agents::droid_permission_required(recent, bottom_eight)
}

fn kiro_permission_required(recent: &str) -> bool {
    agents::kiro_permission_required(recent)
}

fn kiro_is_working(recent: &str) -> bool {
    agents::kiro_is_working(recent)
}

fn gemini_permission_required(recent: &str) -> bool {
    recent.contains("│ apply this change")
        || recent.contains("│ allow execution")
        || (recent.contains("yes")
            && (recent.contains("waiting for user confirmation")
                || recent.contains("│ do you want to proceed")
                || recent.contains("do you want to proceed?")))
        || recent.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with('❯') && (line.contains("yes") || line.contains("allow"))
        })
}

fn copilot_permission_required(recent: &str) -> bool {
    let escape_hint = recent.contains("esc to cancel") || recent.contains("esc cancel");
    let confirmation_hint = [
        "enter to select",
        "enter to confirm",
        "enter to submit",
        "enter accept",
    ]
    .iter()
    .any(|hint| recent.contains(hint));
    escape_hint && confirmation_hint
}

fn copilot_has_cancel_hint(recent: &str) -> bool {
    [
        "esc to cancel",
        "esc cancel",
        "esc again to cancel",
        "esc interrupt",
    ]
    .iter()
    .any(|hint| recent.contains(hint))
}

fn copilot_background_agents_working(recent: &str) -> bool {
    recent_nonempty_lines(recent, 6).lines().any(|line| {
        line.trim_start()
            .strip_prefix('\u{25ce}')
            .is_some_and(|message| {
                message
                    .trim_start()
                    .starts_with("waiting for background agents")
            })
    })
}

fn claude_is_working(bottom: &str, bottom_five: &str, title: &str) -> bool {
    let title_spinner = title.chars().next().is_some_and(is_claude_spinner)
        && title.chars().nth(1).is_some_and(char::is_whitespace);
    title_spinner
        || bottom.lines().any(claude_live_turn_line)
        || bottom.lines().any(claude_background_agents_line)
        || claude_mcp_tasks_running(bottom)
        || (bottom_five.lines().any(claude_btw_command_line)
            && bottom_five
                .lines()
                .any(|line| line.trim_end().ends_with("esc to close")))
}

fn claude_btw_command_line(line: &str) -> bool {
    let line = line.trim_start();
    line.strip_prefix("/btw")
        .is_some_and(|suffix| suffix.chars().next().is_none_or(char::is_whitespace))
}

fn claude_mcp_tasks_running(bottom: &str) -> bool {
    let lines: Vec<_> = bottom.lines().collect();
    let has_running_task_line = lines.iter().enumerate().any(|(index, line)| {
        let activity = line.trim_start();
        let Some(marker) = activity.chars().next() else {
            return false;
        };
        let activity_text = activity[marker.len_utf8()..].trim_start();
        if !is_claude_activity_marker(marker)
            || activity_text.chars().next().is_none_or(char::is_whitespace)
        {
            return false;
        }

        claude_mcp_task_count_line(activity_text)
            || (1..=4).any(|offset| {
                let summary_index = index + offset;
                summary_index < lines.len()
                    && lines[index + 1..summary_index]
                        .iter()
                        .all(|line| matches!(line.chars().next(), Some(' ' | '\t')))
                    && claude_mcp_task_count_line(lines[summary_index])
            })
    });
    has_running_task_line
        && ![
            "do you want to proceed?",
            "esc to cancel",
            "waiting for permission",
            "do you want to allow this connection?",
            "tab to amend",
            "ctrl+e to explain",
        ]
        .iter()
        .any(|signal| bottom.contains(signal))
}

fn claude_mcp_task_count_line(line: &str) -> bool {
    let Some((_, summary)) = line.split_once('\u{00b7}') else {
        return false;
    };
    let mut parts = summary.split_whitespace();
    let Some(count) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
        return false;
    };
    count > 0
        && matches!(parts.next(), Some("mcp"))
        && matches!(parts.next(), Some("task" | "tasks"))
        && matches!(parts.next(), Some("still"))
        && matches!(parts.next(), Some("running"))
        && parts.next().is_none()
}

fn claude_live_turn_line(line: &str) -> bool {
    let line = line.trim_start();
    if let Some(spinner) = line
        .chars()
        .next()
        .filter(|spinner| matches!(spinner, '\u{23f8}' | '\u{23f5}'))
    {
        let after_spinner = &line[spinner.len_utf8()..];
        return after_spinner
            .split_once("esc to interrupt")
            .is_some_and(|(_, suffix)| {
                suffix
                    .chars()
                    .next()
                    .is_none_or(|ch| ch.is_whitespace() || ch == '\u{00b7}')
            });
    }
    let Some((before_ellipsis, after_ellipsis)) = line.split_once('\u{2026}') else {
        return false;
    };
    let before_ellipsis = before_ellipsis.trim_start();
    let Some(marker) = before_ellipsis.chars().next() else {
        return false;
    };
    let activity = &before_ellipsis[marker.len_utf8()..];
    if !is_claude_activity_marker(marker)
        || !activity.chars().next().is_some_and(char::is_whitespace)
        || activity.trim().is_empty()
    {
        return false;
    }
    claude_ellipsis_is_live(after_ellipsis)
}

fn claude_ellipsis_is_live(after_ellipsis: &str) -> bool {
    let suffix = after_ellipsis.trim_start();
    if suffix.is_empty() {
        return true;
    }
    let Some(duration) = suffix.strip_prefix('(') else {
        return false;
    };
    let digits = duration.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let Some(unit) = duration.chars().nth(digits) else {
        return false;
    };
    if !matches!(unit, 's' | 'm' | 'h') {
        return false;
    }
    duration
        .chars()
        .nth(digits + 1)
        .is_none_or(|next| next.is_whitespace() || matches!(next, '\u{00b7}' | ')'))
}

fn claude_background_agents_line(line: &str) -> bool {
    let line = line.trim_start();
    let Some(marker) = line.chars().next() else {
        return false;
    };
    if !is_claude_activity_marker(marker) {
        return false;
    }
    let Some(rest) = line[marker.len_utf8()..]
        .trim_start()
        .strip_prefix("waiting for ")
    else {
        return false;
    };
    let Some((count, rest)) = rest.split_once(" background agent") else {
        return false;
    };
    count.parse::<u32>().is_ok_and(|count| count > 0)
        && rest.strip_prefix('s').unwrap_or(rest).trim() == "to finish"
}

fn is_claude_spinner(character: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&character) || ('\u{25d0}'..='\u{25d3}').contains(&character)
}

fn is_claude_activity_marker(character: char) -> bool {
    matches!(
        character,
        '*' | '\u{00b7}' | '\u{2722}' | '\u{2736}' | '\u{273b}' | '\u{273d}'
    )
}

fn opencode_permission_required(recent: &str) -> bool {
    if recent.contains("\u{25b3} permission required") {
        return true;
    }

    recent.contains("esc dismiss")
        && (recent.contains("enter confirm")
            || recent.contains("enter submit")
            || recent.contains("enter toggle"))
        && (recent.contains("\u{2191}\u{2193} select") || recent.contains("\u{21c6} tab"))
}

fn recent_nonempty_lines(screen: &str, limit: usize) -> String {
    screen
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

fn top_nonempty_lines(screen: &str, limit: usize) -> String {
    screen
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(limit)
        .collect::<Vec<_>>()
        .join("\n")
}

fn codex_trust_directory_prompt(top: &str) -> bool {
    let Some(first_line) = top.lines().next() else {
        return false;
    };
    let directory = first_line.strip_prefix("> you are in ").unwrap_or("");
    !directory.trim().is_empty() && top.contains("do you trust the contents of this directory?")
}

fn codex_recent_blocker(recent: &str) -> bool {
    let startup_update = recent.contains("update available!")
        && recent.contains("update now")
        && recent.contains("skip until next version")
        && recent.contains("press enter to continue");
    let weak_choice = recent.contains("[y/n]") || recent.contains("yes (y)");
    let question_with_choice = recent.lines().enumerate().any(|(index, line)| {
        let prompt = line.contains("do you want to") || line.contains("would you like to");
        prompt
            && recent
                .lines()
                .skip(index + 1)
                .take(4)
                .any(|choice| choice.contains("yes") || choice.contains('❯'))
    });
    startup_update || weak_choice || question_with_choice
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
    let basename = name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    let basename = [".exe", ".cmd", ".bat", ".ps1"]
        .iter()
        .find_map(|suffix| basename.strip_suffix(suffix))
        .unwrap_or(&basename);
    match basename {
        "claude" | "claude-code" => Some(AgentKind::Claude),
        "pi" => Some(AgentKind::Pi),
        "qodercli" | "qoderclicn" | "qoder" | "qodercn" => Some(AgentKind::QoderCli),
        "droid" => Some(AgentKind::Droid),
        "kiro" | "kiro-cli" => Some(AgentKind::Kiro),
        "cline" => Some(AgentKind::Cline),
        "codex" => Some(AgentKind::Codex),
        "gemini" => Some(AgentKind::Gemini),
        "opencode" | "opencode2" | "open-code" => Some(AgentKind::OpenCode),
        "copilot" | "github-copilot" | "ghcs" => Some(AgentKind::GithubCopilot),
        _ => None,
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessEntry {
    pid: u32,
    parent_pid: u32,
    name: String,
    command_line: Option<String>,
}

#[cfg(any(windows, test))]
fn identify_descendant(root_pid: u32, processes: &[ProcessEntry]) -> Option<AgentKind> {
    descendant_process_ids(root_pid, processes)
        .into_iter()
        .filter_map(|pid| processes.iter().find(|process| process.pid == pid))
        .find_map(|process| {
            identify_process_command(&process.name, process.command_line.as_deref())
        })
}

#[cfg(any(windows, test))]
fn classify_agent_process_scan(
    root_present: bool,
    enumeration_complete: bool,
    command_line_read_failed: bool,
    detected: Option<AgentKind>,
) -> AgentProcessScan {
    if let Some(agent) = detected {
        AgentProcessScan::Found(agent)
    } else if root_present && enumeration_complete && !command_line_read_failed {
        AgentProcessScan::Absent
    } else {
        AgentProcessScan::Unavailable
    }
}

#[cfg(any(windows, test))]
fn descendant_process_ids(root_pid: u32, processes: &[ProcessEntry]) -> Vec<u32> {
    let mut descendants = vec![root_pid];
    let mut index = 0;
    while index < descendants.len() {
        let parent = descendants[index];
        index += 1;
        for process in processes.iter().filter(|entry| entry.parent_pid == parent) {
            if !descendants.contains(&process.pid) {
                descendants.push(process.pid);
            }
        }
    }
    descendants.into_iter().skip(1).collect()
}

#[cfg(windows)]
pub(crate) fn detect_in_process_tree(root_pid: u32) -> AgentProcessScan {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return AgentProcessScan::Unavailable;
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
    if unsafe { Process32FirstW(snapshot, &mut entry) } == 0 {
        return AgentProcessScan::Unavailable;
    }
    let enumeration_complete = loop {
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
            command_line: None,
            name,
        });
        if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
            break unsafe { GetLastError() } == ERROR_NO_MORE_FILES;
        }
    };
    let root_present = processes.iter().any(|process| process.pid == root_pid);
    let descendant_ids = descendant_process_ids(root_pid, &processes);
    let mut command_line_read_failed = false;
    for process in &mut processes {
        if descendant_ids.contains(&process.pid)
            && matches!(
                process.name.to_ascii_lowercase().as_str(),
                "cmd.exe" | "node.exe" | "powershell.exe" | "pwsh.exe"
            )
        {
            process.command_line = read_command_line(process.pid);
            command_line_read_failed |= process.command_line.is_none();
        }
    }
    classify_agent_process_scan(
        root_present,
        enumeration_complete,
        command_line_read_failed,
        identify_descendant(root_pid, &processes),
    )
}

#[cfg(windows)]
fn read_command_line(pid: u32) -> Option<String> {
    use std::ffi::c_void;
    use std::mem::{size_of, MaybeUninit};
    use std::ptr::null_mut;
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, STATUS_SUCCESS, UNICODE_STRING};
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Peb {
        reserved1: [u8; 2],
        being_debugged: u8,
        reserved2: u8,
        reserved3: [*mut c_void; 2],
        ldr: *mut c_void,
        process_parameters: *mut ProcessParameters,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ProcessParameters {
        maximum_length: u32,
        length: u32,
        flags: u32,
        debug_flags: u32,
        console_handle: HANDLE,
        console_flags: u32,
        standard_input: HANDLE,
        standard_output: HANDLE,
        standard_error: HANDLE,
        current_directory: [usize; 3],
        dll_path: UNICODE_STRING,
        image_path_name: UNICODE_STRING,
        command_line: UNICODE_STRING,
    }
    #[repr(C)]
    struct BasicInfo {
        exit_status: i32,
        peb_base_address: *mut Peb,
        affinity_mask: usize,
        base_priority: i32,
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
    }
    unsafe fn read_remote<T: Copy>(process: HANDLE, address: *const c_void) -> Option<T> {
        if address.is_null() {
            return None;
        }
        let mut value = MaybeUninit::<T>::uninit();
        let mut bytes_read = 0;
        let ok = ReadProcessMemory(
            process,
            address,
            value.as_mut_ptr().cast(),
            size_of::<T>(),
            &mut bytes_read,
        ) != 0;
        (ok && bytes_read == size_of::<T>()).then(|| value.assume_init())
    }

    let process =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if process.is_null() {
        return None;
    }
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let _handle = Handle(process);
    let mut basic = MaybeUninit::<BasicInfo>::uninit();
    let status = unsafe {
        NtQueryInformationProcess(
            process,
            ProcessBasicInformation,
            basic.as_mut_ptr().cast(),
            size_of::<BasicInfo>() as u32,
            null_mut(),
        )
    };
    if status != STATUS_SUCCESS {
        return None;
    }
    let basic = unsafe { basic.assume_init() };
    let peb = unsafe { read_remote::<Peb>(process, basic.peb_base_address.cast()) }?;
    let params =
        unsafe { read_remote::<ProcessParameters>(process, peb.process_parameters.cast()) }?;
    let text = params.command_line;
    if text.Buffer.is_null() || text.Length == 0 || text.Length % 2 != 0 {
        return None;
    }
    let mut wide = vec![0u16; usize::from(text.Length / 2)];
    let mut bytes_read = 0;
    let ok = unsafe {
        ReadProcessMemory(
            process,
            text.Buffer.cast(),
            wide.as_mut_ptr().cast(),
            usize::from(text.Length),
            &mut bytes_read,
        )
    } != 0;
    (ok && bytes_read == usize::from(text.Length))
        .then(|| String::from_utf16(&wide).ok())
        .flatten()
}

#[cfg(any(windows, test))]
fn identify_process_command(name: &str, command_line: Option<&str>) -> Option<AgentKind> {
    if let Some(agent) = identify_process(name) {
        return Some(agent);
    }
    let basename = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let executable = basename
        .strip_suffix(".exe")
        .unwrap_or(basename)
        .to_ascii_lowercase();
    let argv = parse_windows_command_line(command_line?)?;
    match executable.as_str() {
        "cmd" => {
            let command = argv
                .iter()
                .position(|arg| matches!(arg.to_ascii_lowercase().as_str(), "/c" | "/k"))
                .and_then(|switch| argv.get(switch + 1))?;
            let first_command_arg = parse_windows_command_line(command)?.into_iter().next()?;
            identify_process(&first_command_arg)
        }
        "node" => argv.iter().skip(1).find_map(|arg| {
            let normalized = arg.replace('/', "\\").to_ascii_lowercase();
            if normalized
                .ends_with("\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\cli.js")
                || normalized.ends_with(
                    "\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\bundle\\cli.js",
                )
            {
                Some(AgentKind::Pi)
            } else if normalized.ends_with("\\node_modules\\codex\\bin\\codex.js")
                || normalized.ends_with("\\node_modules\\@openai\\codex\\bin\\codex.js")
            {
                Some(AgentKind::Codex)
            } else if normalized.ends_with("\\node_modules\\opencode-ai\\bin\\opencode")
                || normalized.ends_with("\\node_modules\\opencode-ai\\bin\\opencode.js")
            {
                Some(AgentKind::OpenCode)
            } else if normalized.ends_with("\\node_modules\\@google\\gemini-cli\\dist\\index.js")
                || normalized.ends_with("\\node_modules\\@google\\gemini-cli\\bundle\\gemini.js")
            {
                Some(AgentKind::Gemini)
            } else {
                None
            }
        }),
        "bun" => argv.iter().skip(1).find_map(|arg| {
            let normalized = arg.replace('/', "\\").to_ascii_lowercase();
            (normalized.ends_with("\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\cli.js")
                || normalized.ends_with(
                    "\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\bundle\\cli.js",
                ))
            .then_some(AgentKind::Pi)
        }),
        "powershell" | "pwsh" => argv
            .iter()
            .find(|arg| arg.to_ascii_lowercase().ends_with(".ps1"))
            .and_then(|script| identify_process(script)),
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn parse_windows_command_line(command_line: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in command_line.chars() {
        match character {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    (!args.is_empty()).then_some(args)
}

#[cfg(not(windows))]
pub(crate) fn detect_in_process_tree(_root_pid: u32) -> AgentProcessScan {
    AgentProcessScan::Unavailable
}

#[cfg(test)]
mod tests {
    use super::{
        classify_agent_process_scan, detect_state, detect_state_with_osc, identify_descendant,
        identify_process, identify_process_command, AgentKind, AgentProcessScan, AgentState,
        ProcessEntry,
    };

    #[test]
    fn process_scan_failures_are_not_treated_as_agent_exit() {
        assert_eq!(
            classify_agent_process_scan(true, true, false, None),
            AgentProcessScan::Absent
        );
        for (root_present, enumeration_complete, command_line_read_failed) in [
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            assert_eq!(
                classify_agent_process_scan(
                    root_present,
                    enumeration_complete,
                    command_line_read_failed,
                    None
                ),
                AgentProcessScan::Unavailable
            );
        }
        assert_eq!(
            classify_agent_process_scan(false, false, true, Some(AgentKind::Claude)),
            AgentProcessScan::Found(AgentKind::Claude)
        );
    }

    #[test]
    fn recognizes_herdr_agent_process_names() {
        assert_eq!(identify_process("pi.exe"), Some(AgentKind::Pi));
        assert_eq!(identify_process("qodercli.exe"), Some(AgentKind::QoderCli));
        assert_eq!(identify_process("qoder.cmd"), Some(AgentKind::QoderCli));
        assert_eq!(identify_process("qodercn"), Some(AgentKind::QoderCli));
        assert_eq!(identify_process("droid.exe"), Some(AgentKind::Droid));
        assert_eq!(identify_process("kiro.exe"), Some(AgentKind::Kiro));
        assert_eq!(identify_process("kiro-cli.cmd"), Some(AgentKind::Kiro));
        assert_eq!(identify_process("cline.cmd"), Some(AgentKind::Cline));
        assert_eq!(identify_process("claude.exe"), Some(AgentKind::Claude));
        assert_eq!(identify_process("claude-code.cmd"), Some(AgentKind::Claude));
        assert_eq!(identify_process("codex.exe"), Some(AgentKind::Codex));
        assert_eq!(identify_process("gemini.cmd"), Some(AgentKind::Gemini));
        assert_eq!(identify_process("opencode2"), Some(AgentKind::OpenCode));
        assert_eq!(
            identify_process("copilot.exe"),
            Some(AgentKind::GithubCopilot)
        );
        assert_eq!(identify_process("ghcs.cmd"), Some(AgentKind::GithubCopilot));
        assert_eq!(AgentKind::Pi.label(), "Pi");
        assert_eq!(AgentKind::QoderCli.label(), "Qoder CLI");
        assert_eq!(AgentKind::Droid.label(), "Droid");
        assert_eq!(AgentKind::Kiro.label(), "Kiro");
        assert_eq!(AgentKind::Cline.label(), "Cline");
        assert_eq!(AgentKind::GithubCopilot.label(), "GitHub Copilot");
        assert_eq!(
            identify_process("C:\\tools\\open-code.exe"),
            Some(AgentKind::OpenCode)
        );
        assert_eq!(identify_process("powershell.exe"), None);
        assert_eq!(identify_process("node.exe"), None);
    }

    #[test]
    fn follows_herdr_claude_state_signals_and_priority() {
        assert_eq!(
            detect_state_with_osc(
                AgentKind::Claude,
                "Do you want to proceed?\nEsc to cancel\nEnter to confirm",
                "",
                "4;0;"
            ),
            AgentState::Blocked,
            "a visible blocker must beat stale idle progress"
        );
        assert_eq!(
            detect_state(AgentKind::Claude, "", "\u{280b} Claude"),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Claude, "", "Claude"),
            AgentState::Unknown
        );
        assert_eq!(
            detect_state_with_osc(AgentKind::Claude, "", "", "4;0;"),
            AgentState::Idle
        );
        assert_eq!(
            detect_state(AgentKind::Claude, "Ready\n❯ ", ""),
            AgentState::Idle
        );
    }

    #[test]
    fn follows_herdr_pi_working_signal() {
        assert_eq!(
            detect_state(AgentKind::Pi, "Working...", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Pi, "working...", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Pi, "Working", ""),
            AgentState::Unknown,
            "the manifest requires the full literal working marker"
        );
        assert_eq!(
            detect_state(AgentKind::Pi, "Ready for input", ""),
            AgentState::Unknown
        );
    }

    #[test]
    fn follows_herdr_qodercli_blocked_and_working_signals() {
        for prompt in [
            "Permission required",
            "Allow once or always?",
            "Asking user",
            "Enter your response",
            "Review your answers:",
            "Shell awaiting input",
            "Waiting for user confirmation\nYes / No",
            "Awaiting approval: Allow / Reject",
        ] {
            assert_eq!(
                detect_state(AgentKind::QoderCli, prompt, ""),
                AgentState::Blocked,
                "expected blocker for {prompt:?}"
            );
        }
        assert_eq!(
            detect_state(AgentKind::QoderCli, "Waiting for user confirmation", ""),
            AgentState::Unknown,
            "confirmation text alone is not enough"
        );
        assert_eq!(
            detect_state(AgentKind::QoderCli, "(Esc to cancel, press q to quit)", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::QoderCli, "⠋ Thinking", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::QoderCli, "⠋ 123", ""),
            AgentState::Unknown,
            "the manifest requires spinner text with an alphabetic character"
        );
    }

    #[test]
    fn follows_herdr_droid_blocked_and_working_signals() {
        assert_eq!(
            detect_state(
                AgentKind::Droid,
                "Confirm execution\nEnter to select\nEsc to cancel\n↑↓ to navigate\n> Yes, allow",
                ""
            ),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(
                AgentKind::Droid,
                "Choose an option\nEnter select · Esc cancel · ↑/↓ navigate",
                ""
            ),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(
                AgentKind::Droid,
                "Enter to select\nEsc to cancel\n↑↓ to navigate",
                ""
            ),
            AgentState::Unknown,
            "selection hints without an allow/cancel choice are not a blocker"
        );
        assert_eq!(
            detect_state(
                AgentKind::Droid,
                "Old menu:\nEnter select · Esc cancel · ↑↓ navigate\n\n1\n2\n3\n4\n5\n6\n7\nReady",
                ""
            ),
            AgentState::Unknown,
            "the menu rule only inspects the last eight non-empty lines"
        );
        assert_eq!(
            detect_state(AgentKind::Droid, "Esc to stop", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Droid, "Working", ""),
            AgentState::Unknown
        );
    }

    #[test]
    fn follows_herdr_kiro_blocked_working_and_idle_signals() {
        for prompt in [
            "Tool requires approval\nYes, single permission",
            "Tool requires approval\nTrust, always allow",
            "Tool requires approval\nNo (Tab to edit)",
            "Pending from subagents: 2 tool approvals\nApprove all pending",
            "Pending from subagents: tool approval\nExit (cancel subagents)",
        ] {
            assert_eq!(
                detect_state(AgentKind::Kiro, prompt, ""),
                AgentState::Blocked,
                "expected blocker for {prompt:?}"
            );
        }
        assert_eq!(
            detect_state(AgentKind::Kiro, "Requires approval", ""),
            AgentState::Unknown,
            "approval text without a visible choice is not a blocker"
        );
        assert_eq!(
            detect_state(AgentKind::Kiro, "Kiro is working", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Kiro, "Esc to cancel\n◑ Searching", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Kiro, "Esc to cancel\n◑ 123", ""),
            AgentState::Unknown,
            "the spinner hint requires alphabetic status text"
        );
        assert_eq!(
            detect_state(
                AgentKind::Kiro,
                "Ask a question or describe a task\n/copy to clipboard",
                ""
            ),
            AgentState::Idle
        );
        assert_eq!(
            detect_state(
                AgentKind::Kiro,
                "Ask a question or describe a task\n/copy to clipboard\nKiro is working",
                ""
            ),
            AgentState::Working,
            "the working marker suppresses the idle prompt"
        );
    }

    #[test]
    fn follows_herdr_cline_tool_approval_and_working_rules() {
        for prompt in [
            "Let Cline use this tool",
            "[Act Mode] Execute command? Yes",
            "[Act Mode] Use this tool? Yes",
            "[Plan Mode] Execute command? Yes",
            "[Plan Mode] Use this tool? Yes",
        ] {
            assert_eq!(
                detect_state(AgentKind::Cline, prompt, ""),
                AgentState::Blocked,
                "expected blocker for {prompt:?}"
            );
        }
        assert_eq!(
            detect_state(AgentKind::Cline, "[Act Mode] Execute command? No", ""),
            AgentState::Working,
            "an incomplete approval prompt falls through to Herdr's visible-output working rule"
        );
        assert_eq!(
            detect_state(AgentKind::Cline, "ordinary visible Cline output", ""),
            AgentState::Working
        );
        assert_eq!(detect_state(AgentKind::Cline, "", ""), AgentState::Unknown);
    }

    #[test]
    fn follows_herdr_copilot_blocker_and_working_signals() {
        assert_eq!(
            detect_state(
                AgentKind::GithubCopilot,
                "Select an option\nEsc to cancel\nEnter to confirm",
                ""
            ),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(AgentKind::GithubCopilot, "Esc again to cancel", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(
                AgentKind::GithubCopilot,
                "\u{25ce} Waiting for background agents",
                ""
            ),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::GithubCopilot, "Esc to cancel", ""),
            AgentState::Working,
            "a lone cancel hint is working chrome, not a confirmation blocker"
        );
        assert_eq!(
            detect_state(AgentKind::GithubCopilot, "Normal prompt", ""),
            AgentState::Unknown
        );
    }

    #[test]
    fn follows_herdr_claude_live_turn_and_background_activity_rules() {
        assert_eq!(
            detect_state(AgentKind::Claude, "* Searching the web…", ""),
            AgentState::Working,
            "an active marker and trailing ellipsis identify a live turn"
        );
        assert_eq!(
            detect_state(AgentKind::Claude, "✽ Searching the web… (2m · 4s)", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(
                AgentKind::Claude,
                "· Waiting for 2 background agents to finish",
                ""
            ),
            AgentState::Working
        );
        assert_eq!(
            detect_state(
                AgentKind::Claude,
                "✢ Searching MCP tools\n· 1 MCP tasks still running",
                ""
            ),
            AgentState::Working
        );
        assert_eq!(
            detect_state(
                AgentKind::Claude,
                "✢ Searching MCP tools · 1 MCP tasks still running",
                ""
            ),
            AgentState::Working
        );
        assert_eq!(
            detect_state(
                AgentKind::Claude,
                "✢ Searching MCP tools\n    Waiting for the tool response\n    · 2 MCP tasks still running",
                ""
            ),
            AgentState::Working,
            "Herdr allows up to three wrapped activity lines before the MCP summary"
        );
        assert_eq!(
            detect_state(AgentKind::Claude, "· 1 MCP tasks still running", ""),
            AgentState::Unknown,
            "an MCP summary without its marked activity row is not enough"
        );
        assert_eq!(
            detect_state(AgentKind::Claude, " /btw\nEsc to close", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Claude, "", "◐ Claude"),
            AgentState::Working,
            "Herdr recognizes Claude's half-circle busy spinner"
        );
        assert_eq!(
            detect_state(
                AgentKind::Claude,
                "✢ Running tool\n· 1 MCP tasks still running\nDo you want to proceed?\nEsc to cancel",
                ""
            ),
            AgentState::Blocked,
            "visible permission prompts take priority over background activity"
        );
    }

    #[test]
    fn finds_agents_nested_under_the_pane_shell() {
        let processes = vec![
            ProcessEntry {
                pid: 20,
                parent_pid: 10,
                name: "cmd.exe".into(),
                command_line: None,
            },
            ProcessEntry {
                pid: 30,
                parent_pid: 20,
                name: "codex.exe".into(),
                command_line: None,
            },
            ProcessEntry {
                pid: 40,
                parent_pid: 1,
                name: "opencode.exe".into(),
                command_line: None,
            },
        ];
        assert_eq!(identify_descendant(10, &processes), Some(AgentKind::Codex));
        assert_eq!(identify_descendant(40, &processes), None);
    }

    #[test]
    fn recognizes_herdr_style_windows_agent_wrappers() {
        assert_eq!(
            identify_process_command(
                "node.exe",
                Some(
                    r#"node.exe "C:\Users\user\AppData\Roaming\npm\node_modules\@earendil-works\pi-coding-agent\dist\cli.js""#
                )
            ),
            Some(AgentKind::Pi)
        );
        assert_eq!(
            identify_process_command(
                "bun.exe",
                Some(
                    r#"bun.exe "C:\Users\user\AppData\Roaming\npm\node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js""#
                )
            ),
            Some(AgentKind::Pi)
        );
        assert_eq!(
            identify_process_command(
                "cmd.exe",
                Some(
                    r#"cmd.exe /D /S /C "C:\Users\user\AppData\Roaming\npm\codex.cmd --model gpt-5""#
                )
            ),
            Some(AgentKind::Codex)
        );
        assert_eq!(
            identify_process_command(
                "node.exe",
                Some(
                    r#"node.exe "C:\Users\user\AppData\Roaming\npm\node_modules\opencode-ai\bin\opencode""#
                )
            ),
            Some(AgentKind::OpenCode)
        );
        assert_eq!(
            identify_process_command(
                "node.exe",
                Some(
                    r#"node.exe "C:\Users\user\AppData\Roaming\npm\node_modules\@google\gemini-cli\dist\index.js""#
                )
            ),
            Some(AgentKind::Gemini)
        );
    }

    #[test]
    fn ignores_unrelated_node_and_shell_commands() {
        assert_eq!(
            identify_process_command("node.exe", Some(r#"node.exe -e "console.log('codex')""#)),
            None
        );
        assert_eq!(
            identify_process_command("cmd.exe", Some(r#"cmd.exe /C "echo codex.cmd""#)),
            None
        );
    }

    #[test]
    fn identifies_claude_through_a_windows_command_shim() {
        assert_eq!(
            identify_process_command("cmd.exe", Some(r#"cmd.exe /C "claude.cmd""#)),
            Some(AgentKind::Claude)
        );
        assert_eq!(
            identify_process_command(
                "powershell.exe",
                Some(r#"powershell.exe -File "C:\tools\claude.ps1""#)
            ),
            Some(AgentKind::Claude)
        );
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
    fn follows_herdr_gemini_manifest_state_signals() {
        for screen in [
            "│ Apply this change\n❯ Yes",
            "│ Allow execution",
            "Do you want to proceed?\nYes\nWaiting for user confirmation",
            "❯ Allow once",
        ] {
            assert_eq!(
                detect_state(AgentKind::Gemini, screen, ""),
                AgentState::Blocked,
                "Herdr marks explicit Gemini confirmation prompts as blocked: {screen:?}"
            );
        }
        assert_eq!(
            detect_state(AgentKind::Gemini, "Working…\nEsc to cancel", ""),
            AgentState::Working
        );
        assert_eq!(
            detect_state(AgentKind::Gemini, "ordinary shell output", ""),
            AgentState::Unknown,
            "Gemini state stays unknown without one of Herdr's visible cues"
        );
    }

    #[test]
    fn follows_herdr_codex_manifest_blocker_prompts() {
        assert_eq!(
            detect_state(
                AgentKind::Codex,
                "Update available! Update now?\nSkip until next version\nPress enter to continue",
                "Codex"
            ),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(AgentKind::Codex, "Run this command? [y/n]", "Codex"),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(
                AgentKind::Codex,
                "Do you want to continue?\nNo\nYes",
                "Codex"
            ),
            AgentState::Blocked
        );
    }

    #[test]
    fn codex_directory_trust_blocker_requires_herdr_prompt_header() {
        assert_eq!(
            detect_state(
                AgentKind::Codex,
                "> You are in C:\\repo\nDo you trust the contents of this directory?",
                "",
            ),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(
                AgentKind::Codex,
                "Do you trust the contents of this directory?",
                "",
            ),
            AgentState::Unknown
        );
        assert_eq!(
            detect_state(
                AgentKind::Codex,
                "PowerShell\nThe help text asks: do you trust the contents of this directory?",
                "",
            ),
            AgentState::Unknown
        );
    }

    #[test]
    fn codex_interrupted_banner_suppresses_working_fallback() {
        assert_ne!(
            detect_state(
                AgentKind::Codex,
                "• Working (12s) · esc to interrupt\n■ Conversation interrupted",
                ""
            ),
            AgentState::Working
        );
    }

    #[test]
    fn old_interrupted_banner_does_not_hide_new_codex_work() {
        assert_eq!(
            detect_state(
                AgentKind::Codex,
                "■ Conversation interrupted\none\ntwo\nthree\n• Working (2s) · esc to interrupt",
                ""
            ),
            AgentState::Working
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

    #[test]
    fn opencode_permission_rule_needs_the_manifest_control_hints() {
        assert_eq!(
            detect_state(
                AgentKind::OpenCode,
                "Esc dismiss \u{00b7} Enter submit \u{00b7} \u{2191}\u{2193} select",
                "",
            ),
            AgentState::Blocked
        );
        assert_eq!(
            detect_state(AgentKind::OpenCode, "Permission required", ""),
            AgentState::Unknown
        );
        assert_eq!(
            detect_state(
                AgentKind::OpenCode,
                "Esc dismiss \u{00b7} Enter confirm",
                "",
            ),
            AgentState::Unknown
        );
    }

    #[test]
    fn opencode_manifest_interrupt_line_marks_working() {
        assert_eq!(
            detect_state(
                AgentKind::OpenCode,
                "OpenCode \u{2014} ESC again to interrupt",
                "",
            ),
            AgentState::Working
        );
    }
}
