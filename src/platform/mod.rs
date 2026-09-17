#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(crate) use fallback::show_desktop_notification;
#[cfg(windows)]
pub(crate) use windows::show_desktop_notification;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationSound {
    Attention,
    Finished,
}

pub(crate) fn sound_playback_disabled_by_env() -> bool {
    sound_playback_disabled_by_values(
        std::env::var_os("SPINDLE_DISABLE_SOUND"),
        std::env::var_os("HERDR_DISABLE_SOUND"),
        std::env::var_os("NEXTEST"),
    )
}

fn sound_playback_disabled_by_values(
    spindle: Option<std::ffi::OsString>,
    herdr: Option<std::ffi::OsString>,
    nextest: Option<std::ffi::OsString>,
) -> bool {
    spindle.is_some() || herdr.is_some() || nextest.is_some()
}

pub(crate) fn launch_server_daemon(command: &mut std::process::Command) -> std::io::Result<u32> {
    #[cfg(windows)]
    return windows::launch_server_daemon(command);
    #[cfg(not(windows))]
    fallback::launch_server_daemon(command)
}

pub(crate) fn status_command_process(command: &str) -> std::process::Command {
    #[cfg(windows)]
    return windows::status_command_process(command);
    #[cfg(not(windows))]
    fallback::status_command_process(command)
}

#[cfg(windows)]
pub(crate) use windows::{configure_status_command, StatusCommandGuard};

#[cfg(not(windows))]
pub(crate) fn configure_status_command(_command: &mut std::process::Command) {}

#[cfg(not(windows))]
pub(crate) struct StatusCommandGuard;

#[cfg(not(windows))]
impl StatusCommandGuard {
    pub(crate) fn new(_child: &std::process::Child) -> std::io::Result<Self> {
        Ok(Self)
    }
}

#[cfg(not(windows))]
pub(crate) fn play_notification_sound(_sound: NotificationSound) -> std::io::Result<bool> {
    if sound_playback_disabled_by_env() {
        return Ok(false);
    }
    use std::io::Write;
    let mut stderr = std::io::stderr();
    stderr.write_all(b"\x07")?;
    stderr.flush()?;
    Ok(true)
}

#[cfg(windows)]
pub(crate) use windows::play_notification_sound;

#[cfg(not(windows))]
pub(crate) use fallback::should_draw_host_cursor_by_default;
#[cfg(windows)]
pub(crate) use windows::should_draw_host_cursor_by_default;

#[cfg(test)]
mod tests {
    use super::sound_playback_disabled_by_values;

    #[test]
    fn sound_disable_environment_matches_herdr_presence_rule() {
        assert!(sound_playback_disabled_by_values(
            Some("1".into()),
            None,
            None
        ));
        assert!(sound_playback_disabled_by_values(
            None,
            Some("0".into()),
            None
        ));
        assert!(sound_playback_disabled_by_values(
            None,
            None,
            Some("1".into())
        ));
        assert!(!sound_playback_disabled_by_values(None, None, None));
    }
}
