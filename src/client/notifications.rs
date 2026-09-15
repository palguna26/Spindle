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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueuedNotification {
    pub message: String,
    pub visible_at: Instant,
    pub expires_at: Instant,
}

pub(crate) fn deliver(
    queue: &mut VecDeque<QueuedNotification>,
    events: impl IntoIterator<Item = Event>,
    delivery: crate::config::NotificationDelivery,
    delay_seconds: u64,
    now: Instant,
) {
    match delivery {
        crate::config::NotificationDelivery::Off => {}
        crate::config::NotificationDelivery::Herdr => {
            enqueue_with_delay(queue, events, now, delay_seconds)
        }
        crate::config::NotificationDelivery::System => {
            for event in events {
                let notification = message(&event);
                let (title, body) = crate::terminal_notify::split_message(&notification);
                let _ = crate::platform::show_desktop_notification(title, body);
            }
        }
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

pub(crate) fn enqueue_with_delay(
    queue: &mut VecDeque<QueuedNotification>,
    events: impl IntoIterator<Item = Event>,
    now: Instant,
    delay_seconds: u64,
) {
    for event in events {
        let position = queue.len() as u32 + 1;
        let visible_at = now
            + Duration::from_secs(delay_seconds.min(3600))
            + Duration::from_secs(5).saturating_mul(position.saturating_sub(1));
        queue.push_back(QueuedNotification {
            message: message(&event),
            visible_at,
            expires_at: visible_at + Duration::from_secs(5),
        });
    }
}

pub(crate) fn expire(queue: &mut VecDeque<QueuedNotification>, now: Instant) {
    while queue
        .front()
        .is_some_and(|notification| now >= notification.expires_at)
    {
        queue.pop_front();
    }
}

pub(crate) fn visible_message(queue: &VecDeque<QueuedNotification>, now: Instant) -> Option<&str> {
    queue
        .front()
        .filter(|notification| now >= notification.visible_at)
        .map(|notification| notification.message.as_str())
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
    use super::{enqueue_with_delay, expire, notification_kind, visible_message, Event, Kind};
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
        enqueue_with_delay(
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
            0,
        );
        assert_eq!(queue.len(), 2);
        assert!(queue.front().unwrap().message.contains("codex"));
        expire(&mut queue, now + Duration::from_secs(6));
        assert_eq!(queue.len(), 1);
        assert!(queue.front().unwrap().message.contains("claude"));
        expire(&mut queue, now + Duration::from_secs(12));
        assert!(queue.is_empty());
    }

    #[test]
    fn delayed_notifications_stay_hidden_until_their_deadline() {
        let now = Instant::now();
        let mut queue = std::collections::VecDeque::new();
        enqueue_with_delay(
            &mut queue,
            [Event {
                pane_id: "pane-1".into(),
                agent: "codex".into(),
                kind: Kind::Finished,
            }],
            now,
            2,
        );
        assert_eq!(visible_message(&queue, now + Duration::from_secs(1)), None);
        assert!(visible_message(&queue, now + Duration::from_secs(7)).is_some());
    }
}
