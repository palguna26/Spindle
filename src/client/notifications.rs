use crate::detect::AgentState;
use crate::server::session::SessionSnapshot;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

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

pub(crate) fn deliver(
    queue: &mut VecDeque<(String, Instant)>,
    events: impl IntoIterator<Item = Event>,
    delivery: crate::config::NotificationDelivery,
    now: Instant,
) {
    match delivery {
        crate::config::NotificationDelivery::Off => {}
        crate::config::NotificationDelivery::Herdr
        | crate::config::NotificationDelivery::System => enqueue(queue, events, now),
        crate::config::NotificationDelivery::Terminal => {
            for event in events {
                let message = message(&event);
                let _ = crate::terminal_notify::show(&message);
            }
        }
    }
}

pub(crate) fn observe_all(previous: &SessionSnapshot, current: &SessionSnapshot) -> Vec<Event> {
    current
        .panes
        .iter()
        .filter_map(|pane| {
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
        .collect()
}

pub(crate) fn enqueue(
    queue: &mut VecDeque<(String, Instant)>,
    events: impl IntoIterator<Item = Event>,
    now: Instant,
) {
    for event in events {
        let position = queue.len() as u32 + 1;
        queue.push_back((
            message(&event),
            now + Duration::from_secs(5).saturating_mul(position),
        ));
    }
}

pub(crate) fn expire(queue: &mut VecDeque<(String, Instant)>, now: Instant) {
    while queue.front().is_some_and(|(_, deadline)| now >= *deadline) {
        queue.pop_front();
    }
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
    use super::{enqueue, expire, notification_kind, Event, Kind};
    use crate::detect::AgentState;
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

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

    #[test]
    fn queued_notifications_keep_order_and_expire_from_the_front() {
        let now = Instant::now();
        let mut queue = VecDeque::new();
        enqueue(
            &mut queue,
            [
                Event {
                    pane_id: "pane-1".into(),
                    agent: "codex".into(),
                    kind: Kind::NeedsAttention,
                },
                Event {
                    pane_id: "pane-2".into(),
                    agent: "claude".into(),
                    kind: Kind::Finished,
                },
            ],
            now,
        );
        assert_eq!(queue.len(), 2);
        assert!(queue.front().unwrap().0.contains("codex"));
        expire(&mut queue, now + Duration::from_secs(5));
        assert_eq!(queue.len(), 1);
        assert!(queue.front().unwrap().0.contains("claude"));
        expire(&mut queue, now + Duration::from_secs(10));
        assert!(queue.is_empty());
    }
}
