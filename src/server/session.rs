use crate::model::layout::LayoutNode;
use crate::model::status::PaneStatus;
use crate::pane::{PaneConfig, PaneManager, PaneManagerError};
use crate::persist::{load_versioned, save_versioned, SnapshotError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaneRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneView {
    pub pane_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub status: PaneStatus,
    pub scrollback_bytes: usize,
    #[serde(default)]
    pub screen: String,
    #[serde(default)]
    pub cursor: (u16, u16),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabView {
    pub tab_id: String,
    pub name: String,
    pub layout: Option<LayoutNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceView {
    pub workspace_id: String,
    pub name: String,
    pub tabs: Vec<TabView>,
    pub active_tab_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceView {
    pub space_id: String,
    pub name: String,
    pub workspaces: Vec<WorkspaceView>,
    pub active_workspace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub version: u16,
    pub spaces: Vec<SpaceView>,
    pub active_space_id: String,
    pub panes: Vec<PaneView>,
    pub focused_pane_id: Option<String>,
}

pub struct Session {
    pane_manager: PaneManager,
    snapshot: SessionSnapshot,
    next_pane_id: u64,
    snapshot_path: Option<PathBuf>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            pane_manager: PaneManager::default(),
            snapshot: SessionSnapshot {
                version: 1,
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
            snapshot_path: None,
        }
    }
}

impl Session {
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        match load_versioned::<SessionSnapshot>(&path) {
            Ok(mut snapshot) => {
                for pane in &mut snapshot.panes {
                    if pane.status.is_running() {
                        pane.status = PaneStatus::Interrupted {
                            reason: "process was live when the server stopped".into(),
                        };
                    }
                }
                Self {
                    pane_manager: PaneManager::default(),
                    next_pane_id: next_pane_id(&snapshot),
                    snapshot,
                    snapshot_path: Some(path),
                }
            }
            Err(_) => {
                let mut session = Self::default();
                session.snapshot_path = Some(path);
                session
            }
        }
    }

    pub fn save(&self) -> Result<(), SnapshotError> {
        if let Some(path) = &self.snapshot_path {
            save_versioned(path, &self.snapshot)?;
        }
        Ok(())
    }

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
            screen: String::new(),
            cursor: (0, 0),
        });
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn create_workspace(&mut self, name: String) -> Result<Value, String> {
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == self.snapshot.active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let workspace_id = format!("workspace-{}", space.workspaces.len() + 1);
        let tab_id = format!("tab-{}-1", workspace_id);
        space.workspaces.push(WorkspaceView {
            workspace_id: workspace_id.clone(),
            name,
            tabs: vec![TabView {
                tab_id: tab_id.clone(),
                name: "Main".into(),
                layout: None,
            }],
            active_tab_id: tab_id,
        });
        space.active_workspace_id = workspace_id.clone();
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn create_space(&mut self, name: String) -> Result<Value, String> {
        let space_id = format!("space-{}", self.snapshot.spaces.len() + 1);
        let workspace_id = format!("workspace-{}-1", space_id);
        let tab_id = format!("tab-{}-1", workspace_id);
        self.snapshot.spaces.push(SpaceView {
            space_id: space_id.clone(),
            name,
            workspaces: vec![WorkspaceView {
                workspace_id: workspace_id.clone(),
                name: "Current project".into(),
                tabs: vec![TabView {
                    tab_id: tab_id.clone(),
                    name: "Main".into(),
                    layout: None,
                }],
                active_tab_id: tab_id,
            }],
            active_workspace_id: workspace_id,
        });
        self.snapshot.active_space_id = space_id.clone();
        Ok(serde_json::json!({ "space_id": space_id }))
    }

    pub fn switch_space(&mut self, space_id: &str) -> Result<Value, String> {
        if !self
            .snapshot
            .spaces
            .iter()
            .any(|space| space.space_id == space_id)
        {
            return Err(format!("space '{space_id}' does not exist"));
        }
        self.snapshot.active_space_id = space_id.into();
        Ok(serde_json::json!({ "space_id": space_id }))
    }

    pub fn rename_space(&mut self, space_id: &str, name: String) -> Result<Value, String> {
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == space_id)
            .ok_or_else(|| format!("space '{space_id}' does not exist"))?;
        space.name = name;
        Ok(serde_json::json!({ "space_id": space_id }))
    }

    pub fn delete_space(&mut self, space_id: &str) -> Result<Value, String> {
        if self.snapshot.spaces.len() == 1 {
            return Err("cannot delete the last space".into());
        }
        let index = self
            .snapshot
            .spaces
            .iter()
            .position(|space| space.space_id == space_id)
            .ok_or_else(|| format!("space '{space_id}' does not exist"))?;
        if self.space_has_running_panes(index) {
            return Err("stop all panes in the space before deleting it".into());
        }
        self.snapshot.spaces.remove(index);
        if self.snapshot.active_space_id == space_id {
            self.snapshot.active_space_id = self.snapshot.spaces[0].space_id.clone();
        }
        Ok(serde_json::json!({ "space_id": space_id }))
    }

    pub fn switch_workspace(&mut self, workspace_id: &str) -> Result<Value, String> {
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == self.snapshot.active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        if !space
            .workspaces
            .iter()
            .any(|workspace| workspace.workspace_id == workspace_id)
        {
            return Err(format!("workspace '{workspace_id}' does not exist"));
        }
        space.active_workspace_id = workspace_id.into();
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn rename_workspace(&mut self, workspace_id: &str, name: String) -> Result<Value, String> {
        let workspace = self.workspace_mut(workspace_id)?;
        workspace.name = name;
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn delete_workspace(&mut self, workspace_id: &str) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let space_index = self
            .snapshot
            .spaces
            .iter()
            .position(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        if self.snapshot.spaces[space_index].workspaces.len() == 1 {
            return Err("cannot delete the last workspace".into());
        }
        let workspace_index = self.snapshot.spaces[space_index]
            .workspaces
            .iter()
            .position(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| format!("workspace '{workspace_id}' does not exist"))?;
        if self.workspace_has_running_panes(space_index, workspace_index) {
            return Err("stop all panes in the workspace before deleting it".into());
        }
        self.snapshot.spaces[space_index]
            .workspaces
            .remove(workspace_index);
        let space = &mut self.snapshot.spaces[space_index];
        if space.active_workspace_id == workspace_id {
            space.active_workspace_id = space.workspaces[0].workspace_id.clone();
        }
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn create_tab(&mut self, name: String) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        let tab_id = format!(
            "tab-{}-{}",
            workspace.workspace_id,
            workspace.tabs.len() + 1
        );
        workspace.tabs.push(TabView {
            tab_id: tab_id.clone(),
            name,
            layout: None,
        });
        workspace.active_tab_id = tab_id.clone();
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn switch_tab(&mut self, tab_id: &str) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        if !workspace.tabs.iter().any(|tab| tab.tab_id == tab_id) {
            return Err(format!("tab '{tab_id}' does not exist"));
        }
        workspace.active_tab_id = tab_id.into();
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn rename_tab(&mut self, tab_id: &str, name: String) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        let tab = workspace
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == tab_id)
            .ok_or_else(|| format!("tab '{tab_id}' does not exist"))?;
        tab.name = name;
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn focus_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        if !self
            .snapshot
            .panes
            .iter()
            .any(|pane| pane.pane_id == pane_id)
        {
            return Err(format!("pane '{pane_id}' does not exist"));
        }
        self.snapshot.focused_pane_id = Some(pane_id.into());
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn close_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        self.pane_manager
            .remove(pane_id)
            .map_err(|error| format!("{error:?}"))?;
        let workspace = self.active_workspace_mut()?;
        let tab = workspace
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == workspace.active_tab_id)
            .ok_or_else(|| "active tab does not exist".to_string())?;
        tab.layout = tab
            .layout
            .take()
            .and_then(|layout| layout.close_pane(pane_id));
        self.snapshot.panes.retain(|pane| pane.pane_id != pane_id);
        if self.snapshot.focused_pane_id.as_deref() == Some(pane_id) {
            self.snapshot.focused_pane_id =
                self.snapshot.panes.first().map(|pane| pane.pane_id.clone());
        }
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

    fn active_workspace_mut(&mut self) -> Result<&mut WorkspaceView, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let workspace_id = space.active_workspace_id.clone();
        space
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| "active workspace does not exist".to_string())
    }

    fn workspace_mut(&mut self, workspace_id: &str) -> Result<&mut WorkspaceView, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        space
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| format!("workspace '{workspace_id}' does not exist"))
    }

    fn space_has_running_panes(&self, space_index: usize) -> bool {
        (0..self.snapshot.spaces[space_index].workspaces.len())
            .any(|workspace_index| self.workspace_has_running_panes(space_index, workspace_index))
    }

    fn workspace_has_running_panes(&self, space_index: usize, workspace_index: usize) -> bool {
        let pane_ids: Vec<&str> = self.snapshot.spaces[space_index].workspaces[workspace_index]
            .tabs
            .iter()
            .filter_map(|tab| tab.layout.as_ref())
            .flat_map(LayoutNode::pane_ids)
            .collect();
        self.snapshot
            .panes
            .iter()
            .any(|pane| pane_ids.contains(&pane.pane_id.as_str()) && pane.status.is_running())
    }
}

fn next_pane_id(snapshot: &SessionSnapshot) -> u64 {
    snapshot
        .panes
        .iter()
        .filter_map(|pane| pane.pane_id.strip_prefix("pane-"))
        .filter_map(|id| id.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

impl Session {
    pub fn refresh_snapshot(&mut self) {
        for pane in &mut self.snapshot.panes {
            if let Some(current) = self.pane_manager.get(&pane.pane_id) {
                pane.status = current.status.clone();
                pane.scrollback_bytes = current.scrollback.len();
                let terminal = current.terminal.snapshot();
                pane.screen = terminal.contents;
                pane.cursor = terminal.cursor;
            }
        }
    }
}

impl From<PaneManagerError> for String {
    fn from(error: PaneManagerError) -> Self {
        format!("{error:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::{PaneView, Session};
    use crate::model::status::PaneStatus;

    #[test]
    fn workspace_and_tab_operations_update_active_state() {
        let mut session = Session::default();
        let workspace = session.create_workspace("Feature".into()).unwrap();
        let workspace_id = workspace["workspace_id"].as_str().unwrap();
        assert_eq!(
            session.snapshot().spaces[0].active_workspace_id,
            workspace_id
        );

        let tab = session.create_tab("Logs".into()).unwrap();
        let tab_id = tab["tab_id"].as_str().unwrap();
        assert_eq!(
            session.snapshot().spaces[0].workspaces[1].active_tab_id,
            tab_id
        );

        session
            .rename_workspace(workspace_id, "Feature work".into())
            .unwrap();
        session.rename_tab(tab_id, "Build logs".into()).unwrap();
        assert_eq!(
            session.snapshot().spaces[0].workspaces[1].name,
            "Feature work"
        );
        assert_eq!(
            session.snapshot().spaces[0].workspaces[1].tabs[1].name,
            "Build logs"
        );
    }

    #[test]
    fn invalid_focus_and_switch_are_rejected() {
        let mut session = Session::default();
        assert!(session.focus_pane("missing").is_err());
        assert!(session.switch_workspace("missing").is_err());
        assert!(session.switch_tab("missing").is_err());
    }

    #[test]
    fn last_containers_cannot_be_deleted() {
        let mut session = Session::default();
        assert!(session.delete_space("space-1").is_err());
        assert!(session.delete_workspace("workspace-1").is_err());
    }

    #[test]
    fn running_panes_protect_their_space() {
        let mut session = Session::default();
        session.create_space("Second".into()).unwrap();
        session.switch_space("space-1").unwrap();
        session.snapshot.panes.push(PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            screen: String::new(),
            cursor: (0, 0),
        });
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout =
            Some(crate::model::layout::LayoutNode::pane("pane-1"));
        assert!(session.delete_space("space-1").is_err());
    }
}
