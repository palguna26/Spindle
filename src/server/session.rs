use crate::model::layout::LayoutNode;
use crate::model::status::PaneStatus;
use crate::pane::PaneEvent;
use crate::pane::{PaneConfig, PaneManager, PaneManagerError};
use crate::persist::{load_versioned, save_versioned, SnapshotError};
use crate::protocol::Event;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const GEOMETRY_LEASE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaneRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
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
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub alternate_screen: bool,
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
    #[serde(default)]
    pub event_sequence: u64,
}

pub struct Session {
    pane_manager: PaneManager,
    snapshot: SessionSnapshot,
    next_pane_id: u64,
    snapshot_path: Option<PathBuf>,
    events: VecDeque<Event<Value>>,
    geometry_owner: Option<String>,
    geometry_owner_seen: Option<Instant>,
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
                event_sequence: 0,
            },
            next_pane_id: 1,
            snapshot_path: None,
            events: VecDeque::new(),
            geometry_owner: None,
            geometry_owner_seen: None,
        }
    }
}

impl Session {
    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, SnapshotError> {
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
                Ok(Self {
                    pane_manager: PaneManager::default(),
                    next_pane_id: next_pane_id(&snapshot),
                    snapshot,
                    snapshot_path: Some(path),
                    events: VecDeque::new(),
                    geometry_owner: None,
                    geometry_owner_seen: None,
                })
            }
            Err(SnapshotError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self {
                    snapshot_path: Some(path),
                    ..Self::default()
                })
            }
            Err(error) => Err(error),
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

    pub fn events_since(&mut self, sequence: u64) -> Vec<Event<Value>> {
        self.poll();
        self.events
            .iter()
            .filter(|event| event.sequence > sequence)
            .cloned()
            .collect()
    }

    pub fn attach(&mut self, client_id: String) -> Value {
        let can_claim = self.geometry_owner.is_none()
            || self.geometry_owner.as_deref() == Some(client_id.as_str())
            || self
                .geometry_owner_seen
                .map(|seen| seen.elapsed() >= GEOMETRY_LEASE)
                .unwrap_or(false);
        if can_claim {
            self.geometry_owner = Some(client_id.clone());
            self.geometry_owner_seen = Some(Instant::now());
        }
        let owner = self.geometry_owner.as_deref().unwrap_or("none");
        serde_json::json!({
            "attached": true,
            "client_id": client_id,
            "geometry_owner": owner,
            "active": owner == client_id,
        })
    }

    pub fn can_resize(&self, client_id: &str) -> bool {
        self.geometry_owner
            .as_deref()
            .map(|owner| owner == client_id)
            .unwrap_or(true)
    }

    pub fn detach(&mut self, client_id: &str) -> Value {
        let released = self.geometry_owner.as_deref() == Some(client_id);
        if released {
            self.geometry_owner = None;
            self.geometry_owner_seen = None;
        }
        serde_json::json!({ "detached": true, "released_geometry": released })
    }

    pub fn touch_client(&mut self, client_id: &str) {
        if self.geometry_owner.as_deref() == Some(client_id) {
            self.geometry_owner_seen = Some(Instant::now());
        }
    }

    pub fn create_pane(&mut self, request: CreatePaneRequest) -> Result<Value, String> {
        self.create_pane_with_direction(request, crate::model::layout::Direction::Vertical)
    }

    pub fn split_pane(
        &mut self,
        request: CreatePaneRequest,
        direction: crate::model::layout::Direction,
    ) -> Result<Value, String> {
        self.create_pane_with_direction(request, direction)
    }

    fn create_pane_with_direction(
        &mut self,
        request: CreatePaneRequest,
        direction: crate::model::layout::Direction,
    ) -> Result<Value, String> {
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
                    env: request.env.clone(),
                    cols: request.cols,
                    rows: request.rows,
                },
            )
            .map_err(|error| format!("{error:?}"))?;

        let focused = self.snapshot.focused_pane_id.clone();
        let tab = self.active_tab_mut()?;
        tab.layout = Some(match tab.layout.take() {
            None => LayoutNode::pane(&pane_id),
            Some(layout) => {
                match focused
                    .as_deref()
                    .and_then(|target| layout.clone().split_pane(target, direction, &pane_id))
                {
                    Some(layout) => layout,
                    None => layout.split(direction, 0.5, &pane_id),
                }
            }
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
            title: String::new(),
            alternate_screen: false,
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

    pub fn close_tab(&mut self, tab_id: &str) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let space = self
            .snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let workspace_id = space.active_workspace_id.clone();
        let workspace = space
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == space.active_workspace_id)
            .ok_or_else(|| "active workspace does not exist".to_string())?;
        if workspace.tabs.len() == 1 {
            return Err("cannot close the last tab".into());
        }
        let index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == tab_id)
            .ok_or_else(|| format!("tab '{tab_id}' does not exist"))?;
        let pane_ids = workspace.tabs[index]
            .layout
            .as_ref()
            .map(LayoutNode::pane_ids)
            .unwrap_or_default();
        if self
            .snapshot
            .panes
            .iter()
            .any(|pane| pane_ids.contains(&pane.pane_id.as_str()) && pane.status.is_running())
        {
            return Err("stop all panes in the tab before closing it".into());
        }
        for pane_id in &pane_ids {
            let _ = self.pane_manager.remove(pane_id);
        }
        self.snapshot
            .panes
            .retain(|pane| !pane_ids.iter().any(|id| *id == pane.pane_id));
        if self
            .snapshot
            .focused_pane_id
            .as_ref()
            .is_some_and(|pane_id| pane_ids.iter().any(|id| *id == pane_id))
        {
            self.snapshot.focused_pane_id =
                self.snapshot.panes.first().map(|pane| pane.pane_id.clone());
        }
        let workspace = &mut self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == active_space_id)
            .expect("active space was found")
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .expect("active workspace was found");
        workspace.tabs.remove(index);
        if workspace.active_tab_id == tab_id {
            workspace.active_tab_id = workspace.tabs[0].tab_id.clone();
        }
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

    pub fn focus_next(&mut self) -> Result<Value, String> {
        let current = self.snapshot.focused_pane_id.clone();
        let next = {
            let tab = self.active_tab_mut()?;
            tab.layout
                .as_ref()
                .and_then(|layout| layout.next_pane(current.as_deref()))
                .map(str::to_string)
        };
        let pane_id = next.ok_or_else(|| "active tab has no panes".to_string())?;
        self.snapshot.focused_pane_id = Some(pane_id.clone());
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn resize_pane(&mut self, pane_id: &str, delta: f32) -> Result<Value, String> {
        let tab = self.active_tab_mut()?;
        let layout = tab
            .layout
            .as_mut()
            .ok_or_else(|| "active tab has no panes".to_string())?;
        if !layout.resize_pane(pane_id, delta) {
            return Err(format!("pane '{pane_id}' does not exist in the active tab"));
        }
        Ok(serde_json::json!({ "pane_id": pane_id, "delta": delta }))
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

    pub fn restart_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        let env = self
            .pane_manager
            .get(pane_id)
            .map(|pane| pane.config.env.clone())
            .unwrap_or_default();
        let (command, args, cwd, status) = self
            .snapshot
            .panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .map(|pane| {
                (
                    pane.command.clone(),
                    pane.args.clone(),
                    pane.cwd.clone(),
                    pane.status.clone(),
                )
            })
            .ok_or_else(|| format!("pane '{pane_id}' does not exist"))?;
        if status.is_running() {
            return Err(format!("pane '{pane_id}' is already running"));
        }
        if self.pane_manager.get(pane_id).is_some() {
            self.pane_manager
                .remove(pane_id)
                .map_err(|error| format!("{error:?}"))?;
        }
        self.pane_manager
            .spawn(
                pane_id,
                PaneConfig {
                    command,
                    args,
                    cwd,
                    env,
                    cols: 80,
                    rows: 24,
                },
            )
            .map_err(|error| format!("{error:?}"))?;
        let pane = self
            .snapshot
            .panes
            .iter_mut()
            .find(|pane| pane.pane_id == pane_id)
            .expect("pane was found before restart");
        pane.status = PaneStatus::Running;
        pane.screen.clear();
        pane.cursor = (0, 0);
        self.snapshot.focused_pane_id = Some(pane_id.into());
        Ok(serde_json::json!({ "pane_id": pane_id, "restarted": true }))
    }

    pub fn poll(&mut self) {
        for event in self.pane_manager.poll() {
            let (name, payload) = match event {
                PaneEvent::Output { pane_id, bytes } => (
                    "pane_output",
                    serde_json::json!({ "pane_id": pane_id, "bytes": bytes }),
                ),
                PaneEvent::Status { pane_id, status } => (
                    "pane_status",
                    serde_json::json!({ "pane_id": pane_id, "status": status }),
                ),
            };
            self.snapshot.event_sequence += 1;
            self.events.push_back(Event {
                version: crate::protocol::PROTOCOL_VERSION,
                sequence: self.snapshot.event_sequence,
                event: name.into(),
                payload,
            });
            if self.events.len() > 4096 {
                self.events.pop_front();
            }
        }
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

    fn active_tab_mut(&mut self) -> Result<&mut TabView, String> {
        let workspace = self.active_workspace_mut()?;
        let tab_id = workspace.active_tab_id.clone();
        workspace
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == tab_id)
            .ok_or_else(|| "active tab does not exist".to_string())
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
                pane.title = terminal.title;
                pane.alternate_screen = terminal.alternate_screen;
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
    use super::{CreatePaneRequest, PaneView, Session};
    use crate::model::status::PaneStatus;
    use std::time::Duration;

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
    fn pane_requests_accept_runtime_environment() {
        let request: CreatePaneRequest = serde_json::from_value(serde_json::json!({
            "command": "powershell.exe",
            "cwd": "C:/",
            "cols": 80,
            "rows": 24,
            "env": { "SPINDLE_TEST": "1" }
        }))
        .unwrap();
        assert_eq!(request.env["SPINDLE_TEST"], "1");
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
        assert!(session.close_tab("tab-1").is_err());
    }

    #[test]
    fn tabs_can_be_closed_without_closing_the_last_tab() {
        let mut session = Session::default();
        let tab = session.create_tab("Logs".into()).unwrap();
        let tab_id = tab["tab_id"].as_str().unwrap().to_string();
        session.close_tab(&tab_id).unwrap();
        assert_eq!(session.snapshot().spaces[0].workspaces[0].tabs.len(), 1);
    }

    #[test]
    fn a_new_client_can_claim_an_expired_geometry_lease() {
        let mut session = Session::default();
        assert_eq!(session.attach("first".into())["active"], true);
        assert_eq!(session.attach("second".into())["active"], false);
        std::thread::sleep(Duration::from_millis(1_050));
        assert_eq!(session.attach("second".into())["active"], true);
    }

    #[test]
    fn invalid_snapshot_is_not_replaced_with_empty_state() {
        let path = std::env::temp_dir().join(format!(
            "spindle-invalid-session-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, b"not-json").unwrap();
        assert!(Session::load_or_default(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not-json");
        std::fs::remove_file(path).unwrap();
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
            title: String::new(),
            alternate_screen: false,
        });
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout =
            Some(crate::model::layout::LayoutNode::pane("pane-1"));
        assert!(session.delete_space("space-1").is_err());
    }

    #[test]
    fn running_panes_cannot_be_restarted() {
        let mut session = Session::default();
        session.snapshot.panes.push(PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            screen: String::new(),
            cursor: (0, 0),
            title: String::new(),
            alternate_screen: false,
        });
        assert!(session.restart_pane("pane-1").is_err());
    }
}
