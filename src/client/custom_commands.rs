use std::io;
use std::path::PathBuf;
use std::process::Command;

use super::ControlClient;
use crate::config::CustomCommand;
use crate::popup_size::PopupSize;
use crate::server::session::SessionSnapshot;
use serde_json::json;

pub(crate) fn run(
    command: &CustomCommand,
    snapshot: &SessionSnapshot,
    client: &ControlClient,
) -> io::Result<()> {
    if command.command.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "custom command is empty",
        ));
    }
    match command.action_type.as_str() {
        "" | "shell" => run_shell(command, snapshot, client.endpoint()),
        "pane" => open_pane(command, snapshot, client, false),
        "popup" => open_pane(command, snapshot, client, true),
        "plugin_action" => crate::client::plugins::launch_action(&command.command, snapshot),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported custom command type '{other}'"),
        )),
    }
}

fn run_shell(
    command: &CustomCommand,
    snapshot: &SessionSnapshot,
    endpoint: &str,
) -> io::Result<()> {
    let (env, cwd) = context(snapshot, endpoint);
    let mut process = shell_command(&command.command);
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    process.envs(env).spawn().map(|_| ())
}

fn open_pane(
    command: &CustomCommand,
    snapshot: &SessionSnapshot,
    client: &ControlClient,
    popup: bool,
) -> io::Result<()> {
    let (env, cwd) = context(snapshot, client.endpoint());
    let cwd = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let (program, args) = pane_argv(&command.command);
    let terminal_size = crossterm::terminal::size().map_err(|error| {
        io::Error::other(format!(
            "could not read terminal size for custom command: {error}"
        ))
    })?;
    let width = command.width.unwrap_or(PopupSize::Cells(80));
    let height = command.height.unwrap_or(PopupSize::Cells(24));
    let cols = width.resolve(terminal_size.0).max(10);
    let rows = height.resolve(terminal_size.1).max(4);
    let payload = json!({
        "command": program,
        "args": args,
        "cwd": cwd.display().to_string(),
        "env": env.into_iter().collect::<std::collections::BTreeMap<_, _>>(),
        "cols": if popup { cols.saturating_sub(2).max(4) } else { cols },
        "rows": if popup { rows.saturating_sub(2).max(4) } else { rows },
        "popup": popup,
        "overlay": !popup,
        "popup_width_spec": if popup { Some(width) } else { None },
        "popup_height_spec": if popup { Some(height) } else { None },
    });
    client
        .interactive_request("custom-command-pane", "create_pane", payload)
        .map(|_| ())
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

fn pane_argv(command: &str) -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        (
            std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into()),
            vec!["/d".into(), "/c".into(), command.into()],
        )
    }
    #[cfg(not(windows))]
    {
        ("sh".into(), vec!["-lc".into(), command.into()])
    }
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut process =
            Command::new(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into()));
        process.args(["/d", "/c", command]);
        process
    }
    #[cfg(not(windows))]
    {
        let mut process = Command::new("sh");
        process.args(["-lc", command]);
        process
    }
}

fn context(snapshot: &SessionSnapshot, endpoint: &str) -> (Vec<(String, String)>, Option<PathBuf>) {
    let mut env = Vec::new();
    env.push(("SPINDLE_SOCKET_PATH".into(), endpoint.into()));
    env.push(("HERDR_SOCKET_PATH".into(), endpoint.into()));
    if let Ok(exe) = std::env::current_exe() {
        let value = exe.display().to_string();
        env.push(("SPINDLE_BIN_PATH".into(), value.clone()));
        env.push(("HERDR_BIN_PATH".into(), value));
    }

    let mut cwd = None;
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    else {
        return (env, cwd);
    };
    let Some(workspace_id) = space.active_workspace_id.as_deref() else {
        return (env, cwd);
    };
    env.push(("SPINDLE_ACTIVE_WORKSPACE_ID".into(), workspace_id.into()));
    env.push(("HERDR_ACTIVE_WORKSPACE_ID".into(), workspace_id.into()));
    let Some(workspace) = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
    else {
        return (env, cwd);
    };
    env.push((
        "SPINDLE_ACTIVE_TAB_ID".into(),
        workspace.active_tab_id.clone(),
    ));
    env.push((
        "HERDR_ACTIVE_TAB_ID".into(),
        workspace.active_tab_id.clone(),
    ));
    if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
        env.push(("SPINDLE_ACTIVE_PANE_ID".into(), pane_id.into()));
        env.push(("HERDR_ACTIVE_PANE_ID".into(), pane_id.into()));
        if let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) {
            let pane_cwd = PathBuf::from(&pane.cwd);
            env.push(("SPINDLE_ACTIVE_PANE_CWD".into(), pane.cwd.clone()));
            env.push(("HERDR_ACTIVE_PANE_CWD".into(), pane.cwd.clone()));
            if pane_cwd.is_dir() {
                cwd = Some(pane_cwd);
            }
        }
    }
    (env, cwd)
}

#[cfg(test)]
mod tests {
    use super::context;

    #[test]
    fn context_includes_active_container_ids() {
        let snapshot = crate::server::session::Session::default()
            .snapshot()
            .clone();
        let (env, _) = context(&snapshot, "test-endpoint");
        let values = env
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            values.get("HERDR_ACTIVE_WORKSPACE_ID").map(String::as_str),
            Some("workspace-1")
        );
        assert_eq!(
            values.get("HERDR_ACTIVE_TAB_ID").map(String::as_str),
            Some("tab-1")
        );
    }
}
