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
const MAX_EVENT_HISTORY_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaneRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub label: Option<String>,
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
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default)]
    pub label: Option<String>,
    pub status: PaneStatus,
    pub scrollback_bytes: usize,
    #[serde(default)]
    pub scrollback: Vec<u8>,
    #[serde(default)]
    pub screen: String,
    #[serde(default)]
    pub cursor: (u16, u16),
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub alternate_screen: bool,
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

fn history_path(session_path: &Path) -> PathBuf {
    session_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("session-history.json")
}

fn load_history(session_path: &Path, snapshot: &mut SessionSnapshot) -> Result<(), SnapshotError> {
    let history = match load_versioned::<HistorySnapshot>(&history_path(session_path)) {
        Ok(history) => history,
        Err(SnapshotError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(())
        }
        Err(error) => return Err(error),
    };
    for pane in &mut snapshot.panes {
        let Some(saved) = history
            .panes
            .iter()
            .find(|saved| saved.pane_id == pane.pane_id)
        else {
            continue;
        };
        pane.scrollback = saved.scrollback.clone();
        pane.scrollback_bytes = saved.scrollback.len();
        pane.screen = saved.screen.clone();
        pane.cursor = saved.cursor;
        pane.title = saved.title.clone();
        pane.alternate_screen = saved.alternate_screen;
    }
    Ok(())
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
    #[serde(default)]
    pub repository_path: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistorySnapshot {
    panes: Vec<PaneHistory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PaneHistory {
    pane_id: String,
    scrollback: Vec<u8>,
    screen: String,
    cursor: (u16, u16),
    title: String,
    alternate_screen: bool,
}

pub struct Session {
    pane_manager: PaneManager,
    snapshot: SessionSnapshot,
    next_pane_id: u64,
    snapshot_path: Option<PathBuf>,
    events: VecDeque<Event<Value>>,
    event_bytes: usize,
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
                        repository_path: None,
                        branch: None,
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
            event_bytes: 0,
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
                load_history(&path, &mut snapshot)?;
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
                    event_bytes: 0,
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
            let mut metadata = self.snapshot.clone();
            let history = HistorySnapshot {
                panes: metadata
                    .panes
                    .iter()
                    .map(|pane| PaneHistory {
                        pane_id: pane.pane_id.clone(),
                        scrollback: pane.scrollback.clone(),
                        screen: pane.screen.clone(),
                        cursor: pane.cursor,
                        title: pane.title.clone(),
                        alternate_screen: pane.alternate_screen,
                    })
                    .collect(),
            };
            for pane in &mut metadata.panes {
                pane.scrollback.clear();
                pane.screen.clear();
                pane.cursor = (0, 0);
                pane.title.clear();
                pane.alternate_screen = false;
            }
            save_versioned(path, &metadata)?;
            save_versioned(&history_path(path), &history)?;
        }
        Ok(())
    }

    pub fn set_default_workspace_context(
        &mut self,
        repository_path: String,
        branch: Option<String>,
    ) {
        if let Some(workspace) = self
            .snapshot
            .spaces
            .first_mut()
            .and_then(|space| space.workspaces.first_mut())
        {
            if workspace.repository_path.is_none() {
                workspace.repository_path = Some(repository_path);
            }
            if workspace.branch.is_none() {
                workspace.branch = branch;
            }
        }
    }

    pub fn snapshot(&self) -> &SessionSnapshot {
        &self.snapshot
    }

    pub fn events_since(&mut self, sequence: u64) -> Vec<Event<Value>> {
        self.poll();
        self.refresh_snapshot();
        let _ = self.save();
        self.events
            .iter()
            .filter(|event| event.sequence > sequence)
            .cloned()
            .collect()
    }

    pub fn event_gap(&self, sequence: u64) -> bool {
        if self.snapshot.event_sequence <= sequence {
            return false;
        }
        self.events
            .front()
            .map(|event| event.sequence > sequence + 1)
            .unwrap_or(true)
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
            cols: request.cols,
            rows: request.rows,
            label: request.label,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            title: String::new(),
            alternate_screen: false,
        });
        self.record_pane_events(vec![PaneEvent::Status {
            pane_id: pane_id.clone(),
            status: PaneStatus::Running,
        }]);
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn create_workspace(&mut self, name: String) -> Result<Value, String> {
        self.create_workspace_with_context(name, None, None)
    }

    pub fn create_workspace_with_context(
        &mut self,
        name: String,
        repository_path: Option<String>,
        branch: Option<String>,
    ) -> Result<Value, String> {
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == self.snapshot.active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let workspace_id = next_numbered_id(
            "workspace",
            space
                .workspaces
                .iter()
                .map(|workspace| workspace.workspace_id.clone()),
        );
        let tab_id = format!("tab-{}-1", workspace_id);
        space.workspaces.push(WorkspaceView {
            workspace_id: workspace_id.clone(),
            name,
            repository_path,
            branch,
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
        let space_id = next_numbered_id(
            "space",
            self.snapshot
                .spaces
                .iter()
                .map(|space| space.space_id.clone()),
        );
        let workspace_id = format!("workspace-{}-1", space_id);
        let tab_id = format!("tab-{}-1", workspace_id);
        self.snapshot.spaces.push(SpaceView {
            space_id: space_id.clone(),
            name,
            workspaces: vec![WorkspaceView {
                workspace_id: workspace_id.clone(),
                name: "Current project".into(),
                repository_path: None,
                branch: None,
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
        let tab_id = next_numbered_id(
            &format!("tab-{}", workspace.workspace_id),
            workspace.tabs.iter().map(|tab| tab.tab_id.clone()),
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

    pub fn rename_pane(&mut self, pane_id: &str, label: String) -> Result<Value, String> {
        let pane = self
            .snapshot
            .panes
            .iter_mut()
            .find(|pane| pane.pane_id == pane_id)
            .ok_or_else(|| format!("pane '{pane_id}' does not exist"))?;
        pane.label = (!label.trim().is_empty()).then_some(label);
        Ok(serde_json::json!({ "pane_id": pane_id, "label": pane.label }))
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

    pub fn focus_previous(&mut self) -> Result<Value, String> {
        let current = self.snapshot.focused_pane_id.clone();
        let previous = {
            let tab = self.active_tab_mut()?;
            tab.layout
                .as_ref()
                .and_then(|layout| layout.previous_pane(current.as_deref()))
                .map(str::to_string)
        };
        let pane_id = previous.ok_or_else(|| "active tab has no panes".to_string())?;
        self.snapshot.focused_pane_id = Some(pane_id.clone());
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn focus_direction(&mut self, direction: &str) -> Result<Value, String> {
        let movement = match direction {
            "left" => crate::model::layout::FocusDirection::Left,
            "right" => crate::model::layout::FocusDirection::Right,
            "up" => crate::model::layout::FocusDirection::Up,
            "down" => crate::model::layout::FocusDirection::Down,
            other => return Err(format!("unknown focus direction '{other}'")),
        };
        let current = self
            .snapshot
            .focused_pane_id
            .clone()
            .ok_or_else(|| "no pane is focused".to_string())?;
        let pane_id = self
            .active_tab_mut()?
            .layout
            .as_ref()
            .and_then(|layout| layout.directional_pane(&current, movement))
            .map(str::to_string)
            .ok_or_else(|| "no pane in that direction".to_string())?;
        self.snapshot.focused_pane_id = Some(pane_id.clone());
        Ok(serde_json::json!({ "pane_id": pane_id, "direction": direction }))
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
        let in_active_tab = self
            .active_tab_mut()?
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().contains(&pane_id));
        if !in_active_tab {
            return Err(format!("pane '{pane_id}' does not exist in the active tab"));
        }
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
        let events = self
            .pane_manager
            .stop(pane_id)
            .map_err(|error| format!("{error:?}"))?;
        self.record_pane_events(events);
        Ok(())
    }

    pub fn restart_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        let env = self
            .pane_manager
            .get(pane_id)
            .map(|pane| pane.config.env.clone())
            .unwrap_or_default();
        let (command, args, cwd, cols, rows, status) = self
            .snapshot
            .panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .map(|pane| {
                (
                    pane.command.clone(),
                    pane.args.clone(),
                    pane.cwd.clone(),
                    pane.cols,
                    pane.rows,
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
                    cols,
                    rows,
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
        self.record_pane_events(vec![PaneEvent::Status {
            pane_id: pane_id.into(),
            status: PaneStatus::Running,
        }]);
        Ok(serde_json::json!({ "pane_id": pane_id, "restarted": true }))
    }

    pub fn poll(&mut self) {
        let events = self.pane_manager.poll();
        self.record_pane_events(events);
    }

    fn record_pane_events(&mut self, events: Vec<PaneEvent>) {
        for event in events {
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
            if let Some(event) = self.events.back() {
                self.event_bytes += serde_json::to_vec(event)
                    .map(|bytes| bytes.len())
                    .unwrap_or(0);
            }
            while self.event_bytes > MAX_EVENT_HISTORY_BYTES {
                let Some(event) = self.events.pop_front() else {
                    break;
                };
                self.event_bytes = self.event_bytes.saturating_sub(
                    serde_json::to_vec(&event)
                        .map(|bytes| bytes.len())
                        .unwrap_or(0),
                );
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

fn next_numbered_id(prefix: &str, ids: impl IntoIterator<Item = String>) -> String {
    let ids: std::collections::BTreeSet<String> = ids.into_iter().collect();
    let mut number = 1;
    loop {
        let candidate = format!("{prefix}-{number}");
        if !ids.contains(&candidate) {
            return candidate;
        }
        number += 1;
    }
}

impl Session {
    pub fn refresh_snapshot(&mut self) {
        for pane in &mut self.snapshot.panes {
            if let Some(current) = self.pane_manager.get(&pane.pane_id) {
                pane.status = current.status.clone();
                pane.cols = current.terminal.snapshot().cols;
                pane.rows = current.terminal.snapshot().rows;
                pane.scrollback_bytes = current.scrollback.len();
                pane.scrollback = current.scrollback.iter().copied().collect();
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
    use crate::model::layout::LayoutNode;
    use crate::model::status::PaneStatus;
    use crate::pane::PaneEvent;
    use std::time::Duration;

    #[test]
    fn workspace_and_tab_operations_update_active_state() {
        let mut session = Session::default();
        let workspace = session
            .create_workspace_with_context(
                "Feature".into(),
                Some("C:/repo".into()),
                Some("main".into()),
            )
            .unwrap();
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
            session.snapshot().spaces[0].workspaces[1]
                .repository_path
                .as_deref(),
            Some("C:/repo")
        );
        assert_eq!(
            session.snapshot().spaces[0].workspaces[1].branch.as_deref(),
            Some("main")
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
    fn default_workspace_context_is_filled_without_overwriting_metadata() {
        let mut session = Session::default();
        session.set_default_workspace_context("C:/repo".into(), Some("main".into()));
        session.set_default_workspace_context("C:/other".into(), Some("feature".into()));
        let workspace = &session.snapshot().spaces[0].workspaces[0];
        assert_eq!(workspace.repository_path.as_deref(), Some("C:/repo"));
        assert_eq!(workspace.branch.as_deref(), Some("main"));
    }

    #[test]
    fn old_pane_snapshots_use_terminal_defaults() {
        let pane: PaneView = serde_json::from_value(serde_json::json!({
            "pane_id": "pane-1",
            "command": "cmd.exe",
            "args": [],
            "cwd": "C:/",
            "status": "Running",
            "scrollback_bytes": 0
        }))
        .unwrap();
        assert_eq!((pane.cols, pane.rows), (80, 24));
    }

    #[test]
    fn invalid_focus_and_switch_are_rejected() {
        let mut session = Session::default();
        assert!(session.focus_pane("missing").is_err());
        assert!(session.switch_workspace("missing").is_err());
        assert!(session.switch_tab("missing").is_err());
    }

    #[test]
    fn pane_labels_can_be_renamed_and_cleared() {
        let mut session = Session::default();
        session.snapshot.panes.push(PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: None,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            title: String::new(),
            alternate_screen: false,
        });
        session.rename_pane("pane-1", "Shell".into()).unwrap();
        assert_eq!(session.snapshot.panes[0].label.as_deref(), Some("Shell"));
        session.rename_pane("pane-1", " ".into()).unwrap();
        assert!(session.snapshot.panes[0].label.is_none());
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
    fn closing_a_pane_outside_the_active_tab_is_rejected_before_removal() {
        let mut session = Session::default();
        session.create_tab("Other".into()).unwrap();
        session.switch_tab("tab-1").unwrap();
        session.snapshot.panes.push(PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: None,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            title: String::new(),
            alternate_screen: false,
        });
        session.snapshot.spaces[0].workspaces[0].tabs[1].layout = Some(LayoutNode::pane("pane-1"));
        assert!(session.close_pane("pane-1").is_err());
        assert_eq!(session.snapshot.panes.len(), 1);
    }

    #[test]
    fn new_container_ids_do_not_collide_after_middle_deletions() {
        let mut session = Session::default();
        session.create_space("Second".into()).unwrap();
        session.create_space("Third".into()).unwrap();
        session.delete_space("space-2").unwrap();
        assert_eq!(
            session.create_space("Replacement".into()).unwrap()["space_id"],
            "space-2"
        );

        session.switch_space("space-1").unwrap();
        session.create_workspace("Second workspace".into()).unwrap();
        session.create_workspace("Third workspace".into()).unwrap();
        session.delete_workspace("workspace-2").unwrap();
        assert_eq!(
            session
                .create_workspace("Replacement workspace".into())
                .unwrap()["workspace_id"],
            "workspace-2"
        );

        let first_tab = session.create_tab("Second tab".into()).unwrap();
        let second_tab = session.create_tab("Third tab".into()).unwrap();
        session
            .close_tab(second_tab["tab_id"].as_str().unwrap())
            .unwrap();
        assert_ne!(
            session.create_tab("Replacement tab".into()).unwrap()["tab_id"],
            first_tab["tab_id"]
        );
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
    fn terminal_history_is_stored_separately_from_metadata() {
        let directory =
            std::env::temp_dir().join(format!("spindle-history-test-{}", std::process::id()));
        let path = directory.join("session.json");
        let mut session = Session::load_or_default(&path).unwrap();
        session.snapshot.panes.push(PaneView {
            pane_id: "pane-1".into(),
            command: "cmd.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: None,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 3,
            scrollback: vec![1, 2, 3],
            screen: "screen".into(),
            cursor: (2, 1),
            title: "title".into(),
            alternate_screen: true,
        });
        session.save().unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let history: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("session-history.json")).unwrap())
                .unwrap();
        assert_eq!(
            metadata["data"]["panes"][0]["scrollback"],
            serde_json::json!([])
        );
        assert_eq!(
            history["data"]["panes"][0]["scrollback"],
            serde_json::json!([1, 2, 3])
        );
        let restored = Session::load_or_default(&path).unwrap();
        assert_eq!(restored.snapshot().panes[0].scrollback, vec![1, 2, 3]);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn corrupt_history_is_rejected_without_replacing_files() {
        let directory =
            std::env::temp_dir().join(format!("spindle-corrupt-history-{}", std::process::id()));
        let path = directory.join("session.json");
        let session = Session::load_or_default(&path).unwrap();
        session.save().unwrap();
        let metadata_before = std::fs::read(&path).unwrap();
        let history_path = directory.join("session-history.json");
        std::fs::write(&history_path, b"not-json").unwrap();
        assert!(Session::load_or_default(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), metadata_before);
        assert_eq!(std::fs::read(&history_path).unwrap(), b"not-json");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn direct_pane_events_are_ordered_in_history() {
        let mut session = Session::default();
        session.record_pane_events(vec![
            PaneEvent::Status {
                pane_id: "pane-1".into(),
                status: PaneStatus::Halted {
                    reason: "stopped".into(),
                },
            },
            PaneEvent::Output {
                pane_id: "pane-1".into(),
                bytes: b"done".to_vec(),
            },
        ]);
        assert_eq!(session.events.len(), 2);
        assert_eq!(session.events[0].sequence, 1);
        assert_eq!(session.events[1].sequence, 2);
    }

    #[test]
    fn evicted_history_requires_a_snapshot_resync() {
        let mut session = Session::default();
        for _ in 0..6000 {
            session.record_pane_events(vec![PaneEvent::Output {
                pane_id: "pane-1".into(),
                bytes: vec![1],
            }]);
        }
        assert!(session.event_gap(0));
        let events = session.events_since(0);
        assert!(!events.is_empty());
        assert!(events.len() < 6000);
        assert!(!session.event_gap(session.snapshot.event_sequence));
    }

    #[test]
    fn large_output_history_stays_within_the_byte_budget() {
        let mut session = Session::default();
        session.record_pane_events(vec![PaneEvent::Output {
            pane_id: "pane-1".into(),
            bytes: vec![1; 600 * 1024],
        }]);
        assert!(session.events.is_empty());
        assert!(session.event_gap(0));
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
            cols: 80,
            rows: 24,
            label: None,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
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
            cols: 80,
            rows: 24,
            label: None,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            title: String::new(),
            alternate_screen: false,
        });
        assert!(session.restart_pane("pane-1").is_err());
    }
}
