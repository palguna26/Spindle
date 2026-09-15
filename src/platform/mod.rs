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

#[cfg(not(windows))]
pub(crate) fn play_notification_sound(_sound: NotificationSound) -> std::io::Result<bool> {
    use std::io::Write;
    let mut stderr = std::io::stderr();
    stderr.write_all(b"\x07")?;
    stderr.flush()?;
    Ok(true)
}

#[cfg(windows)]
pub(crate) use windows::play_notification_sound;
