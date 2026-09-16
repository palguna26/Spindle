mod layout;
mod mobile;
mod navigation;
mod navigator;

use super::context_menu::ContextMenu;
use super::copy_mode::{CopyMode, SelectionKind};
use super::global_menu::GlobalMenu;
use super::input::{Action, Keymap};
use super::scrollbar;
use super::scrollbar::max_offset_for_pane;
use super::selection::TextSelection;
use super::settings::Settings;
use crate::model::status::PaneStatus;
use crate::server::session::{SessionSnapshot, WorkspaceView};
pub(crate) use layout::{
    pane_borders_for_rect, pane_content_area, pane_content_area_for_snapshot, pane_inner_area,
    pane_inner_size_with_options, pane_rectangles, pane_sizes, sidebar_area, split_handles,
    PaneRect, PaneSize,
};
use navigation::render_tabs;
pub use navigation::{
    hit_test, hit_test_with_sidebar, hit_test_with_sidebar_scroll,
    hit_test_with_sidebar_scroll_and_sort, hit_test_with_sidebar_scroll_and_sort_and_groups,
    render_tab_drop_indicator, render_workspace_drop_indicator, sidebar_scroll_max,
    sidebar_scroll_max_with_sort, sidebar_scroll_max_with_sort_and_groups,
    sidebar_scroll_offset_from_drag_row, sidebar_scroll_offset_from_drag_row_with_sort,
    sidebar_scroll_offset_from_drag_row_with_sort_and_groups, sidebar_scroll_region,
    sidebar_scroll_thumb_grab_offset, sidebar_scroll_thumb_grab_offset_with_sort,
    sidebar_scroll_thumb_grab_offset_with_sort_and_groups, tab_drop_target, workspace_drop_target,
    workspace_drop_target_with_groups, ClickTarget,
};
pub use navigator::{hit_test_navigator, render_navigator, Hit as NavigatorHit};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub(super) struct ThemePalette {
    accent: Color,
    focused_border: Color,
}

impl ThemePalette {
    fn from_name(name: Option<&str>) -> Self {
        match name.unwrap_or("catppuccin").to_ascii_lowercase().as_str() {
            "catppuccin" => Self {
                accent: Color::Rgb(203, 166, 247),
                focused_border: Color::Rgb(245, 224, 220),
            },
            "catppuccin-latte" => Self {
                accent: Color::Rgb(136, 57, 239),
                focused_border: Color::Rgb(76, 79, 105),
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
            "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => Self {
                accent: Color::Rgb(46, 125, 233),
                focused_border: Color::Rgb(52, 59, 88),
            },
            "gruvbox-light" => Self {
                accent: Color::Rgb(7, 102, 120),
                focused_border: Color::Rgb(60, 56, 54),
            },
            "one-dark" => Self {
                accent: Color::Rgb(97, 175, 239),
                focused_border: Color::Rgb(171, 178, 191),
            },
            "one-light" => Self {
                accent: Color::Rgb(64, 120, 242),
                focused_border: Color::Rgb(56, 58, 66),
            },
            "solarized" | "solarized-light" => Self {
                accent: Color::Rgb(38, 139, 210),
                focused_border: Color::Rgb(101, 123, 131),
            },
            "kanagawa" => Self {
                accent: Color::Rgb(126, 156, 216),
                focused_border: Color::Rgb(220, 215, 186),
            },
            "kanagawa-lotus" => Self {
                accent: Color::Rgb(77, 105, 155),
                focused_border: Color::Rgb(84, 83, 75),
            },
            "rose-pine" => Self {
                accent: Color::Rgb(196, 167, 231),
                focused_border: Color::Rgb(224, 222, 244),
            },
            "rose-pine-dawn" => Self {
                accent: Color::Rgb(144, 122, 169),
                focused_border: Color::Rgb(87, 82, 121),
            },
            "vesper" => Self {
                accent: Color::Rgb(255, 199, 153),
                focused_border: Color::White,
            },
            _ => Self {
                accent: Color::Cyan,
                focused_border: Color::White,
            },
        }
    }

    pub(super) fn from_config(config: &crate::config::Config) -> Self {
        let mut palette = Self::from_name(config.theme_name.as_deref());
        if let Some(value) = config.theme_custom_accent.as_deref() {
            palette.accent = parse_theme_color(value).unwrap_or(palette.accent);
        }
        palette
    }

    pub(super) fn panel_bg(config: &crate::config::Config) -> Color {
        if let Some(value) = config.theme_custom_panel_bg.as_deref() {
            if let Some(color) = parse_theme_color(value) {
                return color;
            }
        }
        match config
            .theme_name
            .as_deref()
            .unwrap_or("catppuccin")
            .to_ascii_lowercase()
            .as_str()
        {
            "catppuccin" => Color::Rgb(24, 24, 37),
            "catppuccin-latte" => Color::Rgb(239, 241, 245),
            "dracula" => Color::Rgb(40, 42, 54),
            "gruvbox" => Color::Rgb(40, 40, 40),
            "gruvbox-light" => Color::Rgb(251, 241, 199),
            "nord" => Color::Rgb(46, 52, 64),
            "tokyo-night" | "tokyonight" => Color::Rgb(26, 27, 38),
            "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => Color::Rgb(225, 226, 231),
            "one-dark" => Color::Rgb(40, 44, 52),
            "one-light" => Color::Rgb(250, 250, 250),
            "solarized" => Color::Rgb(0, 43, 54),
            "solarized-light" => Color::Rgb(253, 246, 227),
            "kanagawa" => Color::Rgb(31, 31, 40),
            "kanagawa-lotus" => Color::Rgb(248, 246, 240),
            "rose-pine" => Color::Rgb(25, 23, 36),
            "rose-pine-dawn" => Color::Rgb(250, 244, 237),
            "vesper" => Color::Rgb(16, 16, 16),
            _ => Color::Reset,
        }
    }

    pub(super) fn sidebar_bg(config: &crate::config::Config) -> Color {
        config
            .theme_custom_sidebar_bg
            .as_deref()
            .and_then(parse_theme_color)
            .unwrap_or(Color::Reset)
    }

    pub(super) fn active_row_bg(config: &crate::config::Config) -> Color {
        config
            .theme_custom_active_row_bg
            .as_deref()
            .and_then(parse_theme_color)
            .unwrap_or(Color::DarkGray)
    }

    pub(super) fn selection_bg(config: &crate::config::Config) -> Color {
        config
            .theme_custom_selection_bg
            .as_deref()
            .and_then(parse_theme_color)
            .unwrap_or(Color::DarkGray)
    }

    pub(super) fn surface0(config: &crate::config::Config) -> Color {
        if let Some(color) = config
            .theme_custom_surface0
            .as_deref()
            .and_then(parse_theme_color)
        {
            return color;
        }
        match config
            .theme_name
            .as_deref()
            .unwrap_or("catppuccin")
            .to_ascii_lowercase()
            .as_str()
        {
            "catppuccin" => Color::Rgb(49, 50, 68),
            "catppuccin-latte" => Color::Rgb(204, 208, 218),
            "terminal" => Color::Reset,
            "tokyo-night" | "tokyonight" => Color::Rgb(36, 40, 59),
            "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => Color::Rgb(196, 200, 218),
            "dracula" => Color::Rgb(68, 71, 90),
            "nord" => Color::Rgb(59, 66, 82),
            "gruvbox" => Color::Rgb(60, 56, 54),
            "gruvbox-light" => Color::Rgb(235, 219, 178),
            "one-dark" => Color::Rgb(44, 49, 58),
            "one-light" => Color::Rgb(240, 240, 241),
            "solarized" => Color::Rgb(7, 54, 66),
            "solarized-light" => Color::Rgb(238, 232, 213),
            "kanagawa" => Color::Rgb(42, 42, 55),
            "kanagawa-lotus" => Color::Rgb(220, 213, 172),
            "rose-pine" => Color::Rgb(31, 29, 46),
            "rose-pine-dawn" => Color::Rgb(242, 233, 225),
            "vesper" => Color::Rgb(35, 35, 35),
            _ => Self::panel_bg(config),
        }
    }

    pub(super) fn panel_contrast_fg(config: &crate::config::Config) -> Color {
        match Self::panel_bg(config) {
            Color::Reset => Self::surface_dim(config),
            color => color,
        }
    }

    fn surface_dim(config: &crate::config::Config) -> Color {
        match config
            .theme_name
            .as_deref()
            .unwrap_or("catppuccin")
            .to_ascii_lowercase()
            .as_str()
        {
            "catppuccin" => Color::Rgb(30, 30, 46),
            "catppuccin-latte" => Color::Rgb(230, 233, 239),
            "terminal" => Color::DarkGray,
            "tokyo-night" | "tokyonight" => Color::Rgb(26, 27, 38),
            "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => Color::Rgb(210, 211, 218),
            "dracula" => Color::Rgb(40, 42, 54),
            "nord" => Color::Rgb(46, 52, 64),
            "gruvbox" => Color::Rgb(40, 40, 40),
            "gruvbox-light" => Color::Rgb(242, 229, 188),
            "one-dark" => Color::Rgb(40, 44, 52),
            "one-light" => Color::Rgb(245, 245, 246),
            "solarized" => Color::Rgb(0, 43, 54),
            "solarized-light" => Color::Rgb(238, 232, 213),
            "kanagawa" => Color::Rgb(31, 31, 40),
            "kanagawa-lotus" => Color::Rgb(213, 206, 163),
            "rose-pine" => Color::Rgb(38, 35, 58),
            "rose-pine-dawn" => Color::Rgb(242, 233, 225),
            "vesper" => Color::Rgb(16, 16, 16),
            _ => Color::DarkGray,
        }
    }
}

fn parse_theme_color(value: &str) -> Option<Color> {
    let value = value.trim().to_ascii_lowercase();
    if matches!(value.as_str(), "reset" | "default" | "none" | "transparent") {
        return Some(Color::Reset);
    }
    if let Some(inner) = value.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let values = inner
            .split(',')
            .map(|value| value.trim().parse::<u8>().ok())
            .collect::<Option<Vec<_>>>()?;
        if values.len() == 3 {
            return Some(Color::Rgb(values[0], values[1], values[2]));
        }
        return None;
    }
    let Some(hex) = value.strip_prefix('#') else {
        return Some(match value.as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" | "purple" => Color::Magenta,
            "cyan" => Color::Cyan,
            "white" => Color::White,
            "gray" | "grey" => Color::Gray,
            "darkgray" | "darkgrey" => Color::DarkGray,
            "lightred" => Color::LightRed,
            "lightgreen" => Color::LightGreen,
            "lightyellow" => Color::LightYellow,
            "lightblue" => Color::LightBlue,
            "lightmagenta" => Color::LightMagenta,
            "lightcyan" => Color::LightCyan,
            _ => return None,
        });
    };
    match hex.len() {
        3 => {
            let digits = hex
                .chars()
                .map(|digit| digit.to_digit(16).map(|value| (value * 17) as u8))
                .collect::<Option<Vec<_>>>()?;
            Some(Color::Rgb(digits[0], digits[1], digits[2]))
        }
        6 => Some(Color::Rgb(
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        )),
        _ => None,
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
    render_with_sidebar_scroll_and_cursor_and_agent_sort_and_navigation_and_groups(
        frame,
        snapshot,
        connected,
        sidebar_collapsed,
        sidebar_scroll,
        show_host_cursor,
        agent_priority_sort,
        navigation_workspace,
        &HashSet::new(),
        &HashMap::new(),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_sidebar_scroll_and_cursor_and_agent_sort_and_navigation_and_groups(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    connected: bool,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
    show_host_cursor: bool,
    agent_priority_sort: bool,
    navigation_workspace: Option<(&str, &str)>,
    collapsed_groups: &HashSet<String>,
    scroll_offsets: &HashMap<String, usize>,
) {
    let theme = ThemePalette::from_config(&crate::config::load());
    let main = layout::main_areas_for_snapshot(snapshot, frame.area(), sidebar_collapsed);
    mobile::render_header(frame, main.mobile_header, snapshot);
    navigation::render_sidebar_with_scroll_sort_and_navigation_and_groups(
        frame,
        snapshot,
        main.sidebar,
        sidebar_collapsed,
        sidebar_scroll,
        agent_priority_sort,
        navigation_workspace,
        collapsed_groups,
    );
    render_tabs(frame, snapshot, main.tabs);
    let panes = pane_rectangles(snapshot, main.panes);
    let config = crate::config::load();
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
        for pane_rect in &panes {
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
            let borders = pane_borders_for_rect(
                pane_rect.rect,
                &panes,
                config.pane_borders,
                config.pane_outer_borders,
                config.pane_gaps,
            );
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(borders)
                        .title(title)
                        .border_style(Style::default().fg(border_color)),
                ),
                pane_rect.rect,
            );
            if config.pane_scrollbars && !pane.alternate_screen {
                let inner = pane_inner_area(pane_rect.rect, borders, true);
                render_pane_scrollbar(
                    frame,
                    inner,
                    pane,
                    snapshot.focused_pane_id.as_deref() == Some(&pane_rect.pane_id),
                    scroll_offsets.get(&pane_rect.pane_id).copied().unwrap_or(0),
                );
            }
            if show_host_cursor
                && pane.cursor_visible
                && snapshot.focused_pane_id.as_deref() == Some(&pane_rect.pane_id)
            {
                let inner = pane_inner_area(
                    pane_rect.rect,
                    borders,
                    config.pane_scrollbars && !pane.alternate_screen,
                );
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
            let popup = popup_rect_with_specs(
                main.panes,
                snapshot.popup_width,
                snapshot.popup_height,
                snapshot.popup_width_spec,
                snapshot.popup_height_spec,
            );
            frame.render_widget(Clear, popup);
            let border = Block::default()
                .borders(Borders::ALL)
                .title(popup_title(pane))
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
    } else if !connected || panes.is_empty() {
        let focused = snapshot.focused_pane_id.as_deref().unwrap_or("none");
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
    } else {
        return;
    };
    frame.render_widget(Paragraph::new(chrome), footer_area(frame.area()));
}

fn render_pane_scrollbar(
    frame: &mut Frame<'_>,
    inner: Rect,
    pane: &crate::server::session::PaneView,
    focused: bool,
    offset: usize,
) {
    if inner.width == 0 || inner.height == 0 || pane.scrollback_bytes == 0 {
        return;
    }
    let track = Rect::new(inner.right(), inner.y, 1, inner.height);
    let max_offset = max_offset_for_pane(pane, inner.height);
    if max_offset == 0 {
        return;
    }
    scrollbar::render(
        frame.buffer_mut(),
        max_offset,
        inner.height,
        track,
        focused,
        offset,
    );
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
    let panes = pane_rectangles(
        snapshot,
        layout::pane_content_area_for_snapshot(snapshot, frame.area(), sidebar_collapsed),
    );
    let Some(pane) = panes
        .into_iter()
        .find(|pane| pane.pane_id == selection.pane_id)
    else {
        return;
    };
    let all_panes = pane_rectangles(
        snapshot,
        layout::pane_content_area_for_snapshot(snapshot, frame.area(), sidebar_collapsed),
    );
    let config = crate::config::load();
    let borders = layout::pane_borders_for_rect(
        pane.rect,
        &all_panes,
        config.pane_borders,
        config.pane_outer_borders,
        config.pane_gaps,
    );
    let alternate_screen = snapshot
        .panes
        .iter()
        .find(|view| view.pane_id == pane.pane_id)
        .is_some_and(|view| view.alternate_screen);
    let inner = pane_inner_area(
        pane.rect,
        borders,
        config.pane_scrollbars && !alternate_screen,
    );
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
    let panes = pane_rectangles(
        snapshot,
        layout::pane_content_area_for_snapshot(snapshot, frame.area(), sidebar_collapsed),
    );
    let Some(pane) = panes.iter().find(|pane| pane.pane_id == mode.pane_id) else {
        return;
    };
    let config = crate::config::load();
    let borders = layout::pane_borders_for_rect(
        pane.rect,
        &panes,
        config.pane_borders,
        config.pane_outer_borders,
        config.pane_gaps,
    );
    let alternate_screen = snapshot
        .panes
        .iter()
        .find(|view| view.pane_id == pane.pane_id)
        .is_some_and(|view| view.alternate_screen);
    let inner = pane_inner_area(
        pane.rect,
        borders,
        config.pane_scrollbars && !alternate_screen,
    );
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
            frame.render_widget(
                Paragraph::new(hint),
                mode_bar_area(snapshot, frame.area(), sidebar_collapsed),
            );
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
    frame.render_widget(
        Paragraph::new(hint),
        mode_bar_area(snapshot, frame.area(), sidebar_collapsed),
    );
}

fn footer_area(area: Rect) -> Rect {
    Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1)
}

pub(crate) fn render_prefix_mode(
    frame: &mut Frame<'_>,
    keymap: &Keymap,
    snapshot: &SessionSnapshot,
    sidebar_collapsed: bool,
) {
    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(ratatui::style::Modifier::BOLD);
    let base_style = Style::default().fg(Color::White);
    let line = Line::from(vec![
        Span::styled(
            " PREFIX ",
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ),
        Span::styled("esc", key_style),
        Span::styled(" cancel  ", base_style),
        Span::styled(keymap.prefix_label(), key_style),
        Span::styled(" send prefix  ", base_style),
        Span::styled(keymap.binding_label(Action::WorkspacePicker), key_style),
        Span::styled(" workspace nav  ", base_style),
        Span::styled(keymap.binding_label(Action::Help), key_style),
        Span::styled(" keybinds", base_style),
    ]);
    frame.render_widget(
        Paragraph::new(line),
        mode_bar_area(snapshot, frame.area(), sidebar_collapsed),
    );
}

fn mode_bar_area(snapshot: &SessionSnapshot, area: Rect, sidebar_collapsed: bool) -> Rect {
    let config = crate::config::load();
    mode_bar_area_with_position(snapshot, area, sidebar_collapsed, config.tab_bar_position)
}

fn mode_bar_area_with_position(
    snapshot: &SessionSnapshot,
    area: Rect,
    sidebar_collapsed: bool,
    tab_bar_position: crate::config::TabBarPosition,
) -> Rect {
    let main = layout::main_areas_for_snapshot_with_tab_bar_position(
        snapshot,
        area,
        sidebar_collapsed,
        tab_bar_position,
    );
    if matches!(tab_bar_position, crate::config::TabBarPosition::Bottom) && !main.tabs.is_empty() {
        main.tabs
    } else {
        footer_area(area)
    }
}

pub fn render_palette(frame: &mut Frame<'_>, selected: usize) {
    let area = centered_rect(60, 70, frame.area());
    let panel_bg = ThemePalette::panel_bg(&crate::config::load());
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
        Paragraph::new(rows)
            .style(Style::default().bg(panel_bg))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Command palette (↑/↓, Enter, Esc)"),
            ),
        area,
    );
}

pub fn render_help(frame: &mut Frame<'_>, keymap: &Keymap) {
    let area = centered_rect(72, 90, frame.area());
    let panel_bg = ThemePalette::panel_bg(&crate::config::load());
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
        Paragraph::new(rows)
            .style(Style::default().bg(panel_bg))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Spindle help — mouse and keyboard"),
            ),
        area,
    );
}

pub fn render_onboarding(frame: &mut Frame<'_>) {
    let area = onboarding_area(frame.area());
    let panel_bg = ThemePalette::panel_bg(&crate::config::load());
    let content = vec![
        Line::from("Welcome to Spindle"),
        Line::from("Persistent PowerShell sessions with a mouse-first layout."),
        Line::from(""),
        Line::from("Click panes, tabs, and workspaces. Right-click for actions."),
        Line::from(""),
        Line::from("Ctrl-b opens Spindle controls; Ctrl-b ? shows every shortcut."),
        Line::from(""),
        Line::from("Press Enter to continue."),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(content)
            .style(Style::default().bg(panel_bg))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Welcome")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
        area,
    );
}

pub(crate) fn onboarding_area(area: Rect) -> Rect {
    centered_rect(80, 80, area)
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

pub(crate) fn render_resize_mode(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    sidebar_collapsed: bool,
) {
    let bar = mode_bar_area(snapshot, frame.area(), sidebar_collapsed);
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

pub(crate) fn popup_rect_with_specs(
    area: Rect,
    width: u16,
    height: u16,
    width_spec: Option<crate::popup_size::PopupSize>,
    height_spec: Option<crate::popup_size::PopupSize>,
) -> Rect {
    let width = if let Some(spec) = width_spec {
        spec.resolve(area.width)
    } else if width == 0 {
        (area.width / 2).max(4)
    } else {
        width
    }
    .min(area.width);
    let height = if let Some(spec) = height_spec {
        spec.resolve(area.height)
    } else if height == 0 {
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
        .or_else(|| {
            pane.display_title
                .as_deref()
                .filter(|title| !title.is_empty())
        })
        .or_else(|| (!pane.title.is_empty()).then_some(pane.title.as_str()))
        .unwrap_or(pane.command.as_str());
    let alternate = if pane.alternate_screen { " [alt]" } else { "" };
    format!(
        "{} {} {}{} — {}{}",
        pane.status.indicator(),
        pane.pane_id,
        label,
        pane.agent
            .as_ref()
            .map(|agent| {
                let state = format!(" {}", pane.agent_display_state_label());
                format!(
                    " [{}{state}]",
                    pane.display_agent
                        .as_deref()
                        .unwrap_or_else(|| agent.label())
                )
            })
            .unwrap_or_default(),
        status_detail(&pane.status),
        alternate
    )
}

fn popup_title(pane: &crate::server::session::PaneView) -> String {
    pane.label
        .as_deref()
        .filter(|label| !label.is_empty())
        .or_else(|| (!pane.title.is_empty()).then_some(pane.title.as_str()))
        .unwrap_or(pane.command.as_str())
        .to_owned()
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
        active_title, pane_content_area, pane_rectangles, pane_title, pane_title_text, popup_title,
        render, render_action_error, render_help, render_onboarding, render_palette,
        render_prefix_mode, render_selection, render_startup_error, render_with_connection,
        status_color,
    };
    use crate::model::layout::LayoutNode;
    use crate::model::status::PaneStatus;
    use crate::server::session::{
        PaneView, Session, SessionSnapshot, SpaceView, TabView, WorkspaceView,
    };
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
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
    fn herdr_theme_choices_have_distinct_renderer_accents() {
        assert_eq!(
            super::ThemePalette::from_name(Some("one-dark")).accent,
            Color::Rgb(97, 175, 239)
        );
        assert_eq!(
            super::ThemePalette::from_name(Some("catppuccin-latte")).accent,
            Color::Rgb(136, 57, 239)
        );
    }

    #[test]
    fn missing_theme_uses_herdr_catppuccin_default() {
        assert_eq!(
            super::ThemePalette::from_name(None).accent,
            Color::Rgb(203, 166, 247)
        );
    }

    #[test]
    fn custom_theme_accent_overrides_the_base_theme() {
        let mut config = crate::config::Config {
            theme_name: Some("nord".into()),
            theme_custom_accent: Some("#010203".into()),
            ..crate::config::Config::default()
        };
        assert_eq!(
            super::ThemePalette::from_config(&config).accent,
            Color::Rgb(1, 2, 3)
        );
        config.theme_custom_accent = Some("#bad".into());
        assert_eq!(
            super::ThemePalette::from_config(&config).accent,
            Color::Rgb(187, 170, 221)
        );
        config.theme_custom_accent = Some("rgb(1, 2, 3)".into());
        assert_eq!(
            super::ThemePalette::from_config(&config).accent,
            Color::Rgb(1, 2, 3)
        );
        config.theme_custom_accent = Some("magenta".into());
        assert_eq!(
            super::ThemePalette::from_config(&config).accent,
            Color::Magenta
        );
        config.theme_custom_accent = Some("not-a-color".into());
        assert_eq!(
            super::ThemePalette::from_config(&config).accent,
            Color::Rgb(136, 192, 208)
        );
    }

    #[test]
    fn custom_theme_panel_background_overrides_the_base_theme() {
        let config = crate::config::Config {
            theme_name: Some("nord".into()),
            theme_custom_panel_bg: Some("rgb(1, 2, 3)".into()),
            ..crate::config::Config::default()
        };
        assert_eq!(super::ThemePalette::panel_bg(&config), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn custom_theme_sidebar_surfaces_override_herdr_defaults() {
        let config = crate::config::Config {
            theme_custom_sidebar_bg: Some("#010203".into()),
            theme_custom_active_row_bg: Some("rgb(4, 5, 6)".into()),
            theme_custom_selection_bg: Some("#070809".into()),
            ..crate::config::Config::default()
        };
        assert_eq!(
            super::ThemePalette::sidebar_bg(&config),
            Color::Rgb(1, 2, 3)
        );
        assert_eq!(
            super::ThemePalette::active_row_bg(&config),
            Color::Rgb(4, 5, 6)
        );
        assert_eq!(
            super::ThemePalette::selection_bg(&config),
            Color::Rgb(7, 8, 9)
        );
        assert_eq!(
            super::ThemePalette::surface0(&config),
            Color::Rgb(49, 50, 68)
        );
        let config = crate::config::Config {
            theme_custom_surface0: Some("#070809".into()),
            ..config
        };
        assert_eq!(super::ThemePalette::surface0(&config), Color::Rgb(7, 8, 9));
    }

    #[test]
    fn surface_zero_matches_herdr_theme_defaults() {
        let config = crate::config::Config {
            theme_name: Some("dracula".into()),
            ..crate::config::Config::default()
        };
        assert_eq!(
            super::ThemePalette::surface0(&config),
            Color::Rgb(68, 71, 90)
        );
    }

    #[test]
    fn panel_contrast_foreground_matches_herdr_surface_rule() {
        let config = crate::config::Config::default();
        assert_eq!(
            super::ThemePalette::panel_contrast_fg(&config),
            Color::Rgb(24, 24, 37)
        );
        let config = crate::config::Config {
            theme_custom_panel_bg: Some("reset".into()),
            ..config
        };
        assert_eq!(
            super::ThemePalette::panel_contrast_fg(&config),
            Color::Rgb(30, 30, 46)
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
        assert!(content.contains("switch"));
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
    fn onboarding_overlay_explains_the_first_controls() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(render_onboarding).unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Welcome to Spindle"));
        assert!(content.contains("Ctrl-b"));
        assert!(content.contains("controls"));
        assert!(content.contains("Press Enter to continue"));
    }

    #[test]
    fn prefix_mode_shows_herdr_style_transient_chrome() {
        let backend = TestBackend::new(160, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let session = Session::default();
                render_prefix_mode(frame, &Keymap::default(), session.snapshot(), false);
            })
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("PREFIX"));
        assert!(content.contains("cancel"));
        assert!(content.contains("workspace nav"));
        assert!(content.contains("keybinds"));
    }

    #[test]
    fn bottom_tab_mode_bar_uses_the_tab_row_like_herdr() {
        let session = Session::default();
        let area = Rect::new(0, 0, 160, 8);
        let bar = super::mode_bar_area_with_position(
            session.snapshot(),
            area,
            false,
            crate::config::TabBarPosition::Bottom,
        );

        assert_eq!(bar.y, 7);
        assert_eq!(bar.height, 1);
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
                    tokens: std::collections::HashMap::new(),
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
                display_agent: None,
                display_title: None,
                state_labels: std::collections::BTreeMap::new(),
                tokens: std::collections::HashMap::new(),
                agent_session: None,
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
            popup_width_spec: None,
            popup_height_spec: None,
            overlay_pane_id: None,
            overlay_previous_focus: None,
            overlay_previous_zoomed: false,
            event_sequence: 0,
        };
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 24);
        let pane_rect = pane_rectangles(&snapshot, pane_content_area(area))[0].rect;
        let inner = pane_rect;
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
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(!content.contains("focused: pane-1"));
        assert!(!content.contains("panes: 1"));
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            agent_session: None,
            tokens: std::collections::HashMap::new(),
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
        pane.display_agent = Some("Codex Review".into());
        assert!(pane_title_text(&pane).contains("[Codex Review working]"));
        pane.display_title = Some("Review shell".into());
        assert!(pane_title_text(&pane).contains("Review shell"));
        assert_eq!(
            pane_title(&pane).spans[0].style.fg,
            Some(Color::Rgb(255, 165, 0))
        );
        pane.agent_state = Some(crate::detect::AgentState::Idle);
        pane.agent_done = true;
        assert!(pane_title_text(&pane).contains("[Codex Review done]"));
        let wire = serde_json::to_value(&pane).unwrap();
        assert_eq!(wire["agent_state"], "idle");
        assert_eq!(wire["agent_done"], true);
    }

    #[test]
    fn popup_title_prefers_declared_label_and_falls_back_to_command() {
        let mut pane = PaneView {
            pane_id: "pane-1".into(),
            command: "powershell.exe".into(),
            args: Vec::new(),
            cwd: "C:/".into(),
            cols: 80,
            rows: 24,
            label: Some("Plugin Popup".into()),
            agent: None,
            agent_state: None,
            agent_done: false,
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            tokens: std::collections::HashMap::new(),
            agent_session: None,
            status: PaneStatus::Running,
            scrollback_bytes: 0,
            scrollback: Vec::new(),
            screen: String::new(),
            cursor: (0, 0),
            cursor_visible: true,
            title: "Manifest title".into(),
            alternate_screen: false,
            hyperlinks: Vec::new(),
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
        assert_eq!(popup_title(&pane), "Plugin Popup");
        pane.label = None;
        assert_eq!(popup_title(&pane), "Manifest title");
        pane.title.clear();
        assert_eq!(popup_title(&pane), "powershell.exe");
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
            display_agent: None,
            display_title: None,
            state_labels: std::collections::BTreeMap::new(),
            tokens: std::collections::HashMap::new(),
            agent_session: None,
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
