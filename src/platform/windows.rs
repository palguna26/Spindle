use std::io;
use std::mem::size_of;
use std::ptr::null_mut;
use std::time::Duration;

use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_TIP, NIIF_INFO, NIIF_NOSOUND, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, LoadIconW, IDI_APPLICATION, MB_ICONEXCLAMATION, MB_OK,
};

use super::NotificationSound;

pub(crate) fn launch_server_daemon(command: &mut std::process::Command) -> io::Result<u32> {
    #[allow(non_camel_case_types)]
    #[derive(serde::Deserialize)]
    struct Win32_Process;

    #[derive(serde::Serialize)]
    #[allow(non_camel_case_types)]
    struct Win32_ProcessStartup {
        #[serde(rename = "CreateFlags")]
        create_flags: u32,
        #[serde(rename = "EnvironmentVariables")]
        environment_variables: Vec<String>,
    }

    #[derive(serde::Serialize)]
    struct CreateInput {
        #[serde(rename = "CommandLine")]
        command_line: String,
        #[serde(rename = "CurrentDirectory")]
        current_directory: String,
        #[serde(rename = "ProcessStartupInformation")]
        process_startup_information: Win32_ProcessStartup,
    }

    #[derive(serde::Deserialize)]
    struct CreateOutput {
        #[serde(rename = "ProcessId")]
        process_id: Option<u32>,
        #[serde(rename = "ReturnValue")]
        return_value: u32,
    }

    let current_directory = command
        .get_current_dir()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut environment = std::collections::BTreeMap::<String, String>::new();
    for (key, value) in std::env::vars() {
        environment.insert(key, value);
    }
    for (key, value) in command.get_envs() {
        let key = key.to_string_lossy().into_owned();
        match value {
            Some(value) => {
                environment.insert(key, value.to_string_lossy().into_owned());
            }
            None => {
                environment.remove(&key);
            }
        }
    }
    let input = CreateInput {
        command_line: windows_command_line(command)?,
        current_directory: current_directory.to_string_lossy().into_owned(),
        process_startup_information: Win32_ProcessStartup {
            create_flags: windows_sys::Win32::System::Threading::DETACHED_PROCESS,
            environment_variables: environment
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect(),
        },
    };
    let connection = wmi::WMIConnection::new()
        .map_err(|error| io::Error::other(format!("failed to connect to WMI: {error}")))?;
    let output: CreateOutput = connection
        .exec_class_method::<Win32_Process, _>("Create", &input)
        .map_err(|error| io::Error::other(format!("WMI process creation failed: {error}")))?;
    if output.return_value != 0 {
        return Err(io::Error::other(format!(
            "WMI process creation returned error {}",
            output.return_value
        )));
    }
    output
        .process_id
        .ok_or_else(|| io::Error::other("WMI process creation returned no process id"))
}

fn windows_command_line(command: &std::process::Command) -> io::Result<String> {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|value| {
            let value = value
                .to_str()
                .ok_or_else(|| io::Error::other("server command contains invalid Unicode"))?;
            Ok(quote_windows_arg(value))
        })
        .collect::<io::Result<Vec<_>>>()
        .map(|parts| parts.join(" "))
}

fn quote_windows_arg(value: &str) -> String {
    if !value.is_empty() && !value.chars().any(|ch| " \t\"".contains(ch)) {
        return value.to_owned();
    }
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
        } else if ch == '"' {
            quoted.push_str(&"\\".repeat(slashes * 2 + 1));
            quoted.push(ch);
            slashes = 0;
        } else {
            quoted.push_str(&"\\".repeat(slashes));
            quoted.push(ch);
            slashes = 0;
        }
    }
    quoted.push_str(&"\\".repeat(slashes * 2));
    quoted.push('"');
    quoted
}

pub(crate) fn play_notification_sound(sound: NotificationSound) -> io::Result<bool> {
    let kind = match sound {
        NotificationSound::Attention => MB_ICONEXCLAMATION,
        NotificationSound::Finished => MB_OK,
    };
    let played = unsafe { MessageBeep(kind) };
    if played == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(true)
    }
}

pub(crate) fn show_desktop_notification(title: &str, body: Option<&str>) -> io::Result<bool> {
    let title = sanitize(title, 64);
    let body = sanitize(body.unwrap_or(&title), 256);
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("spindle-windows-notification".into())
        .spawn(move || show_on_thread(&title, &body, ready_tx))?;
    ready_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => io::Error::new(
                io::ErrorKind::TimedOut,
                "Windows notification setup timed out",
            ),
            std::sync::mpsc::RecvTimeoutError::Disconnected => {
                io::Error::other("Windows notification thread exited before reporting readiness")
            }
        })?
}

pub(crate) fn should_draw_host_cursor_by_default() -> bool {
    true
}

fn show_on_thread(
    title: &str,
    body: &str,
    ready_tx: std::sync::mpsc::SyncSender<io::Result<bool>>,
) {
    let class_name = wide_null("STATIC");
    let window_name = wide_null("Spindle notifications");
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        let _ = ready_tx.send(Err(io::Error::last_os_error()));
        return;
    }

    let mut notification = unsafe { std::mem::zeroed::<NOTIFYICONDATAW>() };
    notification.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    notification.hWnd = hwnd;
    notification.uID = 1;
    notification.hIcon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
    notification.uFlags = NIF_TIP;
    if !notification.hIcon.is_null() {
        notification.uFlags |= NIF_ICON;
    }
    copy_wide(&mut notification.szTip, "Spindle");

    if unsafe { Shell_NotifyIconW(NIM_ADD, &notification) } == 0 {
        let _ = ready_tx.send(Err(io::Error::other(
            "failed to add Spindle notification-area icon",
        )));
        unsafe {
            DestroyWindow(hwnd);
        }
        return;
    }

    notification.uFlags = NIF_INFO;
    notification.dwInfoFlags = NIIF_INFO | NIIF_NOSOUND;
    copy_wide(&mut notification.szInfoTitle, title);
    copy_wide(&mut notification.szInfo, body);
    if unsafe { Shell_NotifyIconW(NIM_MODIFY, &notification) } == 0 {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &notification);
            DestroyWindow(hwnd);
        }
        let _ = ready_tx.send(Err(io::Error::other(
            "failed to show Spindle desktop notification",
        )));
        return;
    }

    let _ = ready_tx.send(Ok(true));
    std::thread::sleep(Duration::from_secs(10));
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &notification);
        DestroyWindow(hwnd);
    }
}

fn sanitize(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(limit)
        .collect()
}

fn copy_wide<const N: usize>(destination: &mut [u16; N], value: &str) {
    destination.fill(0);
    let mut offset = 0;
    for ch in value.chars() {
        let mut units = [0; 2];
        let encoded = ch.encode_utf16(&mut units);
        if offset + encoded.len() >= N {
            break;
        }
        destination[offset..offset + encoded.len()].copy_from_slice(encoded);
        offset += encoded.len();
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn notification_text_is_bounded_and_control_free() {
        assert_eq!(sanitize("a\n\tb", 10), "ab");
        assert_eq!(sanitize("abcdef", 3), "abc");
    }
}
