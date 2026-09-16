mod layout;
mod navigation;
mod navigator;

use super::context_menu::ContextMenu;
use super::copy_mode::{CopyMode, SelectionKind};
use super::global_menu::GlobalMenu;
use super::input::{Action, Keymap};
use super::selection::TextSelection;
use super::settings::Settings;
use crate::model::status::PaneStatus;
use crate::server::session::{SessionSnapshot, WorkspaceView};
pub(crate) use layout::{
    pane_content_area, pane_content_area_with_sidebar, pane_inner_size, pane_rectangles,
    pane_sizes, sidebar_area, split_handles, PaneSize,
};
use navigation::render_tabs;
pub use navigation::{
    hit_test, hit_test_with_sidebar, hit_test_with_sidebar_scroll,
    hit_test_with_sidebar_scroll_and_sort, sidebar_scroll_max, sidebar_scroll_max_with_sort,
    sidebar_scroll_offset_from_drag_row, sidebar_scroll_offset_from_drag_row_with_sort,
    sidebar_scroll_region, sidebar_scroll_thumb_grab_offset,
    sidebar_scroll_thumb_grab_offset_with_sort, ClickTarget,
};
pub use navigator::{hit_test_navigator, render_navigator, Hit as NavigatorHit};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Debug, Clone, Copy)]
struct ThemePalette {
    accent: Color,
    focused_border: Color,
}

impl ThemePalette {
    fn from_name(name: Option<&str>) -> Self {
        match name.unwrap_or("terminal").to_ascii_lowercase().as_str() {
            "catppuccin" => Self {
                accent: Color::Rgb(203, 166, 247),
                focused_border: Color::Rgb(245, 224, 220),
            },
            "dracula" => Self {
                accent: Color::Rgb(189, 147, 249),
                focused_border: Color::Rgb(248, 248, 242),
            },
            "gruvbox" => Self {
                accent: Color::Rgb(250, 189, 47),
                focused_border: Color::Rgb(251, 73, 52),
            },
            "nord" => Self {
                accent: Color::Rgb(136, 192, 208),
                focused_border: Color::Rgb(236, 239, 244),
            },
            "tokyo-night" | "tokyonight" => Self {
                accent: Color::Rgb(122, 162, 247),
                focused_border: Color::Rgb(192, 202, 245),
            },
            _ => Self {
                accent: Color::Cyan,
                focused_border: Color::White,
            },
        }
    }
}

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
    render_with_sidebar_scroll(frame, snapshot, connected, sidebar_collapsed, 0);
}

pub fn render_with_sidebar_scroll(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    connected: bool,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
) {
    render_with_sidebar_scroll_and_cursor(
        frame,
        snapshot,
        connected,
        sidebar_collapsed,
        sidebar_scroll,
        true,
    );
}

pub fn render_with_sidebar_scroll_and_cursor(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    connected: bool,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
    show_host_cursor: bool,
) {
    render_with_sidebar_scroll_and_cursor_and_agent_sort(
        frame,
        snapshot,
        connected,
        sidebar_collapsed,
        sidebar_scroll,
        show_host_cursor,
        false,
    );
}

pub fn render_with_sidebar_scroll_and_cursor_and_agent_sort(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    connected: bool,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
    show_host_cursor: bool,
    agent_priority_sort: bool,
) {
    render_with_sidebar_scroll_and_cursor_and_agent_sort_and_navigation(
        frame,
        snapshot,
        connected,
        sidebar_collapsed,
        sidebar_scroll,
        show_host_cursor,
        agent_priority_sort,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_sidebar_scroll_and_cursor_and_agent_sort_and_navigation(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    connected: bool,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
    show_host_cursor: bool,
    agent_priority_sort: bool,
    navigation_workspace: Option<(&str, &str)>,
) {
    let theme = ThemePalette::from_name(crate::config::load().theme_name.as_deref());
    let main = layout::main_areas_with_sidebar(frame.area(), sidebar_collapsed);
    navigation::render_sidebar_with_scroll_sort_and_navigation(
        frame,
        snapshot,
        main.sidebar,
        sidebar_collapsed,
        sidebar_scroll,
        agent_priority_sort,
        navigation_workspace,
    );
    render_tabs(frame, snapshot, main.tabs);
    let panes = pane_rectangles(snapshot, main.panes)
        .into_iter()
        .filter(|pane_rect| {
            snapshot
                .panes
                .iter()
                .any(|pane| pane.pane_id == pane_rect.pane_id)
        })
        .collect::<Vec<_>>();
    if panes.is_empty() {
        let message = if active_workspace(snapshot).is_some() {
            format!(
                "No shell in this tab\n{}\nCtrl-b : then select New PowerShell pane\nCtrl-b c starts a new tab",
                active_title(snapshot)
            )
        } else {
            "No active workspace\nPress Ctrl-b c to create one".to_owned()
        };
        frame.render_widget(
            Paragraph::new(message).block(Block::default().borders(Borders::ALL).title("Session")),
            main.panes,
        );
    } else {
        let mut host_cursor = None;
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
                theme.focused_border
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
            if show_host_cursor
                && pane.cursor_visible
                && snapshot.focused_pane_id.as_deref() == Some(&pane_rect.pane_id)
            {
                let inner = Block::default().borders(Borders::ALL).inner(pane_rect.rect);
                let (col, row) = pane.cursor;
                if col < inner.width && row < inner.height {
                    host_cursor = Some((inner.x + col, inner.y + row));
                }
            }
        }
        if let Some(position) = host_cursor {
            frame.set_cursor_position(position);
        }
    }
    if let Some(popup_id) = snapshot.popup_pane_id.as_deref() {
        if let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == popup_id) {
            let popup = popup_rect(main.panes, snapshot.popup_width, snapshot.popup_height);
            frame.render_widget(Clear, popup);
            let border = Block::default()
                .borders(Borders::ALL)
                .title("Popup")
                .border_style(Style::default().fg(theme.focused_border));
            frame.render_widget(
                Paragraph::new(pane.screen.lines().map(Line::from).collect::<Vec<_>>())
                    .block(border),
                popup,
            );
            if show_host_cursor && pane.cursor_visible {
                let inner = Block::default().borders(Borders::ALL).inner(popup);
                let (col, row) = pane.cursor;
                if col < inner.width && row < inner.height {
                    frame.set_cursor_position((inner.x + col, inner.y + row));
                }
            }
        }
    }
    let focused = snapshot.focused_pane_id.as_deref().unwrap_or("none");
    let chrome = if let Some((space_id, workspace_id)) = navigation_workspace {
        let name = snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == space_id)
            .and_then(|space| {
                space
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == workspace_id)
            })
            .map(|workspace| workspace.name.as_str())
            .unwrap_or("workspace");
        Line::from(vec![
            Span::styled(
                " NAVIGATE ",
                Style::default().fg(Color::Black).bg(theme.accent),
            ),
            Span::raw(format!("  {name}  ↑/↓ choose  Enter open  Esc cancel")),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Spindle ", Style::default().fg(theme.accent)),
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
        ])
    };
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

pub(crate) fn render_copy_mode(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    mode: &CopyMode,
    sidebar_collapsed: bool,
) {
    let Some(pane) = pane_rectangles(
        snapshot,
        pane_content_area_with_sidebar(frame.area(), sidebar_collapsed),
    )
    .into_iter()
    .find(|pane| pane.pane_id == mode.pane_id) else {
        return;
    };
    let inner = Block::default().borders(Borders::ALL).inner(pane.rect);
    let selected = mode.selection.map(|selection| {
        if selection.anchor <= mode.cursor {
            (selection.anchor, mode.cursor, selection.kind)
        } else {
            (mode.cursor, selection.anchor, selection.kind)
        }
    });
    let search_width = unicode_width::UnicodeWidthStr::width(mode.search_query.as_str());
    for visible_row in 0..usize::from(inner.height) {
        let row = mode.viewport_top + visible_row;
        if row >= mode.rows.len() {
            continue;
        }
        for cell_col in 0..usize::from(inner.width) {
            let point = super::copy_mode::Point { row, col: cell_col };
            let in_selection = selected.is_some_and(|(start, end, kind)| match kind {
                SelectionKind::Line => row >= start.row && row <= end.row,
                SelectionKind::Character => point >= start && point <= end,
            });
            let is_cursor = point == mode.cursor;
            let is_match = mode.search_matches.iter().any(|found| {
                found.row == row
                    && cell_col >= found.col
                    && cell_col < found.col.saturating_add(search_width)
            });
            if in_selection || is_cursor || is_match {
                if let Some(cell) = frame
                    .buffer_mut()
                    .cell_mut((inner.x + cell_col as u16, inner.y + visible_row as u16))
                {
                    let (fg, bg) = if is_cursor {
                        (Color::Black, Color::Yellow)
                    } else if in_selection {
                        (Color::Black, Color::Cyan)
                    } else {
                        (Color::Black, Color::Green)
                    };
                    cell.set_style(Style::default().fg(fg).bg(bg));
                }
            }
        }
    }
    let mode_style = Style::default().fg(Color::Black).bg(Color::Yellow);
    let key_style = Style::default().fg(Color::Cyan);
    let base_style = Style::default().fg(Color::White);
    let hint = if mode.search_prompt {
        Line::from(vec![
            Span::styled(" COPY ", mode_style),
            Span::styled(mode.search_marker().to_string(), key_style),
            Span::raw(mode.search_query.clone()),
            Span::styled("█", key_style),
            Span::styled("  enter search  esc cancel", base_style),
        ])
    } else {
        let searching = !mode.search_query.is_empty();
        let exit = if searching || mode.selection.is_some() {
            ("esc", " clear  q exit")
        } else {
            ("q/esc", " exit")
        };
        if frame.area().width < 112 {
            let hint = Line::from(vec![
                Span::styled(" COPY ", mode_style),
                Span::styled("hjkl", key_style),
                Span::raw(" "),
                Span::styled("w/b/e W/B/E", key_style),
                Span::raw(" "),
                Span::styled("{ }", key_style),
                Span::raw(" "),
                Span::styled("/ ?", key_style),
                Span::raw(" "),
                Span::styled("n/N", key_style),
                Span::raw(" "),
                Span::styled("v/space V", key_style),
                Span::raw(" "),
                Span::styled("y/enter", key_style),
                Span::raw(" "),
                Span::styled(exit.0, key_style),
                Span::styled(exit.1, base_style),
            ]);
            frame.render_widget(Paragraph::new(hint), footer_area(frame.area()));
            return;
        }
        let match_status = mode
            .search_index
            .map(|index| format!(" {}/{}", index + 1, mode.search_matches.len()))
            .or_else(|| searching.then(|| " 0/0".to_owned()))
            .unwrap_or_default();
        Line::from(vec![
            Span::styled(" COPY ", mode_style),
            Span::raw(" "),
            Span::styled("h/j/k/l", key_style),
            Span::styled(" move  ", base_style),
            Span::styled("w/b/e W/B/E", key_style),
            Span::styled(" words  ", base_style),
            Span::styled("{ }", key_style),
            Span::styled(" paragraphs  ", base_style),
            Span::styled("/ ?", key_style),
            Span::styled(" search  ", base_style),
            Span::styled("n/N", key_style),
            Span::styled(format!(" repeat{match_status}  "), base_style),
            Span::styled("v/space", key_style),
            Span::styled(
                if mode.selection.is_some() {
                    " selecting  "
                } else {
                    " select  "
                },
                base_style,
            ),
            Span::styled("y/enter", key_style),
            Span::styled(" copy  ", base_style),
            Span::styled(exit.0, key_style),
            Span::styled(exit.1, base_style),
        ])
    };
    frame.render_widget(Paragraph::new(hint), footer_area(frame.area()));
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

pub fn render_help(frame: &mut Frame<'_>, keymap: &Keymap) {
    let area = centered_rect(72, 90, frame.area());
    let rows = [
        "Mouse",
        "Click sidebar, tabs, or panes to switch or focus.",
        "Sidebar: wheel, drag thumb, click track.",
        "Drag split borders to resize; right-click for actions.",
        "PageUp/PageDown or wheel scroll history; drag text to copy.",
        "Double-click selects a word.",
        "Ctrl-click visible web URLs to open them.",
        "Terminal apps receive mouse events when requested.",
        "Keyboard (configured prefix and bindings)",
        "",
        "Create, rename, and delete actions are in the command palette.",
        "Sidebar agent badges: W working, ! blocked, I idle, ? unknown.",
        "Click the sidebar title to switch grouped/priority agent order.",
        "Press any key or click to close.",
    ]
    .into_iter()
    .map(Line::from)
    .collect::<Vec<_>>();
    let mut rows = rows;
    rows.splice(
        10..10,
        [
            format!(
                "{}: new tab; {}: next tab",
                keymap.binding_label(Action::NewTab),
                keymap.binding_label(Action::NextTab)
            ),
            format!(
                "{}: previous tab; {}: close pane",
                keymap.binding_label(Action::PreviousTab),
                keymap.binding_label(Action::ClosePane)
            ),
            format!(
                "{}: workspace picker; {}: navigator",
                keymap.binding_label(Action::WorkspacePicker),
                keymap.binding_label(Action::SessionNavigator)
            ),
            format!(
                "{}: split vertical; {}: zoom pane",
                keymap.binding_label(Action::SplitVertical),
                keymap.binding_label(Action::ToggleZoom)
            ),
            format!(
                "{}: help; {}: palette; {}: detach",
                keymap.binding_label(Action::Help),
                keymap.binding_label(Action::CommandPalette),
                keymap.binding_label(Action::Detach)
            ),
        ]
        .into_iter()
        .map(Line::from),
    );
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

pub fn render_action_error(frame: &mut Frame<'_>, error: &str) {
    let frame_area = frame.area();
    let height = 5;
    let width = frame_area.width.min(90);
    if frame_area.height < height || width < 5 {
        return;
    }
    let area = Rect::new(
        frame_area.x + frame_area.width.saturating_sub(width) / 2,
        frame_area.bottom().saturating_sub(height + 1),
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(error).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Action failed")
                .border_style(Style::default().fg(Color::Red)),
        ),
        area,
    );
}

pub fn render_status_notice(frame: &mut Frame<'_>, message: &str) {
    let frame_area = frame.area();
    let height = 3;
    let width = frame_area.width.min(50);
    if frame_area.height < height || width < 5 {
        return;
    }
    let area = Rect::new(
        frame_area.x + frame_area.width.saturating_sub(width) / 2,
        frame_area.bottom().saturating_sub(height + 1),
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(message).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Spindle")
                .border_style(Style::default().fg(Color::Green)),
        ),
        area,
    );
}

pub fn render_notification(frame: &mut Frame<'_>, message: &str) {
    let frame_area = frame.area();
    let height = 3;
    let width = frame_area.width.min(72);
    if frame_area.height < height || width < 5 {
        return;
    }
    let area = Rect::new(
        frame_area.x + frame_area.width.saturating_sub(width) / 2,
        frame_area.bottom().saturating_sub(height + 1),
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(message).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Agent notification")
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        area,
    );
}

pub(crate) fn render_resize_mode(frame: &mut Frame<'_>) {
    let area = frame.area();
    let bar = Rect::new(0, area.height.saturating_sub(1), area.width, 1);
    frame.render_widget(
        Paragraph::new(" RESIZE  h/j/k/l or arrows · Enter/Esc exit")
            .style(Style::default().fg(Color::Black).bg(Color::Yellow)),
        bar,
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

pub(crate) fn render_global_menu(frame: &mut Frame<'_>, menu: &GlobalMenu) {
    let sidebar = sidebar_area(frame.area(), false);
    let rect = GlobalMenu::rect(sidebar, frame.area());
    let lines = GlobalMenu::items()
        .iter()
        .enumerate()
        .map(|(index, (label, _))| {
            Line::from(Span::styled(
                format!(" {label}"),
                if index == menu.selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White).bg(Color::DarkGray)
                },
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Menu")
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        rect,
    );
}

pub(super) fn render_settings(frame: &mut Frame<'_>, settings: &Settings) {
    let area = Settings::rect(frame.area());
    let sections = super::settings::Section::ALL
        .iter()
        .map(|section| {
            if *section == settings.section {
                Span::styled(
                    format!("[ {} ] ", section.title()),
                    Style::default().fg(Color::Black).bg(Color::Cyan),
                )
            } else {
                Span::styled(format!("  {}   ", section.title()), Style::default())
            }
        })
        .collect::<Vec<_>>();
    let lines = std::iter::once(Line::from(sections))
        .chain(std::iter::once(Line::from("")))
        .chain(
            settings
                .choices()
                .iter()
                .enumerate()
                .map(|(index, choice)| {
                    let marker = if index == settings.selected {
                        "> "
                    } else {
                        "  "
                    };
                    let style = if index == settings.selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default()
                    };
                    Line::from(Span::styled(format!("{marker}{choice}"), style))
                }),
        )
        .chain(std::iter::once(Line::from("")))
        .chain(std::iter::once(Line::from(
            "←/→ section  ↑/↓ choice  Enter save  Esc close",
        )))
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Settings")
                .border_style(Style::default().fg(Color::Cyan)),
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

pub(crate) fn popup_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = if width == 0 {
        (area.width / 2).max(4)
    } else {
        width
    }
    .min(area.width);
    let height = if height == 0 {
        (area.height / 2).max(4)
    } else {
        height
    }
    .min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn active_workspace(snapshot: &SessionSnapshot) -> Option<&WorkspaceView> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace_id = space.active_workspace_id.as_ref()?;
    space
        .workspaces
        .iter()
        .find(|workspace| &workspace.workspace_id == workspace_id)
}

fn active_title(snapshot: &SessionSnapshot) -> String {
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    else {
        return "No active session".into();
    };
    let Some(workspace) = active_workspace(snapshot) else {
        return "No active session".into();
    };
    workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)
        .map(|tab| format!("{} / {} / {}", space.name, workspace.name, tab.name))
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
                let state = format!(" {}", pane.agent_display_state().label());
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
    use super::super::copy_mode::{CopyMode, CopySelection, Point, SelectionKind};
    use super::super::input::Keymap;
    use super::super::selection::TextSelection;
    use super::{
        active_title, pane_content_area, pane_rectangles, pane_title, pane_title_text, render,
        render_action_error, render_help, render_palette, render_selection, render_startup_error,
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
    fn closed_last_workspace_shows_a_way_to_start_again() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.spaces[0].workspaces.clear();
        snapshot.spaces[0].active_workspace_id = None;
        snapshot.focused_pane_id = None;
        terminal.draw(|frame| render(frame, &snapshot)).unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("No active workspace"));
        assert!(content.contains("Ctrl-b c"));
    }

    #[test]
    fn empty_active_tab_explains_how_to_start_a_shell() {
        let backend = TestBackend::new(80, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        let snapshot = Session::default().snapshot().clone();
        terminal.draw(|frame| render(frame, &snapshot)).unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("No shell in this tab"));
        assert!(content.contains("New PowerShell pane"));
        assert!(content.contains("Ctrl-b c starts a new tab"));
    }

    #[test]
    fn stale_layout_renders_the_empty_tab_message() {
        let backend = TestBackend::new(80, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        let session = Session::default();
        let mut snapshot = session.snapshot().clone();
        snapshot.spaces[0].workspaces[0].tabs[0].layout = Some(LayoutNode::pane("pane-missing"));
        terminal.draw(|frame| render(frame, &snapshot)).unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("No shell in this tab"));
        assert!(content.contains("New PowerShell pane"));
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
        let keymap = Keymap::default();
        terminal.draw(|frame| render_help(frame, &keymap)).unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Spindle help"));
        assert!(content.contains("right-click for actions"));
        assert!(content.contains("drag thumb"));
        assert!(content.contains("Ctrl-click visible web URLs"));
        assert!(content.contains("palette"), "help contents: {content}");
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
    fn action_error_toast_shows_the_failure_without_replacing_the_session() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_action_error(frame, "split pane failed: pane is missing"))
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Action failed"));
        assert!(content.contains("split pane failed"));
        assert!(content.contains("pane is missing"));
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
                    is_linked_worktree: false,
                    worktree_group: None,
                    tabs: vec![TabView {
                        tab_id: "tab-1".into(),
                        name: "Main".into(),
                        layout: Some(LayoutNode::pane("pane-1")),
                        focused_pane_id: Some("pane-1".into()),
                        zoomed: false,
                    }],
                    active_tab_id: "tab-1".into(),
                }],
                active_workspace_id: Some("workspace-1".into()),
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
                agent_done: false,
                status: PaneStatus::Running,
                scrollback_bytes: 0,
                scrollback: Vec::new(),
                screen: "hello".into(),
                cursor: (0, 0),
                cursor_visible: true,
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
                hyperlinks: Vec::new(),
            }],
            focused_pane_id: Some("pane-1".into()),
            popup_pane_id: None,
            popup_width: 0,
            popup_height: 0,
            overlay_pane_id: None,
            overlay_previous_focus: None,
            overlay_previous_zoomed: false,
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
        terminal
            .backend_mut()
            .assert_cursor_position(ratatui::layout::Position {
                x: inner.x,
                y: inner.y,
            });
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((inner.x, inner.y)).unwrap().bg, Color::Cyan);
        assert_eq!(
            buffer.cell((inner.x + 3, inner.y)).unwrap().bg,
            Color::Reset
        );

        let mut copy_mode =
            CopyMode::new("pane-1".into(), b"hello", 24, 80, inner.height, 0, (1, 0));
        copy_mode.cursor = Point { row: 0, col: 1 };
        copy_mode.selection = Some(CopySelection {
            anchor: Point { row: 0, col: 0 },
            kind: SelectionKind::Line,
        });
        terminal
            .draw(|frame| {
                render_with_connection(frame, &snapshot, true);
                super::render_copy_mode(frame, &snapshot, &copy_mode, false);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer.cell((inner.x + 1, inner.y)).unwrap().bg,
            Color::Yellow
        );
        assert_eq!(buffer.cell((inner.x + 8, inner.y)).unwrap().bg, Color::Cyan);
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(content.contains("w/b/e W/B/E"));
        assert!(content.contains("n/N"));
        assert!(content.contains("v/space V"));
        assert!(content.contains("y/enter"));
        assert!(content.contains("esc clear  q exit"));
    }

    #[test]
    fn pane_title_shows_terminal_title_and_alt_mode() {
        let mut pane = PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: None,
            agent: Some(crate::detect::AgentKind::Codex),
            agent_state: Some(crate::detect::AgentState::Working),
            agent_done: false,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: true,
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
            hyperlinks: Vec::new(),
        };
        assert!(pane_title_text(&pane).contains("Editor"));
        assert!(pane_title_text(&pane).contains("[Codex working]"));
        assert!(pane_title_text(&pane).contains("running"));
        assert!(pane_title_text(&pane).contains("[alt]"));
        assert_eq!(
            pane_title(&pane).spans[0].style.fg,
            Some(Color::Rgb(255, 165, 0))
        );
        pane.agent_state = Some(crate::detect::AgentState::Idle);
        pane.agent_done = true;
        assert!(pane_title_text(&pane).contains("[Codex done]"));
        let wire = serde_json::to_value(&pane).unwrap();
        assert_eq!(wire["agent_state"], "idle");
        assert_eq!(wire["agent_done"], true);
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
            agent_done: false,
            status: PaneStatus::Completed { exit_code: 0 },
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: true,
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
            hyperlinks: Vec::new(),
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
