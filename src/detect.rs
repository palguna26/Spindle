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
    let basename = name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    let basename = [".exe", ".cmd", ".bat"]
        .iter()
        .find_map(|suffix| basename.strip_suffix(suffix))
        .unwrap_or(&basename);
    match basename {
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
                "cmd.exe" | "node.exe"
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
        detect_state, identify_descendant, identify_process, identify_process_command, AgentKind,
        AgentState, ProcessEntry,
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
