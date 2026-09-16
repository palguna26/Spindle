use crate::server::session::SessionSnapshot;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub(super) fn render_header(frame: &mut Frame<'_>, area: Rect, snapshot: &SessionSnapshot) {
    if area.is_empty() {
        return;
    }
    let accent = super::ThemePalette::from_config(&crate::config::load()).accent;
    let switch = super::layout::mobile_switch_rect(area);
    let title_width = switch.x.saturating_sub(area.x);
    let title = mobile_title(snapshot);
    let title = title
        .chars()
        .take(usize::from(title_width.saturating_sub(1)))
        .collect::<String>();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().fg(accent)),
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        Rect::new(area.x, area.y, title_width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " switch",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))),
        switch,
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(format!(" {}", agent_summary(snapshot))),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}

fn mobile_title(snapshot: &SessionSnapshot) -> String {
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    else {
        return "no workspace".into();
    };
    let Some(workspace_id) = space.active_workspace_id.as_deref() else {
        return "no workspace".into();
    };
    let Some(workspace) = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
    else {
        return "no workspace".into();
    };
    let active = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == workspace.active_tab_id)
        .unwrap_or(0);
    let tab_name = workspace
        .tabs
        .get(active)
        .map(|tab| tab.name.as_str())
        .unwrap_or("1");
    if workspace.tabs.len() <= 1 {
        format!("{}  tab {}", workspace.name, tab_name)
    } else {
        format!(
            "{}  tab {} · {}/{}",
            workspace.name,
            tab_name,
            active + 1,
            workspace.tabs.len()
        )
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
    use super::{agent_summary, mobile_title};
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
    fn mobile_title_matches_herdr_compact_workspace_and_tab_format() {
        let mut session = Session::default();
        session.create_tab("Logs".into()).unwrap();
        assert_eq!(
            mobile_title(session.snapshot()),
            "Current project  tab Logs · 2/2"
        );
    }

    #[test]
    fn mobile_title_handles_an_empty_active_workspace() {
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.spaces[0].workspaces.clear();
        snapshot.spaces[0].active_workspace_id = None;
        assert_eq!(mobile_title(&snapshot), "no workspace");
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
