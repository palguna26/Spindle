use crate::model::layout::{Direction as SplitDirection, LayoutNode};
use crate::model::status::PaneStatus;
use crate::server::session::SessionSnapshot;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame<'_>, snapshot: &SessionSnapshot) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());
    if let Some(layout) = active_layout(snapshot) {
        render_layout(frame, layout, snapshot, areas[0]);
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(active_title(snapshot)))
                .block(Block::default().borders(Borders::ALL).title("Session")),
            areas[0],
        );
    }
    let focused = snapshot.focused_pane_id.as_deref().unwrap_or("none");
    let chrome = Line::from(vec![
        Span::styled(" Spindle ", Style::default().fg(Color::Cyan)),
        Span::raw(format!("focused: {focused}")),
        Span::raw(format!("  panes: {}", snapshot.panes.len())),
    ]);
    frame.render_widget(Paragraph::new(chrome), areas[1]);
}

fn active_layout(snapshot: &SessionSnapshot) -> Option<&LayoutNode> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == space.active_workspace_id)?;
    let tab = workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)?;
    tab.layout.as_ref()
}

fn active_title(snapshot: &SessionSnapshot) -> String {
    snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .and_then(|space| {
            space
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == space.active_workspace_id)
                .map(|workspace| format!("{} / {}", space.name, workspace.name))
        })
        .unwrap_or_else(|| "No active session".into())
}

fn render_layout(frame: &mut Frame<'_>, node: &LayoutNode, snapshot: &SessionSnapshot, area: Rect) {
    match node {
        LayoutNode::Pane { pane_id } => {
            let Some(pane) = snapshot.panes.iter().find(|pane| &pane.pane_id == pane_id) else {
                return;
            };
            let title = format!(
                "{} {} {}",
                pane.status.indicator(),
                pane.pane_id,
                pane.command
            );
            let lines = pane.screen.lines().map(Line::from).collect::<Vec<_>>();
            let border_color = if snapshot.focused_pane_id.as_deref() == Some(pane_id) {
                Color::White
            } else {
                status_color(&pane.status)
            };
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .border_style(Style::default().fg(border_color)),
                ),
                area,
            );
        }
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let percentage = (*ratio * 100.0).round() as u16;
            let constraints = [
                Constraint::Percentage(percentage),
                Constraint::Percentage(100 - percentage),
            ];
            let layout_direction = match direction {
                SplitDirection::Horizontal => Direction::Horizontal,
                SplitDirection::Vertical => Direction::Vertical,
            };
            let areas = Layout::default()
                .direction(layout_direction)
                .constraints(constraints)
                .split(area);
            render_layout(frame, first, snapshot, areas[0]);
            render_layout(frame, second, snapshot, areas[1]);
        }
    }
}

pub fn status_color(status: &PaneStatus) -> Color {
    match status {
        PaneStatus::Running => Color::Yellow,
        PaneStatus::Completed { .. } => Color::Green,
        PaneStatus::Halted { .. } | PaneStatus::Interrupted { .. } => Color::Red,
    }
}

#[cfg(test)]
mod tests {
    use super::{render, status_color};
    use crate::model::status::PaneStatus;
    use crate::server::session::Session;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    #[test]
    fn status_colors_follow_process_lifecycle() {
        assert_eq!(status_color(&PaneStatus::Running), Color::Yellow);
        assert_eq!(
            status_color(&PaneStatus::Completed { exit_code: 0 }),
            Color::Green
        );
        assert_eq!(
            status_color(&PaneStatus::Halted {
                reason: "failed".into()
            }),
            Color::Red
        );
    }

    #[test]
    fn empty_session_renders_its_active_context() {
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let session = Session::default();
        terminal
            .draw(|frame| render(frame, session.snapshot()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("Default"));
        assert!(content.contains("Current project"));
    }
}
