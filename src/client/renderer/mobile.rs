use crate::server::session::SessionSnapshot;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub(super) fn render_header(frame: &mut Frame<'_>, area: Rect, snapshot: &SessionSnapshot) {
    if area.is_empty() {
        return;
    }
    let switch = super::layout::mobile_switch_rect(area);
    let title_width = switch.x.saturating_sub(area.x);
    let title = super::active_title(snapshot);
    let title = title
        .chars()
        .take(usize::from(title_width.saturating_sub(1)))
        .collect::<String>();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().fg(Color::Cyan)),
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        Rect::new(area.x, area.y, title_width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " switch",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))),
        switch,
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(format!(" {}  Ctrl-b g navigator", agent_summary(snapshot))),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}

fn agent_summary(snapshot: &SessionSnapshot) -> &'static str {
    let mut blocked = 0;
    let mut done = 0;
    let mut working = 0;
    let mut idle = 0;
    let mut agents = 0;
    for pane in &snapshot.panes {
        if pane.agent.is_none() {
            continue;
        }
        agents += 1;
        if pane.agent_done {
            done += 1;
            continue;
        }
        match pane
            .agent_state
            .unwrap_or(crate::detect::AgentState::Unknown)
        {
            crate::detect::AgentState::Blocked => blocked += 1,
            crate::detect::AgentState::Working => working += 1,
            crate::detect::AgentState::Idle | crate::detect::AgentState::Unknown => idle += 1,
        }
    }
    if agents == 0 {
        "no agents"
    } else if blocked > 0 {
        "attention needed"
    } else if working > 0 {
        "agents working"
    } else if done > 0 {
        "agents done"
    } else if idle > 0 {
        "all idle"
    } else {
        "no agents"
    }
}

#[cfg(test)]
mod tests {
    use super::agent_summary;
    use crate::server::session::{Session, SessionSnapshot};

    fn snapshot_with_agent(state: &str, done: bool) -> SessionSnapshot {
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "agent-pane",
                "command": "codex",
                "args": [],
                "cwd": "C:/work",
                "status": "Running",
                "scrollback_bytes": 0,
                "agent": "codex",
                "agent_state": state,
                "agent_done": done
            }))
            .unwrap(),
        );
        snapshot
    }

    #[test]
    fn mobile_summary_prioritizes_attention_and_completion() {
        assert_eq!(
            agent_summary(&snapshot_with_agent("blocked", false)),
            "attention needed"
        );
        assert_eq!(
            agent_summary(&snapshot_with_agent("idle", true)),
            "agents done"
        );
        assert_eq!(
            agent_summary(&snapshot_with_agent("working", false)),
            "agents working"
        );
    }
}
