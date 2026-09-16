use std::io;

pub(crate) fn show_desktop_notification(_title: &str, _body: Option<&str>) -> io::Result<bool> {
    Ok(false)
}

pub(crate) fn should_draw_host_cursor_by_default() -> bool {
    false
}
