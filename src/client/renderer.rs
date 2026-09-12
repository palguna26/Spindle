mod layout;
mod navigation;

use crate::model::status::PaneStatus;
use crate::server::session::SessionSnapshot;
use layout::{main_areas, pane_rectangles};
pub(crate) use layout::{pane_content_area, pane_inner_size, pane_sizes, split_handles, PaneSize};
pub use navigation::{hit_test, ClickTarget};
use navigation::{render_sidebar, render_tabs};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame<'_>, snapshot: &SessionSnapshot) {
    render_with_connection(frame, snapshot, true);
}

pub fn render_with_connection(frame: &mut Frame<'_>, snapshot: &SessionSnapshot, connected: bool) {
    let main = main_areas(frame.area());
    render_sidebar(frame, snapshot, main.sidebar);
    render_tabs(frame, snapshot, main.tabs);
    let panes = pane_rectangles(snapshot, main.panes);
    if panes.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(active_title(snapshot)))
                .block(Block::default().borders(Borders::ALL).title("Session")),
            main.panes,
        );
    } else {
        for pane_rect in panes {
            let Some(pane) = snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == pane_rect.pane_id)
            else {
                continue;
            };
            let title = pane_title(pane);
            let lines = pane.screen.lines().map(Line::from).collect::<Vec<_>>();
            let border_color = if snapshot.focused_pane_id.as_deref() == Some(&pane_rect.pane_id) {
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
                pane_rect.rect,
            );
        }
    }
    let focused = snapshot.focused_pane_id.as_deref().unwrap_or("none");
    let chrome = Line::from(vec![
        Span::styled(" Spindle ", Style::default().fg(Color::Cyan)),
        Span::styled(
            if connected {
                "connected"
            } else {
                "connection lost — retrying"
            },
            Style::default().fg(if connected { Color::Green } else { Color::Red }),
        ),
        Span::raw("  "),
        Span::raw(format!("focused: {focused}")),
        Span::raw(format!("  panes: {}", snapshot.panes.len())),
        Span::raw(format!("  {}", active_title(snapshot))),
    ]);
    frame.render_widget(Paragraph::new(chrome), footer_area(frame.area()));
}

fn footer_area(area: Rect) -> Rect {
    Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1)
}

pub fn render_palette(frame: &mut Frame<'_>, selected: usize) {
    let area = centered_rect(60, 70, frame.area());
    let rows = crate::client::palette::Command::ALL
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let marker = if index == selected { "> " } else { "  " };
            Line::from(format!("{marker}{}", command.label()))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(rows).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Command palette (↑/↓, Enter, Esc)"),
        ),
        area,
    );
}

pub fn render_prompt(frame: &mut Frame<'_>, title: &str, input: &str) {
    let area = centered_rect(60, 25, frame.area());
    frame.render_widget(
        Paragraph::new(input.to_string()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{title} (Enter to save, Esc to cancel)")),
        ),
        area,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = (area.height * height / 100).max(1).min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
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
                .and_then(|workspace| {
                    workspace
                        .tabs
                        .iter()
                        .find(|tab| tab.tab_id == workspace.active_tab_id)
                        .map(|tab| format!("{} / {} / {}", space.name, workspace.name, tab.name))
                })
        })
        .unwrap_or_else(|| "No active session".into())
}

fn pane_title(pane: &crate::server::session::PaneView) -> Line<'static> {
    let indicator = pane.status.indicator().to_string();
    let title = pane_title_text(pane);
    Line::from(vec![
        Span::styled(indicator, Style::default().fg(status_color(&pane.status))),
        Span::raw(title[pane.status.indicator().len_utf8()..].to_string()),
    ])
}

fn pane_title_text(pane: &crate::server::session::PaneView) -> String {
    let label = pane
        .label
        .as_deref()
        .filter(|label| !label.is_empty())
        .or_else(|| (!pane.title.is_empty()).then_some(pane.title.as_str()))
        .unwrap_or(pane.command.as_str());
    let alternate = if pane.alternate_screen { " [alt]" } else { "" };
    format!(
        "{} {} {} — {}{}",
        pane.status.indicator(),
        pane.pane_id,
        label,
        status_detail(&pane.status),
        alternate
    )
}

fn status_detail(status: &PaneStatus) -> String {
    match status {
        PaneStatus::Running => "running".into(),
        PaneStatus::Completed { exit_code } => format!("exit {exit_code}"),
        PaneStatus::Halted { reason } | PaneStatus::Interrupted { reason } => reason.clone(),
    }
}

pub fn status_color(status: &PaneStatus) -> Color {
    match status {
        PaneStatus::Running => Color::Rgb(255, 165, 0),
        PaneStatus::Completed { .. } => Color::Green,
        PaneStatus::Halted { .. } | PaneStatus::Interrupted { .. } => Color::Red,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        active_title, pane_title, pane_title_text, render, render_with_connection, status_color,
    };
    use crate::model::status::PaneStatus;
    use crate::server::session::{PaneView, Session};
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    #[test]
    fn status_colors_follow_process_lifecycle() {
        assert_eq!(status_color(&PaneStatus::Running), Color::Rgb(255, 165, 0));
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
        assert!(content.contains("Spaces"));
        assert!(content.contains("Main"));
    }

    #[test]
    fn disconnected_state_is_visible_in_status_chrome() {
        let backend = TestBackend::new(60, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let session = Session::default();
        terminal
            .draw(|frame| render_with_connection(frame, session.snapshot(), false))
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("connection lost"));
    }

    #[test]
    fn pane_title_shows_terminal_title_and_alt_mode() {
        let pane = PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: None,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            title: "Editor".into(),
            alternate_screen: true,
        };
        assert!(pane_title_text(&pane).contains("Editor"));
        assert!(pane_title_text(&pane).contains("running"));
        assert!(pane_title_text(&pane).contains("[alt]"));
        assert_eq!(
            pane_title(&pane).spans[0].style.fg,
            Some(Color::Rgb(255, 165, 0))
        );
    }

    #[test]
    fn pane_title_shows_failure_reason_and_exit_code() {
        let mut pane = PaneView {
            pane_id: "pane-1".into(),
            command: "cmd.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: None,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            title: String::new(),
            alternate_screen: false,
        };
        assert!(pane_title_text(&pane).contains("exit 0"));
        pane.status = PaneStatus::Halted {
            reason: "process failed".into(),
        };
        assert!(pane_title_text(&pane).contains("process failed"));
    }

    #[test]
    fn active_context_includes_the_tab_name() {
        let mut session = Session::default();
        session.create_tab("Logs".into()).unwrap();
        assert_eq!(
            active_title(session.snapshot()),
            "Default / Current project / Logs"
        );
    }
}
