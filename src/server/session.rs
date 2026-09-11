use crate::model::layout::LayoutNode;
use crate::model::status::PaneStatus;
use crate::pane::{PaneConfig, PaneManager, PaneManagerError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaneRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaneView {
    pub pane_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub status: PaneStatus,
    pub scrollback_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TabView {
    pub tab_id: String,
    pub name: String,
    pub layout: Option<LayoutNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceView {
    pub workspace_id: String,
    pub name: String,
    pub tabs: Vec<TabView>,
    pub active_tab_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpaceView {
    pub space_id: String,
    pub name: String,
    pub workspaces: Vec<WorkspaceView>,
    pub active_workspace_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSnapshot {
    pub spaces: Vec<SpaceView>,
    pub active_space_id: String,
    pub panes: Vec<PaneView>,
    pub focused_pane_id: Option<String>,
}

pub struct Session {
    pane_manager: PaneManager,
    snapshot: SessionSnapshot,
    next_pane_id: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            pane_manager: PaneManager::default(),
            snapshot: SessionSnapshot {
                spaces: vec![SpaceView {
                    space_id: "space-1".into(),
                    name: "Default".into(),
                    workspaces: vec![WorkspaceView {
                        workspace_id: "workspace-1".into(),
                        name: "Current project".into(),
                        tabs: vec![TabView {
                            tab_id: "tab-1".into(),
                            name: "Main".into(),
                            layout: None,
                        }],
                        active_tab_id: "tab-1".into(),
                    }],
                    active_workspace_id: "workspace-1".into(),
                }],
                active_space_id: "space-1".into(),
                panes: Vec::new(),
                focused_pane_id: None,
            },
            next_pane_id: 1,
        }
    }
}

impl Session {
    pub fn snapshot(&self) -> &SessionSnapshot {
        &self.snapshot
    }

    pub fn create_pane(&mut self, request: CreatePaneRequest) -> Result<Value, String> {
        if request.command.trim().is_empty() {
            return Err("command cannot be empty".into());
        }
        if request.cols == 0 || request.rows == 0 {
            return Err("pane dimensions must be greater than zero".into());
        }
        let pane_id = format!("pane-{}", self.next_pane_id);
        self.next_pane_id += 1;
        self.pane_manager
            .spawn(
                &pane_id,
                PaneConfig {
                    command: request.command.clone(),
                    args: request.args.clone(),
                    cwd: request.cwd.clone(),
                    cols: request.cols,
                    rows: request.rows,
                },
            )
            .map_err(|error| format!("{error:?}"))?;

        let tab = &mut self.snapshot.spaces[0].workspaces[0].tabs[0];
        tab.layout = Some(match tab.layout.take() {
            None => LayoutNode::pane(&pane_id),
            Some(layout) => layout.split(crate::model::layout::Direction::Vertical, 0.5, &pane_id),
        });
        self.snapshot.focused_pane_id = Some(pane_id.clone());
        self.snapshot.panes.push(PaneView {
            pane_id: pane_id.clone(),
            command: request.command,
            args: request.args,
            cwd: request.cwd,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
        });
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn send_input(&mut self, pane_id: &str, bytes: &[u8]) -> Result<(), String> {
        self.pane_manager
            .send_input(pane_id, bytes)
            .map_err(|error| format!("{error:?}"))
    }

    pub fn resize(&mut self, pane_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        self.pane_manager
            .resize(pane_id, cols, rows)
            .map_err(|error| format!("{error:?}"))
    }

    pub fn stop_pane(&mut self, pane_id: &str) -> Result<(), String> {
        self.pane_manager
            .stop(pane_id)
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }

    pub fn poll(&mut self) {
        self.pane_manager.poll();
    }
}

impl Session {
    pub fn refresh_snapshot(&mut self) {
        for pane in &mut self.snapshot.panes {
            if let Some(current) = self.pane_manager.get(&pane.pane_id) {
                pane.status = current.status.clone();
                pane.scrollback_bytes = current.scrollback.len();
            }
        }
    }
}

impl From<PaneManagerError> for String {
    fn from(error: PaneManagerError) -> Self {
        format!("{error:?}")
    }
}
