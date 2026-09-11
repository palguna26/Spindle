use crate::model::status::PaneStatus;
use crate::server::session::SessionSnapshot;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame<'_>, snapshot: &SessionSnapshot) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());
    let body = snapshot
        .spaces
        .iter()
        .flat_map(|space| {
            space.workspaces.iter().flat_map(move |workspace| {
                workspace.tabs.iter().map(move |tab| {
                    format!(
                        "{} / {} / {}{}",
                        space.name,
                        workspace.name,
                        tab.name,
                        tab.layout.as_ref().map(|_| " [split layout]").unwrap_or("")
                    )
                })
            })
        })
        .map(Line::from)
        .collect::<Vec<_>>();
    let focused = snapshot.focused_pane_id.as_deref().unwrap_or("none");
    let chrome = Line::from(vec![
        Span::styled(" Spindle ", Style::default().fg(Color::Cyan)),
        Span::raw(format!("focused: {focused}")),
        Span::raw(format!("  panes: {}", snapshot.panes.len())),
    ]);
    frame.render_widget(
        Paragraph::new(body).block(Block::default().borders(Borders::ALL).title("Session")),
        areas[0],
    );
    frame.render_widget(Paragraph::new(chrome), areas[1]);
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
    use super::status_color;
    use crate::model::status::PaneStatus;
    use ratatui::style::Color;

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
}
