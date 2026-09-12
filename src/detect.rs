//! Best-effort identification of coding agents running inside pane shells.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDisplayState {
    Unknown,
    Idle,
    Working,
    Blocked,
    Done,
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
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
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
    let blocked = match agent {
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
    };
    if blocked {
        return AgentState::Blocked;
    }

    let working = match agent {
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
    };
    if working {
        return AgentState::Working;
    }

    if has_visible_idle_signal(agent, &bottom_three, title, osc_progress) {
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
        AgentKind::Codex => !title.trim().is_empty(),
        AgentKind::Claude => {
            title.starts_with("\u{2733} ")
                || osc_progress.starts_with("4;0")
                || recent_nonempty_lines(screen, 3)
                    .lines()
                    .any(|line| line.trim_start().starts_with('\u{276f}'))
        }
        AgentKind::OpenCode => false,
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
            command_line: None,
            name,
        });
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    let descendant_ids = descendant_process_ids(root_pid, &processes);
    for process in &mut processes {
        if descendant_ids.contains(&process.pid)
            && matches!(
                process.name.to_ascii_lowercase().as_str(),
                "cmd.exe" | "node.exe" | "powershell.exe" | "pwsh.exe"
            )
        {
            process.command_line = read_command_line(process.pid);
        }
    }
    identify_descendant(root_pid, &processes)
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
            if normalized.ends_with("\\node_modules\\codex\\bin\\codex.js")
                || normalized.ends_with("\\node_modules\\@openai\\codex\\bin\\codex.js")
            {
                Some(AgentKind::Codex)
            } else if normalized.ends_with("\\node_modules\\opencode-ai\\bin\\opencode")
                || normalized.ends_with("\\node_modules\\opencode-ai\\bin\\opencode.js")
            {
                Some(AgentKind::OpenCode)
            } else {
                None
            }
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
pub(crate) fn detect_in_process_tree(_root_pid: u32) -> Option<AgentKind> {
    None
}

#[cfg(test)]
mod tests {
    use super::{
        detect_state, detect_state_with_osc, identify_descendant, identify_process,
        identify_process_command, AgentKind, AgentState, ProcessEntry,
    };

    #[test]
    fn recognizes_herdr_agent_process_names() {
        assert_eq!(identify_process("claude.exe"), Some(AgentKind::Claude));
        assert_eq!(identify_process("claude-code.cmd"), Some(AgentKind::Claude));
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
