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
    #[serde(default)]
    pub agent: Option<crate::detect::AgentKind>,
    #[serde(default)]
    pub agent_state: Option<crate::detect::AgentState>,
    #[serde(default)]
    pub agent_done: bool,
    pub scrollback_bytes: usize,
    #[serde(default)]
    pub scrollback: Vec<u8>,
    #[serde(default)]
    pub screen: String,
    #[serde(default)]
    pub cursor: (u16, u16),
    #[serde(default)]
    pub cursor_visible: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub alternate_screen: bool,
    #[serde(default)]
    pub mouse_reporting: bool,
    #[serde(default)]
    pub mouse_release: bool,
    #[serde(default)]
    pub mouse_motion: bool,
    #[serde(default)]
    pub mouse_any_motion: bool,
    #[serde(default)]
    pub sgr_mouse: bool,
    #[serde(default)]
    pub utf8_mouse: bool,
    #[serde(default)]
    pub application_cursor: bool,
    #[serde(default)]
    pub bracketed_paste: bool,
    #[serde(default)]
    pub right_click_passthrough: bool,
}

impl PaneView {
    /// Route plain PageUp/PageDown like Herdr: host scrollback is used only
    /// when the terminal mode indicates that the foreground app does not own it.
    pub fn plain_page_keys_use_host_scrollback(&self) -> bool {
        !self.alternate_screen
            && !self.mouse_reporting
            && (!self.application_cursor || self.bracketed_paste)
    }

    pub fn agent_display_state(&self) -> crate::detect::AgentDisplayState {
        use crate::detect::{AgentDisplayState as Display, AgentState};
        if self.agent_done {
            return Display::Done;
        }
        match self.agent_state.unwrap_or(AgentState::Unknown) {
            AgentState::Unknown => Display::Unknown,
            AgentState::Idle => Display::Idle,
            AgentState::Working => Display::Working,
            AgentState::Blocked => Display::Blocked,
        }
    }
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
        pane.cursor_visible = false;
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
    #[serde(default)]
    pub focused_pane_id: Option<String>,
    #[serde(default)]
    pub zoomed: bool,
}

impl TabView {
    fn normalize_focus(&mut self, fallback: Option<&str>) -> Option<String> {
        let pane_ids = self
            .layout
            .as_ref()
            .map(LayoutNode::pane_ids)
            .unwrap_or_default();
        let focus = self
            .focused_pane_id
            .clone()
            .filter(|focused| pane_ids.contains(&focused.as_str()))
            .or_else(|| {
                fallback
                    .filter(|focused| pane_ids.contains(focused))
                    .map(str::to_owned)
            })
            .or_else(|| pane_ids.first().map(|pane_id| (*pane_id).to_owned()));
        self.focused_pane_id = focus.clone();
        focus
    }
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
    #[serde(default)]
    pub active_workspace_id: Option<String>,
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
                            focused_pane_id: None,
                            zoomed: false,
                        }],
                        active_tab_id: "tab-1".into(),
                    }],
                    active_workspace_id: Some("workspace-1".into()),
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
                    pane.agent = None;
                    pane.agent_state = None;
                    pane.agent_done = false;
                    pane.mouse_reporting = false;
                    pane.mouse_release = false;
                    pane.mouse_motion = false;
                    pane.mouse_any_motion = false;
                    pane.sgr_mouse = false;
                    pane.utf8_mouse = false;
                    pane.application_cursor = false;
                    pane.bracketed_paste = false;
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
                pane.cursor_visible = false;
                pane.title.clear();
                pane.alternate_screen = false;
                pane.mouse_reporting = false;
                pane.mouse_release = false;
                pane.mouse_motion = false;
                pane.mouse_any_motion = false;
                pane.sgr_mouse = false;
                pane.utf8_mouse = false;
                pane.application_cursor = false;
                pane.bracketed_paste = false;
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

    pub fn ensure_active_pane(&mut self, request: CreatePaneRequest) -> Result<Value, String> {
        self.reconcile_active_tab_layout()?;
        self.sync_focus_to_active_tab()?;
        if let Some(pane_id) = self.active_tab_mut()?.focused_pane_id.clone() {
            let interrupted = self
                .snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == pane_id)
                .is_some_and(|pane| matches!(pane.status, PaneStatus::Interrupted { .. }));
            if interrupted {
                self.restart_pane(&pane_id)?;
                return Ok(serde_json::json!({
                    "pane_id": pane_id,
                    "created": false,
                    "restarted": true
                }));
            }
            if self
                .snapshot
                .panes
                .iter()
                .any(|pane| pane.pane_id == pane_id)
            {
                return Ok(serde_json::json!({ "pane_id": pane_id, "created": false }));
            }
        }

        let mut result = self.create_pane(request)?;
        if let Some(object) = result.as_object_mut() {
            object.insert("created".into(), serde_json::json!(true));
        }
        Ok(result)
    }

    fn reconcile_active_tab_layout(&mut self) -> Result<(), String> {
        let known_panes = self
            .snapshot
            .panes
            .iter()
            .map(|pane| pane.pane_id.clone())
            .collect::<std::collections::HashSet<_>>();
        let tab = self.active_tab_mut()?;
        let Some(layout) = tab.layout.take() else {
            tab.focused_pane_id = None;
            return Ok(());
        };
        let missing = layout
            .pane_ids()
            .into_iter()
            .filter(|pane_id| !known_panes.contains(*pane_id))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut repaired = Some(layout);
        for pane_id in missing {
            repaired = repaired.and_then(|layout| layout.close_pane(&pane_id));
        }
        tab.layout = repaired;
        if tab.focused_pane_id.as_deref().is_some_and(|focused| {
            !tab.layout
                .as_ref()
                .is_some_and(|layout| layout.pane_ids().contains(&focused))
        }) {
            tab.focused_pane_id = None;
        }
        Ok(())
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

        let tab = self.active_tab_mut()?;
        let focused = tab.focused_pane_id.clone().or_else(|| {
            tab.layout
                .as_ref()
                .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()))
        });
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
        tab.focused_pane_id = Some(pane_id.clone());
        self.snapshot.focused_pane_id = Some(pane_id.clone());
        self.snapshot.panes.push(PaneView {
            pane_id: pane_id.clone(),
            command: request.command,
            args: request.args,
            cwd: request.cwd,
            cols: request.cols,
            rows: request.rows,
            label: request.label,
            agent: None,
            agent_state: None,
            agent_done: false,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: false,
            title: String::new(),
            alternate_screen: false,
            mouse_reporting: false,
            mouse_release: false,
            mouse_motion: false,
            mouse_any_motion: false,
            sgr_mouse: false,
            utf8_mouse: false,
            application_cursor: false,
            bracketed_paste: false,
            right_click_passthrough: false,
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
                focused_pane_id: None,
                zoomed: false,
            }],
            active_tab_id: tab_id,
        });
        space.active_workspace_id = Some(workspace_id.clone());
        self.sync_focus_to_active_tab()?;
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
                    focused_pane_id: None,
                    zoomed: false,
                }],
                active_tab_id: tab_id,
            }],
            active_workspace_id: Some(workspace_id),
        });
        self.snapshot.active_space_id = space_id.clone();
        self.sync_focus_to_active_tab()?;
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
        self.sync_focus_to_active_tab()?;
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
        self.sync_focus_to_active_tab()?;
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
        space.active_workspace_id = Some(workspace_id.into());
        self.sync_focus_to_active_tab()?;
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn rename_workspace(&mut self, workspace_id: &str, name: String) -> Result<Value, String> {
        let workspace = self.workspace_mut(workspace_id)?;
        workspace.name = name;
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn delete_workspace(&mut self, workspace_id: &str) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        self.close_workspace(&active_space_id, workspace_id)?;
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
            focused_pane_id: None,
            zoomed: false,
        });
        workspace.active_tab_id = tab_id.clone();
        self.sync_focus_to_active_tab()?;
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn switch_tab(&mut self, tab_id: &str) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        if !workspace.tabs.iter().any(|tab| tab.tab_id == tab_id) {
            return Err(format!("tab '{tab_id}' does not exist"));
        }
        workspace.active_tab_id = tab_id.into();
        self.sync_focus_to_active_tab()?;
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
        let workspace_id = space
            .active_workspace_id
            .clone()
            .ok_or_else(|| "active space has no workspace".to_string())?;
        let workspace = space
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| "active workspace does not exist".to_string())?;
        let index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == tab_id)
            .ok_or_else(|| format!("tab '{tab_id}' does not exist"))?;
        if workspace.tabs.len() == 1 {
            self.close_workspace(&active_space_id, &workspace_id)?;
            return Ok(serde_json::json!({ "tab_id": tab_id, "closed_workspace": true }));
        }
        let pane_ids = workspace.tabs[index]
            .layout
            .as_ref()
            .map(LayoutNode::pane_ids)
            .unwrap_or_default();
        for pane_id in &pane_ids {
            let _ = self.pane_manager.remove(pane_id);
        }
        self.snapshot
            .panes
            .retain(|pane| !pane_ids.iter().any(|id| *id == pane.pane_id));
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
            workspace.active_tab_id = workspace.tabs[index.min(workspace.tabs.len() - 1)]
                .tab_id
                .clone();
        }
        self.sync_focus_to_active_tab()?;
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn focus_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        let tab = self.active_tab_mut()?;
        if !tab
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
        {
            return Err(format!("pane '{pane_id}' does not exist in the active tab"));
        }
        tab.focused_pane_id = Some(pane_id.into());
        self.snapshot.focused_pane_id = Some(pane_id.into());
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn swap_panes(
        &mut self,
        source_pane_id: &str,
        target_pane_id: &str,
    ) -> Result<Value, String> {
        let tab = self.active_tab_mut()?;
        let layout = tab
            .layout
            .as_mut()
            .ok_or_else(|| "active tab has no panes".to_string())?;
        if !layout.swap_panes(source_pane_id, target_pane_id) {
            return Err("both panes must exist in the active tab and be different".into());
        }
        tab.focused_pane_id = Some(source_pane_id.to_owned());
        self.snapshot.focused_pane_id = Some(source_pane_id.to_owned());
        Ok(serde_json::json!({
            "source_pane_id": source_pane_id,
            "target_pane_id": target_pane_id
        }))
    }

    pub fn toggle_pane_zoom(&mut self, pane_id: &str) -> Result<Value, String> {
        let zoomed = {
            let tab = self.active_tab_mut()?;
            if !tab
                .layout
                .as_ref()
                .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
            {
                return Err(format!("pane '{pane_id}' does not exist in the active tab"));
            }
            tab.focused_pane_id = Some(pane_id.into());
            tab.zoomed = !tab.zoomed;
            tab.zoomed
        };
        self.snapshot.focused_pane_id = Some(pane_id.into());
        Ok(serde_json::json!({ "pane_id": pane_id, "zoomed": zoomed }))
    }

    pub fn toggle_right_click_passthrough(&mut self, pane_id: &str) -> Result<Value, String> {
        let pane = self
            .snapshot
            .panes
            .iter_mut()
            .find(|pane| pane.pane_id == pane_id)
            .ok_or_else(|| format!("pane '{pane_id}' does not exist"))?;
        pane.right_click_passthrough = !pane.right_click_passthrough;
        Ok(serde_json::json!({
            "pane_id": pane_id,
            "right_click_passthrough": pane.right_click_passthrough
        }))
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
        let tab = self.active_tab_mut()?;
        let current = tab.focused_pane_id.clone();
        let next = tab
            .layout
            .as_ref()
            .and_then(|layout| layout.next_pane(current.as_deref()))
            .map(str::to_string);
        let pane_id = next.ok_or_else(|| "active tab has no panes".to_string())?;
        tab.focused_pane_id = Some(pane_id.clone());
        self.snapshot.focused_pane_id = Some(pane_id.clone());
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn focus_previous(&mut self) -> Result<Value, String> {
        let tab = self.active_tab_mut()?;
        let current = tab.focused_pane_id.clone();
        let previous = tab
            .layout
            .as_ref()
            .and_then(|layout| layout.previous_pane(current.as_deref()))
            .map(str::to_string);
        let pane_id = previous.ok_or_else(|| "active tab has no panes".to_string())?;
        tab.focused_pane_id = Some(pane_id.clone());
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
        let tab = self.active_tab_mut()?;
        let current = tab
            .focused_pane_id
            .clone()
            .ok_or_else(|| "no pane is focused".to_string())?;
        let pane_id = tab
            .layout
            .as_ref()
            .and_then(|layout| layout.directional_pane(&current, movement))
            .map(str::to_string)
            .ok_or_else(|| "no pane in that direction".to_string())?;
        tab.focused_pane_id = Some(pane_id.clone());
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

    pub fn set_split_ratio(&mut self, path: &[bool], ratio: f32) -> Result<Value, String> {
        let tab = self.active_tab_mut()?;
        let layout = tab
            .layout
            .as_mut()
            .ok_or_else(|| "active tab has no panes".to_string())?;
        if !layout.set_split_ratio(path, ratio) {
            return Err("split path does not identify a split".into());
        }
        Ok(serde_json::json!({
            "path": path,
            "ratio": crate::model::layout::clamp_ratio(ratio),
        }))
    }

    pub fn close_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let space = self
            .snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let workspace_id = space
            .active_workspace_id
            .clone()
            .ok_or_else(|| "active space has no workspace".to_string())?;
        let workspace = space
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| "active workspace does not exist".to_string())?;
        let tab_index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == workspace.active_tab_id)
            .ok_or_else(|| "active tab does not exist".to_string())?;
        let tab = &workspace.tabs[tab_index];
        let pane_count = tab
            .layout
            .as_ref()
            .map_or(0, |layout| layout.pane_ids().len());
        if !tab
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
        {
            return Err(format!("pane '{pane_id}' does not exist in the active tab"));
        }
        if pane_count == 1 && workspace.tabs.len() == 1 {
            self.close_workspace(&active_space_id, &workspace_id)?;
            return Ok(serde_json::json!({
                "pane_id": pane_id,
                "closed_workspace": true
            }));
        }
        self.pane_manager
            .remove(pane_id)
            .map_err(|error| format!("{error:?}"))?;

        // Herdr removes a tab when its only pane closes and sibling tabs exist.
        if pane_count == 1 {
            self.snapshot.panes.retain(|pane| pane.pane_id != pane_id);
            let workspace = self
                .snapshot
                .spaces
                .iter_mut()
                .find(|space| space.space_id == active_space_id)
                .and_then(|space| {
                    space
                        .workspaces
                        .iter_mut()
                        .find(|workspace| workspace.workspace_id == workspace_id)
                })
                .ok_or_else(|| "active workspace does not exist".to_string())?;
            let closing_tab_id = workspace.tabs[tab_index].tab_id.clone();
            workspace.tabs.remove(tab_index);
            if workspace.active_tab_id == closing_tab_id {
                workspace.active_tab_id = workspace.tabs[tab_index.min(workspace.tabs.len() - 1)]
                    .tab_id
                    .clone();
            }
            self.sync_focus_to_active_tab()?;
            return Ok(serde_json::json!({ "pane_id": pane_id, "closed_tab": closing_tab_id }));
        }

        let focused_pane_id = {
            let workspace = self.active_workspace_mut()?;
            let tab = workspace
                .tabs
                .iter_mut()
                .find(|tab| tab.tab_id == workspace.active_tab_id)
                .ok_or_else(|| "active tab does not exist".to_string())?;
            let was_focused = tab.focused_pane_id.as_deref() == Some(pane_id);
            tab.layout = tab
                .layout
                .take()
                .and_then(|layout| layout.close_pane(pane_id));
            if was_focused {
                tab.focused_pane_id = tab
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()));
            }
            tab.focused_pane_id.clone()
        };
        self.snapshot.panes.retain(|pane| pane.pane_id != pane_id);
        self.snapshot.focused_pane_id = focused_pane_id;
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
        pane.agent = None;
        pane.agent_state = None;
        pane.agent_done = false;
        pane.screen.clear();
        pane.cursor = (0, 0);
        pane.cursor_visible = false;
        let tab = self.active_tab_mut()?;
        if tab
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
        {
            tab.focused_pane_id = Some(pane_id.into());
            self.snapshot.focused_pane_id = Some(pane_id.into());
        }
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

    fn close_workspace(&mut self, space_id: &str, workspace_id: &str) -> Result<(), String> {
        let space_index = self
            .snapshot
            .spaces
            .iter()
            .position(|space| space.space_id == space_id)
            .ok_or_else(|| "space does not exist".to_string())?;
        let workspace_index = self.snapshot.spaces[space_index]
            .workspaces
            .iter()
            .position(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| format!("workspace '{workspace_id}' does not exist"))?;
        let pane_ids = self.snapshot.spaces[space_index].workspaces[workspace_index]
            .tabs
            .iter()
            .flat_map(|tab| {
                tab.layout
                    .as_ref()
                    .map(LayoutNode::pane_ids)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        for pane_id in &pane_ids {
            let _ = self.pane_manager.remove(pane_id);
        }
        self.snapshot
            .panes
            .retain(|pane| !pane_ids.iter().any(|id| id == &pane.pane_id));

        let space = &mut self.snapshot.spaces[space_index];
        let was_active = space.active_workspace_id.as_deref() == Some(workspace_id);
        space.workspaces.remove(workspace_index);
        if was_active {
            space.active_workspace_id = space
                .workspaces
                .get(workspace_index.min(space.workspaces.len().saturating_sub(1)))
                .map(|workspace| workspace.workspace_id.clone());
        }

        if self.snapshot.spaces[space_index].workspaces.is_empty() {
            let next_space = (1..self.snapshot.spaces.len())
                .map(|offset| (space_index + offset) % self.snapshot.spaces.len())
                .find(|index| !self.snapshot.spaces[*index].workspaces.is_empty());
            if let Some(next_space) = next_space {
                self.snapshot.active_space_id = self.snapshot.spaces[next_space].space_id.clone();
            } else {
                self.snapshot.spaces[space_index].active_workspace_id = None;
            }
        }
        self.sync_focus_to_active_tab()
    }

    fn active_workspace_mut(&mut self) -> Result<&mut WorkspaceView, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let workspace_id = space
            .active_workspace_id
            .clone()
            .ok_or_else(|| "active space has no workspace".to_string())?;
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

    fn sync_focus_to_active_tab(&mut self) -> Result<(), String> {
        let current_focus = self.snapshot.focused_pane_id.clone();
        let active_space_id = self.snapshot.active_space_id.clone();
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let Some(workspace_id) = space.active_workspace_id.as_deref() else {
            self.snapshot.focused_pane_id = None;
            return Ok(());
        };
        let workspace = space
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| "active workspace does not exist".to_string())?;
        let tab = workspace
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == workspace.active_tab_id)
            .ok_or_else(|| "active tab does not exist".to_string())?;
        self.snapshot.focused_pane_id = tab.normalize_focus(current_focus.as_deref());
        Ok(())
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
                pane.agent = current.agent;
                pane.agent_state = current.agent_state;
                pane.agent_done = current.agent_done;
                pane.cols = current.terminal.snapshot().cols;
                pane.rows = current.terminal.snapshot().rows;
                pane.scrollback_bytes = current.scrollback.len();
                pane.scrollback = current.scrollback.iter().copied().collect();
                let terminal = current.terminal.snapshot();
                pane.screen = terminal.contents;
                pane.cursor = terminal.cursor;
                pane.cursor_visible = terminal.cursor_visible;
                pane.title = terminal.title;
                pane.alternate_screen = terminal.alternate_screen;
                pane.mouse_reporting = terminal.mouse_reporting;
                pane.mouse_release = terminal.mouse_release;
                pane.mouse_motion = terminal.mouse_motion;
                pane.mouse_any_motion = terminal.mouse_any_motion;
                pane.sgr_mouse = terminal.sgr_mouse;
                pane.utf8_mouse = terminal.utf8_mouse;
                pane.application_cursor = terminal.application_cursor;
                pane.bracketed_paste = terminal.bracketed_paste;
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
    fn plain_page_keys_follow_herdr_terminal_mode_rules() {
        let mut pane: PaneView = serde_json::from_value(serde_json::json!({
            "pane_id": "pane-1",
            "command": "powershell.exe",
            "args": [],
            "cwd": "C:/",
            "status": "Running",
            "scrollback_bytes": 0
        }))
        .unwrap();
        assert!(pane.plain_page_keys_use_host_scrollback());

        pane.application_cursor = true;
        assert!(!pane.plain_page_keys_use_host_scrollback());
        pane.bracketed_paste = true;
        assert!(pane.plain_page_keys_use_host_scrollback());
        pane.mouse_reporting = true;
        assert!(!pane.plain_page_keys_use_host_scrollback());
        pane.mouse_reporting = false;
        pane.alternate_screen = true;
        assert!(!pane.plain_page_keys_use_host_scrollback());
    }

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
            Some(workspace_id.to_owned())
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
        assert!(!pane.agent_done);
    }

    #[test]
    fn old_tab_snapshots_default_to_no_saved_focus() {
        let tab: super::TabView = serde_json::from_value(serde_json::json!({
            "tab_id": "tab-1",
            "name": "Main",
            "layout": { "Pane": { "pane_id": "pane-1" } }
        }))
        .unwrap();
        assert_eq!(tab.focused_pane_id, None);
        assert!(!tab.zoomed);
    }

    #[test]
    fn invalid_focus_and_switch_are_rejected() {
        let mut session = Session::default();
        assert!(session.focus_pane("missing").is_err());
        assert!(session.switch_workspace("missing").is_err());
        assert!(session.switch_tab("missing").is_err());
    }

    #[test]
    fn ensure_active_pane_does_not_accept_a_stale_layout_reference() {
        let mut session = Session::default();
        let tab = &mut session.snapshot.spaces[0].workspaces[0].tabs[0];
        tab.layout = Some(LayoutNode::pane("pane-missing"));
        tab.focused_pane_id = Some("pane-missing".into());
        session.snapshot.focused_pane_id = Some("pane-missing".into());

        let result = session.ensure_active_pane(CreatePaneRequest {
            command: "spindle-command-that-does-not-exist.exe".into(),
            args: Vec::new(),
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            label: None,
            env: Default::default(),
            cols: 80,
            rows: 24,
        });

        assert!(result.is_err(), "must try to create a replacement shell");
        assert!(session.snapshot.panes.is_empty());
        assert!(session.snapshot.focused_pane_id.is_none());
        assert!(session.snapshot.spaces[0].workspaces[0].tabs[0]
            .layout
            .is_none());
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
            agent: None,
            agent_state: None,
            agent_done: false,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: false,
            title: String::new(),
            alternate_screen: false,
            mouse_reporting: false,
            mouse_release: false,
            mouse_motion: false,
            mouse_any_motion: false,
            sgr_mouse: false,
            utf8_mouse: false,
            application_cursor: false,
            bracketed_paste: false,
            right_click_passthrough: false,
        });
        session.rename_pane("pane-1", "Shell".into()).unwrap();
        assert_eq!(session.snapshot.panes[0].label.as_deref(), Some("Shell"));
        session.rename_pane("pane-1", " ".into()).unwrap();
        assert!(session.snapshot.panes[0].label.is_none());
        let toggled = session.toggle_right_click_passthrough("pane-1").unwrap();
        assert_eq!(toggled["right_click_passthrough"], true);
        assert!(session.snapshot.panes[0].right_click_passthrough);
    }

    #[test]
    fn closing_last_tab_leaves_an_empty_recoverable_space() {
        let mut session = Session::default();
        assert!(session.delete_space("space-1").is_err());
        session.close_tab("tab-1").unwrap();
        assert!(session.snapshot().spaces[0].workspaces.is_empty());
        assert_eq!(session.snapshot().spaces[0].active_workspace_id, None);
        assert_eq!(session.snapshot().focused_pane_id, None);

        let restored = session.create_workspace("Restored".into()).unwrap();
        assert_eq!(
            session.snapshot().spaces[0].active_workspace_id.as_deref(),
            restored["workspace_id"].as_str()
        );
        assert_eq!(session.snapshot().spaces[0].workspaces.len(), 1);
        session
            .delete_workspace(restored["workspace_id"].as_str().unwrap())
            .unwrap();
        assert!(session.snapshot().spaces[0].workspaces.is_empty());

        let restored_again = session.create_workspace("Restored again".into()).unwrap();
        assert_eq!(
            session.snapshot().spaces[0].active_workspace_id.as_deref(),
            restored_again["workspace_id"].as_str()
        );
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
    fn closing_a_workspace_last_tab_selects_a_sibling_workspace() {
        let mut session = Session::default();
        let created = session.create_workspace("Feature".into()).unwrap();
        let closed_tab = format!("tab-{}-1", created["workspace_id"].as_str().unwrap());
        session.close_tab(&closed_tab).unwrap();
        assert_eq!(session.snapshot().spaces[0].workspaces.len(), 1);
        assert_eq!(
            session.snapshot().spaces[0].active_workspace_id.as_deref(),
            Some("workspace-1")
        );
    }

    #[test]
    fn closing_last_workspace_in_space_focuses_next_nonempty_space() {
        let mut session = Session::default();
        let second_space = session.create_space("Other project".into()).unwrap();
        let second_space_id = second_space["space_id"].as_str().unwrap();
        let second_workspace = session.snapshot().spaces[1].workspaces[0]
            .workspace_id
            .clone();

        session.delete_workspace(&second_workspace).unwrap();

        assert_eq!(session.snapshot().active_space_id, "space-1");
        assert_eq!(session.snapshot().spaces[1].active_workspace_id, None);
        assert_ne!(session.snapshot().active_space_id, second_space_id);
        assert_eq!(
            session.snapshot().focused_pane_id,
            None,
            "focus should be empty because the sibling workspace has no shell yet"
        );
    }

    #[test]
    fn setting_a_split_ratio_targets_a_nested_split() {
        let mut session = Session::default();
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(
            LayoutNode::pane("one")
                .split(crate::model::layout::Direction::Horizontal, 0.5, "two")
                .split(crate::model::layout::Direction::Vertical, 0.5, "three"),
        );
        session
            .set_split_ratio(&[false], 0.7)
            .expect("nested split path should be valid");
        let Some(LayoutNode::Split { first, .. }) =
            &session.snapshot.spaces[0].workspaces[0].tabs[0].layout
        else {
            panic!("expected root split");
        };
        assert!(matches!(first.as_ref(), LayoutNode::Split { ratio, .. }
            if (*ratio - 0.7).abs() < f32::EPSILON));
        assert!(session.set_split_ratio(&[true], 0.6).is_err());
    }

    #[test]
    fn swapping_panes_changes_layout_slots_and_focuses_the_source() {
        let mut session = Session::default();
        let tab = &mut session.snapshot.spaces[0].workspaces[0].tabs[0];
        tab.layout = Some(LayoutNode::pane("one").split(
            crate::model::layout::Direction::Horizontal,
            0.5,
            "two",
        ));
        tab.focused_pane_id = Some("one".into());

        session.swap_panes("one", "two").unwrap();

        let tab = &session.snapshot.spaces[0].workspaces[0].tabs[0];
        assert_eq!(tab.layout.as_ref().unwrap().pane_ids(), vec!["two", "one"]);
        assert_eq!(tab.focused_pane_id.as_deref(), Some("one"));
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("one"));
        assert!(session.swap_panes("one", "missing").is_err());
    }

    #[test]
    fn pane_zoom_targets_focus_and_toggles_per_tab() {
        let mut session = Session::default();
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(
            LayoutNode::pane("one").split(crate::model::layout::Direction::Horizontal, 0.5, "two"),
        );
        assert_eq!(
            session.toggle_pane_zoom("two").unwrap(),
            serde_json::json!({ "pane_id": "two", "zoomed": true })
        );
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("two"));
        assert!(session.snapshot.spaces[0].workspaces[0].tabs[0].zoomed);
        assert_eq!(
            session.toggle_pane_zoom("two").unwrap(),
            serde_json::json!({ "pane_id": "two", "zoomed": false })
        );
        assert!(!session.snapshot.spaces[0].workspaces[0].tabs[0].zoomed);
        assert!(session.toggle_pane_zoom("missing").is_err());
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
            agent: None,
            agent_state: None,
            agent_done: false,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: false,
            title: String::new(),
            alternate_screen: false,
            mouse_reporting: false,
            mouse_release: false,
            mouse_motion: false,
            mouse_any_motion: false,
            sgr_mouse: false,
            utf8_mouse: false,
            application_cursor: false,
            bracketed_paste: false,
            right_click_passthrough: false,
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
            agent: Some(crate::detect::AgentKind::Codex),
            agent_state: Some(crate::detect::AgentState::Idle),
            agent_done: true,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 3,
            scrollback: vec![1, 2, 3],
            screen: "screen".into(),
            cursor: (2, 1),
            cursor_visible: true,
            title: "title".into(),
            alternate_screen: true,
            mouse_reporting: false,
            mouse_release: false,
            mouse_motion: false,
            mouse_any_motion: false,
            sgr_mouse: false,
            utf8_mouse: false,
            application_cursor: false,
            bracketed_paste: false,
            right_click_passthrough: false,
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
        assert_eq!(metadata["data"]["panes"][0]["agent_state"], "idle");
        assert_eq!(metadata["data"]["panes"][0]["agent_done"], true);
        let restored = Session::load_or_default(&path).unwrap();
        assert_eq!(restored.snapshot().panes[0].scrollback, vec![1, 2, 3]);
        assert_eq!(restored.snapshot().panes[0].agent, None);
        assert_eq!(restored.snapshot().panes[0].agent_state, None);
        assert!(!restored.snapshot().panes[0].agent_done);
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
            agent: None,
            agent_state: None,
            agent_done: false,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: false,
            title: String::new(),
            alternate_screen: false,
            mouse_reporting: false,
            mouse_release: false,
            mouse_motion: false,
            mouse_any_motion: false,
            sgr_mouse: false,
            utf8_mouse: false,
            application_cursor: false,
            bracketed_paste: false,
            right_click_passthrough: false,
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
            agent: None,
            agent_state: None,
            agent_done: false,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: false,
            title: String::new(),
            alternate_screen: false,
            mouse_reporting: false,
            mouse_release: false,
            mouse_motion: false,
            mouse_any_motion: false,
            sgr_mouse: false,
            utf8_mouse: false,
            application_cursor: false,
            bracketed_paste: false,
            right_click_passthrough: false,
        });
        assert!(session.restart_pane("pane-1").is_err());
    }
}
