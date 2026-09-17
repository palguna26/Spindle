use crate::server::session::{SessionSnapshot, WorkspaceView};

#[derive(Clone)]
pub(crate) struct StatusCommandContext {
    pub(crate) cache_key: String,
    pub(crate) cwd: Option<std::path::PathBuf>,
    pub(crate) environment: Vec<(String, String)>,
}

pub(crate) fn for_workspace(
    snapshot: &SessionSnapshot,
    workspace: &WorkspaceView,
) -> StatusCommandContext {
    let pane_id = snapshot.focused_pane_id.clone();
    let pane = pane_id
        .as_deref()
        .and_then(|pane_id| snapshot.panes.iter().find(|pane| pane.pane_id == pane_id));
    let cwd = pane
        .map(|pane| std::path::PathBuf::from(&pane.cwd))
        .filter(|cwd| cwd.is_dir());
    let endpoint = std::env::var("SPINDLE_SOCKET_PATH")
        .or_else(|_| std::env::var("HERDR_SOCKET_PATH"))
        .unwrap_or_default();
    let mut environment = vec![
        (
            "SPINDLE_ACTIVE_WORKSPACE_ID".into(),
            workspace.workspace_id.clone(),
        ),
        (
            "HERDR_ACTIVE_WORKSPACE_ID".into(),
            workspace.workspace_id.clone(),
        ),
        (
            "SPINDLE_ACTIVE_TAB_ID".into(),
            workspace.active_tab_id.clone(),
        ),
        (
            "HERDR_ACTIVE_TAB_ID".into(),
            workspace.active_tab_id.clone(),
        ),
    ];
    if let Some(pane_id) = pane_id.as_deref() {
        environment.push(("SPINDLE_ACTIVE_PANE_ID".into(), pane_id.into()));
        environment.push(("HERDR_ACTIVE_PANE_ID".into(), pane_id.into()));
        if let Some(pane) = pane {
            environment.push(("SPINDLE_ACTIVE_PANE_CWD".into(), pane.cwd.clone()));
            environment.push(("HERDR_ACTIVE_PANE_CWD".into(), pane.cwd.clone()));
        }
    }
    if !endpoint.is_empty() {
        environment.push(("SPINDLE_SOCKET_PATH".into(), endpoint.clone()));
        environment.push(("HERDR_SOCKET_PATH".into(), endpoint));
    }
    if let Ok(executable) = std::env::current_exe() {
        let executable = executable.display().to_string();
        environment.push(("SPINDLE_BIN_PATH".into(), executable.clone()));
        environment.push(("HERDR_BIN_PATH".into(), executable));
    }
    let cache_key = format!(
        "{}\0{}\0{}\0{}",
        workspace.workspace_id,
        workspace.active_tab_id,
        pane_id.as_deref().unwrap_or_default(),
        cwd.as_ref()
            .map(|cwd| cwd.display().to_string())
            .unwrap_or_default(),
    );
    StatusCommandContext {
        cache_key,
        cwd,
        environment,
    }
}
