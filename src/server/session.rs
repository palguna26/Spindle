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
const GIT_BRANCH_REFRESH_INTERVAL: Duration = Duration::from_millis(1500);
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
    #[serde(default)]
    pub popup: bool,
    #[serde(default)]
    pub overlay: bool,
    #[serde(default)]
    pub(crate) popup_width_spec: Option<crate::popup_size::PopupSize>,
    #[serde(default)]
    pub(crate) popup_height_spec: Option<crate::popup_size::PopupSize>,
}

#[derive(Debug)]
pub struct AgentReportRequest {
    pub agent: crate::detect::AgentKind,
    pub state: crate::detect::AgentState,
    pub source: String,
    pub seq: Option<u64>,
    pub session_id: Option<String>,
    pub session_path: Option<String>,
}

#[derive(Debug)]
pub struct AgentSessionReportRequest {
    pub agent: crate::detect::AgentKind,
    pub source: String,
    pub seq: Option<u64>,
    pub session_id: Option<String>,
    pub session_path: Option<String>,
}

#[derive(Debug)]
pub struct ClearAgentAuthorityRequest {
    pub source: Option<String>,
    pub seq: Option<u64>,
}

#[derive(Debug)]
pub struct DisplayAgentReportRequest {
    pub source: String,
    pub display_agent: Option<String>,
    pub clear: bool,
    pub seq: Option<u64>,
}

#[derive(Debug)]
pub struct DisplayTitleReportRequest {
    pub source: String,
    pub display_title: Option<String>,
    pub clear: bool,
    pub seq: Option<u64>,
}

#[derive(Debug)]
pub struct DisplayStateLabelsReportRequest {
    pub source: String,
    pub state_labels: std::collections::BTreeMap<String, String>,
    pub clear: bool,
    pub seq: Option<u64>,
}

#[derive(Debug)]
pub struct MetadataTokensReportRequest {
    pub source: String,
    pub tokens: std::collections::HashMap<String, Option<String>>,
    pub ttl: Option<std::time::Duration>,
    pub seq: Option<u64>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub state_labels: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub tokens: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<crate::pane::AgentSessionInfo>,
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
    pub hyperlinks: Vec<crate::terminal::HyperlinkCell>,
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

    pub fn agent_display_name(&self) -> Option<&str> {
        self.display_agent
            .as_deref()
            .or_else(|| self.agent.map(crate::detect::AgentKind::label))
    }

    pub fn agent_display_state_label(&self) -> String {
        let state = self.agent_display_state();
        self.state_labels
            .get(state.label())
            .cloned()
            .unwrap_or_else(|| state.label().to_owned())
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
        pane.hyperlinks = saved.hyperlinks.clone();
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
    #[serde(default)]
    pub is_linked_worktree: bool,
    #[serde(default)]
    pub worktree_group: Option<String>,
    pub tabs: Vec<TabView>,
    pub active_tab_id: String,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub tokens: std::collections::HashMap<String, String>,
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
    pub popup_pane_id: Option<String>,
    #[serde(default)]
    pub popup_width: u16,
    #[serde(default)]
    pub popup_height: u16,
    #[serde(default)]
    pub(crate) popup_width_spec: Option<crate::popup_size::PopupSize>,
    #[serde(default)]
    pub(crate) popup_height_spec: Option<crate::popup_size::PopupSize>,
    #[serde(default)]
    pub overlay_pane_id: Option<String>,
    #[serde(default)]
    pub overlay_previous_focus: Option<String>,
    #[serde(default)]
    pub overlay_previous_zoomed: bool,
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
    #[serde(default)]
    hyperlinks: Vec<crate::terminal::HyperlinkCell>,
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
    last_git_branch_refresh: Instant,
    workspace_metadata_tokens:
        std::collections::HashMap<String, crate::metadata_tokens::MetadataTokens>,
    workspace_metadata_token_sequences: std::collections::HashMap<(String, String), u64>,
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
                        is_linked_worktree: false,
                        worktree_group: None,
                        tabs: vec![TabView {
                            tab_id: "tab-1".into(),
                            name: "Main".into(),
                            layout: None,
                            focused_pane_id: None,
                            zoomed: false,
                        }],
                        active_tab_id: "tab-1".into(),
                        tokens: std::collections::HashMap::new(),
                    }],
                    active_workspace_id: Some("workspace-1".into()),
                }],
                active_space_id: "space-1".into(),
                panes: Vec::new(),
                focused_pane_id: None,
                popup_pane_id: None,
                popup_width: 0,
                popup_height: 0,
                popup_width_spec: None,
                popup_height_spec: None,
                overlay_pane_id: None,
                overlay_previous_focus: None,
                overlay_previous_zoomed: false,
                event_sequence: 0,
            },
            next_pane_id: 1,
            snapshot_path: None,
            events: VecDeque::new(),
            event_bytes: 0,
            geometry_owner: None,
            geometry_owner_seen: None,
            last_git_branch_refresh: Instant::now(),
            workspace_metadata_tokens: std::collections::HashMap::new(),
            workspace_metadata_token_sequences: std::collections::HashMap::new(),
        }
    }
}

impl Session {
    pub fn load_or_default_with_scrollback(
        path: impl AsRef<Path>,
        scrollback_limit: usize,
    ) -> Result<Self, SnapshotError> {
        let mut session = Self::load_or_default(path)?;
        session.pane_manager = PaneManager::new(scrollback_limit.max(1));
        Ok(session)
    }

    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, SnapshotError> {
        let path = path.as_ref().to_path_buf();
        match load_versioned::<SessionSnapshot>(&path) {
            Ok(mut snapshot) => {
                load_history(&path, &mut snapshot)?;
                normalize_workspace_ids(&mut snapshot);
                // Popups are transient terminals in Herdr. Do not restore a
                // popup slot or its pane after a server restart.
                let popup_pane_id = snapshot.popup_pane_id.take();
                snapshot.popup_width = 0;
                snapshot.popup_height = 0;
                snapshot.popup_width_spec = None;
                snapshot.popup_height_spec = None;
                let overlay_pane_id = snapshot.overlay_pane_id.take();
                snapshot.overlay_previous_focus = None;
                snapshot.overlay_previous_zoomed = false;
                let transient_pane_ids = popup_pane_id.into_iter().chain(overlay_pane_id);
                for pane_id in transient_pane_ids {
                    snapshot.panes.retain(|pane| pane.pane_id != pane_id);
                }
                snapshot.focused_pane_id = active_layout_focus(&snapshot);
                for pane in &mut snapshot.panes {
                    pane.agent = None;
                    pane.agent_state = None;
                    pane.agent_done = false;
                    pane.display_agent = None;
                    pane.display_title = None;
                    pane.state_labels.clear();
                    pane.agent_session = None;
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
                    last_git_branch_refresh: Instant::now(),
                    workspace_metadata_tokens: std::collections::HashMap::new(),
                    workspace_metadata_token_sequences: std::collections::HashMap::new(),
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
                        hyperlinks: pane.hyperlinks.clone(),
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
                pane.hyperlinks.clear();
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
            workspace.is_linked_worktree = workspace
                .repository_path
                .as_deref()
                .is_some_and(crate::server::git::is_linked_worktree);
            workspace.worktree_group = workspace
                .repository_path
                .as_deref()
                .and_then(crate::server::git::worktree_group_key);
        }
    }

    pub fn snapshot(&self) -> &SessionSnapshot {
        &self.snapshot
    }

    pub fn process_info(&mut self, pane_id: &str) -> Result<Value, String> {
        self.poll();
        self.refresh_snapshot();
        let pane = self
            .snapshot
            .panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .ok_or_else(|| format!("pane '{pane_id}' does not exist"))?;
        let process_id = self
            .pane_manager
            .process_id(pane_id)
            .map_err(|error| format!("{error:?}"))?;
        Ok(serde_json::json!({
            "pane_id": pane.pane_id,
            "pid": process_id,
            "command": pane.command,
            "args": pane.args,
            "cwd": pane.cwd,
            "status": pane.status,
        }))
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

    pub fn record_worktree_event(&mut self, event: &str, payload: Value) -> Result<Value, String> {
        if !matches!(
            event,
            "worktree_created" | "worktree_opened" | "worktree_removed"
        ) {
            return Err(format!("unsupported worktree event: {event}"));
        }
        self.record_event(event, payload);
        Ok(serde_json::json!({ "event": event }))
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
        self.create_pane_with_direction(
            request,
            crate::model::layout::Direction::Vertical,
            0.5,
            true,
            false,
        )
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
        let workspace = self.active_workspace_mut()?;
        if workspace.tabs.is_empty() {
            let tab_id = format!("tab-{}-1", workspace.workspace_id);
            workspace.tabs.push(TabView {
                tab_id: tab_id.clone(),
                name: "Main".into(),
                layout: None,
                focused_pane_id: None,
                zoomed: false,
            });
            workspace.active_tab_id = tab_id;
        } else if !workspace
            .tabs
            .iter()
            .any(|tab| tab.tab_id == workspace.active_tab_id)
        {
            workspace.active_tab_id = workspace.tabs[0].tab_id.clone();
        }
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
        self.create_pane_with_direction(request, direction, 0.5, true, false)
    }

    pub fn split_pane_with_options(
        &mut self,
        request: CreatePaneRequest,
        direction: crate::model::layout::Direction,
        ratio: f32,
        focus: bool,
        right_click_passthrough: bool,
    ) -> Result<Value, String> {
        self.create_pane_with_direction(request, direction, ratio, focus, right_click_passthrough)
    }

    fn create_pane_with_direction(
        &mut self,
        request: CreatePaneRequest,
        direction: crate::model::layout::Direction,
        ratio: f32,
        focus: bool,
        right_click_passthrough: bool,
    ) -> Result<Value, String> {
        if request.command.trim().is_empty() {
            return Err("command cannot be empty".into());
        }
        if request.cols == 0 || request.rows == 0 {
            return Err("pane dimensions must be greater than zero".into());
        }
        if !ratio.is_finite() || !(0.05..=0.95).contains(&ratio) {
            return Err("split ratio must be finite and between 0.05 and 0.95".into());
        }
        if request.popup && request.overlay {
            return Err("pane cannot be both popup and overlay".into());
        }
        if request.popup && self.snapshot.popup_pane_id.is_some() {
            return Err("a popup pane is already open".into());
        }
        if request.overlay && self.snapshot.overlay_pane_id.is_some() {
            return Err("an overlay pane is already open".into());
        }
        let overlay_previous_focus = if request.overlay {
            active_layout_focus(&self.snapshot)
        } else {
            None
        };
        let pane_id = format!("pane-{}", self.next_pane_id);
        self.next_pane_id += 1;
        let mut pane_env = request.env.clone();
        pane_env.insert("SPINDLE_ENV".into(), "1".into());
        pane_env.insert("HERDR_ENV".into(), "1".into());
        pane_env.insert("SPINDLE_PANE_ID".into(), pane_id.clone());
        pane_env.insert("HERDR_PANE_ID".into(), pane_id.clone());
        if let Some((workspace_id, tab_id)) = self.active_pane_context_ids() {
            pane_env.insert("SPINDLE_WORKSPACE_ID".into(), workspace_id.clone());
            pane_env.insert("HERDR_WORKSPACE_ID".into(), workspace_id);
            pane_env.insert("SPINDLE_TAB_ID".into(), tab_id.clone());
            pane_env.insert("HERDR_TAB_ID".into(), tab_id);
        }
        if let Ok(executable) = std::env::current_exe() {
            let executable = executable.to_string_lossy().into_owned();
            pane_env.insert("SPINDLE_BIN_PATH".into(), executable.clone());
            pane_env.insert("HERDR_BIN_PATH".into(), executable);
        }
        self.pane_manager
            .spawn(
                &pane_id,
                PaneConfig {
                    command: request.command.clone(),
                    args: request.args.clone(),
                    cwd: request.cwd.clone(),
                    env: pane_env,
                    cols: request.cols,
                    rows: request.rows,
                },
            )
            .map_err(|error| format!("{error:?}"))?;

        if request.popup {
            self.snapshot.popup_pane_id = Some(pane_id.clone());
            self.snapshot.popup_width = request.cols;
            self.snapshot.popup_height = request.rows;
            self.snapshot.popup_width_spec = request.popup_width_spec;
            self.snapshot.popup_height_spec = request.popup_height_spec;
        } else {
            let tab = self.active_tab_mut()?;
            let previous_zoomed = tab.zoomed;
            let focused = tab.focused_pane_id.clone().or_else(|| {
                tab.layout
                    .as_ref()
                    .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()))
            });
            tab.layout = Some(match tab.layout.take() {
                None => LayoutNode::pane(&pane_id),
                Some(layout) => {
                    match focused.as_deref().and_then(|target| {
                        layout
                            .clone()
                            .split_pane_with_ratio(target, direction, ratio, &pane_id)
                    }) {
                        Some(layout) => layout,
                        None => layout.split(direction, ratio, &pane_id),
                    }
                }
            });
            if focus {
                tab.focused_pane_id = Some(pane_id.clone());
            }
            if request.overlay {
                tab.zoomed = true;
                self.snapshot.overlay_pane_id = Some(pane_id.clone());
                self.snapshot.overlay_previous_focus = overlay_previous_focus;
                self.snapshot.overlay_previous_zoomed = previous_zoomed;
            }
        }
        if focus {
            self.snapshot.focused_pane_id = Some(pane_id.clone());
        }
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            tokens: std::collections::HashMap::new(),
            agent_session: None,
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
            right_click_passthrough,
            hyperlinks: Vec::new(),
        });
        self.record_event("pane_created", serde_json::json!({ "pane_id": pane_id }));
        self.record_pane_events(vec![PaneEvent::Status {
            pane_id: pane_id.clone(),
            status: PaneStatus::Running,
        }]);
        if !request.popup {
            self.record_active_layout_event();
        }
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
        let workspace_id = next_numbered_id(
            "workspace",
            self.snapshot
                .spaces
                .iter()
                .flat_map(|space| space.workspaces.iter())
                .map(|workspace| workspace.workspace_id.clone()),
        );
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == self.snapshot.active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let tab_id = format!("tab-{}-1", workspace_id);
        let is_linked_worktree = repository_path
            .as_deref()
            .is_some_and(crate::server::git::is_linked_worktree);
        let worktree_group = repository_path
            .as_deref()
            .and_then(crate::server::git::worktree_group_key);
        space.workspaces.push(WorkspaceView {
            workspace_id: workspace_id.clone(),
            name,
            repository_path,
            branch,
            is_linked_worktree,
            worktree_group,
            tabs: vec![TabView {
                tab_id: tab_id.clone(),
                name: "Main".into(),
                layout: None,
                focused_pane_id: None,
                zoomed: false,
            }],
            active_tab_id: tab_id,
            tokens: std::collections::HashMap::new(),
        });
        space.active_workspace_id = Some(workspace_id.clone());
        self.sync_focus_to_active_tab()?;
        self.record_event(
            "workspace_created",
            serde_json::json!({ "workspace_id": workspace_id }),
        );
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
                is_linked_worktree: false,
                worktree_group: None,
                tabs: vec![TabView {
                    tab_id: tab_id.clone(),
                    name: "Main".into(),
                    layout: None,
                    focused_pane_id: None,
                    zoomed: false,
                }],
                active_tab_id: tab_id,
                tokens: std::collections::HashMap::new(),
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
        self.record_event(
            "workspace_focused",
            serde_json::json!({ "workspace_id": workspace_id }),
        );
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn focus_workspace(&mut self, workspace_id: &str) -> Result<Value, String> {
        let space_id = self
            .snapshot
            .spaces
            .iter()
            .find(|space| {
                space
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == workspace_id)
            })
            .map(|space| space.space_id.clone())
            .ok_or_else(|| format!("workspace '{workspace_id}' does not exist"))?;

        self.snapshot.active_space_id = space_id.clone();
        let space = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == space_id)
            .expect("workspace's space was found above");
        space.active_workspace_id = Some(workspace_id.into());
        self.sync_focus_to_active_tab()?;
        self.record_event(
            "workspace_focused",
            serde_json::json!({ "workspace_id": workspace_id, "space_id": space_id }),
        );
        Ok(serde_json::json!({
            "space_id": space_id,
            "workspace_id": workspace_id
        }))
    }

    pub fn rename_workspace(&mut self, workspace_id: &str, name: String) -> Result<Value, String> {
        let workspace = self.workspace_mut(workspace_id)?;
        workspace.name = name.clone();
        self.record_event(
            "workspace_renamed",
            serde_json::json!({ "workspace_id": workspace_id, "label": name }),
        );
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn move_workspace_anywhere(
        &mut self,
        workspace_id: &str,
        insert_index: usize,
    ) -> Result<Value, String> {
        let Some((space_index, workspace_index)) = self
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find_map(|(space_index, space)| {
                space
                    .workspaces
                    .iter()
                    .position(|workspace| workspace.workspace_id == workspace_id)
                    .map(|workspace_index| (space_index, workspace_index))
            })
        else {
            return Err(format!("workspace '{workspace_id}' does not exist"));
        };

        let workspaces = &self.snapshot.spaces[space_index].workspaces;
        let source = &workspaces[workspace_index];
        if source.is_linked_worktree {
            return Err(format!(
                "linked worktree workspace '{workspace_id}' moves with its primary workspace"
            ));
        }
        let group = source.worktree_group.as_deref();
        let roots: Vec<_> = workspaces
            .iter()
            .filter(|workspace| !workspace.is_linked_worktree)
            .collect();
        let source_root_index = roots
            .iter()
            .position(|workspace| workspace.workspace_id == workspace_id)
            .expect("non-linked workspace is a root");
        if insert_index > roots.len() {
            return Err(format!(
                "insert index {insert_index} is out of bounds for {} root workspaces",
                roots.len()
            ));
        }
        let target_index = if source_root_index < insert_index {
            insert_index.saturating_sub(1)
        } else {
            insert_index
        }
        .min(roots.len().saturating_sub(1));
        if source_root_index == target_index {
            return Ok(serde_json::json!({
                "workspace_id": workspace_id,
                "insert_index": insert_index,
                "moved": false,
            }));
        }

        let moved_ids: Vec<_> = workspaces
            .iter()
            .filter(|workspace| {
                workspace.workspace_id == workspace_id
                    || group.is_some_and(|group| workspace.worktree_group.as_deref() == Some(group))
            })
            .map(|workspace| workspace.workspace_id.clone())
            .collect();
        let mut blocks: Vec<Vec<_>> = Vec::new();
        for workspace in workspaces {
            if workspace.is_linked_worktree {
                continue;
            }
            let block_ids: Vec<_> = workspaces
                .iter()
                .filter(|candidate| {
                    candidate.workspace_id == workspace.workspace_id
                        || workspace
                            .worktree_group
                            .as_deref()
                            .is_some_and(|group| candidate.worktree_group.as_deref() == Some(group))
                })
                .map(|candidate| candidate.workspace_id.clone())
                .collect();
            if !blocks.iter().any(|block| block == &block_ids) {
                blocks.push(block_ids);
            }
        }
        let moved_block_index = blocks
            .iter()
            .position(|block| block.iter().any(|id| id == workspace_id))
            .expect("workspace block exists");
        let moved_block = blocks.remove(moved_block_index);
        blocks.insert(target_index, moved_block);
        let ordered_ids: Vec<_> = blocks.into_iter().flatten().collect();
        let old_workspaces = std::mem::take(&mut self.snapshot.spaces[space_index].workspaces);
        self.snapshot.spaces[space_index].workspaces = ordered_ids
            .into_iter()
            .filter_map(|id| {
                old_workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == id)
                    .cloned()
            })
            .collect();
        self.record_event(
            "workspace_reordered",
            serde_json::json!({
                "workspace_id": workspace_id,
                "space_id": self.snapshot.spaces[space_index].space_id,
                "insert_index": insert_index,
                "workspace_ids": moved_ids,
            }),
        );
        Ok(serde_json::json!({
            "workspace_id": workspace_id,
            "insert_index": insert_index,
            "moved": true,
        }))
    }

    pub fn delete_workspace(&mut self, workspace_id: &str) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        self.close_workspace(&active_space_id, workspace_id)?;
        Ok(serde_json::json!({ "workspace_id": workspace_id }))
    }

    pub fn delete_workspace_anywhere(&mut self, workspace_id: &str) -> Result<Value, String> {
        self.delete_workspace_anywhere_with_group(workspace_id, false)
    }

    pub fn delete_workspace_anywhere_with_group(
        &mut self,
        workspace_id: &str,
        close_group: bool,
    ) -> Result<Value, String> {
        let _space_id = self
            .snapshot
            .spaces
            .iter()
            .find(|space| {
                space
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == workspace_id)
            })
            .map(|space| space.space_id.clone())
            .ok_or_else(|| format!("workspace '{workspace_id}' does not exist"))?;
        let target = self
            .snapshot
            .spaces
            .iter()
            .flat_map(|space| space.workspaces.iter())
            .find(|workspace| workspace.workspace_id == workspace_id)
            .expect("workspace was found above");
        let group_key = target.worktree_group.clone();
        let group_ids = group_key.as_deref().map(|key| {
            self.snapshot
                .spaces
                .iter()
                .flat_map(|space| space.workspaces.iter())
                .filter(|workspace| workspace.worktree_group.as_deref() == Some(key))
                .map(|workspace| workspace.workspace_id.clone())
                .collect::<Vec<_>>()
        });
        let group_has_linked_member = group_ids.as_ref().is_some_and(|ids| {
            ids.iter().any(|id| {
                self.snapshot
                    .spaces
                    .iter()
                    .flat_map(|space| space.workspaces.iter())
                    .any(|workspace| workspace.workspace_id == *id && workspace.is_linked_worktree)
            })
        });
        let ids = if close_group && group_has_linked_member {
            group_ids
                .filter(|ids| !ids.is_empty())
                .unwrap_or_else(|| vec![workspace_id.to_owned()])
        } else {
            if !target.is_linked_worktree && group_has_linked_member {
                return Err(
                    "workspace has linked worktree workspaces; use --group to close the group"
                        .into(),
                );
            }
            vec![workspace_id.to_owned()]
        };
        for id in &ids {
            let space_id = self
                .snapshot
                .spaces
                .iter()
                .find(|space| {
                    space
                        .workspaces
                        .iter()
                        .any(|workspace| &workspace.workspace_id == id)
                })
                .map(|space| space.space_id.clone())
                .ok_or_else(|| format!("workspace '{id}' does not exist"))?;
            self.close_workspace(&space_id, id)?;
        }
        Ok(serde_json::json!({
            "workspace_id": workspace_id,
            "closed_workspace_ids": ids,
        }))
    }

    pub fn create_tab(&mut self, name: String) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        let workspace_id = workspace.workspace_id.clone();
        let tab_id = next_numbered_id(
            &format!("tab-{}", workspace_id),
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
        self.record_event(
            "tab_created",
            serde_json::json!({ "tab_id": tab_id, "workspace_id": workspace_id }),
        );
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn move_tab_anywhere(
        &mut self,
        tab_id: &str,
        insert_index: usize,
    ) -> Result<Value, String> {
        let Some((space_index, workspace_index, tab_index)) = self
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find_map(|(space_index, space)| {
                space
                    .workspaces
                    .iter()
                    .enumerate()
                    .find_map(|(workspace_index, workspace)| {
                        workspace
                            .tabs
                            .iter()
                            .position(|tab| tab.tab_id == tab_id)
                            .map(|tab_index| (space_index, workspace_index, tab_index))
                    })
            })
        else {
            return Err(format!("tab '{tab_id}' does not exist"));
        };

        let workspace = &mut self.snapshot.spaces[space_index].workspaces[workspace_index];
        if insert_index > workspace.tabs.len() {
            return Err(format!(
                "insert index {insert_index} is out of bounds for {} tabs",
                workspace.tabs.len()
            ));
        }
        let target_index = if tab_index < insert_index {
            insert_index.saturating_sub(1)
        } else {
            insert_index
        }
        .min(workspace.tabs.len().saturating_sub(1));
        if tab_index == target_index {
            return Ok(serde_json::json!({
                "tab_id": tab_id,
                "workspace_id": workspace.workspace_id,
                "insert_index": insert_index,
                "moved": false,
            }));
        }

        let moved = workspace.tabs.remove(tab_index);
        workspace.tabs.insert(target_index, moved);
        let workspace_id = workspace.workspace_id.clone();
        self.record_event(
            "tab_moved",
            serde_json::json!({
                "tab_id": tab_id,
                "workspace_id": workspace_id,
                "insert_index": insert_index,
            }),
        );
        Ok(serde_json::json!({
            "tab_id": tab_id,
            "workspace_id": workspace_id,
            "insert_index": insert_index,
            "moved": true,
        }))
    }

    pub fn move_pane_to_new_tab(&mut self, pane_id: &str, name: String) -> Result<Value, String> {
        self.move_pane_to_new_tab_with_focus(pane_id, name, true)
    }

    pub fn move_pane_to_new_tab_in_workspace(
        &mut self,
        pane_id: &str,
        name: String,
        target_workspace_id: Option<&str>,
        focus: bool,
    ) -> Result<Value, String> {
        let space_index = self
            .snapshot
            .spaces
            .iter()
            .position(|space| space.space_id == self.snapshot.active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let source_workspace_id = self.snapshot.spaces[space_index]
            .active_workspace_id
            .clone()
            .ok_or_else(|| "active space has no workspace".to_string())?;
        let source_workspace_index = self.snapshot.spaces[space_index]
            .workspaces
            .iter()
            .position(|workspace| workspace.workspace_id == source_workspace_id)
            .ok_or_else(|| "active workspace does not exist".to_string())?;
        let target_workspace_index = target_workspace_id
            .map(|id| {
                self.snapshot.spaces[space_index]
                    .workspaces
                    .iter()
                    .position(|workspace| workspace.workspace_id == id)
                    .ok_or_else(|| format!("workspace '{id}' does not exist"))
            })
            .transpose()?
            .unwrap_or(source_workspace_index);
        let source_tab_id = {
            let source_workspace =
                &mut self.snapshot.spaces[space_index].workspaces[source_workspace_index];
            let source_index = source_workspace
                .tabs
                .iter()
                .position(|tab| tab.tab_id == source_workspace.active_tab_id)
                .ok_or_else(|| "active tab does not exist".to_string())?;
            let source = &mut source_workspace.tabs[source_index];
            let pane_count = source
                .layout
                .as_ref()
                .map_or(0, |layout| layout.pane_ids().len());
            if pane_count == 0
                || !source
                    .layout
                    .as_ref()
                    .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
            {
                return Err(format!("pane '{pane_id}' does not exist in the active tab"));
            }
            let source_tab_id = source.tab_id.clone();
            if pane_count == 1 {
                source.layout = None;
                source.focused_pane_id = None;
            } else {
                source.layout = source
                    .layout
                    .take()
                    .and_then(|layout| layout.close_pane(pane_id));
                source.focused_pane_id = source
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()));
            }
            source_tab_id
        };
        let tab_id = {
            let target = &self.snapshot.spaces[space_index].workspaces[target_workspace_index];
            next_numbered_id(
                &format!("tab-{}", target.workspace_id),
                target.tabs.iter().map(|tab| tab.tab_id.clone()),
            )
        };
        let target_workspace_id = self.snapshot.spaces[space_index].workspaces
            [target_workspace_index]
            .workspace_id
            .clone();
        let target = &mut self.snapshot.spaces[space_index].workspaces[target_workspace_index];
        target.tabs.push(TabView {
            tab_id: tab_id.clone(),
            name: if name.trim().is_empty() {
                "Moved pane".into()
            } else {
                name
            },
            layout: Some(LayoutNode::pane(pane_id)),
            focused_pane_id: Some(pane_id.into()),
            zoomed: false,
        });
        if focus {
            target.active_tab_id = tab_id.clone();
            self.snapshot.spaces[space_index].active_workspace_id =
                Some(target_workspace_id.clone());
            self.snapshot.focused_pane_id = Some(pane_id.into());
        } else {
            self.sync_focus_to_active_tab()?;
        }
        self.record_event(
            "pane_moved",
            serde_json::json!({
                "pane_id": pane_id,
                "previous_tab_id": source_tab_id,
                "workspace_id": target_workspace_id,
                "tab_id": tab_id,
            }),
        );
        self.record_active_layout_event();
        Ok(
            serde_json::json!({ "pane_id": pane_id, "workspace_id": target_workspace_id, "tab_id": tab_id }),
        )
    }

    pub fn move_pane_to_new_tab_anywhere(
        &mut self,
        pane_id: &str,
        name: String,
        target_workspace_id: Option<&str>,
        focus: bool,
    ) -> Result<Value, String> {
        let (source_space_index, source_workspace_index, source_tab_index) = self
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find_map(|(space_index, space)| {
                space
                    .workspaces
                    .iter()
                    .enumerate()
                    .find_map(|(workspace_index, workspace)| {
                        workspace
                            .tabs
                            .iter()
                            .enumerate()
                            .find_map(|(tab_index, tab)| {
                                tab.layout
                                    .as_ref()
                                    .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
                                    .then_some((space_index, workspace_index, tab_index))
                            })
                    })
            })
            .ok_or_else(|| format!("pane '{pane_id}' does not exist"))?;
        let (target_space_index, target_workspace_index) =
            if let Some(target_id) = target_workspace_id {
                self.snapshot
                    .spaces
                    .iter()
                    .enumerate()
                    .find_map(|(space_index, space)| {
                        space
                            .workspaces
                            .iter()
                            .position(|workspace| workspace.workspace_id == target_id)
                            .map(|workspace_index| (space_index, workspace_index))
                    })
                    .ok_or_else(|| format!("workspace '{target_id}' does not exist"))?
            } else {
                (source_space_index, source_workspace_index)
            };
        let source_tab_id = self.snapshot.spaces[source_space_index].workspaces
            [source_workspace_index]
            .tabs[source_tab_index]
            .tab_id
            .clone();
        {
            let source = &mut self.snapshot.spaces[source_space_index].workspaces
                [source_workspace_index]
                .tabs[source_tab_index];
            if source
                .layout
                .as_ref()
                .map_or(0, |layout| layout.pane_ids().len())
                == 1
            {
                source.layout = None;
                source.focused_pane_id = None;
            } else {
                source.layout = source
                    .layout
                    .take()
                    .and_then(|layout| layout.close_pane(pane_id));
                source.focused_pane_id = source
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()));
            }
        }
        let (tab_id, target_workspace_id) = {
            let target =
                &mut self.snapshot.spaces[target_space_index].workspaces[target_workspace_index];
            let tab_id = next_numbered_id(
                &format!("tab-{}", target.workspace_id),
                target.tabs.iter().map(|tab| tab.tab_id.clone()),
            );
            let target_workspace_id = target.workspace_id.clone();
            target.tabs.push(TabView {
                tab_id: tab_id.clone(),
                name: if name.trim().is_empty() {
                    "Moved pane".into()
                } else {
                    name
                },
                layout: Some(LayoutNode::pane(pane_id)),
                focused_pane_id: Some(pane_id.into()),
                zoomed: false,
            });
            if focus {
                target.active_tab_id = tab_id.clone();
            }
            (tab_id, target_workspace_id)
        };
        if focus {
            self.snapshot.active_space_id =
                self.snapshot.spaces[target_space_index].space_id.clone();
            self.snapshot.spaces[target_space_index].active_workspace_id =
                Some(target_workspace_id.clone());
            self.snapshot.focused_pane_id = Some(pane_id.into());
        } else {
            self.sync_focus_to_active_tab()?;
        }
        self.record_event(
            "pane_moved",
            serde_json::json!({
                "pane_id": pane_id,
                "previous_tab_id": source_tab_id,
                "workspace_id": target_workspace_id,
                "tab_id": tab_id,
            }),
        );
        self.record_active_layout_event();
        Ok(
            serde_json::json!({ "pane_id": pane_id, "workspace_id": target_workspace_id, "tab_id": tab_id }),
        )
    }

    pub fn move_pane_to_new_tab_with_focus(
        &mut self,
        pane_id: &str,
        name: String,
        focus: bool,
    ) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        let source_index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == workspace.active_tab_id)
            .ok_or_else(|| "active tab does not exist".to_string())?;
        let source = &mut workspace.tabs[source_index];
        let source_tab_id = source.tab_id.clone();
        let pane_count = source
            .layout
            .as_ref()
            .map_or(0, |layout| layout.pane_ids().len());
        if pane_count == 0
            || !source
                .layout
                .as_ref()
                .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
        {
            return Err(format!("pane '{pane_id}' does not exist in the active tab"));
        }
        if pane_count == 1 {
            workspace.tabs.remove(source_index);
        } else {
            source.layout = source
                .layout
                .take()
                .and_then(|layout| layout.close_pane(pane_id));
            source.focused_pane_id = source
                .layout
                .as_ref()
                .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()));
        }
        let tab_id = next_numbered_id(
            &format!("tab-{}", workspace.workspace_id),
            workspace.tabs.iter().map(|tab| tab.tab_id.clone()),
        );
        workspace.tabs.push(TabView {
            tab_id: tab_id.clone(),
            name: if name.trim().is_empty() {
                "Moved pane".into()
            } else {
                name
            },
            layout: Some(LayoutNode::pane(pane_id)),
            focused_pane_id: Some(pane_id.into()),
            zoomed: false,
        });
        if focus {
            workspace.active_tab_id = tab_id.clone();
            self.snapshot.focused_pane_id = Some(pane_id.into());
        }
        self.record_event(
            "pane_moved",
            serde_json::json!({
                "pane_id": pane_id,
                "previous_tab_id": source_tab_id,
                "tab_id": tab_id,
            }),
        );
        self.record_active_layout_event();
        Ok(serde_json::json!({ "pane_id": pane_id, "tab_id": tab_id }))
    }

    pub fn move_pane_to_new_workspace(
        &mut self,
        pane_id: &str,
        workspace_name: String,
        tab_name: String,
        focus: bool,
    ) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let space_index = self
            .snapshot
            .spaces
            .iter()
            .position(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?;
        let source_workspace_id = self.snapshot.spaces[space_index]
            .active_workspace_id
            .clone()
            .ok_or_else(|| "active space has no workspace".to_string())?;
        let workspace_index = self.snapshot.spaces[space_index]
            .workspaces
            .iter()
            .position(|workspace| workspace.workspace_id == source_workspace_id)
            .ok_or_else(|| "active workspace does not exist".to_string())?;
        let workspace = &mut self.snapshot.spaces[space_index].workspaces[workspace_index];
        let source_index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == workspace.active_tab_id)
            .ok_or_else(|| "active tab does not exist".to_string())?;
        let source_tab_id = workspace.tabs[source_index].tab_id.clone();
        let source = &workspace.tabs[source_index];
        if !source
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
        {
            return Err(format!("pane '{pane_id}' does not exist in the active tab"));
        }
        let source_was_single = source
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().len() == 1);
        if source_was_single {
            workspace.tabs[source_index].layout = None;
            workspace.tabs[source_index].focused_pane_id = None;
        } else {
            let source = &mut workspace.tabs[source_index];
            source.layout = source
                .layout
                .take()
                .and_then(|layout| layout.close_pane(pane_id));
            source.focused_pane_id = source
                .layout
                .as_ref()
                .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()));
        }
        let workspace_id = next_numbered_id(
            "workspace",
            self.snapshot
                .spaces
                .iter()
                .flat_map(|space| space.workspaces.iter())
                .map(|workspace| workspace.workspace_id.clone()),
        );
        let tab_id = format!("tab-{}-1", workspace_id);
        let new_workspace_name = if workspace_name.trim().is_empty() {
            "Moved pane".into()
        } else {
            workspace_name
        };
        let new_tab_name = if tab_name.trim().is_empty() {
            "Main".into()
        } else {
            tab_name
        };
        self.snapshot.spaces[space_index]
            .workspaces
            .push(WorkspaceView {
                workspace_id: workspace_id.clone(),
                name: new_workspace_name,
                repository_path: None,
                branch: None,
                is_linked_worktree: false,
                worktree_group: None,
                tabs: vec![TabView {
                    tab_id: tab_id.clone(),
                    name: new_tab_name,
                    layout: Some(LayoutNode::pane(pane_id)),
                    focused_pane_id: Some(pane_id.into()),
                    zoomed: false,
                }],
                active_tab_id: tab_id.clone(),
                tokens: std::collections::HashMap::new(),
            });
        if focus {
            self.snapshot.spaces[space_index].active_workspace_id = Some(workspace_id.clone());
            self.snapshot.focused_pane_id = Some(pane_id.into());
        } else {
            self.sync_focus_to_active_tab()?;
        }
        self.record_event(
            "workspace_created",
            serde_json::json!({ "workspace_id": workspace_id, "space_id": active_space_id }),
        );
        self.record_event(
            "pane_moved",
            serde_json::json!({
                "pane_id": pane_id,
                "previous_tab_id": source_tab_id,
                "workspace_id": workspace_id,
                "tab_id": tab_id,
            }),
        );
        self.record_active_layout_event();
        Ok(
            serde_json::json!({ "pane_id": pane_id, "workspace_id": workspace_id, "tab_id": tab_id }),
        )
    }

    pub fn move_pane_to_tab(
        &mut self,
        pane_id: &str,
        target_tab_id: &str,
        target_pane_id: Option<&str>,
        direction: crate::model::layout::Direction,
        ratio: f32,
        focus: bool,
    ) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        let source_index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == workspace.active_tab_id)
            .ok_or_else(|| "active tab does not exist".to_string())?;
        let target_index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == target_tab_id)
            .ok_or_else(|| format!("tab '{target_tab_id}' does not exist"))?;
        if source_index == target_index {
            return Err("pane is already in the target tab".into());
        }
        if workspace.tabs[target_index].zoomed {
            return Err("cannot move a pane into a zoomed tab".into());
        }
        if let Some(target_pane_id) = target_pane_id {
            if !workspace.tabs[target_index]
                .layout
                .as_ref()
                .is_some_and(|layout| layout.pane_ids().contains(&target_pane_id))
            {
                return Err(format!("target pane '{target_pane_id}' does not exist"));
            }
        }
        let source = &workspace.tabs[source_index];
        let source_tab_id = source.tab_id.clone();
        if !source
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().contains(&pane_id))
        {
            return Err(format!("pane '{pane_id}' does not exist in the active tab"));
        }

        let source_was_single = source
            .layout
            .as_ref()
            .is_some_and(|layout| layout.pane_ids().len() == 1);
        if source_was_single {
            workspace.tabs.remove(source_index);
        } else {
            let source = &mut workspace.tabs[source_index];
            source.layout = source
                .layout
                .take()
                .and_then(|layout| layout.close_pane(pane_id));
            source.focused_pane_id = source
                .layout
                .as_ref()
                .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()));
        }

        let target = workspace
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == target_tab_id)
            .expect("target tab remains after removing a different source tab");
        target.layout = if let Some(layout) = target.layout.take() {
            if let Some(target_pane_id) = target_pane_id {
                layout
                    .split_pane_with_ratio(target_pane_id, direction, ratio, pane_id)
                    .ok_or_else(|| format!("target pane '{target_pane_id}' does not exist"))
                    .map(Some)?
            } else {
                Some(layout.split(direction, ratio, pane_id))
            }
        } else {
            Some(LayoutNode::pane(pane_id))
        };
        if focus {
            target.focused_pane_id = Some(pane_id.into());
        }
        workspace.active_tab_id = target_tab_id.into();
        if focus {
            self.snapshot.focused_pane_id = Some(pane_id.into());
        }
        self.record_event(
            "pane_moved",
            serde_json::json!({
                "pane_id": pane_id,
                "previous_tab_id": source_tab_id,
                "tab_id": target_tab_id,
            }),
        );
        self.record_active_layout_event();
        Ok(serde_json::json!({
            "pane_id": pane_id,
            "tab_id": target_tab_id,
            "source_tab_removed": source_was_single,
        }))
    }

    pub fn switch_tab(&mut self, tab_id: &str) -> Result<Value, String> {
        let workspace = self.active_workspace_mut()?;
        if !workspace.tabs.iter().any(|tab| tab.tab_id == tab_id) {
            return Err(format!("tab '{tab_id}' does not exist"));
        }
        workspace.active_tab_id = tab_id.into();
        self.sync_focus_to_active_tab()?;
        self.record_event("tab_focused", serde_json::json!({ "tab_id": tab_id }));
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn switch_tab_anywhere(&mut self, tab_id: &str) -> Result<Value, String> {
        let Some((space_index, workspace_index, _tab_index)) = self
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find_map(|(space_index, space)| {
                space
                    .workspaces
                    .iter()
                    .enumerate()
                    .find_map(|(workspace_index, workspace)| {
                        workspace
                            .tabs
                            .iter()
                            .position(|tab| tab.tab_id == tab_id)
                            .map(|tab_index| (space_index, workspace_index, tab_index))
                    })
            })
        else {
            return Err(format!("tab '{tab_id}' does not exist"));
        };
        let workspace_id = self.snapshot.spaces[space_index].workspaces[workspace_index]
            .workspace_id
            .clone();
        let space_id = self.snapshot.spaces[space_index].space_id.clone();
        self.snapshot.active_space_id = space_id.clone();
        self.snapshot.spaces[space_index].active_workspace_id = Some(workspace_id.clone());
        self.snapshot.spaces[space_index].workspaces[workspace_index].active_tab_id = tab_id.into();
        self.sync_focus_to_active_tab()?;
        self.record_event(
            "tab_focused",
            serde_json::json!({
                "tab_id": tab_id,
                "workspace_id": workspace_id,
                "space_id": space_id,
            }),
        );
        Ok(serde_json::json!({ "tab_id": tab_id, "workspace_id": workspace_id }))
    }

    pub fn rename_tab(&mut self, tab_id: &str, name: String) -> Result<Value, String> {
        let label = {
            let workspace = self.active_workspace_mut()?;
            let tab = workspace
                .tabs
                .iter_mut()
                .find(|tab| tab.tab_id == tab_id)
                .ok_or_else(|| format!("tab '{tab_id}' does not exist"))?;
            tab.name = name;
            tab.name.clone()
        };
        self.record_event(
            "tab_renamed",
            serde_json::json!({ "tab_id": tab_id, "label": label }),
        );
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn rename_tab_anywhere(&mut self, tab_id: &str, name: String) -> Result<Value, String> {
        let (workspace_id, label) = {
            let Some(workspace) = self
                .snapshot
                .spaces
                .iter_mut()
                .flat_map(|space| space.workspaces.iter_mut())
                .find(|workspace| workspace.tabs.iter().any(|tab| tab.tab_id == tab_id))
            else {
                return Err(format!("tab '{tab_id}' does not exist"));
            };
            let workspace_id = workspace.workspace_id.clone();
            let tab = workspace
                .tabs
                .iter_mut()
                .find(|tab| tab.tab_id == tab_id)
                .unwrap();
            tab.name = name;
            (workspace_id, tab.name.clone())
        };
        self.record_event(
            "tab_renamed",
            serde_json::json!({ "tab_id": tab_id, "workspace_id": workspace_id, "label": label }),
        );
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn close_tab(&mut self, tab_id: &str) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let workspace_id = self
            .snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == active_space_id)
            .ok_or_else(|| "active space does not exist".to_string())?
            .active_workspace_id
            .clone()
            .ok_or_else(|| "active space has no workspace".to_string())?;
        let (index, tab_count, pane_ids) = {
            let workspace = self
                .snapshot
                .spaces
                .iter()
                .find(|space| space.space_id == active_space_id)
                .ok_or_else(|| "active space does not exist".to_string())?
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == workspace_id)
                .ok_or_else(|| "active workspace does not exist".to_string())?;
            let index = workspace
                .tabs
                .iter()
                .position(|tab| tab.tab_id == tab_id)
                .ok_or_else(|| format!("tab '{tab_id}' does not exist"))?;
            let pane_ids = workspace.tabs[index]
                .layout
                .as_ref()
                .map(|layout| {
                    layout
                        .pane_ids()
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (index, workspace.tabs.len(), pane_ids)
        };
        self.record_event(
            "tab_closed",
            serde_json::json!({ "tab_id": tab_id, "workspace_id": workspace_id }),
        );
        if tab_count == 1 {
            self.close_workspace(&active_space_id, &workspace_id)?;
            return Ok(serde_json::json!({ "tab_id": tab_id, "closed_workspace": true }));
        }
        for pane_id in &pane_ids {
            let _ = self.pane_manager.remove(pane_id);
        }
        self.snapshot
            .panes
            .retain(|pane| !pane_ids.contains(&pane.pane_id));
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

    pub fn close_tab_anywhere(&mut self, tab_id: &str) -> Result<Value, String> {
        let active_space_id = self.snapshot.active_space_id.clone();
        let active_workspace_id = self
            .snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == active_space_id)
            .and_then(|space| space.active_workspace_id.clone());
        let Some((space_id, workspace_id, tab_count, tab_index, pane_ids)) =
            self.snapshot.spaces.iter().find_map(|space| {
                space.workspaces.iter().find_map(|workspace| {
                    workspace
                        .tabs
                        .iter()
                        .position(|tab| tab.tab_id == tab_id)
                        .map(|tab_index| {
                            (
                                space.space_id.clone(),
                                workspace.workspace_id.clone(),
                                workspace.tabs.len(),
                                tab_index,
                                workspace.tabs[tab_index]
                                    .layout
                                    .as_ref()
                                    .map(|layout| {
                                        layout
                                            .pane_ids()
                                            .into_iter()
                                            .map(str::to_owned)
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default(),
                            )
                        })
                })
            })
        else {
            return Err(format!("tab '{tab_id}' does not exist"));
        };
        if active_space_id == space_id
            && active_workspace_id.as_deref() == Some(workspace_id.as_str())
        {
            return self.close_tab(tab_id);
        }
        self.record_event(
            "tab_closed",
            serde_json::json!({
                "tab_id": tab_id, "workspace_id": workspace_id, "space_id": space_id,
            }),
        );
        if tab_count == 1 {
            self.close_workspace(&space_id, &workspace_id)?;
            return Ok(serde_json::json!({ "tab_id": tab_id, "closed_workspace": true }));
        }
        for pane_id in &pane_ids {
            let _ = self.pane_manager.remove(pane_id);
        }
        self.snapshot
            .panes
            .retain(|pane| !pane_ids.contains(&pane.pane_id));
        let workspace = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == space_id)
            .unwrap()
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .unwrap();
        workspace.tabs.remove(tab_index);
        if workspace.active_tab_id == tab_id {
            workspace.active_tab_id = workspace.tabs[tab_index.min(workspace.tabs.len() - 1)]
                .tab_id
                .clone();
        }
        self.sync_focus_to_active_tab()?;
        Ok(serde_json::json!({ "tab_id": tab_id }))
    }

    pub fn focus_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        let Some((space_index, workspace_index, tab_index)) = self
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find_map(|(space_index, space)| {
                space
                    .workspaces
                    .iter()
                    .enumerate()
                    .find_map(|(workspace_index, workspace)| {
                        workspace
                            .tabs
                            .iter()
                            .enumerate()
                            .find_map(|(tab_index, tab)| {
                                tab.layout
                                    .as_ref()
                                    .filter(|layout| layout.pane_ids().contains(&pane_id))
                                    .map(|_| (space_index, workspace_index, tab_index))
                            })
                    })
            })
        else {
            return Err(format!("pane '{pane_id}' does not exist"));
        };

        let space_id = self.snapshot.spaces[space_index].space_id.clone();
        let workspace_id = self.snapshot.spaces[space_index].workspaces[workspace_index]
            .workspace_id
            .clone();
        let tab_id = self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index]
            .tab_id
            .clone();
        self.snapshot.active_space_id = space_id;
        self.snapshot.spaces[space_index].active_workspace_id = Some(workspace_id);
        self.snapshot.spaces[space_index].workspaces[workspace_index].active_tab_id = tab_id;
        let tab =
            &mut self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index];
        tab.focused_pane_id = Some(pane_id.into());
        self.snapshot.focused_pane_id = Some(pane_id.into());
        self.mark_active_tab_agents_seen(&[pane_id.to_owned()]);
        self.record_event("pane_focused", serde_json::json!({ "pane_id": pane_id }));
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn swap_panes(
        &mut self,
        source_pane_id: &str,
        target_pane_id: &str,
    ) -> Result<Value, String> {
        let locate =
            |session: &Session, pane_id: &str| {
                session
                    .snapshot
                    .spaces
                    .iter()
                    .enumerate()
                    .find_map(|(space_index, space)| {
                        space.workspaces.iter().enumerate().find_map(
                            |(workspace_index, workspace)| {
                                workspace
                                    .tabs
                                    .iter()
                                    .enumerate()
                                    .find_map(|(tab_index, tab)| {
                                        tab.layout
                                            .as_ref()
                                            .filter(|layout| layout.pane_ids().contains(&pane_id))
                                            .map(|_| (space_index, workspace_index, tab_index))
                                    })
                            },
                        )
                    })
            };
        let source_location = locate(self, source_pane_id)
            .ok_or_else(|| format!("pane '{source_pane_id}' does not exist"))?;
        let target_location = locate(self, target_pane_id)
            .ok_or_else(|| format!("pane '{target_pane_id}' does not exist"))?;
        if source_location != target_location {
            return Err("both panes must exist in the same tab".into());
        }
        let (space_index, workspace_index, tab_index) = source_location;
        let space_id = self.snapshot.spaces[space_index].space_id.clone();
        let workspace_id = self.snapshot.spaces[space_index].workspaces[workspace_index]
            .workspace_id
            .clone();
        let tab_id = self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index]
            .tab_id
            .clone();
        self.snapshot.active_space_id = space_id;
        self.snapshot.spaces[space_index].active_workspace_id = Some(workspace_id);
        self.snapshot.spaces[space_index].workspaces[workspace_index].active_tab_id = tab_id;
        let tab =
            &mut self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index];
        let layout = tab
            .layout
            .as_mut()
            .ok_or_else(|| "active tab has no panes".to_string())?;
        if !layout.swap_panes(source_pane_id, target_pane_id) {
            return Err("both panes must exist in the active tab and be different".into());
        }
        tab.focused_pane_id = Some(source_pane_id.to_owned());
        self.snapshot.focused_pane_id = Some(source_pane_id.to_owned());
        self.record_active_layout_event();
        Ok(serde_json::json!({
            "source_pane_id": source_pane_id,
            "target_pane_id": target_pane_id
        }))
    }

    pub fn toggle_pane_zoom(&mut self, pane_id: &str) -> Result<Value, String> {
        let Some((space_index, workspace_index, tab_index)) = self
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find_map(|(space_index, space)| {
                space
                    .workspaces
                    .iter()
                    .enumerate()
                    .find_map(|(workspace_index, workspace)| {
                        workspace
                            .tabs
                            .iter()
                            .enumerate()
                            .find_map(|(tab_index, tab)| {
                                tab.layout
                                    .as_ref()
                                    .filter(|layout| layout.pane_ids().contains(&pane_id))
                                    .map(|_| (space_index, workspace_index, tab_index))
                            })
                    })
            })
        else {
            return Err(format!("pane '{pane_id}' does not exist"));
        };
        let space_id = self.snapshot.spaces[space_index].space_id.clone();
        let workspace_id = self.snapshot.spaces[space_index].workspaces[workspace_index]
            .workspace_id
            .clone();
        let tab_id = self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index]
            .tab_id
            .clone();
        self.snapshot.active_space_id = space_id;
        self.snapshot.spaces[space_index].active_workspace_id = Some(workspace_id);
        self.snapshot.spaces[space_index].workspaces[workspace_index].active_tab_id = tab_id;
        let tab =
            &mut self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index];
        tab.focused_pane_id = Some(pane_id.into());
        tab.zoomed = !tab.zoomed;
        let zoomed = tab.zoomed;
        self.snapshot.focused_pane_id = Some(pane_id.into());
        self.record_active_layout_event();
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

    pub fn set_right_click_passthrough(
        &mut self,
        pane_id: &str,
        enabled: bool,
    ) -> Result<Value, String> {
        let pane = self
            .snapshot
            .panes
            .iter_mut()
            .find(|pane| pane.pane_id == pane_id)
            .ok_or_else(|| format!("pane '{pane_id}' does not exist"))?;
        pane.right_click_passthrough = enabled;
        Ok(serde_json::json!({
            "pane_id": pane_id,
            "right_click_passthrough": enabled
        }))
    }

    pub fn rename_pane(&mut self, pane_id: &str, label: String) -> Result<Value, String> {
        let updated_label = {
            let pane = self
                .snapshot
                .panes
                .iter_mut()
                .find(|pane| pane.pane_id == pane_id)
                .ok_or_else(|| format!("pane '{pane_id}' does not exist"))?;
            pane.label = (!label.trim().is_empty()).then_some(label);
            pane.label.clone()
        };
        self.record_event(
            "pane_updated",
            serde_json::json!({ "pane_id": pane_id, "label": updated_label }),
        );
        Ok(serde_json::json!({ "pane_id": pane_id, "label": updated_label }))
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

    pub fn focus_direction(
        &mut self,
        direction: &str,
        source_pane_id: Option<&str>,
    ) -> Result<Value, String> {
        let movement = match direction {
            "left" => crate::model::layout::FocusDirection::Left,
            "right" => crate::model::layout::FocusDirection::Right,
            "up" => crate::model::layout::FocusDirection::Up,
            "down" => crate::model::layout::FocusDirection::Down,
            other => return Err(format!("unknown focus direction '{other}'")),
        };
        if let Some(source_pane_id) = source_pane_id {
            self.focus_pane(source_pane_id)?;
        }
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
        let Some((space_index, workspace_index, tab_index)) = self
            .snapshot
            .spaces
            .iter()
            .enumerate()
            .find_map(|(space_index, space)| {
                space
                    .workspaces
                    .iter()
                    .enumerate()
                    .find_map(|(workspace_index, workspace)| {
                        workspace
                            .tabs
                            .iter()
                            .enumerate()
                            .find_map(|(tab_index, tab)| {
                                tab.layout
                                    .as_ref()
                                    .filter(|layout| layout.pane_ids().contains(&pane_id))
                                    .map(|_| (space_index, workspace_index, tab_index))
                            })
                    })
            })
        else {
            return Err(format!("pane '{pane_id}' does not exist"));
        };
        let space_id = self.snapshot.spaces[space_index].space_id.clone();
        let workspace_id = self.snapshot.spaces[space_index].workspaces[workspace_index]
            .workspace_id
            .clone();
        let tab_id = self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index]
            .tab_id
            .clone();
        self.snapshot.active_space_id = space_id;
        self.snapshot.spaces[space_index].active_workspace_id = Some(workspace_id);
        self.snapshot.spaces[space_index].workspaces[workspace_index].active_tab_id = tab_id;
        let tab =
            &mut self.snapshot.spaces[space_index].workspaces[workspace_index].tabs[tab_index];
        let layout = tab
            .layout
            .as_mut()
            .ok_or_else(|| "active tab has no panes".to_string())?;
        if !layout.resize_pane(pane_id, delta) {
            return Err(format!("pane '{pane_id}' could not be resized"));
        }
        self.record_active_layout_event();
        Ok(serde_json::json!({ "pane_id": pane_id, "delta": delta }))
    }

    pub fn resize_pane_direction(
        &mut self,
        amount: f32,
        direction: crate::model::layout::FocusDirection,
    ) -> Result<Value, String> {
        if !amount.is_finite() {
            return Err("resize amount must be finite".into());
        }
        let amount = amount.abs().min(0.5);
        let tab = self.active_tab_mut()?;
        let pane_id = tab
            .focused_pane_id
            .clone()
            .ok_or_else(|| "no pane is focused".to_string())?;
        let layout = tab
            .layout
            .as_mut()
            .ok_or_else(|| "active tab has no layout".to_string())?;
        if !layout.resize_pane_direction(&pane_id, direction, amount) {
            return Err("no split in that direction".into());
        }
        self.record_active_layout_event();
        let direction = match direction {
            crate::model::layout::FocusDirection::Left => "left",
            crate::model::layout::FocusDirection::Right => "right",
            crate::model::layout::FocusDirection::Up => "up",
            crate::model::layout::FocusDirection::Down => "down",
        };
        Ok(serde_json::json!({
            "pane_id": pane_id,
            "amount": amount,
            "direction": direction,
        }))
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
        self.record_active_layout_event();
        Ok(serde_json::json!({
            "path": path,
            "ratio": crate::model::layout::clamp_ratio(ratio),
        }))
    }

    pub fn close_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        if self.snapshot.popup_pane_id.as_deref() == Some(pane_id) {
            return self.close_popup_pane(pane_id);
        }
        if self.snapshot.overlay_pane_id.as_deref() == Some(pane_id) {
            return self.close_overlay_pane(pane_id);
        }
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
            self.record_event("pane_closed", serde_json::json!({ "pane_id": pane_id }));
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
            self.record_event("pane_closed", serde_json::json!({ "pane_id": pane_id }));
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
        self.record_event("pane_closed", serde_json::json!({ "pane_id": pane_id }));
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn close_pane_anywhere(&mut self, pane_id: &str) -> Result<Value, String> {
        if self.snapshot.popup_pane_id.as_deref() == Some(pane_id)
            || self.snapshot.overlay_pane_id.as_deref() == Some(pane_id)
        {
            return self.close_pane(pane_id);
        }
        let active_space_id = self.snapshot.active_space_id.clone();
        let active_workspace_id = self
            .snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == active_space_id)
            .and_then(|space| space.active_workspace_id.clone());
        let Some((space_id, workspace_id, tab_id, tab_index, pane_count, tab_count)) =
            self.snapshot.spaces.iter().find_map(|space| {
                space.workspaces.iter().find_map(|workspace| {
                    workspace
                        .tabs
                        .iter()
                        .enumerate()
                        .find_map(|(tab_index, tab)| {
                            tab.layout
                                .as_ref()
                                .filter(|layout| layout.pane_ids().contains(&pane_id))
                                .map(|layout| {
                                    (
                                        space.space_id.clone(),
                                        workspace.workspace_id.clone(),
                                        tab.tab_id.clone(),
                                        tab_index,
                                        layout.pane_ids().len(),
                                        workspace.tabs.len(),
                                    )
                                })
                        })
                })
            })
        else {
            return Err(format!("pane '{pane_id}' does not exist"));
        };
        if active_space_id == space_id
            && active_workspace_id.as_deref() == Some(workspace_id.as_str())
        {
            let active_tab_id = self
                .snapshot
                .spaces
                .iter()
                .find(|space| space.space_id == space_id)
                .and_then(|space| {
                    space
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.workspace_id == workspace_id)
                })
                .map(|workspace| workspace.active_tab_id.clone());
            if active_tab_id.as_deref() == Some(tab_id.as_str()) {
                return self.close_pane(pane_id);
            }
        }
        if pane_count == 1 && tab_count == 1 {
            self.close_workspace(&space_id, &workspace_id)?;
            self.record_event("pane_closed", serde_json::json!({ "pane_id": pane_id }));
            return Ok(serde_json::json!({ "pane_id": pane_id, "closed_workspace": true }));
        }
        self.pane_manager
            .remove(pane_id)
            .map_err(|error| format!("{error:?}"))?;
        self.snapshot.panes.retain(|pane| pane.pane_id != pane_id);
        let workspace = self
            .snapshot
            .spaces
            .iter_mut()
            .find(|space| space.space_id == space_id)
            .unwrap()
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .unwrap();
        if pane_count == 1 {
            workspace.tabs.remove(tab_index);
            if workspace.active_tab_id == tab_id {
                workspace.active_tab_id = workspace.tabs[tab_index.min(workspace.tabs.len() - 1)]
                    .tab_id
                    .clone();
            }
        } else {
            let tab = &mut workspace.tabs[tab_index];
            tab.layout = tab
                .layout
                .take()
                .and_then(|layout| layout.close_pane(pane_id));
            if tab.focused_pane_id.as_deref() == Some(pane_id) {
                tab.focused_pane_id = tab
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.pane_ids().first().map(|id| (*id).to_owned()));
            }
        }
        self.sync_focus_to_active_tab()?;
        self.record_event("pane_closed", serde_json::json!({ "pane_id": pane_id }));
        Ok(serde_json::json!({ "pane_id": pane_id }))
    }

    pub fn close_popup_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        if self.snapshot.popup_pane_id.as_deref() != Some(pane_id) {
            return Err(format!("popup pane '{pane_id}' is not open"));
        }
        self.pane_manager
            .remove(pane_id)
            .map_err(|error| format!("{error:?}"))?;
        self.snapshot.panes.retain(|pane| pane.pane_id != pane_id);
        self.snapshot.popup_pane_id = None;
        self.snapshot.popup_width = 0;
        self.snapshot.popup_height = 0;
        self.snapshot.popup_width_spec = None;
        self.snapshot.popup_height_spec = None;
        self.sync_focus_to_active_tab()?;
        self.record_event("pane_closed", serde_json::json!({ "pane_id": pane_id }));
        Ok(serde_json::json!({ "pane_id": pane_id, "closed_popup": true }))
    }

    pub fn close_overlay_pane(&mut self, pane_id: &str) -> Result<Value, String> {
        if self.snapshot.overlay_pane_id.as_deref() != Some(pane_id) {
            return Err(format!("overlay pane '{pane_id}' is not open"));
        }
        self.pane_manager
            .remove(pane_id)
            .map_err(|error| format!("{error:?}"))?;
        for space in &mut self.snapshot.spaces {
            for workspace in &mut space.workspaces {
                for tab in &mut workspace.tabs {
                    tab.layout = tab
                        .layout
                        .take()
                        .and_then(|layout| layout.close_pane(pane_id));
                    if tab
                        .focused_pane_id
                        .as_deref()
                        .is_some_and(|focused| focused == pane_id)
                    {
                        tab.focused_pane_id = tab.layout.as_ref().and_then(|layout| {
                            layout.pane_ids().first().map(|id| (*id).to_owned())
                        });
                    }
                }
            }
        }
        let previous_focus = self.snapshot.overlay_previous_focus.take();
        let previous_zoomed = self.snapshot.overlay_previous_zoomed;
        self.snapshot.panes.retain(|pane| pane.pane_id != pane_id);
        self.snapshot.overlay_pane_id = None;
        self.snapshot.overlay_previous_zoomed = false;
        if let Some(previous_focus) = previous_focus {
            for space in &mut self.snapshot.spaces {
                for workspace in &mut space.workspaces {
                    for tab in &mut workspace.tabs {
                        if tab.layout.as_ref().is_some_and(|layout| {
                            layout.pane_ids().contains(&previous_focus.as_str())
                        }) {
                            tab.focused_pane_id = Some(previous_focus.clone());
                            tab.zoomed = previous_zoomed;
                        }
                    }
                }
            }
        }
        self.sync_focus_to_active_tab()?;
        self.record_event("pane_closed", serde_json::json!({ "pane_id": pane_id }));
        Ok(serde_json::json!({ "pane_id": pane_id, "closed_overlay": true }))
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
        pane.display_agent = None;
        pane.display_title = None;
        pane.state_labels.clear();
        pane.agent_session = None;
        pane.scrollback.clear();
        pane.scrollback_bytes = 0;
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
        for tokens in self.workspace_metadata_tokens.values_mut() {
            tokens.expire_at(Instant::now());
        }
    }

    pub fn report_agent(
        &mut self,
        pane_id: &str,
        report: AgentReportRequest,
    ) -> Result<Value, String> {
        let changed = self
            .pane_manager
            .report_agent(
                pane_id,
                crate::pane::AgentReport {
                    agent: report.agent,
                    state: report.state,
                    source: report.source,
                    seq: report.seq,
                    session_id: report.session_id,
                    session_path: report.session_path,
                },
            )
            .map_err(|error| format!("{error:?}"))?;
        if changed {
            self.record_pane_events(vec![PaneEvent::AgentStatusChanged {
                pane_id: pane_id.into(),
                agent_state: report.state,
            }]);
        }
        self.refresh_snapshot();
        Ok(serde_json::json!({ "pane_id": pane_id, "updated": changed }))
    }

    pub fn report_agent_session(
        &mut self,
        pane_id: &str,
        report: AgentSessionReportRequest,
    ) -> Result<Value, String> {
        let changed = self
            .pane_manager
            .report_agent_session(
                pane_id,
                crate::pane::AgentSessionReport {
                    agent: report.agent,
                    source: report.source,
                    seq: report.seq,
                    session_id: report.session_id,
                    session_path: report.session_path,
                },
            )
            .map_err(|error| format!("{error:?}"))?;
        self.refresh_snapshot();
        Ok(serde_json::json!({
            "pane_id": pane_id,
            "updated": changed
        }))
    }

    pub fn clear_agent_authority(
        &mut self,
        pane_id: &str,
        report: ClearAgentAuthorityRequest,
    ) -> Result<Value, String> {
        let cleared = self
            .pane_manager
            .clear_agent_authority(pane_id, report.source.as_deref(), report.seq)
            .map_err(|error| format!("{error:?}"))?;
        self.refresh_snapshot();
        Ok(serde_json::json!({ "pane_id": pane_id, "cleared": cleared }))
    }

    pub fn report_display_agent(
        &mut self,
        pane_id: &str,
        report: DisplayAgentReportRequest,
    ) -> Result<Value, String> {
        let changed = self
            .pane_manager
            .report_display_agent(
                pane_id,
                report.source,
                report.display_agent,
                report.clear,
                report.seq,
            )
            .map_err(|error| format!("{error:?}"))?;
        self.refresh_snapshot();
        Ok(serde_json::json!({ "pane_id": pane_id, "updated": changed }))
    }

    pub fn report_display_title(
        &mut self,
        pane_id: &str,
        report: DisplayTitleReportRequest,
    ) -> Result<Value, String> {
        let changed = self
            .pane_manager
            .report_display_title(
                pane_id,
                report.source,
                report.display_title,
                report.clear,
                report.seq,
            )
            .map_err(|error| format!("{error:?}"))?;
        self.refresh_snapshot();
        Ok(serde_json::json!({ "pane_id": pane_id, "updated": changed }))
    }

    pub fn report_display_state_labels(
        &mut self,
        pane_id: &str,
        report: DisplayStateLabelsReportRequest,
    ) -> Result<Value, String> {
        let changed = self
            .pane_manager
            .report_display_state_labels(
                pane_id,
                report.source,
                report.state_labels,
                report.clear,
                report.seq,
            )
            .map_err(|error| format!("{error:?}"))?;
        self.refresh_snapshot();
        Ok(serde_json::json!({ "pane_id": pane_id, "updated": changed }))
    }

    pub fn report_metadata_tokens(
        &mut self,
        pane_id: &str,
        report: MetadataTokensReportRequest,
    ) -> Result<Value, String> {
        let changed = self
            .pane_manager
            .report_metadata_tokens(
                pane_id,
                report.source,
                report.tokens,
                report.ttl,
                report.seq,
            )
            .map_err(|error| format!("{error:?}"))?;
        self.refresh_snapshot();
        Ok(serde_json::json!({ "pane_id": pane_id, "updated": changed }))
    }

    pub fn report_workspace_metadata(
        &mut self,
        workspace_id: &str,
        source: String,
        tokens: std::collections::HashMap<String, Option<String>>,
        ttl: Option<Duration>,
        seq: Option<u64>,
    ) -> Result<Value, String> {
        if !self
            .snapshot
            .spaces
            .iter()
            .flat_map(|space| space.workspaces.iter())
            .any(|workspace| workspace.workspace_id == workspace_id)
        {
            return Err(format!("workspace '{workspace_id}' does not exist"));
        }
        let sequence_key = (workspace_id.to_owned(), source);
        if seq.is_some_and(|next| {
            self.workspace_metadata_token_sequences
                .get(&sequence_key)
                .is_some_and(|last| next <= *last)
        }) {
            return Ok(serde_json::json!({ "workspace_id": workspace_id, "updated": false }));
        }
        let store = self
            .workspace_metadata_tokens
            .entry(workspace_id.to_owned())
            .or_default();
        if store.key_count_after_patch(&tokens) > crate::metadata_tokens::MAX_KEYS {
            return Err(format!(
                "workspace metadata may contain at most {} tokens",
                crate::metadata_tokens::MAX_KEYS
            ));
        }
        if let Some(seq) = seq {
            self.workspace_metadata_token_sequences
                .insert(sequence_key, seq);
        }
        let changed = store.patch(tokens, ttl, Instant::now());
        self.refresh_snapshot();
        Ok(serde_json::json!({ "workspace_id": workspace_id, "updated": changed }))
    }

    pub fn report_metadata(
        &mut self,
        pane_id: &str,
        agent: DisplayAgentReportRequest,
        title: DisplayTitleReportRequest,
        state_labels: DisplayStateLabelsReportRequest,
        tokens: MetadataTokensReportRequest,
    ) -> Result<Value, String> {
        let agent_result = self.report_display_agent(pane_id, agent)?;
        let title_result = self.report_display_title(pane_id, title)?;
        let labels_result = self.report_display_state_labels(pane_id, state_labels)?;
        let tokens_result = self.report_metadata_tokens(pane_id, tokens)?;
        Ok(serde_json::json!({
            "pane_id": pane_id,
            "updated": agent_result["updated"] == true
                || title_result["updated"] == true
                || labels_result["updated"] == true
                || tokens_result["updated"] == true
        }))
    }

    pub fn release_agent(
        &mut self,
        pane_id: &str,
        source: &str,
        agent: crate::detect::AgentKind,
        seq: Option<u64>,
    ) -> Result<Value, String> {
        let final_status = self
            .pane_manager
            .get(pane_id)
            .and_then(|pane| pane.agent_state);
        let released = self
            .pane_manager
            .release_agent(pane_id, source, agent, seq)
            .map_err(|error| format!("{error:?}"))?;
        if released {
            self.record_pane_events(vec![PaneEvent::AgentDetected {
                pane_id: pane_id.into(),
                agent: None,
                released: true,
                final_status,
            }]);
        }
        self.refresh_snapshot();
        Ok(serde_json::json!({ "pane_id": pane_id, "released": released }))
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
                PaneEvent::AgentDetected {
                    pane_id,
                    agent,
                    released,
                    final_status,
                } => (
                    "pane_agent_detected",
                    serde_json::json!({
                        "pane_id": pane_id,
                        "agent": agent,
                        "released": released,
                        "final_status": final_status,
                    }),
                ),
                PaneEvent::AgentStatusChanged {
                    pane_id,
                    agent_state,
                } => (
                    "pane_agent_status_changed",
                    serde_json::json!({ "pane_id": pane_id, "agent_status": agent_state }),
                ),
            };
            self.record_event(name, payload);
        }
    }

    fn record_event(&mut self, name: &str, payload: Value) {
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

    fn record_active_layout_event(&mut self) {
        let Some((workspace_id, tab_id, zoomed, pane_ids)) = self
            .snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == self.snapshot.active_space_id)
            .and_then(|space| {
                space
                    .active_workspace_id
                    .as_deref()
                    .and_then(|workspace_id| {
                        space
                            .workspaces
                            .iter()
                            .find(|workspace| workspace.workspace_id == workspace_id)
                    })
            })
            .and_then(|workspace| {
                workspace
                    .tabs
                    .iter()
                    .find(|tab| tab.tab_id == workspace.active_tab_id)
                    .map(|tab| {
                        (
                            workspace.workspace_id.clone(),
                            tab.tab_id.clone(),
                            tab.zoomed,
                            tab.layout
                                .as_ref()
                                .map(|layout| {
                                    layout
                                        .pane_ids()
                                        .into_iter()
                                        .map(str::to_owned)
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                        )
                    })
            })
        else {
            return;
        };
        self.record_event(
            "layout_updated",
            serde_json::json!({
                "workspace_id": workspace_id,
                "tab_id": tab_id,
                "zoomed": zoomed,
                "pane_ids": pane_ids,
            }),
        );
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
        self.sync_focus_to_active_tab()?;
        self.record_event(
            "workspace_closed",
            serde_json::json!({ "workspace_id": workspace_id, "space_id": space_id }),
        );
        Ok(())
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

    fn active_pane_context_ids(&self) -> Option<(String, String)> {
        let space = self
            .snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == self.snapshot.active_space_id)?;
        let workspace_id = space.active_workspace_id.as_ref()?;
        let workspace = space
            .workspaces
            .iter()
            .find(|workspace| &workspace.workspace_id == workspace_id)?;
        Some((
            workspace.workspace_id.clone(),
            workspace.active_tab_id.clone(),
        ))
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
        let pane_ids = tab
            .layout
            .as_ref()
            .map(LayoutNode::pane_ids)
            .unwrap_or_default()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.mark_active_tab_agents_seen(&pane_ids);
        Ok(())
    }

    fn mark_active_tab_agents_seen(&mut self, pane_ids: &[String]) {
        self.pane_manager.mark_agent_done_seen(pane_ids);
        for pane in &mut self.snapshot.panes {
            if pane_ids.iter().any(|pane_id| pane_id == &pane.pane_id) && pane.agent.is_some() {
                pane.agent_done = false;
            }
        }
    }

    fn workspace_mut(&mut self, workspace_id: &str) -> Result<&mut WorkspaceView, String> {
        self.snapshot
            .spaces
            .iter_mut()
            .flat_map(|space| space.workspaces.iter_mut())
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

fn active_layout_focus(snapshot: &SessionSnapshot) -> Option<String> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace_id = space.active_workspace_id.as_deref()?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)?;
    let tab = workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)?;
    tab.focused_pane_id.clone().or_else(|| {
        tab.layout.as_ref().and_then(|layout| {
            layout
                .pane_ids()
                .first()
                .map(|pane_id| (*pane_id).to_owned())
        })
    })
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

fn normalize_workspace_ids(snapshot: &mut SessionSnapshot) {
    let mut used_ids: std::collections::BTreeSet<String> = snapshot
        .spaces
        .iter()
        .flat_map(|space| space.workspaces.iter())
        .map(|workspace| workspace.workspace_id.clone())
        .collect();
    let mut seen_ids = std::collections::BTreeSet::new();

    for space in &mut snapshot.spaces {
        for workspace in &mut space.workspaces {
            let old_id = workspace.workspace_id.clone();
            if seen_ids.insert(old_id.clone()) {
                continue;
            }

            let new_id = next_numbered_id("workspace", used_ids.iter().cloned());
            used_ids.insert(new_id.clone());
            workspace.workspace_id = new_id.clone();
            if space.active_workspace_id.as_deref() == Some(old_id.as_str()) {
                space.active_workspace_id = Some(new_id.clone());
            }

            let old_tab_prefix = format!("tab-{old_id}-");
            let new_tab_prefix = format!("tab-{new_id}-");
            for tab in &mut workspace.tabs {
                if let Some(suffix) = tab.tab_id.strip_prefix(&old_tab_prefix).map(str::to_owned) {
                    let old_tab_id =
                        std::mem::replace(&mut tab.tab_id, format!("{new_tab_prefix}{suffix}"));
                    if workspace.active_tab_id == old_tab_id {
                        workspace.active_tab_id = tab.tab_id.clone();
                    }
                }
            }
        }
    }
}

impl Session {
    pub fn refresh_snapshot(&mut self) {
        self.refresh_git_branches();
        for space in &mut self.snapshot.spaces {
            for workspace in &mut space.workspaces {
                workspace.tokens = self
                    .workspace_metadata_tokens
                    .get(&workspace.workspace_id)
                    .map(crate::metadata_tokens::MetadataTokens::values)
                    .unwrap_or_default();
            }
        }
        for pane in &mut self.snapshot.panes {
            if let Some(current) = self.pane_manager.get(&pane.pane_id) {
                pane.status = current.status.clone();
                pane.agent = current.agent;
                pane.agent_state = current.agent_state;
                pane.agent_done = current.agent_done;
                pane.display_agent = current.display_agent.clone();
                pane.display_title = current.display_title.clone();
                pane.state_labels = current.display_state_labels.clone();
                pane.tokens = current.metadata_tokens.values();
                pane.agent_session = current.agent_session.clone();
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
                pane.hyperlinks = terminal.hyperlinks;
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
        let popup_exited = self
            .snapshot
            .popup_pane_id
            .as_deref()
            .is_some_and(|pane_id| {
                self.snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == pane_id)
                    .is_some_and(|pane| !matches!(pane.status, PaneStatus::Running))
            });
        if popup_exited {
            if let Some(pane_id) = self.snapshot.popup_pane_id.clone() {
                let _ = self.close_popup_pane(&pane_id);
            }
        }
        let overlay_exited = self
            .snapshot
            .overlay_pane_id
            .as_deref()
            .is_some_and(|pane_id| {
                self.snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == pane_id)
                    .is_some_and(|pane| !matches!(pane.status, PaneStatus::Running))
            });
        if overlay_exited {
            if let Some(pane_id) = self.snapshot.overlay_pane_id.clone() {
                let _ = self.close_overlay_pane(&pane_id);
            }
        }
    }

    fn refresh_git_branches(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_git_branch_refresh) < GIT_BRANCH_REFRESH_INTERVAL {
            return;
        }
        self.last_git_branch_refresh = now;

        let mut changes = Vec::new();
        for space in &mut self.snapshot.spaces {
            for workspace in &mut space.workspaces {
                let Some(path) = workspace.repository_path.as_deref() else {
                    continue;
                };
                let branch = crate::server::git::branch(Path::new(path));
                if workspace.branch != branch {
                    workspace.branch = branch.clone();
                    changes.push((workspace.workspace_id.clone(), branch));
                }
            }
        }
        for (workspace_id, branch) in changes {
            self.record_event(
                "workspace_updated",
                serde_json::json!({ "workspace_id": workspace_id, "branch": branch }),
            );
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
    use super::{active_layout_focus, CreatePaneRequest, PaneView, Session};
    use crate::model::layout::LayoutNode;
    use crate::model::status::PaneStatus;
    use crate::pane::PaneEvent;
    use std::time::{Duration, Instant};

    #[test]
    fn active_layout_focus_ignores_transient_popup_focus() {
        let mut session = Session::default();
        let tab = &mut session.snapshot.spaces[0].workspaces[0].tabs[0];
        tab.layout = Some(LayoutNode::pane("pane-1"));
        tab.focused_pane_id = Some("pane-1".into());
        session.snapshot.focused_pane_id = Some("popup-1".into());
        session.snapshot.popup_pane_id = Some("popup-1".into());

        assert_eq!(
            active_layout_focus(&session.snapshot).as_deref(),
            Some("pane-1")
        );
    }

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
        let sequence = session.snapshot.event_sequence;
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
        assert!(session.events_since(sequence).iter().any(|event| {
            event.event == "tab_renamed"
                && event.payload["tab_id"] == tab_id
                && event.payload["label"] == "Build logs"
        }));
    }

    #[test]
    fn workspace_metadata_tokens_are_scoped_and_sequence_checked() {
        let mut session = Session::default();
        let workspace_id = "workspace-1";
        let tokens = std::collections::HashMap::from([("summary".into(), Some("indexing".into()))]);
        session
            .report_workspace_metadata(workspace_id, "agent:codex".into(), tokens, None, Some(2))
            .unwrap();
        assert_eq!(
            session.snapshot().spaces[0].workspaces[0].tokens["summary"],
            "indexing"
        );
        session
            .report_workspace_metadata(
                workspace_id,
                "agent:codex".into(),
                std::collections::HashMap::from([("summary".into(), Some("stale".into()))]),
                None,
                Some(1),
            )
            .unwrap();
        assert_eq!(
            session.snapshot().spaces[0].workspaces[0].tokens["summary"],
            "indexing"
        );
        session
            .report_workspace_metadata(
                workspace_id,
                "agent:codex".into(),
                std::collections::HashMap::from([("summary".into(), None)]),
                None,
                Some(3),
            )
            .unwrap();
        assert!(session.snapshot().spaces[0].workspaces[0].tokens.is_empty());
    }

    #[test]
    fn tab_operations_resolve_ids_across_workspaces() {
        let mut session = Session::default();
        session.create_workspace("Other".into()).unwrap();
        let other_tab = session.snapshot.spaces[0].workspaces[1].tabs[0]
            .tab_id
            .clone();
        session.switch_workspace("workspace-1").unwrap();

        session.switch_tab_anywhere(&other_tab).unwrap();
        assert_eq!(
            session.snapshot.spaces[0].active_workspace_id.as_deref(),
            Some("workspace-2")
        );
        assert_eq!(
            session.snapshot.spaces[0].workspaces[1].active_tab_id,
            other_tab
        );
        session
            .rename_tab_anywhere(&other_tab, "Review".into())
            .unwrap();
        assert_eq!(
            session.snapshot.spaces[0].workspaces[1].tabs[0].name,
            "Review"
        );
        assert!(session.events_since(0).iter().any(|event| {
            event.event == "tab_renamed"
                && event.payload["tab_id"] == other_tab
                && event.payload["workspace_id"] == "workspace-2"
                && event.payload["label"] == "Review"
        }));
        session.switch_workspace("workspace-1").unwrap();
        session.close_tab_anywhere(&other_tab).unwrap();
        assert!(session.snapshot.spaces[0]
            .workspaces
            .iter()
            .all(|workspace| workspace.workspace_id != "workspace-2"));
    }

    #[test]
    fn workspace_deletion_resolves_ids_across_spaces() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let workspace_id = session.snapshot.spaces[1].workspaces[0]
            .workspace_id
            .clone();
        session.switch_space("space-1").unwrap();

        session.delete_workspace_anywhere(&workspace_id).unwrap();
        assert!(session.snapshot.spaces[1].workspaces.is_empty());
        assert_eq!(session.snapshot.active_space_id, "space-1");
    }

    #[test]
    fn pane_close_resolves_a_pane_in_an_inactive_workspace() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let workspace_id = session.snapshot.spaces[1].workspaces[0]
            .workspace_id
            .clone();
        session.snapshot.spaces[1].workspaces[0].tabs[0].layout = Some(LayoutNode::pane("pane-1"));
        session.snapshot.spaces[1].workspaces[0].tabs[0].focused_pane_id = Some("pane-1".into());
        session.snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
        );
        session.switch_space("space-1").unwrap();

        session.close_pane_anywhere("pane-1").unwrap();
        assert!(session.snapshot.panes.is_empty());
        assert!(session.snapshot.spaces[1]
            .workspaces
            .iter()
            .all(|workspace| workspace.workspace_id != workspace_id));
        assert_eq!(session.snapshot.active_space_id, "space-1");
    }

    #[test]
    fn agent_states_stay_with_their_panes_when_switching_tabs_and_workspaces() {
        let mut session = Session::default();
        let pane = |pane_id: &str, agent: &str, state: &str| {
            serde_json::from_value::<PaneView>(serde_json::json!({
                "pane_id": pane_id,
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "scrollback_bytes": 0,
                "agent": agent,
                "agent_state": state
            }))
            .unwrap()
        };
        session.snapshot.panes = vec![
            pane("pane-1", "claude", "blocked"),
            pane("pane-2", "codex", "working"),
            pane("pane-3", "gemini", "idle"),
        ];

        let first_tab = &mut session.snapshot.spaces[0].workspaces[0].tabs[0];
        first_tab.layout = Some(LayoutNode::pane("pane-1"));
        first_tab.focused_pane_id = Some("pane-1".into());
        session.snapshot.focused_pane_id = Some("pane-1".into());

        let second_tab_id = session.create_tab("Second tab".into()).unwrap()["tab_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let second_tab = &mut session.snapshot.spaces[0].workspaces[0].tabs[1];
        second_tab.layout = Some(LayoutNode::pane("pane-2"));
        second_tab.focused_pane_id = Some("pane-2".into());

        let second_workspace_id = session.create_workspace("Build".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let workspace_tab = &mut session.snapshot.spaces[0].workspaces[1].tabs[0];
        workspace_tab.layout = Some(LayoutNode::pane("pane-3"));
        workspace_tab.focused_pane_id = Some("pane-3".into());

        session.switch_workspace("workspace-1").unwrap();
        session.switch_tab("tab-1").unwrap();
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-1"));
        session.switch_tab(&second_tab_id).unwrap();
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-2"));
        session.switch_workspace(&second_workspace_id).unwrap();
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-3"));

        session.refresh_snapshot();
        for (pane_id, agent, state) in [
            (
                "pane-1",
                crate::detect::AgentKind::Claude,
                crate::detect::AgentState::Blocked,
            ),
            (
                "pane-2",
                crate::detect::AgentKind::Codex,
                crate::detect::AgentState::Working,
            ),
            (
                "pane-3",
                crate::detect::AgentKind::Gemini,
                crate::detect::AgentState::Idle,
            ),
        ] {
            let pane = session
                .snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == pane_id)
                .unwrap();
            assert_eq!(pane.agent, Some(agent));
            assert_eq!(pane.agent_state, Some(state));
        }
    }

    #[test]
    fn focusing_a_tab_marks_completed_agent_panes_seen() {
        let mut session = Session::default();
        session.snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "agent": "codex",
                "agent_state": "idle",
                "agent_done": true,
                "scrollback_bytes": 0
            }))
            .unwrap(),
        );
        let tab = &mut session.snapshot.spaces[0].workspaces[0].tabs[0];
        tab.layout = Some(LayoutNode::pane("pane-1"));
        tab.focused_pane_id = Some("pane-1".into());
        session.snapshot.focused_pane_id = Some("pane-1".into());

        session.sync_focus_to_active_tab().unwrap();

        assert!(!session.snapshot.panes[0].agent_done);
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
    fn popup_pane_does_not_change_layout_or_background_focus() {
        let mut session = Session::default();
        let request = |popup| {
            serde_json::from_value::<CreatePaneRequest>(serde_json::json!({
                "command": "cmd.exe",
                "args": ["/c", "ping", "127.0.0.1", "-n", "20"],
                "cwd": "C:/",
                "cols": if popup { 40 } else { 80 },
                "rows": if popup { 12 } else { 24 },
                "popup": popup
            }))
            .unwrap()
        };

        let background = session.create_pane(request(false)).unwrap();
        let background_id = background["pane_id"].as_str().unwrap().to_owned();
        let background_layout = session.snapshot.spaces[0].workspaces[0].tabs[0]
            .layout
            .clone();
        let popup = session.create_pane(request(true)).unwrap();
        let popup_id = popup["pane_id"].as_str().unwrap().to_owned();

        assert_eq!(
            session.snapshot.popup_pane_id.as_deref(),
            Some(popup_id.as_str())
        );
        assert_eq!(
            session.snapshot.focused_pane_id.as_deref(),
            Some(popup_id.as_str())
        );
        assert_eq!(
            session.snapshot.spaces[0].workspaces[0].tabs[0].layout,
            background_layout
        );
        assert_eq!(
            session.close_pane(&popup_id).unwrap()["closed_popup"],
            serde_json::json!(true)
        );
        assert_eq!(
            session.snapshot.focused_pane_id.as_deref(),
            Some(background_id.as_str())
        );
        let _ = session.close_pane(&background_id);
    }

    #[test]
    fn overlay_pane_zoom_and_close_restore_background_state() {
        let mut session = Session::default();
        let request = |overlay| {
            serde_json::from_value::<CreatePaneRequest>(serde_json::json!({
                "command": "cmd.exe",
                "args": ["/c", "ping", "127.0.0.1", "-n", "20"],
                "cwd": "C:/",
                "cols": 80,
                "rows": 24,
                "overlay": overlay
            }))
            .unwrap()
        };
        let background = session.create_pane(request(false)).unwrap();
        let background_id = background["pane_id"].as_str().unwrap().to_owned();
        let overlay = session.create_pane(request(true)).unwrap();
        let overlay_id = overlay["pane_id"].as_str().unwrap().to_owned();
        let tab = &session.snapshot.spaces[0].workspaces[0].tabs[0];
        assert!(tab.zoomed);
        assert_eq!(tab.focused_pane_id.as_deref(), Some(overlay_id.as_str()));
        assert_eq!(
            session.snapshot.overlay_pane_id.as_deref(),
            Some(overlay_id.as_str())
        );

        assert_eq!(
            session.close_pane(&overlay_id).unwrap()["closed_overlay"],
            true
        );
        let tab = &session.snapshot.spaces[0].workspaces[0].tabs[0];
        assert!(!tab.zoomed);
        assert_eq!(tab.focused_pane_id.as_deref(), Some(background_id.as_str()));
        assert_eq!(
            session.snapshot.focused_pane_id.as_deref(),
            Some(background_id.as_str())
        );
        let _ = session.close_pane(&background_id);
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
    fn git_branch_refresh_updates_workspace_and_records_event() {
        let mut session = Session::default();
        let workspace = &mut session.snapshot.spaces[0].workspaces[0];
        workspace.repository_path = Some("C:/spindle/path-that-does-not-exist".into());
        workspace.branch = Some("stale".into());
        session.last_git_branch_refresh = Instant::now() - super::GIT_BRANCH_REFRESH_INTERVAL;

        session.refresh_snapshot();

        let workspace = &session.snapshot.spaces[0].workspaces[0];
        assert_eq!(workspace.branch, None);
        assert_eq!(
            session.events.back().map(|event| event.event.as_str()),
            Some("workspace_updated")
        );
        assert_eq!(
            session.events.back().unwrap().payload["workspace_id"],
            "workspace-1"
        );
    }

    #[test]
    fn workspace_group_close_requires_explicit_intent_and_closes_all_members() {
        let mut session = Session::default();
        let linked = session.create_workspace("Linked checkout".into()).unwrap();
        let linked_id = linked["workspace_id"].as_str().unwrap().to_owned();
        for workspace in &mut session.snapshot.spaces[0].workspaces {
            workspace.worktree_group = Some("repo-group".into());
        }
        session.snapshot.spaces[0].workspaces[1].is_linked_worktree = true;

        let error = session
            .delete_workspace_anywhere("workspace-1")
            .expect_err("closing a worktree parent must require --group");
        assert!(error.contains("use --group"));
        assert_eq!(session.snapshot.spaces[0].workspaces.len(), 2);

        let result = session
            .delete_workspace_anywhere_with_group("workspace-1", true)
            .unwrap();
        assert_eq!(result["closed_workspace_ids"].as_array().unwrap().len(), 2);
        assert!(session.snapshot.spaces[0].workspaces.is_empty());
        assert!(result["closed_workspace_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id.as_str() == Some(linked_id.as_str())));
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
        assert_eq!(
            session.process_info("missing").unwrap_err(),
            "pane 'missing' does not exist"
        );
        assert!(session.switch_workspace("missing").is_err());
        assert!(session.switch_tab("missing").is_err());
    }

    #[test]
    fn focusing_a_pane_can_activate_its_workspace_and_tab() {
        let mut session = Session::default();
        let second = session.create_workspace("Second".into()).unwrap();
        let workspace_id = second["workspace_id"].as_str().unwrap().to_owned();
        let pane_id = session
            .create_pane(CreatePaneRequest {
                command: "cmd.exe".into(),
                args: Vec::new(),
                cwd: std::env::current_dir()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                cols: 80,
                rows: 24,
                label: None,
                env: Default::default(),
                popup: false,
                overlay: false,
                popup_width_spec: None,
                popup_height_spec: None,
            })
            .unwrap()["pane_id"]
            .as_str()
            .unwrap()
            .to_owned();
        session.switch_workspace("workspace-1").unwrap();
        session.focus_pane(&pane_id).unwrap();
        assert_eq!(session.snapshot.active_space_id, "space-1");
        assert_eq!(
            session.snapshot.spaces[0].active_workspace_id.as_deref(),
            Some(workspace_id.as_str())
        );
        assert_eq!(
            session.snapshot.focused_pane_id.as_deref(),
            Some(pane_id.as_str())
        );
        assert!(session.events_since(0).iter().any(|event| {
            event.event == "pane_focused" && event.payload["pane_id"] == serde_json::json!(pane_id)
        }));
    }

    #[test]
    fn directional_focus_can_start_from_an_explicit_pane() {
        let mut session = Session::default();
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout =
            Some(LayoutNode::pane("pane-1").split(
                crate::model::layout::Direction::Horizontal,
                0.5,
                "pane-2",
            ));
        let result = session.focus_direction("right", Some("pane-1")).unwrap();
        assert_eq!(result["pane_id"], serde_json::json!("pane-2"));
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-2"));
    }

    #[test]
    fn right_click_passthrough_can_be_set_explicitly() {
        let mut session = Session::default();
        session.snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "scrollback_bytes": 0,
            }))
            .unwrap(),
        );
        let result = session.set_right_click_passthrough("pane-1", true).unwrap();
        assert_eq!(result["right_click_passthrough"], true);
        assert!(session.snapshot.panes[0].right_click_passthrough);
        session
            .set_right_click_passthrough("pane-1", false)
            .unwrap();
        assert!(!session.snapshot.panes[0].right_click_passthrough);
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
            popup: false,
            overlay: false,
            popup_width_spec: None,
            popup_height_spec: None,
        });

        assert!(result.is_err(), "must try to create a replacement shell");
        assert!(session.snapshot.panes.is_empty());
        assert!(session.snapshot.focused_pane_id.is_none());
        assert!(session.snapshot.spaces[0].workspaces[0].tabs[0]
            .layout
            .is_none());
    }

    #[test]
    fn ensure_active_pane_repairs_a_workspace_without_an_active_tab() {
        let mut session = Session::default();
        let workspace = &mut session.snapshot.spaces[0].workspaces[0];
        workspace.tabs.clear();
        workspace.active_tab_id.clear();

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
            popup: false,
            overlay: false,
            popup_width_spec: None,
            popup_height_spec: None,
        });

        assert!(
            result.is_err(),
            "the invalid shell should still fail to spawn"
        );
        let workspace = &session.snapshot.spaces[0].workspaces[0];
        assert_eq!(workspace.tabs.len(), 1);
        assert_eq!(workspace.tabs[0].name, "Main");
        assert_eq!(workspace.active_tab_id, workspace.tabs[0].tab_id);
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            status: PaneStatus::Completed { exit_code: 0 },
            tokens: std::collections::HashMap::new(),
            agent_session: None,
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
            hyperlinks: Vec::new(),
        });
        session.rename_pane("pane-1", "Shell".into()).unwrap();
        assert_eq!(session.snapshot.panes[0].label.as_deref(), Some("Shell"));
        assert_eq!(
            session
                .events_since(0)
                .last()
                .map(|event| event.event.as_str()),
            Some("pane_updated")
        );
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
        let sequence = session.snapshot.event_sequence;
        session.close_tab("tab-1").unwrap();
        assert!(session.snapshot().spaces[0].workspaces.is_empty());
        assert_eq!(session.snapshot().spaces[0].active_workspace_id, None);
        assert_eq!(session.snapshot().focused_pane_id, None);
        assert!(session.events_since(sequence).iter().any(|event| {
            event.event == "workspace_closed"
                && event.payload["workspace_id"] == "workspace-1"
                && event.payload["space_id"] == "space-1"
        }));

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
        let sequence = session.snapshot.event_sequence;
        session.close_tab(&tab_id).unwrap();
        assert_eq!(session.snapshot().spaces[0].workspaces[0].tabs.len(), 1);
        let events = session.events_since(sequence);
        assert!(events.iter().any(|event| {
            event.event == "tab_closed"
                && event.payload["tab_id"] == tab_id
                && event.payload["workspace_id"] == "workspace-1"
        }));
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
    fn focusing_workspace_switches_spaces_and_rejects_unknown_ids() {
        let mut session = Session::default();
        let other = session.create_space("Other project".into()).unwrap();
        let other_space_id = other["space_id"].as_str().unwrap().to_string();
        let other_workspace_id = session.snapshot.spaces[1].workspaces[0]
            .workspace_id
            .clone();

        session.focus_workspace("workspace-1").unwrap();
        assert_eq!(session.snapshot.active_space_id, "space-1");
        let sequence = session.snapshot.event_sequence;
        session.focus_workspace(&other_workspace_id).unwrap();
        assert_eq!(session.snapshot.active_space_id, other_space_id);
        assert_eq!(
            session.snapshot.spaces[1].active_workspace_id.as_deref(),
            Some(other_workspace_id.as_str())
        );
        assert!(session.events_since(sequence).iter().any(|event| {
            event.event == "workspace_focused"
                && event.payload["workspace_id"] == other_workspace_id
                && event.payload["space_id"] == other_space_id
        }));

        let active_space = session.snapshot.active_space_id.clone();
        assert!(session.focus_workspace("missing-workspace").is_err());
        assert_eq!(session.snapshot.active_space_id, active_space);
    }

    #[test]
    fn workspace_ids_are_unique_across_spaces_and_focus_targets_the_created_workspace() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let sequence = session.snapshot.event_sequence;
        let created = session.create_workspace("Feature".into()).unwrap();
        let workspace_id = created["workspace_id"].as_str().unwrap();

        assert_ne!(workspace_id, "workspace-1");
        assert_eq!(
            session.focus_workspace(workspace_id).unwrap()["workspace_id"],
            workspace_id
        );
        assert_eq!(session.snapshot.active_space_id, "space-2");
        assert!(session.events_since(sequence).iter().any(|event| {
            event.event == "workspace_created" && event.payload["workspace_id"] == workspace_id
        }));
    }

    #[test]
    fn moving_only_pane_to_new_tab_preserves_the_pane_and_focus() {
        let mut session = Session::default();
        session.snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "scrollback_bytes": 0
            }))
            .unwrap(),
        );
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(LayoutNode::pane("pane-1"));
        session.snapshot.spaces[0].workspaces[0].tabs[0].focused_pane_id = Some("pane-1".into());

        let result = session
            .move_pane_to_new_tab("pane-1", "Review".into())
            .unwrap();
        assert_eq!(result["pane_id"], "pane-1");
        assert_eq!(session.snapshot.panes.len(), 1);
        let workspace = &session.snapshot.spaces[0].workspaces[0];
        assert_eq!(workspace.tabs.len(), 1);
        assert_eq!(workspace.tabs[0].name, "Review");
        assert_eq!(workspace.tabs[0].focused_pane_id.as_deref(), Some("pane-1"));
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-1"));
    }

    #[test]
    fn moving_pane_to_existing_tab_preserves_process_and_splits_target() {
        let mut session = Session::default();
        session.snapshot.panes.extend([
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-2", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
        ]);
        let first_tab = session.snapshot.spaces[0].workspaces[0].tabs[0]
            .tab_id
            .clone();
        let target_tab = session.create_tab("Target".into()).unwrap()["tab_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let workspace = &mut session.snapshot.spaces[0].workspaces[0];
        workspace.tabs[0].layout = Some(LayoutNode::pane("pane-1"));
        workspace.tabs[0].focused_pane_id = Some("pane-1".into());
        let target = workspace
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == target_tab)
            .unwrap();
        target.layout = Some(LayoutNode::pane("pane-2"));
        target.focused_pane_id = Some("pane-2".into());
        workspace.active_tab_id = first_tab;
        session.sync_focus_to_active_tab().unwrap();

        let sequence = session.snapshot.event_sequence;
        let result = session
            .move_pane_to_tab(
                "pane-1",
                &target_tab,
                Some("pane-2"),
                crate::model::layout::Direction::Horizontal,
                0.5,
                true,
            )
            .unwrap();
        assert_eq!(result["tab_id"], target_tab);
        assert_eq!(session.snapshot.panes.len(), 2);
        assert!(session.events_since(sequence).iter().any(|event| {
            event.event == "pane_moved"
                && event.payload["pane_id"] == "pane-1"
                && event.payload["previous_tab_id"] == "tab-1"
                && event.payload["tab_id"] == target_tab
        }));
        let workspace = &session.snapshot.spaces[0].workspaces[0];
        assert_eq!(workspace.tabs.len(), 1);
        assert!(workspace.tabs[0]
            .layout
            .as_ref()
            .unwrap()
            .pane_ids()
            .contains(&"pane-1"));
        assert!(workspace.tabs[0]
            .layout
            .as_ref()
            .unwrap()
            .pane_ids()
            .contains(&"pane-2"));
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-1"));
    }

    #[test]
    fn moving_pane_to_new_workspace_preserves_process_and_labels() {
        let mut session = Session::default();
        session.snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
        );
        let source_workspace = session.snapshot.spaces[0].workspaces[0]
            .workspace_id
            .clone();
        let source_tab = session.snapshot.spaces[0].workspaces[0].tabs[0]
            .tab_id
            .clone();
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(LayoutNode::pane("pane-1"));
        session.snapshot.spaces[0].workspaces[0].tabs[0].focused_pane_id = Some("pane-1".into());

        let result = session
            .move_pane_to_new_workspace("pane-1", "Feature".into(), "Shell".into(), true)
            .unwrap();
        let workspace_id = result["workspace_id"].as_str().unwrap();
        assert_eq!(session.snapshot.panes.len(), 1);
        assert_eq!(
            session.snapshot.spaces[0].active_workspace_id.as_deref(),
            Some(workspace_id)
        );
        let source = session.snapshot.spaces[0]
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == source_workspace)
            .unwrap();
        assert_eq!(source.tabs[0].tab_id, source_tab);
        assert!(source.tabs[0].layout.is_none());
        let target = session.snapshot.spaces[0]
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .unwrap();
        assert_eq!(target.name, "Feature");
        assert_eq!(target.tabs[0].name, "Shell");
        assert_eq!(target.tabs[0].focused_pane_id.as_deref(), Some("pane-1"));
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-1"));
    }

    #[test]
    fn moving_pane_to_a_new_tab_in_another_workspace_preserves_process() {
        let mut session = Session::default();
        session.snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
        );
        session.create_workspace("Target".into()).unwrap();
        session.switch_workspace("workspace-1").unwrap();
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(LayoutNode::pane("pane-1"));
        session.snapshot.spaces[0].workspaces[0].tabs[0].focused_pane_id = Some("pane-1".into());

        let result = session
            .move_pane_to_new_tab_anywhere("pane-1", "Review".into(), Some("workspace-2"), true)
            .unwrap();
        assert_eq!(result["workspace_id"], "workspace-2");
        assert_eq!(session.snapshot.panes.len(), 1);
        assert_eq!(
            session.snapshot.spaces[0].active_workspace_id.as_deref(),
            Some("workspace-2")
        );
        let target = &session.snapshot.spaces[0].workspaces[1];
        assert_eq!(target.tabs.len(), 2);
        assert_eq!(target.tabs[1].name, "Review");
        assert!(target.tabs[1]
            .layout
            .as_ref()
            .unwrap()
            .pane_ids()
            .contains(&"pane-1"));
    }

    #[test]
    fn workspace_rename_resolves_ids_across_spaces_without_changing_focus() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let workspace_id = session.snapshot.spaces[1].workspaces[0]
            .workspace_id
            .clone();
        session.switch_space("space-1").unwrap();

        let sequence = session.snapshot.event_sequence;
        session
            .rename_workspace(&workspace_id, "Review".into())
            .unwrap();

        assert_eq!(session.snapshot.active_space_id, "space-1");
        assert_eq!(session.snapshot.spaces[1].workspaces[0].name, "Review");
        assert!(session.events_since(sequence).iter().any(|event| {
            event.event == "workspace_renamed"
                && event.payload["workspace_id"] == workspace_id
                && event.payload["label"] == "Review"
        }));
    }

    #[test]
    fn loading_duplicate_workspace_ids_repairs_the_later_id_and_selection() {
        let directory = std::env::temp_dir().join(format!(
            "spindle-duplicate-workspace-ids-{}",
            std::process::id()
        ));
        let path = directory.join("session.json");
        let mut session = Session::load_or_default(&path).unwrap();
        session.create_space("Other project".into()).unwrap();
        session.snapshot.spaces[1].workspaces[0].workspace_id = "workspace-1".into();
        session.snapshot.spaces[1].workspaces[0].tabs[0].tab_id = "tab-workspace-1-1".into();
        session.snapshot.spaces[1].workspaces[0].active_tab_id = "tab-workspace-1-1".into();
        session.snapshot.spaces[1].active_workspace_id = Some("workspace-1".into());
        session.save().unwrap();

        let mut restored = Session::load_or_default(&path).unwrap();
        let repaired_id = restored.snapshot.spaces[1].workspaces[0]
            .workspace_id
            .clone();
        let repaired_tab_id = restored.snapshot.spaces[1].workspaces[0].tabs[0]
            .tab_id
            .clone();
        assert_ne!(repaired_id, "workspace-1");
        assert_eq!(
            restored.snapshot.spaces[1].active_workspace_id.as_deref(),
            Some(repaired_id.as_str())
        );
        assert_eq!(
            restored.snapshot.spaces[1].workspaces[0].active_tab_id,
            repaired_tab_id
        );
        assert!(repaired_tab_id.starts_with(&format!("tab-{repaired_id}-")));
        restored.focus_workspace(&repaired_id).unwrap();
        assert_eq!(restored.snapshot.active_space_id, "space-2");

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn setting_a_split_ratio_targets_a_nested_split() {
        let mut session = Session::default();
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(
            LayoutNode::pane("one")
                .split(crate::model::layout::Direction::Horizontal, 0.5, "two")
                .split(crate::model::layout::Direction::Vertical, 0.5, "three"),
        );
        let sequence = session.snapshot.event_sequence;
        session
            .set_split_ratio(&[false], 0.7)
            .expect("nested split path should be valid");
        assert!(session.events_since(sequence).iter().any(|event| {
            event.event == "layout_updated"
                && event.payload["workspace_id"] == "workspace-1"
                && event.payload["tab_id"] == "tab-1"
                && event.payload["pane_ids"] == serde_json::json!(["one", "two", "three"])
        }));
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
    fn resizing_a_pane_resolves_it_in_an_inactive_workspace() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let tab = &mut session.snapshot.spaces[1].workspaces[0].tabs[0];
        tab.layout = Some(LayoutNode::pane("one").split(
            crate::model::layout::Direction::Horizontal,
            0.5,
            "two",
        ));
        session.switch_space("space-1").unwrap();

        session.resize_pane("one", 0.1).unwrap();

        let Some(LayoutNode::Split { ratio, .. }) =
            &session.snapshot.spaces[1].workspaces[0].tabs[0].layout
        else {
            panic!("expected root split");
        };
        assert!((*ratio - 0.6).abs() < f32::EPSILON);
        assert_eq!(session.snapshot.active_space_id, "space-2");
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
    fn swapping_panes_resolves_both_panes_in_an_inactive_workspace() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let tab = &mut session.snapshot.spaces[1].workspaces[0].tabs[0];
        tab.layout = Some(LayoutNode::pane("one").split(
            crate::model::layout::Direction::Horizontal,
            0.5,
            "two",
        ));
        tab.focused_pane_id = Some("two".into());
        session.switch_space("space-1").unwrap();

        session.swap_panes("one", "two").unwrap();

        let tab = &session.snapshot.spaces[1].workspaces[0].tabs[0];
        assert_eq!(tab.layout.as_ref().unwrap().pane_ids(), vec!["two", "one"]);
        assert_eq!(tab.focused_pane_id.as_deref(), Some("one"));
        assert_eq!(session.snapshot.active_space_id, "space-2");
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("one"));
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
    fn pane_zoom_resolves_a_pane_in_an_inactive_workspace() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let workspace = &mut session.snapshot.spaces[1].workspaces[0];
        workspace.tabs[0].layout = Some(LayoutNode::pane("pane-1"));
        workspace.tabs[0].focused_pane_id = Some("pane-1".into());
        session.snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
        );
        session.switch_space("space-1").unwrap();

        assert_eq!(
            session.toggle_pane_zoom("pane-1").unwrap(),
            serde_json::json!({ "pane_id": "pane-1", "zoomed": true })
        );
        assert_eq!(session.snapshot.active_space_id, "space-2");
        assert_eq!(session.snapshot.focused_pane_id.as_deref(), Some("pane-1"));
        assert!(session.snapshot.spaces[1].workspaces[0].tabs[0].zoomed);
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            tokens: std::collections::HashMap::new(),
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            agent_session: None,
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
            hyperlinks: Vec::new(),
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            tokens: std::collections::HashMap::new(),
            status: PaneStatus::Completed { exit_code: 0 },
            agent_session: None,
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
            hyperlinks: Vec::new(),
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
            PaneEvent::AgentStatusChanged {
                pane_id: "pane-1".into(),
                agent_state: crate::detect::AgentState::Working,
            },
        ]);
        assert_eq!(session.events.len(), 3);
        assert_eq!(session.events[0].sequence, 1);
        assert_eq!(session.events[1].sequence, 2);
        assert_eq!(session.events[2].event, "pane_agent_status_changed");
        assert_eq!(session.events[2].payload["agent_status"], "working");
    }

    #[test]
    fn worktree_events_are_recorded_for_plugin_delivery() {
        let mut session = Session::default();
        session
            .record_worktree_event(
                "worktree_created",
                serde_json::json!({
                    "workspace_id": "workspace-2",
                    "path": "C:/repo-feature",
                    "branch": "feature",
                }),
            )
            .unwrap();
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].event, "worktree_created");
        assert_eq!(session.events[0].payload["branch"], "feature");
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            status: PaneStatus::Running,
            tokens: std::collections::HashMap::new(),
            agent_session: None,
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
            hyperlinks: Vec::new(),
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            tokens: std::collections::HashMap::new(),
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            agent_session: None,
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
            hyperlinks: Vec::new(),
        });
        assert!(session.restart_pane("pane-1").is_err());
    }

    #[test]
    fn restarting_a_pane_clears_the_previous_agents_identity_and_state() {
        let mut session = Session::default();
        session.snapshot.panes.push(PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: vec!["-NoLogo".into(), "-NoProfile".into()],
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: Some("build shell".into()),
            agent: Some(crate::detect::AgentKind::Claude),
            agent_state: Some(crate::detect::AgentState::Blocked),
            agent_done: true,
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            agent_session: None,
            tokens: std::collections::HashMap::new(),
            status: PaneStatus::Halted {
                reason: "process exited".into(),
            },
            scrollback_bytes: b"old scrollback".len(),
            scrollback: b"old scrollback".to_vec(),
            screen: "previous agent screen".into(),
            cursor: (5, 1),
            cursor_visible: true,
            title: "Claude".into(),
            alternate_screen: true,
            mouse_reporting: true,
            mouse_release: true,
            mouse_motion: true,
            mouse_any_motion: true,
            sgr_mouse: true,
            utf8_mouse: true,
            application_cursor: true,
            bracketed_paste: true,
            right_click_passthrough: true,
            hyperlinks: Vec::new(),
        });
        session.snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(LayoutNode::pane("pane-1"));
        session.snapshot.spaces[0].workspaces[0].tabs[0].focused_pane_id = Some("pane-1".into());
        session.snapshot.focused_pane_id = Some("pane-1".into());

        session.restart_pane("pane-1").unwrap();

        let pane = &session.snapshot.panes[0];
        assert!(pane.status.is_running());
        assert_eq!(pane.label.as_deref(), Some("build shell"));
        assert_eq!(pane.agent, None);
        assert_eq!(pane.agent_state, None);
        assert!(!pane.agent_done);
        assert!(pane.scrollback.is_empty());
        assert_eq!(pane.scrollback_bytes, 0);
        assert!(pane.screen.is_empty());
        assert_eq!(pane.cursor, (0, 0));
        assert!(!pane.cursor_visible);
        assert!(pane.title.is_empty());
        assert!(!pane.alternate_screen);
        assert!(!pane.mouse_reporting);
        assert!(!pane.mouse_release);
        assert!(!pane.mouse_motion);
        assert!(!pane.mouse_any_motion);
        assert!(!pane.sgr_mouse);
        assert!(!pane.utf8_mouse);
        assert!(!pane.application_cursor);
        assert!(!pane.bracketed_paste);
        assert!(pane.right_click_passthrough);
    }

    #[test]
    fn moving_a_tab_reorders_it_without_changing_focus() {
        let mut session = Session::default();
        let first = session.snapshot.spaces[0].workspaces[0].tabs[0]
            .tab_id
            .clone();
        let second = session.create_tab("Second".into()).unwrap()["tab_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let third = session.create_tab("Third".into()).unwrap()["tab_id"]
            .as_str()
            .unwrap()
            .to_owned();
        session.switch_tab_anywhere(&second).unwrap();
        let focused = session.snapshot.spaces[0].workspaces[0]
            .active_tab_id
            .clone();

        let result = session.move_tab_anywhere(&first, 3).unwrap();
        let workspace = &session.snapshot.spaces[0].workspaces[0];
        let ids: Vec<_> = workspace
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect();
        let workspace_id = workspace.workspace_id.clone();
        assert_eq!(ids, vec![second.as_str(), third.as_str(), first.as_str()]);
        assert_eq!(workspace.active_tab_id, focused);
        assert_eq!(result["moved"], true);
        assert!(session.events_since(0).iter().any(|event| {
            event.event == "tab_moved"
                && event.payload["tab_id"] == first
                && event.payload["workspace_id"] == workspace_id
        }));
    }

    #[test]
    fn moving_a_workspace_reorders_it_without_changing_focus() {
        let mut session = Session::default();
        let first = session.snapshot.spaces[0].workspaces[0]
            .workspace_id
            .clone();
        let second = session.create_workspace("Second".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let third = session.create_workspace("Third".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        session.focus_workspace(&second).unwrap();

        let result = session.move_workspace_anywhere(&first, 3).unwrap();
        let space = &session.snapshot.spaces[0];
        let ids: Vec<_> = space
            .workspaces
            .iter()
            .map(|workspace| workspace.workspace_id.as_str())
            .collect();
        let active = space.active_workspace_id.clone();
        let space_id = space.space_id.clone();
        assert_eq!(ids, vec![second.as_str(), third.as_str(), first.as_str()]);
        assert_eq!(active.as_deref(), Some(second.as_str()));
        assert_eq!(result["moved"], true);
        assert!(session.events_since(0).iter().any(|event| {
            event.event == "workspace_reordered"
                && event.payload["workspace_id"] == first
                && event.payload["space_id"] == space_id
        }));
    }

    #[test]
    fn moving_a_worktree_workspace_keeps_its_group_together() {
        let mut session = Session::default();
        let primary = session.snapshot.spaces[0].workspaces[0]
            .workspace_id
            .clone();
        let linked = session.create_workspace("Linked".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let other = session.create_workspace("Other".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let workspaces = &mut session.snapshot.spaces[0].workspaces;
        workspaces[0].worktree_group = Some("group-1".into());
        workspaces[1].worktree_group = Some("group-1".into());
        workspaces[1].is_linked_worktree = true;

        session.move_workspace_anywhere(&primary, 2).unwrap();
        let ids: Vec<_> = session.snapshot.spaces[0]
            .workspaces
            .iter()
            .map(|workspace| workspace.workspace_id.as_str())
            .collect();
        assert_eq!(ids, vec![other.as_str(), primary.as_str(), linked.as_str()]);
        assert!(session.move_workspace_anywhere(&linked, 0).is_err());
    }
}
