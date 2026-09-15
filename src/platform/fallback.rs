use std::io;

pub(crate) fn show_desktop_notification(_title: &str, _body: Option<&str>) -> io::Result<bool> {
    Ok(false)
}
