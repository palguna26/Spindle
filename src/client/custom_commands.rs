use std::io;
use std::path::PathBuf;
use std::process::Command;

use crate::config::CustomCommand;
use crate::server::session::SessionSnapshot;

pub(crate) fn run(
    command: &CustomCommand,
    snapshot: &SessionSnapshot,
    endpoint: &str,
) -> io::Result<()> {
    if command.command.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "custom command is empty",
        ));
    }
    if !command.action_type.is_empty() && command.action_type != "shell" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only custom command type 'shell' is supported",
        ));
    }

    let (env, cwd) = context(snapshot, endpoint);
    let mut process = shell_command(&command.command);
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    process.envs(env).spawn().map(|_| ())
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
