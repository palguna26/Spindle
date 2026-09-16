use crate::detect::AgentDisplayState;
use crate::server::session::SessionSnapshot;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    NewWorkspace,
    NewTab,
    Menu(usize),
    Space(String),
    Workspace {
        space_id: String,
        id: String,
    },
    Tab {
        space_id: String,
        workspace_id: String,
        id: String,
    },
    Pane {
        space_id: String,
        workspace_id: String,
        tab_id: String,
        id: String,
    },
    Agent {
        space_id: String,
        workspace_id: String,
        tab_id: String,
        id: String,
    },
}

#[derive(Debug, Clone)]
pub struct Row {
    pub target: Target,
    pub depth: usize,
    pub label: String,
    pub detail: String,
    pub current: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Close,
    Activate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Blocked,
    Working,
    Idle,
    Done,
}

#[derive(Debug, Clone, Default)]
pub struct Navigator {
    mobile: bool,
    query: String,
    search_focused: bool,
    filter: Option<Filter>,
    selected: Option<Target>,
    expanded_spaces: Vec<String>,
    expanded_workspaces: Vec<(String, String)>,
}

impl Navigator {
    pub fn new(snapshot: &SessionSnapshot) -> Self {
        let mut this = Self {
            mobile: false,
            expanded_spaces: snapshot
                .spaces
                .iter()
                .map(|space| space.space_id.clone())
                .collect(),
            expanded_workspaces: snapshot
                .spaces
                .iter()
                .flat_map(|space| {
                    space.workspaces.iter().map(move |workspace| {
                        (space.space_id.clone(), workspace.workspace_id.clone())
                    })
                })
                .collect(),
            ..Self::default()
        };
        this.selected = snapshot.focused_pane_id.as_ref().and_then(|pane_id| {
            snapshot.spaces.iter().find_map(|space| {
                space.workspaces.iter().find_map(|workspace| {
                    workspace
                        .tabs
                        .iter()
                        .find(|tab| {
                            tab.layout
                                .as_ref()
                                .is_some_and(|layout| layout.pane_ids().contains(&pane_id.as_str()))
                        })
                        .map(|tab| Target::Pane {
                            space_id: space.space_id.clone(),
                            workspace_id: workspace.workspace_id.clone(),
                            tab_id: tab.tab_id.clone(),
                            id: pane_id.clone(),
                        })
                })
            })
        });
        this
    }

    pub fn new_mobile(snapshot: &SessionSnapshot) -> Self {
        let mut navigator = Self::new(snapshot);
        navigator.mobile = true;
        navigator
    }

    pub fn is_mobile(&self) -> bool {
        self.mobile
    }

    pub fn rows(&self, snapshot: &SessionSnapshot) -> Vec<Row> {
        let query = self.query.to_lowercase();
        let filtering = self.filter.is_some() || !query.is_empty();
        let mut rows = Vec::new();
        let mut agent_rows = Vec::new();
        for space in &snapshot.spaces {
            let mut space_rows = Vec::new();
            for workspace in &space.workspaces {
                let mut workspace_rows = Vec::new();
                for tab in &workspace.tabs {
                    let mut tab_rows = Vec::new();
                    let panes = tab
                        .layout
                        .as_ref()
                        .map(|layout| layout.pane_ids())
                        .unwrap_or_default();
                    for &pane_id in &panes {
                        let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)
                        else {
                            continue;
                        };
                        if self.filter.is_some_and(|filter| {
                            !matches_filter(filter, pane.agent_display_state())
                        }) {
                            continue;
                        }
                        let label = pane
                            .label
                            .clone()
                            .filter(|name| !name.is_empty())
                            .or_else(|| pane.agent.map(|agent| agent.label().to_owned()))
                            .unwrap_or_else(|| {
                                let title = compact_name(&pane.title);
                                if title.is_empty() {
                                    compact_name(&pane.command)
                                } else {
                                    title
                                }
                            });
                        let detail =
                            format!("{} · {}", pane.agent_display_state().label(), pane.cwd);
                        let target = Target::Pane {
                            space_id: space.space_id.clone(),
                            workspace_id: workspace.workspace_id.clone(),
                            tab_id: tab.tab_id.clone(),
                            id: pane.pane_id.clone(),
                        };
                        if self.mobile && pane.agent.is_some() {
                            let agent_target = Target::Agent {
                                space_id: space.space_id.clone(),
                                workspace_id: workspace.workspace_id.clone(),
                                tab_id: tab.tab_id.clone(),
                                id: pane.pane_id.clone(),
                            };
                            let agent_label =
                                pane.agent.map(|agent| agent.label()).unwrap_or("agent");
                            let agent_detail = pane.agent_display_state().label().to_owned();
                            if query.is_empty()
                                || contains(&[agent_label, &agent_detail, &pane.cwd], &query)
                            {
                                agent_rows.push(Row {
                                    current: snapshot.focused_pane_id.as_deref()
                                        == Some(&pane.pane_id),
                                    target: agent_target,
                                    depth: 0,
                                    label: agent_label.to_owned(),
                                    detail: agent_detail,
                                    expanded: false,
                                });
                            }
                        }
                        if query.is_empty()
                            || contains(&[&label, &detail, &pane.command, &pane.cwd], &query)
                        {
                            tab_rows.push(Row {
                                current: snapshot.focused_pane_id.as_deref() == Some(&pane.pane_id),
                                target,
                                depth: 3,
                                label,
                                detail,
                                expanded: false,
                            });
                        }
                    }
                    let tab_matches = self.filter.is_none()
                        && (query.is_empty() || contains(&[&tab.name], &query));
                    if !tab_rows.is_empty() || tab_matches {
                        let target = Target::Tab {
                            space_id: space.space_id.clone(),
                            workspace_id: workspace.workspace_id.clone(),
                            id: tab.tab_id.clone(),
                        };
                        workspace_rows.push(Row {
                            target,
                            depth: 2,
                            label: tab.name.clone(),
                            detail: format!(
                                "{} pane{}",
                                panes.len(),
                                if panes.len() == 1 { "" } else { "s" }
                            ),
                            current: false,
                            expanded: true,
                        });
                        if !self.mobile
                            && (filtering
                            || self.expanded_workspaces.iter().any(|item| {
                                item == &(space.space_id.clone(), workspace.workspace_id.clone())
                            }))
                        {
                            workspace_rows.extend(tab_rows);
                        }
                    }
                }
                let workspace_matches = self.filter.is_none()
                    && (query.is_empty()
                        || contains(
                            &[
                                &workspace.name,
                                workspace.branch.as_deref().unwrap_or(""),
                                workspace.repository_path.as_deref().unwrap_or(""),
                            ],
                            &query,
                        ));
                if !workspace_rows.is_empty() || workspace_matches {
                    let target = Target::Workspace {
                        space_id: space.space_id.clone(),
                        id: workspace.workspace_id.clone(),
                    };
                    space_rows.push(Row {
                        target,
                        depth: 1,
                        label: workspace.name.clone(),
                        detail: workspace.branch.clone().unwrap_or_default(),
                        current: false,
                        expanded: filtering
                            || self.expanded_workspaces.iter().any(|item| {
                                item.0 == space.space_id && item.1 == workspace.workspace_id
                            }),
                    });
                    if filtering
                        || self.expanded_workspaces.iter().any(|item| {
                            item.0 == space.space_id && item.1 == workspace.workspace_id
                        })
                    {
                        space_rows.extend(workspace_rows);
                    }
                }
            }
            let space_matches =
                self.filter.is_none() && (query.is_empty() || contains(&[&space.name], &query));
            if !space_rows.is_empty() || space_matches {
                let target = Target::Space(space.space_id.clone());
                rows.push(Row {
                    target,
                    depth: 0,
                    label: space.name.clone(),
                    detail: if space.space_id == snapshot.active_space_id {
                        "current space".into()
                    } else {
                        "space".into()
                    },
                    current: false,
                    expanded: filtering || self.expanded_spaces.contains(&space.space_id),
                });
                if filtering || self.expanded_spaces.contains(&space.space_id) {
                    rows.extend(space_rows);
                }
            }
        }
        if self.mobile {
            let mut mobile_rows = Vec::with_capacity(rows.len() + agent_rows.len() + 8);
            mobile_rows.extend(agent_rows);
            mobile_rows.push(Row {
                target: Target::NewWorkspace,
                depth: 0,
                label: "+ new workspace".to_owned(),
                detail: "create workspace".to_owned(),
                current: false,
                expanded: false,
            });
            mobile_rows.extend(rows);
            if snapshot
                .spaces
                .iter()
                .any(|space| space.active_workspace_id.is_some())
            {
                mobile_rows.push(Row {
                    target: Target::NewTab,
                    depth: 0,
                    label: "+ new tab".to_owned(),
                    detail: "create tab".to_owned(),
                    current: false,
                    expanded: false,
                });
            }
            for (index, (label, _)) in crate::client::global_menu::GlobalMenu::items()
                .iter()
                .enumerate()
            {
                mobile_rows.push(Row {
                    target: Target::Menu(index),
                    depth: 0,
                    label: (*label).to_owned(),
                    detail: "menu".to_owned(),
                    current: false,
                    expanded: false,
                });
            }
            mobile_rows
        } else {
            rows
        }
    }

    pub fn selected(&self, snapshot: &SessionSnapshot) -> Option<Target> {
        let rows = self.rows(snapshot);
        self.selected
            .clone()
            .filter(|selected| rows.iter().any(|row| &row.target == selected))
            .or_else(|| {
                let query = self.query.trim().to_lowercase();
                if query.is_empty() {
                    return None;
                }
                rows.iter()
                    .find(|row| contains(&[&row.label, &row.detail], &query))
                    .map(|row| row.target.clone())
            })
            .or_else(|| {
                self.filter
                    .is_some()
                    .then(|| {
                        rows.iter()
                            .find(|row| matches!(row.target, Target::Pane { .. }))
                            .map(|row| row.target.clone())
                    })
                    .flatten()
            })
            .or_else(|| {
                rows.iter()
                    .find(|row| row.current)
                    .map(|row| row.target.clone())
            })
            .or_else(|| rows.first().map(|row| row.target.clone()))
    }

    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn search_focused(&self) -> bool {
        self.search_focused
    }
    pub fn filter_label(&self) -> Option<&'static str> {
        match self.filter {
            Some(Filter::Blocked) => Some("blocked"),
            Some(Filter::Working) => Some("working"),
            Some(Filter::Idle) => Some("idle"),
            Some(Filter::Done) => Some("done"),
            None => None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, snapshot: &SessionSnapshot) -> Outcome {
        let code = key.code;
        if code == KeyCode::Esc {
            if self.search_focused {
                self.search_focused = false;
            } else {
                return Outcome::Close;
            }
            return Outcome::Continue;
        }
        if code == KeyCode::Enter {
            return Outcome::Activate;
        }
        if self.search_focused {
            if code == KeyCode::Up
                || (code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                self.move_by(snapshot, -1);
            } else if code == KeyCode::Down
                || (code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                self.move_by(snapshot, 1);
            } else if code == KeyCode::Backspace {
                self.query.pop();
                self.filter = None;
                self.selected = None;
            } else if code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.query.clear();
                self.filter = None;
                self.selected = None;
            } else if let KeyCode::Char(ch) = code {
                if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
                    self.query.push(ch);
                    self.filter = None;
                    self.selected = None;
                }
            }
            return Outcome::Continue;
        }
        if code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.move_by(snapshot, 8);
            return Outcome::Continue;
        }
        if code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.move_by(snapshot, -8);
            return Outcome::Continue;
        }
        match code {
            KeyCode::Down | KeyCode::Char('j') => self.move_by(snapshot, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(snapshot, -1),
            KeyCode::Char('/') => {
                self.search_focused = true;
                self.filter = None;
            }
            KeyCode::Char(' ') => self.toggle(snapshot),
            KeyCode::Char('a') => {
                self.query.clear();
                self.filter = None;
                self.selected = None;
            }
            KeyCode::Char('b') => self.set_filter(Filter::Blocked),
            KeyCode::Char('w') => self.set_filter(Filter::Working),
            KeyCode::Char('i') => self.set_filter(Filter::Idle),
            KeyCode::Char('d') => self.set_filter(Filter::Done),
            KeyCode::Backspace => {
                self.filter = None;
                self.selected = None;
            }
            KeyCode::Home => {
                self.selected = None;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = self.rows(snapshot).last().map(|row| row.target.clone())
            }
            _ => {}
        }
        Outcome::Continue
    }

    fn set_filter(&mut self, filter: Filter) {
        self.query.clear();
        self.filter = Some(filter);
        self.selected = None;
    }
    fn move_by(&mut self, snapshot: &SessionSnapshot, delta: isize) {
        let rows = self.rows(snapshot);
        if rows.is_empty() {
            self.selected = None;
            return;
        }
        let index = self
            .selected(snapshot)
            .and_then(|target| rows.iter().position(|row| row.target == target))
            .unwrap_or(0);
        let next = (index as isize + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.selected = Some(rows[next].target.clone());
    }
    fn toggle(&mut self, snapshot: &SessionSnapshot) {
        match self.selected(snapshot) {
            Some(Target::Space(id)) => toggle_id(&mut self.expanded_spaces, id),
            Some(Target::Workspace { space_id, id }) => {
                if let Some(index) = self
                    .expanded_workspaces
                    .iter()
                    .position(|item| item == &(space_id.clone(), id.clone()))
                {
                    self.expanded_workspaces.remove(index);
                } else {
                    self.expanded_workspaces.push((space_id, id));
                }
            }
            _ => {}
        }
        self.selected = None;
    }

    pub fn select(&mut self, target: Target) {
        self.selected = Some(target);
    }
    pub fn focus_search(&mut self) {
        self.search_focused = true;
        self.filter = None;
    }
    pub fn move_selection(&mut self, snapshot: &SessionSnapshot, delta: isize) {
        self.move_by(snapshot, delta);
    }
    pub fn toggle_selected(&mut self, snapshot: &SessionSnapshot) {
        self.toggle(snapshot);
    }
}

fn toggle_id(items: &mut Vec<String>, id: String) {
    if let Some(index) = items.iter().position(|item| item == &id) {
        items.remove(index);
    } else {
        items.push(id);
    }
}
fn contains(values: &[&str], query: &str) -> bool {
    values
        .iter()
        .any(|value| value.to_lowercase().contains(query))
}
fn compact_name(value: &str) -> String {
    value
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_owned()
}
fn matches_filter(filter: Filter, state: AgentDisplayState) -> bool {
    matches!(
        (filter, state),
        (Filter::Blocked, AgentDisplayState::Blocked)
            | (Filter::Working, AgentDisplayState::Working)
            | (Filter::Idle, AgentDisplayState::Idle)
            | (Filter::Done, AgentDisplayState::Done)
    )
}

#[cfg(test)]
mod tests {
    use super::{Navigator, Outcome, Target};
    use crate::server::session::{Session, SessionSnapshot};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn snapshot() -> SessionSnapshot {
        let mut session = Session::default();
        session.create_tab("Logs".into()).unwrap();
        session.snapshot().clone()
    }

    fn snapshot_with_pane() -> SessionSnapshot {
        let mut snapshot = snapshot();
        let space = &mut snapshot.spaces[0];
        let workspace = &mut space.workspaces[0];
        let tab = &mut workspace.tabs[0];
        tab.layout = Some(crate::model::layout::LayoutNode::pane("pane-1"));
        snapshot.focused_pane_id = Some("pane-1".into());
        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "pwsh.exe", "args": [], "cwd": "C:/work",
                "status": "Running", "scrollback_bytes": 0, "agent": "codex",
                "agent_state": "working"
            }))
            .unwrap(),
        );
        snapshot
    }

    #[test]
    fn initial_rows_select_current_location_and_escape_closes() {
        let snapshot = snapshot();
        let mut navigator = Navigator::new(&snapshot);
        assert!(navigator.selected(&snapshot).is_some());
        assert_eq!(
            navigator.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &snapshot),
            Outcome::Close
        );
    }

    #[test]
    fn mobile_rows_include_herdr_switcher_actions() {
        let snapshot = snapshot();
        let navigator = Navigator::new_mobile(&snapshot);
        let rows = navigator.rows(&snapshot);
        assert!(navigator.is_mobile());
        assert!(matches!(
            rows.first().map(|row| &row.target),
            Some(Target::NewWorkspace)
        ));
        assert!(rows.iter().any(|row| matches!(row.target, Target::NewTab)));
        assert!(rows.iter().any(|row| matches!(row.target, Target::Menu(0))));
    }

    #[test]
    fn mobile_rows_put_detected_agents_first_like_herdr() {
        let snapshot = snapshot_with_pane();
        let navigator = Navigator::new_mobile(&snapshot);
        let rows = navigator.rows(&snapshot);
        assert!(matches!(
            rows.first().map(|row| &row.target),
            Some(Target::Agent { .. })
        ));
        assert_eq!(rows[0].label, "Codex");
        assert_eq!(rows[0].detail, "working");
        assert!(!rows
            .iter()
            .any(|row| matches!(row.target, Target::Pane { .. })));
    }

    #[test]
    fn slash_opens_search_and_escape_returns_to_tree() {
        let snapshot = snapshot();
        let mut navigator = Navigator::new(&snapshot);
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &snapshot,
        );
        assert_eq!(
            navigator.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &snapshot),
            Outcome::Continue
        );
        assert_eq!(
            navigator.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &snapshot),
            Outcome::Close
        );
    }

    #[test]
    fn search_and_working_filter_keep_the_matching_pane_navigable() {
        let snapshot = snapshot_with_pane();
        let mut navigator = Navigator::new(&snapshot);
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            &snapshot,
        );
        assert!(navigator
            .rows(&snapshot)
            .iter()
            .any(|row| matches!(row.target, Target::Pane { .. })));
        assert!(matches!(
            navigator.selected(&snapshot),
            Some(Target::Pane { .. })
        ));
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &snapshot,
        );
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &snapshot,
        );
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            &snapshot,
        );
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
            &snapshot,
        );
        assert!(navigator
            .rows(&snapshot)
            .iter()
            .any(|row| matches!(row.target, Target::Pane { .. })));
    }

    #[test]
    fn searching_workspace_name_selects_that_workspace_not_its_space_group() {
        let mut session = Session::default();
        let created = session.create_workspace("Review".into()).unwrap();
        let workspace_id = created["workspace_id"].as_str().unwrap().to_string();
        let snapshot = session.snapshot().clone();
        let mut navigator = Navigator::new(&snapshot);
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &snapshot,
        );
        for character in "Review".chars() {
            navigator.handle_key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                &snapshot,
            );
        }

        assert_eq!(
            navigator.selected(&snapshot),
            Some(Target::Workspace {
                space_id: "space-1".into(),
                id: workspace_id,
            })
        );
    }

    #[test]
    fn search_expands_collapsed_space_and_workspace_branches() {
        let snapshot = snapshot_with_pane();
        let mut navigator = Navigator::new(&snapshot);
        navigator.select(Target::Workspace {
            space_id: "space-1".into(),
            id: "workspace-1".into(),
        });
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            &snapshot,
        );
        navigator.select(Target::Space("space-1".into()));
        navigator.handle_key(
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            &snapshot,
        );
        assert!(navigator
            .rows(&snapshot)
            .iter()
            .all(|row| !matches!(row.target, Target::Pane { .. })));

        navigator.handle_key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &snapshot,
        );
        for character in "pwsh.exe".chars() {
            navigator.handle_key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                &snapshot,
            );
        }
        assert!(navigator
            .rows(&snapshot)
            .iter()
            .any(|row| matches!(&row.target, Target::Pane { id, .. } if id == "pane-1")));
        assert!(navigator.rows(&snapshot).iter().any(|row| {
            matches!(&row.target, Target::Space(id) if id == "space-1") && row.expanded
        }));
        assert!(navigator.rows(&snapshot).iter().any(|row| {
            matches!(&row.target, Target::Workspace { id, .. } if id == "workspace-1")
                && row.expanded
        }));
    }

    #[test]
    fn pane_label_uses_the_leaf_name_for_windows_paths() {
        assert_eq!(
            super::compact_name(r"C:\Windows\System32\pwsh.exe"),
            "pwsh.exe"
        );
    }
}
