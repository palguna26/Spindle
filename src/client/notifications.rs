use crate::detect::AgentState;
use crate::server::session::SessionSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    NeedsAttention,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Event {
    pub pane_id: String,
    pub agent: String,
    pub kind: Kind,
}

pub(crate) fn observe(previous: &SessionSnapshot, current: &SessionSnapshot) -> Option<Event> {
    current.panes.iter().find_map(|pane| {
        let agent = pane.agent?;
        let state = pane.agent_state?;
        let old = previous
            .panes
            .iter()
            .find(|old| old.pane_id == pane.pane_id)
            .and_then(|old| old.agent_state)
            .unwrap_or(AgentState::Unknown);
        let active = active_pane_ids(current).contains(&pane.pane_id.as_str());
        notification_kind(old, state, active).map(|kind| Event {
            pane_id: pane.pane_id.clone(),
            agent: agent.label().to_owned(),
            kind,
        })
    })
}

fn notification_kind(previous: AgentState, current: AgentState, active: bool) -> Option<Kind> {
    if active || previous == current {
        return None;
    }
    match current {
        AgentState::Blocked => Some(Kind::NeedsAttention),
        AgentState::Idle if matches!(previous, AgentState::Working | AgentState::Blocked) => {
            Some(Kind::Finished)
        }
        _ => None,
    }
}

fn active_pane_ids(snapshot: &SessionSnapshot) -> Vec<&str> {
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    else {
        return Vec::new();
    };
    let Some(workspace_id) = space.active_workspace_id.as_deref() else {
        return Vec::new();
    };
    let Some(workspace) = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
    else {
        return Vec::new();
    };
    let Some(tab) = workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)
    else {
        return Vec::new();
    };
    tab.layout
        .as_ref()
        .map(|layout| layout.pane_ids())
        .unwrap_or_default()
}

pub(crate) fn message(event: &Event) -> String {
    match event.kind {
        Kind::NeedsAttention => format!("{} ({}) needs attention", event.agent, event.pane_id),
        Kind::Finished => format!("{} ({}) finished", event.agent, event.pane_id),
    }
}

#[cfg(test)]
mod tests {
    use super::{notification_kind, Kind};
    use crate::detect::AgentState;

    #[test]
    fn notifications_match_herdr_transition_rules() {
        assert_eq!(
            notification_kind(AgentState::Working, AgentState::Blocked, false),
            Some(Kind::NeedsAttention)
        );
        assert_eq!(
            notification_kind(AgentState::Working, AgentState::Idle, false),
            Some(Kind::Finished)
        );
        assert_eq!(
            notification_kind(AgentState::Working, AgentState::Idle, true),
            None
        );
    }
}
