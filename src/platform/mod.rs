#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(crate) use fallback::show_desktop_notification;
#[cfg(windows)]
pub(crate) use windows::show_desktop_notification;
