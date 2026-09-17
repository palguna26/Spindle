use std::io;

pub(crate) fn show_desktop_notification(_title: &str, _body: Option<&str>) -> io::Result<bool> {
    Ok(false)
}

pub(crate) fn should_draw_host_cursor_by_default() -> bool {
    false
}

pub(crate) fn launch_server_daemon(command: &mut std::process::Command) -> io::Result<u32> {
    command.spawn().map(|child| child.id())
}

pub(crate) fn status_command_process(command: &str) -> std::process::Command {
    let mut process = std::process::Command::new("sh");
    process.args(["-c", command]);
    process
}
