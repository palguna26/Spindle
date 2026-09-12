mod layout;
mod navigation;

use super::context_menu::ContextMenu;
use super::selection::TextSelection;
use crate::model::status::PaneStatus;
use crate::server::session::SessionSnapshot;
pub(crate) use layout::{
    pane_content_area, pane_content_area_with_sidebar, pane_inner_size, pane_rectangles,
    pane_sizes, split_handles, PaneSize,
};
use navigation::render_tabs;
pub use navigation::{hit_test, hit_test_with_sidebar, ClickTarget};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

pub fn render(frame: &mut Frame<'_>, snapshot: &SessionSnapshot) {
    render_with_connection(frame, snapshot, true);
}

pub fn render_with_connection(frame: &mut Frame<'_>, snapshot: &SessionSnapshot, connected: bool) {
    render_with_sidebar(frame, snapshot, connected, false);
}

pub fn render_with_sidebar(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    connected: bool,
    sidebar_collapsed: bool,
) {
    let main = layout::main_areas_with_sidebar(frame.area(), sidebar_collapsed);
    navigation::render_sidebar_with_collapsed(frame, snapshot, main.sidebar, sidebar_collapsed);
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

#[cfg(test)]
pub(crate) fn render_selection(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    selection: &TextSelection,
) {
    render_selection_with_sidebar(frame, snapshot, selection, false);
}

pub(crate) fn render_selection_with_sidebar(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    selection: &TextSelection,
    sidebar_collapsed: bool,
) {
    if !selection.has_range() {
        return;
    }
    let Some(pane) = pane_rectangles(
        snapshot,
        pane_content_area_with_sidebar(frame.area(), sidebar_collapsed),
    )
    .into_iter()
    .find(|pane| pane.pane_id == selection.pane_id) else {
        return;
    };
    let inner = Block::default().borders(Borders::ALL).inner(pane.rect);
    let ((start_row, start_col), (end_row, end_col)) = selection.ordered();
    for row in start_row..=end_row {
        if row >= inner.height {
            break;
        }
        let first_col = if row == start_row { start_col } else { 0 };
        let last_col = if row == end_row {
            end_col
        } else {
            inner.width.saturating_sub(1)
        };
        for col in first_col..=last_col {
            if col >= inner.width {
                break;
            }
            if let Some(cell) = frame.buffer_mut().cell_mut((inner.x + col, inner.y + row)) {
                cell.set_style(Style::default().fg(Color::Black).bg(Color::Cyan));
            }
        }
    }
}

fn footer_area(area: Rect) -> Rect {
    Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1)
}

pub fn render_palette(frame: &mut Frame<'_>, selected: usize) {
    let area = centered_rect(60, 70, frame.area());
    let commands = crate::client::palette::Command::ALL;
    let visible_rows = usize::from(area.height.saturating_sub(2));
    let start = selected.saturating_add(1).saturating_sub(visible_rows);
    let rows = commands
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_rows)
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

pub fn render_help(frame: &mut Frame<'_>) {
    let area = centered_rect(72, 90, frame.area());
    let rows = [
        "Mouse",
        "Click sidebar, tabs, or panes to switch or focus.",
        "Drag split borders to resize; right-click for actions.",
        "PageUp/PageDown or wheel scroll history; drag text to copy.",
        "Double-click selects a word.",
        "Terminal apps receive mouse events when requested.",
        "",
        "Keyboard (press Ctrl-b, then the key)",
        "c: new tab; n / p: next / previous tab",
        "x / Shift+x: close pane / tab; s / r: stop / restart",
        "b: toggle compact sidebar",
        "h/j/k/l or arrows: focus direction; o / O: cycle panes",
        "v / -: split vertical / horizontal; z: zoom pane",
        "?: help; colon: palette; q / d: detach",
        "",
        "Create, rename, and delete actions are in the command palette.",
        "Sidebar agent badges: W working, ! blocked, I idle, ? unknown.",
        "Press any key or click to close.",
    ]
    .into_iter()
    .map(Line::from)
    .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(rows).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Spindle help — mouse and keyboard"),
        ),
        area,
    );
}

pub fn render_startup_error(frame: &mut Frame<'_>, error: &str) {
    let area = centered_rect(72, 42, frame.area());
    let content = vec![
        Line::from("Spindle is connected, but could not start the shell."),
        Line::from(error),
        Line::from("Press Enter or r to retry. Press Esc or q to detach."),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(content).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Shell start failed"),
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

pub(crate) fn render_context_menu(frame: &mut Frame<'_>, menu: &ContextMenu) {
    let area = menu.rect(frame.area());
    if area.width < 2 || area.height < 2 {
        return;
    }
    let rows = menu
        .visible_range(frame.area())
        .map(|index| {
            let (label, _) = menu.items()[index];
            let marker = if index == menu.selected { "> " } else { "  " };
            let style = if index == menu.selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(format!("{marker}{label}"), style))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(rows).block(Block::default().borders(Borders::ALL)),
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
        "{} {} {}{} — {}{}",
        pane.status.indicator(),
        pane.pane_id,
        label,
        pane.agent
            .map(|agent| {
                let state = pane
                    .agent_state
                    .map(|state| format!(" {}", state.label()))
                    .unwrap_or_default();
                format!(" [{}{state}]", agent.label())
            })
            .unwrap_or_default(),
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
    use super::super::selection::TextSelection;
    use super::{
        active_title, pane_content_area, pane_rectangles, pane_title, pane_title_text, render,
        render_help, render_palette, render_selection, render_startup_error,
        render_with_connection, status_color,
    };
    use crate::model::layout::LayoutNode;
    use crate::model::status::PaneStatus;
    use crate::server::session::{
        PaneView, Session, SessionSnapshot, SpaceView, TabView, WorkspaceView,
    };
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::{Block, Borders};
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
    fn help_overlay_explains_mouse_and_keyboard_controls() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(render_help).unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Spindle help"));
        assert!(content.contains("right-click for actions"));
        assert!(content.contains("palette"));
    }

    #[test]
    fn command_palette_scrolls_the_selected_command_into_view() {
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_palette(frame, crate::client::palette::Command::ALL.len() - 1))
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Toggle right-click passthrough"));
    }

    #[test]
    fn startup_error_overlay_shows_recovery_actions() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_startup_error(frame, "powershell.exe was not found"))
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Shell start failed"));
        assert!(content.contains("powershell.exe was not found"));
        assert!(content.contains("Press Enter or r to retry"));
    }

    #[test]
    fn text_selection_highlights_the_selected_pane_cells() {
        let snapshot = SessionSnapshot {
            version: 1,
            spaces: vec![SpaceView {
                space_id: "space-1".into(),
                name: "Default".into(),
                workspaces: vec![WorkspaceView {
                    workspace_id: "workspace-1".into(),
                    name: "Project".into(),
                    repository_path: None,
                    branch: None,
                    tabs: vec![TabView {
                        tab_id: "tab-1".into(),
                        name: "Main".into(),
                        layout: Some(LayoutNode::pane("pane-1")),
                        focused_pane_id: Some("pane-1".into()),
                        zoomed: false,
                    }],
                    active_tab_id: "tab-1".into(),
                }],
                active_workspace_id: "workspace-1".into(),
            }],
            active_space_id: "space-1".into(),
            panes: vec![PaneView {
                pane_id: "pane-1".into(),
                command: "powershell.exe".into(),
                args: Vec::new(),
                cwd: "C:/".into(),
                cols: 80,
                rows: 24,
                label: None,
                agent: None,
                agent_state: None,
                status: PaneStatus::Running,
                scrollback_bytes: 0,
                scrollback: Vec::new(),
                screen: "hello".into(),
                cursor: (0, 0),
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
            }],
            focused_pane_id: Some("pane-1".into()),
            event_sequence: 0,
        };
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 24);
        let pane_rect = pane_rectangles(&snapshot, pane_content_area(area))[0].rect;
        let inner = Block::default().borders(Borders::ALL).inner(pane_rect);
        let mut selection = TextSelection::new("pane-1".into(), inner, inner.x, inner.y);
        selection.drag(inner.x + 2, inner.y);

        terminal
            .draw(|frame| {
                render_with_connection(frame, &snapshot, true);
                render_selection(frame, &snapshot, &selection);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((inner.x, inner.y)).unwrap().bg, Color::Cyan);
        assert_eq!(
            buffer.cell((inner.x + 3, inner.y)).unwrap().bg,
            Color::Reset
        );
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
            agent: Some(crate::detect::AgentKind::Codex),
            agent_state: Some(crate::detect::AgentState::Working),
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            title: "Editor".into(),
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
        };
        assert!(pane_title_text(&pane).contains("Editor"));
        assert!(pane_title_text(&pane).contains("[Codex working]"));
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
            agent: None,
            agent_state: None,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
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
