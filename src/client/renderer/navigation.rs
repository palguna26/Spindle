use crate::server::session::{SessionSnapshot, WorkspaceView};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::collections::HashSet;

use super::layout::{pane_rectangles, split_handles};

const MIN_TAB_WIDTH: u16 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    MobileSwitcher,
    GlobalMenu,
    NewWorkspace,
    SidebarToggle,
    ToggleAgentSort,
    SidebarScroll(usize),
    Space(String),
    Workspace {
        space_id: String,
        workspace_id: String,
    },
    Agent {
        space_id: String,
        workspace_id: String,
        tab_id: String,
        pane_id: String,
    },
    Tab(String),
    TabScrollLeft,
    TabScrollRight,
    NewTab,
    Pane(String),
    SplitBorder(Vec<bool>),
}

pub fn hit_test(snapshot: &SessionSnapshot, area: Rect, mouse: MouseEvent) -> Option<ClickTarget> {
    hit_test_with_sidebar(snapshot, area, mouse, false)
}

pub fn hit_test_with_sidebar(
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    sidebar_collapsed: bool,
) -> Option<ClickTarget> {
    hit_test_with_sidebar_scroll(snapshot, area, mouse, sidebar_collapsed, 0)
}

pub fn hit_test_with_sidebar_scroll(
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
) -> Option<ClickTarget> {
    hit_test_with_sidebar_scroll_and_sort(
        snapshot,
        area,
        mouse,
        sidebar_collapsed,
        sidebar_scroll,
        false,
    )
}

pub fn hit_test_with_sidebar_scroll_and_sort(
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
    agent_priority_sort: bool,
) -> Option<ClickTarget> {
    hit_test_with_sidebar_scroll_and_sort_and_groups(
        snapshot,
        area,
        mouse,
        sidebar_collapsed,
        sidebar_scroll,
        agent_priority_sort,
        &HashSet::new(),
    )
}

pub fn hit_test_with_sidebar_scroll_and_sort_and_groups(
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
    agent_priority_sort: bool,
    collapsed_groups: &HashSet<String>,
) -> Option<ClickTarget> {
    hit_test_with_sidebar_scroll_and_sort_and_groups_and_tab_scroll(
        snapshot,
        area,
        mouse,
        sidebar_collapsed,
        sidebar_scroll,
        agent_priority_sort,
        collapsed_groups,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn hit_test_with_sidebar_scroll_and_sort_and_groups_and_tab_scroll(
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    sidebar_collapsed: bool,
    sidebar_scroll: usize,
    agent_priority_sort: bool,
    collapsed_groups: &HashSet<String>,
    tab_scroll: usize,
) -> Option<ClickTarget> {
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
    ) {
        return None;
    }
    let x = mouse.column;
    let y = mouse.row;
    let main = super::layout::main_areas_for_snapshot(snapshot, area, sidebar_collapsed);
    if contains(super::layout::mobile_switch_rect(main.mobile_header), x, y) {
        return Some(ClickTarget::MobileSwitcher);
    }
    if contains(main.sidebar, x, y) {
        if y == main.sidebar.bottom().saturating_sub(1) {
            if !sidebar_collapsed && x > main.sidebar.x && x < main.sidebar.x.saturating_add(6) {
                return Some(ClickTarget::NewWorkspace);
            }
            if !sidebar_collapsed
                && x >= main.sidebar.right().saturating_sub(7)
                && x < main.sidebar.right().saturating_sub(2)
            {
                return Some(ClickTarget::GlobalMenu);
            }
            if x == main.sidebar.right().saturating_sub(2) {
                return Some(ClickTarget::SidebarToggle);
            }
        }
        if !sidebar_collapsed && y == main.sidebar.y && x > main.sidebar.x {
            return Some(ClickTarget::ToggleAgentSort);
        }
        let body = sidebar_body(main.sidebar);
        if !contains(body, x, y) {
            return None;
        }
        let rows = sidebar_rows_with_collapsed(snapshot, agent_priority_sort, collapsed_groups);
        let sidebar_config = crate::config::load().sidebar;
        let visual_rows = sidebar_visual_rows(&rows, sidebar_collapsed, &sidebar_config);
        let max_scroll = visual_rows.len().saturating_sub(usize::from(body.height));
        if max_scroll > 0 && body.width > 1 && x == body.right().saturating_sub(1) {
            return Some(ClickTarget::SidebarScroll(sidebar_scroll_for_track_row(
                body,
                visual_rows.len(),
                max_scroll,
                y,
            )));
        }
        let visual_row =
            usize::from(y.saturating_sub(body.y)).saturating_add(sidebar_scroll.min(max_scroll));
        let (Some(row), _) = visual_rows.get(visual_row)? else {
            return None;
        };
        return rows.get(*row).map(|row| match row {
            SidebarRow::Space { space_id, .. } => ClickTarget::Space(space_id.to_string()),
            SidebarRow::Workspace {
                space_id,
                workspace_id,
                ..
            } => ClickTarget::Workspace {
                space_id: space_id.to_string(),
                workspace_id: workspace_id.to_string(),
            },
            SidebarRow::Agent {
                space_id,
                workspace_id,
                tab_id,
                pane,
                ..
            } => ClickTarget::Agent {
                space_id: space_id.to_string(),
                workspace_id: workspace_id.to_string(),
                tab_id: tab_id.to_string(),
                pane_id: pane.pane_id.clone(),
            },
            SidebarRow::AgentHeader => ClickTarget::ToggleAgentSort,
        });
    }
    if contains(main.tabs, x, y) {
        let workspace = active_workspace(snapshot)?;
        let (tab_area, scroll_left, scroll_right) = tab_layout(main.tabs, workspace.tabs.len());
        if scroll_left.is_some_and(|rect| contains(rect, x, y)) {
            return Some(ClickTarget::TabScrollLeft);
        }
        if scroll_right.is_some_and(|rect| contains(rect, x, y)) {
            return Some(ClickTarget::TabScrollRight);
        }
        if contains(new_tab_area(main.tabs), x, y) {
            return Some(ClickTarget::NewTab);
        }
        let active_index = workspace
            .tabs
            .iter()
            .position(|tab| tab.tab_id == workspace.active_tab_id)
            .unwrap_or(0);
        let index =
            tab_index_at_with_scroll(tab_area, x, workspace.tabs.len(), active_index, tab_scroll)?;
        return workspace
            .tabs
            .get(index)
            .map(|tab| ClickTarget::Tab(tab.tab_id.clone()));
    }
    if contains(main.panes, x, y) {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            if let Some(split) = split_handles(snapshot, main.panes)
                .into_iter()
                .find(|split| contains(split.hit_rect, x, y))
            {
                return Some(ClickTarget::SplitBorder(split.path));
            }
        }
        let pane = pane_rectangles(snapshot, main.panes)
            .into_iter()
            .find(|pane| contains(pane.rect, x, y))?;
        return Some(ClickTarget::Pane(pane.pane_id));
    }
    None
}

/// Returns the same-space insertion slot represented by a workspace row.
/// The index follows Herdr's `insert_index` convention. Dropping on a later
/// row places the source after that row; dropping on an earlier row places it
/// before that row.
pub fn workspace_drop_target(
    snapshot: &SessionSnapshot,
    area: Rect,
    sidebar_scroll: usize,
    agent_priority_sort: bool,
    source_workspace_id: &str,
    x: u16,
    y: u16,
) -> Option<(String, String, usize)> {
    workspace_drop_target_with_groups(
        snapshot,
        area,
        sidebar_scroll,
        agent_priority_sort,
        source_workspace_id,
        x,
        y,
        &HashSet::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn workspace_drop_target_with_groups(
    snapshot: &SessionSnapshot,
    area: Rect,
    sidebar_scroll: usize,
    agent_priority_sort: bool,
    source_workspace_id: &str,
    x: u16,
    y: u16,
    collapsed_groups: &HashSet<String>,
) -> Option<(String, String, usize)> {
    let sidebar = super::layout::main_areas_with_sidebar(area, false).sidebar;
    let body = sidebar_body(sidebar);
    if !contains(body, x, y) {
        return None;
    }
    let rows = sidebar_rows_with_collapsed(snapshot, agent_priority_sort, collapsed_groups);
    let sidebar_config = crate::config::load().sidebar;
    let visual_rows = sidebar_visual_rows(&rows, false, &sidebar_config);
    let max_scroll = visual_rows.len().saturating_sub(usize::from(body.height));
    let visual_row =
        usize::from(y.saturating_sub(body.y)).saturating_add(sidebar_scroll.min(max_scroll));
    let (Some(row), _) = visual_rows.get(visual_row)? else {
        return None;
    };
    let row = *row;
    let SidebarRow::Workspace {
        space_id,
        workspace_id,
        ..
    } = rows.get(row)?
    else {
        return None;
    };
    let workspaces = &snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == *space_id)?
        .workspaces;
    let roots: Vec<_> = workspaces
        .iter()
        .filter(|workspace| !workspace.is_linked_worktree)
        .collect();
    let target_workspace = workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == *workspace_id)?;
    let target_root_id = target_workspace
        .worktree_group
        .as_deref()
        .filter(|_| target_workspace.is_linked_worktree)
        .and_then(|group| {
            workspaces
                .iter()
                .find(|workspace| {
                    workspace.worktree_group.as_deref() == Some(group)
                        && !workspace.is_linked_worktree
                })
                .map(|workspace| workspace.workspace_id.as_str())
        })
        .unwrap_or(workspace_id);
    let target_position = roots
        .iter()
        .position(|workspace| workspace.workspace_id == target_root_id)?;
    let source_position = roots
        .iter()
        .position(|workspace| workspace.workspace_id == source_workspace_id)?;
    if source_position == target_position {
        return None;
    }
    let insert_index = if source_position < target_position {
        target_position.saturating_add(1)
    } else {
        target_position
    };
    Some((
        (*space_id).to_owned(),
        target_root_id.to_owned(),
        insert_index,
    ))
}

/// Returns the tab insertion slot represented by a tab-strip row.
pub fn tab_drop_target(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    source_tab_id: &str,
    x: u16,
    y: u16,
) -> Option<(String, String, usize)> {
    let main = super::layout::main_areas_for_snapshot(snapshot, area, collapsed);
    if !contains(main.tabs, x, y) {
        return None;
    }
    let workspace = active_workspace(snapshot)?;
    let active_index = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == workspace.active_tab_id)
        .unwrap_or(0);
    let target_index = tab_index_at(
        tab_strip_area(main.tabs),
        x,
        workspace.tabs.len(),
        active_index,
    )?;
    let target = workspace.tabs.get(target_index)?;
    let source_index = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == source_tab_id)?;
    let insert_index = if source_index < target_index {
        target_index.saturating_add(1)
    } else {
        target_index
    };
    Some((
        workspace.workspace_id.clone(),
        target.tab_id.clone(),
        insert_index,
    ))
}

pub fn sidebar_scroll_max(snapshot: &SessionSnapshot, area: Rect, collapsed: bool) -> usize {
    sidebar_scroll_max_with_sort(snapshot, area, collapsed, false)
}

pub fn sidebar_scroll_max_with_sort(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    agent_priority_sort: bool,
) -> usize {
    sidebar_scroll_max_with_sort_and_groups(
        snapshot,
        area,
        collapsed,
        agent_priority_sort,
        &HashSet::new(),
    )
}

pub fn sidebar_scroll_max_with_sort_and_groups(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    agent_priority_sort: bool,
    collapsed_groups: &HashSet<String>,
) -> usize {
    let sidebar = super::layout::main_areas_with_sidebar(area, collapsed).sidebar;
    let body = sidebar_body(sidebar);
    let rows = sidebar_rows_with_collapsed(snapshot, agent_priority_sort, collapsed_groups);
    let visual_rows = sidebar_visual_rows(&rows, collapsed, &crate::config::load().sidebar);
    visual_rows.len().saturating_sub(usize::from(body.height))
}

pub fn sidebar_scroll_region(area: Rect, collapsed: bool, x: u16, y: u16) -> bool {
    let sidebar = super::layout::main_areas_with_sidebar(area, collapsed).sidebar;
    contains(sidebar_body(sidebar), x, y)
}

pub fn sidebar_scroll_thumb_grab_offset(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
    x: u16,
    y: u16,
) -> Option<u16> {
    sidebar_scroll_thumb_grab_offset_with_sort(snapshot, area, collapsed, scroll, x, y, false)
}

pub fn sidebar_scroll_thumb_grab_offset_with_sort(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
    x: u16,
    y: u16,
    agent_priority_sort: bool,
) -> Option<u16> {
    sidebar_scroll_thumb_grab_offset_with_sort_and_groups(
        snapshot,
        area,
        collapsed,
        scroll,
        x,
        y,
        agent_priority_sort,
        &HashSet::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn sidebar_scroll_thumb_grab_offset_with_sort_and_groups(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
    x: u16,
    y: u16,
    agent_priority_sort: bool,
    collapsed_groups: &HashSet<String>,
) -> Option<u16> {
    let sidebar = super::layout::main_areas_with_sidebar(area, collapsed).sidebar;
    let body = sidebar_body(sidebar);
    let max_scroll = sidebar_scroll_max_with_sort_and_groups(
        snapshot,
        area,
        collapsed,
        agent_priority_sort,
        collapsed_groups,
    );
    if max_scroll == 0
        || body.width <= 1
        || x != body.right().saturating_sub(1)
        || !contains(body, x, y)
    {
        return None;
    }
    let rows = sidebar_rows_with_collapsed(snapshot, agent_priority_sort, collapsed_groups);
    let visual_rows = sidebar_visual_rows(&rows, collapsed, &crate::config::load().sidebar);
    let (thumb_top, thumb_height) =
        sidebar_scrollbar_thumb(body, scroll, max_scroll, visual_rows.len())?;
    let row = y.saturating_sub(body.y);
    (row >= thumb_top && row < thumb_top.saturating_add(thumb_height)).then_some(row - thumb_top)
}

pub fn sidebar_scroll_offset_from_drag_row(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    row: u16,
    grab_row_offset: u16,
) -> usize {
    sidebar_scroll_offset_from_drag_row_with_sort(
        snapshot,
        area,
        collapsed,
        row,
        grab_row_offset,
        false,
    )
}

pub fn sidebar_scroll_offset_from_drag_row_with_sort(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    row: u16,
    grab_row_offset: u16,
    agent_priority_sort: bool,
) -> usize {
    sidebar_scroll_offset_from_drag_row_with_sort_and_groups(
        snapshot,
        area,
        collapsed,
        row,
        grab_row_offset,
        agent_priority_sort,
        &HashSet::new(),
    )
}

pub fn sidebar_scroll_offset_from_drag_row_with_sort_and_groups(
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    row: u16,
    grab_row_offset: u16,
    agent_priority_sort: bool,
    collapsed_groups: &HashSet<String>,
) -> usize {
    let sidebar = super::layout::main_areas_with_sidebar(area, collapsed).sidebar;
    let body = sidebar_body(sidebar);
    let max_scroll = sidebar_scroll_max_with_sort_and_groups(
        snapshot,
        area,
        collapsed,
        agent_priority_sort,
        collapsed_groups,
    );
    let rows = sidebar_rows_with_collapsed(snapshot, agent_priority_sort, collapsed_groups);
    let rows = sidebar_visual_rows(&rows, collapsed, &crate::config::load().sidebar).len();
    let Some((_, thumb_height)) = sidebar_scrollbar_thumb(body, 0, max_scroll, rows) else {
        return 0;
    };
    let track_height = usize::from(body.height);
    let max_thumb_top = track_height.saturating_sub(usize::from(thumb_height));
    if max_thumb_top == 0 {
        return 0;
    }
    let clamped_row = row.clamp(body.y, body.bottom().saturating_sub(1));
    let row_offset = usize::from(clamped_row.saturating_sub(body.y));
    let desired_top = row_offset.saturating_sub(usize::from(grab_row_offset));
    rounded_ratio(desired_top.min(max_thumb_top), max_scroll, max_thumb_top).min(max_scroll)
}

fn sidebar_scroll_for_track_row(
    body: Rect,
    row_count: usize,
    max_scroll: usize,
    row: u16,
) -> usize {
    let Some((_, thumb_height)) = sidebar_scrollbar_thumb(body, 0, max_scroll, row_count) else {
        return 0;
    };
    let track_height = usize::from(body.height);
    let max_thumb_top = track_height.saturating_sub(usize::from(thumb_height));
    if max_thumb_top == 0 {
        return 0;
    }
    let row_offset = usize::from(row.saturating_sub(body.y));
    let desired_top = row_offset.saturating_sub(usize::from(thumb_height / 2));
    rounded_ratio(desired_top.min(max_thumb_top), max_scroll, max_thumb_top).min(max_scroll)
}

fn sidebar_scrollbar_thumb(
    body: Rect,
    scroll: usize,
    max_scroll: usize,
    row_count: usize,
) -> Option<(u16, u16)> {
    let height = usize::from(body.height);
    if max_scroll == 0 || height == 0 {
        return None;
    }
    let thumb_height = ((height * height) as f64 / row_count.max(1) as f64)
        .round()
        .max(1.0)
        .min(height as f64) as usize;
    let max_thumb_top = height.saturating_sub(thumb_height);
    let thumb_top = rounded_ratio(scroll.min(max_scroll), max_thumb_top, max_scroll);
    Some((thumb_top as u16, thumb_height as u16))
}

fn rounded_ratio(value: usize, numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        return 0;
    }
    ((value as f64 * numerator as f64) / denominator as f64).round() as usize
}

fn sidebar_body(area: Rect) -> Rect {
    let sidebar_sections = crate::client::sidebar::sections(area, 0.5);
    let content = if sidebar_sections.workspaces.is_empty() && sidebar_sections.agents.is_empty() {
        crate::client::sidebar::content_area(area)
    } else {
        Rect::new(
            sidebar_sections.workspaces.x,
            sidebar_sections.workspaces.y,
            sidebar_sections.workspaces.width,
            sidebar_sections
                .workspaces
                .height
                .saturating_add(sidebar_sections.agents.height),
        )
    };
    Rect::new(
        content.x.saturating_add(1),
        content.y.saturating_add(1),
        content.width.saturating_sub(1),
        content.height.saturating_sub(2),
    )
}

fn sidebar_visual_rows(
    rows: &[SidebarRow<'_>],
    collapsed: bool,
    config: &crate::config::SidebarConfig,
) -> Vec<(Option<usize>, usize)> {
    rows.iter()
        .enumerate()
        .flat_map(|(index, row)| {
            let height = if collapsed {
                1
            } else {
                match row {
                    SidebarRow::Workspace { .. } => config.spaces.rows.len().max(1),
                    SidebarRow::Agent { pane, .. } => {
                        config.agents.rows_for_agent(pane.agent).len().max(1)
                    }
                    _ => 1,
                }
            };
            let gap = if collapsed {
                0
            } else if let Some(next) = rows.get(index + 1) {
                match (row, next) {
                    (SidebarRow::Workspace { .. }, SidebarRow::Workspace { indented, .. })
                        if !indented =>
                    {
                        config.spaces.row_gap
                    }
                    (SidebarRow::Agent { .. }, SidebarRow::Agent { .. }) => config.agents.row_gap,
                    _ => 0,
                }
            } else {
                0
            };
            (0..height)
                .map(move |line| (Some(index), line))
                .chain((0..gap).map(|_| (None, 0)))
        })
        .collect()
}

fn sidebar_token_spans(
    tokens: &[String],
    values: impl Fn(&str) -> Option<(String, Style)>,
    prefix: &str,
    overlay0: Color,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::raw(prefix.to_owned())];
    let mut visible = 0;
    let mut previous = "";
    for token in tokens {
        let Some((value, style)) = values(token) else {
            continue;
        };
        if visible > 0 {
            spans.push(Span::styled(
                if token == "git_status" || previous == "state_icon" {
                    " ".to_owned()
                } else {
                    " · ".to_owned()
                },
                Style::default().fg(overlay0),
            ));
        }
        spans.push(Span::styled(value, style));
        visible += 1;
        previous = token;
    }
    spans
}

#[allow(clippy::too_many_arguments)]
fn render_space_token_row(
    tokens: &[String],
    name: &str,
    branch: Option<&str>,
    state: Option<crate::detect::AgentDisplayState>,
    metadata: &std::collections::HashMap<String, String>,
    active: bool,
    indent: &str,
    text: Color,
    subtext0: Color,
    overlay0: Color,
    config: &crate::config::Config,
) -> Line<'static> {
    let state_icon = state
        .map(|state| state.sidebar_marker().to_owned())
        .unwrap_or_else(|| if active { "●" } else { "○" }.to_owned());
    let state_text = state
        .map(|state| state.label().to_owned())
        .unwrap_or_else(|| if active { "active" } else { "idle" }.to_owned());
    let workspace_style = Style::default().fg(if active { text } else { subtext0 });
    Line::from(sidebar_token_spans(
        tokens,
        |token| match token.strip_prefix('$').unwrap_or(token) {
            "state_icon" => Some((
                state_icon.clone(),
                Style::default().fg(state.map_or(overlay0, |state| {
                    super::ThemePalette::agent_status_color(config, state)
                })),
            )),
            "state_text" => Some((state_text.clone(), Style::default().fg(subtext0))),
            "workspace" => Some((name.to_owned(), workspace_style)),
            "branch" => branch
                .filter(|branch| !branch.is_empty())
                .map(|branch| (branch.to_owned(), Style::default().fg(subtext0))),
            "git_status" => None,
            custom => metadata
                .get(custom)
                .cloned()
                .map(|value| (value, Style::default().fg(subtext0))),
        },
        indent,
        overlay0,
    ))
}

#[allow(clippy::too_many_arguments)]
fn render_agent_token_row(
    tokens: &[String],
    pane: &crate::server::session::PaneView,
    tab_name: &str,
    workspace_name: &str,
    focused: bool,
    active_row_bg: Color,
    text: Color,
    subtext0: Color,
    overlay0: Color,
    config: &crate::config::Config,
) -> Line<'static> {
    let state = pane.agent_display_state();
    let state_icon = state.sidebar_marker().to_owned();
    let state_text = pane.agent_display_state_label().to_owned();
    let agent = pane.agent_display_name().unwrap_or("Agent").to_owned();
    let title = pane
        .display_title
        .as_deref()
        .filter(|title| !title.is_empty())
        .unwrap_or(&pane.title)
        .to_owned();
    let machine = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "local".to_owned());
    let label = pane.label.clone().filter(|label| !label.is_empty());
    let focused_style = |style: Style| {
        if focused {
            style.bg(active_row_bg)
        } else {
            style
        }
    };
    Line::from(sidebar_token_spans(
        tokens,
        |token| match token.strip_prefix('$').unwrap_or(token) {
            "state_icon" => Some((
                state_icon.clone(),
                focused_style(
                    Style::default().fg(super::ThemePalette::agent_status_color(config, state)),
                ),
            )),
            "state_text" => Some((state_text.clone(), focused_style(Style::default().fg(text)))),
            "machine" => Some((
                machine.clone(),
                focused_style(Style::default().fg(subtext0)),
            )),
            "workspace" => Some((
                workspace_name.to_owned(),
                focused_style(Style::default().fg(text)),
            )),
            "tab" => Some((
                tab_name.to_owned(),
                focused_style(Style::default().fg(subtext0)),
            )),
            "pane" => Some((
                pane.pane_id.clone(),
                focused_style(Style::default().fg(subtext0)),
            )),
            "agent" => Some((
                label.clone().unwrap_or_else(|| agent.clone()),
                focused_style(Style::default().fg(text)),
            )),
            "terminal_title" | "terminal_title_stripped" => {
                Some((title.clone(), focused_style(Style::default().fg(subtext0))))
            }
            custom => pane
                .tokens
                .get(custom)
                .cloned()
                .map(|value| (value, focused_style(Style::default().fg(subtext0)))),
        },
        "    ",
        overlay0,
    ))
}

enum SidebarRow<'a> {
    Space {
        space_id: &'a str,
        name: &'a str,
    },
    Workspace {
        space_id: &'a str,
        workspace_id: &'a str,
        name: &'a str,
        branch: Option<&'a str>,
        tokens: &'a std::collections::HashMap<String, String>,
        agent_state: Option<crate::detect::AgentDisplayState>,
        is_linked_worktree: bool,
        indented: bool,
        last_child: bool,
    },
    Agent {
        space_id: &'a str,
        workspace_id: &'a str,
        tab_id: &'a str,
        tab_name: &'a str,
        workspace_name: &'a str,
        pane: &'a crate::server::session::PaneView,
    },
    AgentHeader,
}

fn sidebar_rows_with_collapsed<'a>(
    snapshot: &'a SessionSnapshot,
    agent_priority_sort: bool,
    collapsed_groups: &HashSet<String>,
) -> Vec<SidebarRow<'a>> {
    let mut rows = Vec::new();
    let mut agents = Vec::new();
    for space in &snapshot.spaces {
        rows.push(SidebarRow::Space {
            space_id: &space.space_id,
            name: &space.name,
        });
        let mut emitted_groups = HashSet::new();
        let mut ordered_workspaces = Vec::new();
        for workspace in &space.workspaces {
            let Some(group) = workspace.worktree_group.as_deref() else {
                ordered_workspaces.push((workspace, false, false));
                continue;
            };
            if !emitted_groups.insert(group) {
                continue;
            }
            let members: Vec<_> = space
                .workspaces
                .iter()
                .filter(|candidate| candidate.worktree_group.as_deref() == Some(group))
                .collect();
            let Some(parent) = members
                .iter()
                .find(|candidate| !candidate.is_linked_worktree)
            else {
                ordered_workspaces.push((workspace, false, false));
                continue;
            };
            if members.len() < 2 {
                ordered_workspaces.push((workspace, false, false));
                continue;
            }
            ordered_workspaces.push((*parent, false, false));
            if collapsed_groups.contains(group) {
                continue;
            }
            let children: Vec<_> = members
                .iter()
                .filter(|candidate| candidate.workspace_id != parent.workspace_id)
                .copied()
                .collect();
            for (index, child) in children.iter().enumerate() {
                ordered_workspaces.push((*child, true, index + 1 == children.len()));
            }
        }
        for (workspace, indented, last_child) in ordered_workspaces {
            rows.push(SidebarRow::Workspace {
                space_id: &space.space_id,
                workspace_id: &workspace.workspace_id,
                name: &workspace.name,
                branch: workspace.branch.as_deref(),
                tokens: &workspace.tokens,
                agent_state: workspace_agent_state(
                    snapshot,
                    &space.space_id,
                    workspace,
                    collapsed_groups,
                ),
                is_linked_worktree: workspace.is_linked_worktree,
                indented,
                last_child,
            });
            for tab in &workspace.tabs {
                let pane_ids = tab
                    .layout
                    .as_ref()
                    .map(|layout| layout.pane_ids())
                    .unwrap_or_default();
                let pane_agents = snapshot.panes.iter().filter(|pane| {
                    pane.agent.is_some() && pane_ids.contains(&pane.pane_id.as_str())
                });
                for pane in pane_agents {
                    let row = SidebarRow::Agent {
                        space_id: &space.space_id,
                        workspace_id: &workspace.workspace_id,
                        tab_id: &tab.tab_id,
                        tab_name: &tab.name,
                        workspace_name: &workspace.name,
                        pane,
                    };
                    agents.push(row);
                }
            }
        }
    }
    if agent_priority_sort {
        agents.sort_by_key(|row| {
            let SidebarRow::Agent { pane, .. } = row else {
                unreachable!()
            };
            std::cmp::Reverse(agent_state_priority(pane.agent_display_state()))
        });
    }
    if !agents.is_empty() {
        rows.push(SidebarRow::AgentHeader);
        rows.extend(agents);
    }
    rows
}

fn agent_state_priority(state: crate::detect::AgentDisplayState) -> u8 {
    match state {
        crate::detect::AgentDisplayState::Blocked => 4,
        crate::detect::AgentDisplayState::Done => 3,
        crate::detect::AgentDisplayState::Working => 2,
        crate::detect::AgentDisplayState::Idle => 1,
        crate::detect::AgentDisplayState::Unknown => 0,
    }
}

fn workspace_agent_state(
    snapshot: &SessionSnapshot,
    space_id: &str,
    workspace: &WorkspaceView,
    collapsed_groups: &HashSet<String>,
) -> Option<crate::detect::AgentDisplayState> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == space_id)?;
    let workspace_ids: Vec<&str> = if let Some(group) = workspace
        .worktree_group
        .as_deref()
        .filter(|group| collapsed_groups.contains(*group))
    {
        space
            .workspaces
            .iter()
            .filter(|candidate| candidate.worktree_group.as_deref() == Some(group))
            .map(|candidate| candidate.workspace_id.as_str())
            .collect()
    } else {
        vec![workspace.workspace_id.as_str()]
    };
    snapshot
        .panes
        .iter()
        .filter(|pane| {
            pane.agent.is_some()
                && space.workspaces.iter().any(|candidate| {
                    workspace_ids.contains(&candidate.workspace_id.as_str())
                        && candidate.tabs.iter().any(|tab| {
                            tab.layout.as_ref().is_some_and(|layout| {
                                layout.pane_ids().contains(&pane.pane_id.as_str())
                            })
                        })
                })
        })
        .map(|pane| pane.agent_display_state())
        .max_by_key(|state| agent_state_priority(*state))
}

#[cfg(test)]
pub(super) fn render_sidebar(frame: &mut Frame<'_>, snapshot: &SessionSnapshot, area: Rect) {
    render_sidebar_with_collapsed(frame, snapshot, area, false);
}

#[cfg(test)]
pub(super) fn render_sidebar_with_collapsed(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
) {
    render_sidebar_with_scroll(frame, snapshot, area, collapsed, 0);
}

#[cfg(test)]
pub(super) fn render_sidebar_with_scroll(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
) {
    render_sidebar_with_scroll_and_sort(frame, snapshot, area, collapsed, scroll, false);
}

#[cfg(test)]
pub(super) fn render_sidebar_with_scroll_and_sort(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
    agent_priority_sort: bool,
) {
    render_sidebar_with_scroll_sort_and_navigation(
        frame,
        snapshot,
        area,
        collapsed,
        scroll,
        agent_priority_sort,
        None,
    );
}

#[cfg(test)]
pub(super) fn render_sidebar_with_scroll_sort_and_navigation(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
    agent_priority_sort: bool,
    navigation_workspace: Option<(&str, &str)>,
) {
    render_sidebar_with_scroll_sort_and_navigation_and_groups(
        frame,
        snapshot,
        area,
        collapsed,
        scroll,
        agent_priority_sort,
        navigation_workspace,
        &HashSet::new(),
        0.5,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_sidebar_with_scroll_sort_and_navigation_and_groups(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
    agent_priority_sort: bool,
    navigation_workspace: Option<(&str, &str)>,
    collapsed_groups: &HashSet<String>,
    sidebar_section_split: f32,
) {
    let config = crate::config::load();
    let accent = super::ThemePalette::from_config(&config).accent;
    let text = super::ThemePalette::text(&config);
    let subtext0 = super::ThemePalette::subtext0(&config);
    let overlay0 = super::ThemePalette::overlay0(&config);
    let surface_dim = super::ThemePalette::surface_dim(&config);
    let sidebar_bg = super::ThemePalette::sidebar_bg(&config);
    let active_row_bg = super::ThemePalette::active_row_bg(&config);
    let rows = sidebar_rows_with_collapsed(snapshot, agent_priority_sort, collapsed_groups);
    let body = sidebar_body(area);
    let sidebar_config = crate::config::load().sidebar;
    let visual_rows = sidebar_visual_rows(&rows, collapsed, &sidebar_config);
    let max_scroll = visual_rows.len().saturating_sub(usize::from(body.height));
    let start = scroll.min(max_scroll);
    let lines = visual_rows
        .iter()
        .skip(start)
        .take(usize::from(body.height))
        .map(|(row_index, line_index)| {
            let Some(row_index) = row_index else {
                return Line::default();
            };
            match &rows[*row_index] {
                SidebarRow::Space { space_id, name } => {
                    let active = *space_id == snapshot.active_space_id;
                    if collapsed {
                        return Line::from(if active { "S " } else { "s " });
                    }
                    let marker = if active { "● " } else { "○ " };
                    Line::from(vec![
                        Span::styled(
                            marker,
                            Style::default().fg(if active { accent } else { overlay0 }),
                        ),
                        Span::styled(
                            (*name).to_owned(),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                    ])
                }
                SidebarRow::Workspace {
                    space_id,
                    workspace_id,
                    name,
                    branch,
                    tokens,
                    agent_state,
                    is_linked_worktree,
                    indented,
                    last_child,
                } => {
                    let previewed =
                        navigation_workspace.is_some_and(|(selected_space, selected_workspace)| {
                            selected_space == *space_id && selected_workspace == *workspace_id
                        });
                    let space = snapshot
                        .spaces
                        .iter()
                        .find(|space| space.space_id == *space_id);
                    let active = *space_id == snapshot.active_space_id
                        && space.is_some_and(|space| {
                            space.active_workspace_id.as_deref() == Some(*workspace_id)
                        });
                    if collapsed {
                        let line = if active { "W " } else { "w " };
                        return if previewed {
                            Line::styled(line, Style::default().fg(Color::Black).bg(accent))
                        } else {
                            Line::from(line)
                        };
                    }
                    let indent = if *indented {
                        if *last_child {
                            "  └─ "
                        } else {
                            "  ├─ "
                        }
                    } else {
                        "  "
                    };
                    let display_name = if *is_linked_worktree {
                        format!(
                            "↳ {}",
                            branch
                                .and_then(|branch| branch.strip_prefix("worktree/"))
                                .or(*branch)
                                .unwrap_or(name)
                        )
                    } else {
                        (*name).to_owned()
                    };
                    let row = sidebar_config
                        .spaces
                        .rows
                        .get(*line_index)
                        .cloned()
                        .unwrap_or_default();
                    let line = render_space_token_row(
                        &row,
                        &display_name,
                        branch.filter(|_| !*is_linked_worktree),
                        *agent_state,
                        tokens,
                        active,
                        indent,
                        text,
                        subtext0,
                        overlay0,
                        &config,
                    );
                    let line = append_summary(line, tokens, overlay0);
                    if previewed {
                        line.style(Style::default().bg(active_row_bg))
                    } else {
                        line
                    }
                }
                SidebarRow::Agent {
                    pane,
                    tab_name,
                    workspace_name,
                    ..
                } => {
                    let focused = snapshot.focused_pane_id.as_deref() == Some(&pane.pane_id);
                    if collapsed {
                        return Line::from("A ");
                    }
                    let row = sidebar_config
                        .agents
                        .rows_for_agent(pane.agent)
                        .get(*line_index)
                        .cloned()
                        .unwrap_or_default();
                    append_summary(
                        render_agent_token_row(
                            &row,
                            pane,
                            tab_name,
                            workspace_name,
                            focused,
                            active_row_bg,
                            text,
                            subtext0,
                            overlay0,
                            &config,
                        ),
                        &pane.tokens,
                        overlay0,
                    )
                }
                SidebarRow::AgentHeader => Line::from(vec![
                    Span::styled("  Agents", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(" · priority", Style::default().fg(Color::DarkGray)),
                ]),
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(sidebar_bg))
            .block(
                ratatui::widgets::Block::default()
                    .borders(ratatui::widgets::Borders::ALL)
                    .title(sidebar_title(
                        area,
                        agent_priority_sort,
                        navigation_workspace.is_some(),
                    )),
            ),
        area,
    );
    if !collapsed {
        let sections = crate::client::sidebar::sections(sidebar_body(area), sidebar_section_split);
        if !sections.divider.is_empty() {
            let divider_style = Style::default().fg(surface_dim).bg(sidebar_bg);
            frame.render_widget(
                Paragraph::new("─".repeat(usize::from(sections.divider.width)))
                    .style(divider_style),
                sections.divider,
            );
        }
    }
    if !area.is_empty() {
        let x = area.right().saturating_sub(2);
        let y = area.bottom().saturating_sub(1);
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
            cell.set_symbol(if collapsed { ">" } else { "<" });
            cell.set_fg(accent);
        }
        if !collapsed && area.width >= 9 {
            let label_x = area.x.saturating_add(1);
            for (offset, character) in "new".chars().enumerate() {
                if let Some(cell) = frame
                    .buffer_mut()
                    .cell_mut((label_x.saturating_add(offset as u16), y))
                {
                    cell.set_symbol(&character.to_string());
                    cell.set_fg(Color::DarkGray);
                }
            }
            let label_x = area.right().saturating_sub(7);
            for (offset, character) in "menu".chars().enumerate() {
                if let Some(cell) = frame
                    .buffer_mut()
                    .cell_mut((label_x.saturating_add(offset as u16), y))
                {
                    cell.set_symbol(&character.to_string());
                    cell.set_fg(Color::DarkGray);
                }
            }
        }
    }
    if max_scroll > 0 && body.width > 1 && body.height > 0 {
        render_sidebar_scrollbar(
            frame,
            body,
            start,
            max_scroll,
            visual_rows.len(),
            overlay0,
            surface_dim,
        );
    }
}

fn visible_metadata_tokens(
    tokens: &std::collections::HashMap<String, String>,
) -> Vec<(&str, &str)> {
    let mut entries = tokens
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(right.1)));
    entries.sort_by_key(|(key, _)| *key != "summary");
    entries
}

fn append_summary(
    mut line: Line<'static>,
    tokens: &std::collections::HashMap<String, String>,
    overlay0: Color,
) -> Line<'static> {
    for (key, value) in visible_metadata_tokens(tokens) {
        if key == "summary" {
            line.spans.push(Span::styled(
                format!(" · {value}"),
                Style::default().fg(overlay0),
            ));
        }
    }
    line
}

fn sidebar_title(area: Rect, agent_priority_sort: bool, navigating: bool) -> String {
    if area.width < 36 {
        return "Spaces".into();
    }
    let mut title = format!(
        "Spaces · agents {}",
        if agent_priority_sort {
            "priority"
        } else {
            "grouped"
        }
    );
    if navigating {
        title.push_str(" · ↑/↓ choose · Enter open · Esc cancel");
    }
    title
}

fn render_sidebar_scrollbar(
    frame: &mut Frame<'_>,
    body: Rect,
    start: usize,
    max_scroll: usize,
    row_count: usize,
    overlay0: Color,
    surface_dim: Color,
) {
    let Some((thumb_top, thumb_height)) =
        sidebar_scrollbar_thumb(body, start, max_scroll, row_count)
    else {
        return;
    };
    let height = usize::from(body.height);
    let x = body.right().saturating_sub(1);
    for row in 0..height {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, body.y + row as u16)) {
            let thumb = row >= usize::from(thumb_top)
                && row < usize::from(thumb_top.saturating_add(thumb_height));
            cell.set_symbol("▕");
            cell.set_fg(if thumb { overlay0 } else { surface_dim });
        }
    }
}

#[cfg(test)]
pub(super) fn render_tabs(frame: &mut Frame<'_>, snapshot: &SessionSnapshot, area: Rect) {
    render_tabs_with_scroll(frame, snapshot, area, 0);
}

pub(super) fn render_tabs_with_scroll(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    area: Rect,
    tab_scroll: usize,
) {
    let Some(workspace) = active_workspace(snapshot) else {
        return;
    };
    let config = crate::config::load();
    let palette = super::ThemePalette::from_config(&config);
    let surface0 = super::ThemePalette::surface0(&config);
    if workspace.tabs.is_empty() {
        frame.render_widget(
            Paragraph::new("No tabs").style(Style::default().bg(surface0)),
            area,
        );
        return;
    }
    let (tab_area, scroll_left, scroll_right) = tab_layout(area, workspace.tabs.len());
    let active_index = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == workspace.active_tab_id)
        .unwrap_or(0);
    let (first_tab, widths) = tab_window_with_scroll(
        tab_area.width,
        workspace.tabs.len(),
        active_index,
        tab_scroll,
    );
    let visible_end = first_tab.saturating_add(widths.len());
    let mut x = tab_area.x;
    for (tab, width) in workspace.tabs.iter().skip(first_tab).zip(widths) {
        let rect = Rect::new(x, area.y, width, area.height);
        let selected = tab.tab_id == workspace.active_tab_id;
        let label = if tab.zoomed {
            format!("{} Z", tab.name)
        } else {
            tab.name.clone()
        };
        let style = if selected {
            Style::default()
                .fg(super::ThemePalette::panel_contrast_fg(&config))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(super::ThemePalette::overlay0(&config))
                .bg(surface0)
                .add_modifier(Modifier::DIM)
        };
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(style),
            rect,
        );
        x = x.saturating_add(width);
    }
    let marker_style = Style::default()
        .fg(super::ThemePalette::overlay0(&config))
        .bg(surface0);
    if first_tab > 0 {
        if let Some(cell) = frame.buffer_mut().cell_mut((tab_area.x, area.y)) {
            cell.set_symbol("…");
            cell.set_style(marker_style);
        }
    }
    if visible_end < workspace.tabs.len() {
        if let Some(cell) = frame
            .buffer_mut()
            .cell_mut((tab_area.right().saturating_sub(1), area.y))
        {
            cell.set_symbol("…");
            cell.set_style(marker_style);
        }
    }
    if let Some(rect) = scroll_left {
        frame.render_widget(
            Paragraph::new(" < ").style(
                Style::default()
                    .fg(if first_tab > 0 {
                        super::ThemePalette::overlay1(&config)
                    } else {
                        super::ThemePalette::overlay0(&config)
                    })
                    .bg(surface0),
            ),
            rect,
        );
    }
    if let Some(rect) = scroll_right {
        frame.render_widget(
            Paragraph::new(" > ").style(
                Style::default()
                    .fg(if visible_end < workspace.tabs.len() {
                        super::ThemePalette::overlay1(&config)
                    } else {
                        super::ThemePalette::overlay0(&config)
                    })
                    .bg(surface0),
            ),
            rect,
        );
    }
    if area.width >= 4 {
        frame.render_widget(
            Paragraph::new(" + ").alignment(Alignment::Center).style(
                Style::default()
                    .fg(super::ThemePalette::overlay1(&config))
                    .bg(super::ThemePalette::panel_bg(&config)),
            ),
            new_tab_area(area),
        );
    }
}

fn tab_strip_area(area: Rect) -> Rect {
    Rect {
        width: area.width.saturating_sub(3),
        ..area
    }
}

fn tab_layout(area: Rect, count: usize) -> (Rect, Option<Rect>, Option<Rect>) {
    let strip = tab_strip_area(area);
    let overflow = count
        .saturating_mul(usize::from(MIN_TAB_WIDTH))
        .saturating_add(count.saturating_sub(1))
        > usize::from(strip.width);
    if overflow && strip.width >= MIN_TAB_WIDTH.saturating_add(6) {
        let left = Rect::new(strip.x, strip.y, 3, strip.height);
        let right = Rect::new(strip.right().saturating_sub(3), strip.y, 3, strip.height);
        let content = Rect::new(
            left.right(),
            strip.y,
            strip.width.saturating_sub(6),
            strip.height,
        );
        (content, Some(left), Some(right))
    } else {
        (strip, None, None)
    }
}

fn new_tab_area(area: Rect) -> Rect {
    Rect::new(
        area.right().saturating_sub(3),
        area.y,
        3.min(area.width),
        area.height,
    )
}

pub fn tab_scroll_max(snapshot: &SessionSnapshot, area: Rect, collapsed: bool) -> usize {
    let main = super::layout::main_areas_for_snapshot(snapshot, area, collapsed);
    let Some(workspace) = active_workspace(snapshot) else {
        return 0;
    };
    let active_index = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == workspace.active_tab_id)
        .unwrap_or(0);
    let (content, _, _) = tab_layout(main.tabs, workspace.tabs.len());
    let (_, widths) = tab_window_with_scroll(
        content.width,
        workspace.tabs.len(),
        active_index,
        usize::MAX,
    );
    workspace.tabs.len().saturating_sub(widths.len())
}

pub fn tab_scroll_region(area: Rect, collapsed: bool, x: u16, y: u16) -> bool {
    let main = super::layout::main_areas_with_sidebar(area, collapsed);
    contains(tab_strip_area(main.tabs), x, y)
}

pub fn render_tab_drop_indicator(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    collapsed: bool,
    insert_index: usize,
) {
    render_tab_drop_indicator_with_scroll(frame, snapshot, collapsed, insert_index, 0);
}

pub fn render_tab_drop_indicator_with_scroll(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    collapsed: bool,
    insert_index: usize,
    tab_scroll: usize,
) {
    let accent = super::ThemePalette::from_config(&crate::config::load()).accent;
    let area = super::layout::main_areas_for_snapshot(snapshot, frame.area(), collapsed).tabs;
    let Some(workspace) = active_workspace(snapshot) else {
        return;
    };
    if workspace.tabs.is_empty() || area.height == 0 {
        return;
    }
    let active_index = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == workspace.active_tab_id)
        .unwrap_or(0);
    let (content, _, _) = tab_layout(area, workspace.tabs.len());
    let (first_tab, widths) = tab_window_with_scroll(
        content.width,
        workspace.tabs.len(),
        active_index,
        tab_scroll,
    );
    let x = widths
        .iter()
        .take(insert_index.saturating_sub(first_tab).min(widths.len()))
        .fold(content.x, |x, width| x.saturating_add(*width))
        .min(content.right().saturating_sub(1));
    if let Some(cell) = frame.buffer_mut().cell_mut((x, area.y)) {
        cell.set_symbol("│");
        cell.set_fg(accent);
    }
}

pub fn render_workspace_drop_indicator(frame: &mut Frame<'_>, collapsed: bool, row: u16) {
    if collapsed {
        return;
    }
    let accent = super::ThemePalette::from_config(&crate::config::load()).accent;
    let area = super::layout::main_areas_with_sidebar(frame.area(), false).sidebar;
    let body = sidebar_body(area);
    if row < body.y || row >= body.bottom() {
        return;
    }
    for x in body.x..body.right().saturating_sub(1) {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, row)) {
            cell.set_symbol("─");
            cell.set_fg(accent);
        }
    }
}

fn equal_widths(total: u16, count: usize) -> Vec<u16> {
    if count == 0 {
        return Vec::new();
    }
    let count = count.min(u16::MAX as usize) as u16;
    let base = total / count;
    let extra = total % count;
    (0..count)
        .map(|index| base + u16::from(index < extra))
        .collect()
}

fn tab_index_at(area: Rect, x: u16, count: usize, active_index: usize) -> Option<usize> {
    tab_index_at_with_scroll(area, x, count, active_index, 0)
}

fn tab_index_at_with_scroll(
    area: Rect,
    x: u16,
    count: usize,
    active_index: usize,
    tab_scroll: usize,
) -> Option<usize> {
    if count == 0 || area.width == 0 {
        return None;
    }
    let (first_tab, widths) = tab_window_with_scroll(area.width, count, active_index, tab_scroll);
    let offset = x.checked_sub(area.x)?;
    let mut edge = 0u16;
    widths
        .iter()
        .position(|width| {
            let contains = offset >= edge && offset < edge.saturating_add(*width);
            edge = edge.saturating_add(*width);
            contains
        })
        .map(|index| first_tab + index)
}

#[cfg(test)]
fn tab_window(total: u16, count: usize, active_index: usize) -> (usize, Vec<u16>) {
    tab_window_with_scroll(total, count, active_index, 0)
}

fn tab_window_with_scroll(
    total: u16,
    count: usize,
    active_index: usize,
    tab_scroll: usize,
) -> (usize, Vec<u16>) {
    if count == 0 || total == 0 {
        return (0, Vec::new());
    }
    let desired = count
        .saturating_mul(usize::from(MIN_TAB_WIDTH))
        .saturating_add(count.saturating_sub(1));
    if desired <= usize::from(total) {
        return (0, equal_widths(total, count));
    }
    let visible = usize::from((total / MIN_TAB_WIDTH).max(1)).min(count);
    let first = if tab_scroll == 0 {
        active_index
            .min(count - 1)
            .saturating_sub(visible / 2)
            .min(count - visible)
    } else {
        tab_scroll.min(count - visible)
    };
    (first, equal_widths(total, visible))
}

fn active_workspace(snapshot: &SessionSnapshot) -> Option<&WorkspaceView> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    space.active_workspace_id.as_ref().and_then(|workspace_id| {
        space
            .workspaces
            .iter()
            .find(|workspace| &workspace.workspace_id == workspace_id)
    })
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}

#[cfg(test)]
mod tests {
    use super::super::layout::{main_areas, main_areas_with_sidebar};
    use super::super::layout::{pane_rectangles, pane_sizes, split_areas, split_handles};
    use super::{
        agent_state_priority, hit_test, hit_test_with_sidebar, hit_test_with_sidebar_scroll,
        hit_test_with_sidebar_scroll_and_sort, hit_test_with_sidebar_scroll_and_sort_and_groups,
        render_sidebar, render_sidebar_with_collapsed, render_sidebar_with_scroll, render_tabs,
        sidebar_rows_with_collapsed, tab_drop_target, tab_window, workspace_drop_target,
        ClickTarget, MIN_TAB_WIDTH,
    };
    use crate::model::layout::{Direction as SplitDirection, LayoutNode};
    use crate::server::session::{SessionSnapshot, SpaceView, TabView, WorkspaceView};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::Terminal;
    use std::collections::HashSet;

    #[test]
    fn narrow_sidebar_uses_a_compact_title_like_herdr() {
        assert_eq!(
            super::sidebar_title(Rect::new(0, 0, 20, 30), false, false),
            "Spaces"
        );
        assert!(super::sidebar_title(Rect::new(0, 0, 40, 30), true, true).contains("priority"));
    }

    #[test]
    fn expanded_sidebar_matches_herdr_default_width_at_normal_terminal_size() {
        let main = main_areas_with_sidebar(Rect::new(0, 0, 120, 30), false);
        assert_eq!(main.sidebar.width, 26);
    }

    #[test]
    fn narrow_terminal_exposes_the_mobile_switcher_hit_target() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 40, 10);
        let main = main_areas_with_sidebar(area, false);
        assert!(main.sidebar.is_empty());
        assert!(main.tabs.is_empty());
        assert_eq!(main.panes.y, 2);
        assert_eq!(
            hit_test(&snapshot, area, click(area.right() - 2, area.y)),
            Some(ClickTarget::MobileSwitcher)
        );
    }

    #[test]
    fn collapsed_worktree_groups_hide_children_from_render_and_hit_testing() {
        let mut snapshot = sample_snapshot();
        snapshot.spaces[0].workspaces[0].worktree_group = Some("repo".into());
        snapshot.spaces[0].workspaces[1].is_linked_worktree = true;
        snapshot.spaces[0].workspaces[1].worktree_group = Some("repo".into());
        let full = sidebar_rows_with_collapsed(&snapshot, false, &HashSet::new());
        let collapsed =
            sidebar_rows_with_collapsed(&snapshot, false, &HashSet::from(["repo".to_owned()]));
        assert_eq!(
            full.iter()
                .filter_map(|row| match row {
                    super::SidebarRow::Workspace { workspace_id, .. } => Some(*workspace_id),
                    _ => None,
                })
                .count(),
            2
        );
        assert_eq!(
            collapsed
                .iter()
                .filter_map(|row| match row {
                    super::SidebarRow::Workspace { workspace_id, .. } => Some(*workspace_id),
                    _ => None,
                })
                .count(),
            1
        );
        let area = Rect::new(0, 0, 100, 30);
        let body = super::sidebar_body(main_areas(area).sidebar);
        assert_eq!(
            hit_test_with_sidebar_scroll_and_sort_and_groups(
                &snapshot,
                area,
                click(body.x + 3, body.y + 4),
                false,
                0,
                false,
                &HashSet::from(["repo".to_owned()]),
            ),
            None
        );
    }

    #[test]
    fn workspace_drop_target_uses_herdr_insert_index_order() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 100, 30);
        assert_eq!(
            workspace_drop_target(&snapshot, area, 0, false, "workspace-2", 4, 2),
            Some(("space-1".into(), "workspace-1".into(), 0))
        );
        assert_eq!(
            workspace_drop_target(&snapshot, area, 0, false, "workspace-1", 4, 4),
            Some(("space-1".into(), "workspace-2".into(), 2))
        );
    }

    #[test]
    fn workspace_drop_target_maps_linked_children_to_their_herdr_group_root() {
        let mut snapshot = sample_snapshot();
        snapshot.spaces[0].workspaces[0].worktree_group = Some("repo".into());
        snapshot.spaces[0].workspaces[1].is_linked_worktree = true;
        snapshot.spaces[0].workspaces[1].worktree_group = Some("repo".into());
        let mut third = snapshot.spaces[0].workspaces[1].clone();
        third.workspace_id = "workspace-3".into();
        third.name = "Build".into();
        third.is_linked_worktree = false;
        third.worktree_group = None;
        snapshot.spaces[0].workspaces.push(third);

        let area = Rect::new(0, 0, 100, 30);
        assert_eq!(
            workspace_drop_target(&snapshot, area, 0, false, "workspace-3", 4, 2),
            Some(("space-1".into(), "workspace-1".into(), 0))
        );
        assert_eq!(
            workspace_drop_target(&snapshot, area, 0, false, "workspace-1", 4, 4),
            None,
            "dropping a root on its own linked child must be a no-op"
        );
    }

    #[test]
    fn tab_drop_target_uses_herdr_insert_index_order() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 100, 30);
        let tabs = main_areas(area).tabs;
        let second_tab_x = tabs.x + tabs.width / 2 + tabs.width / 4;
        assert_eq!(
            tab_drop_target(&snapshot, area, false, "tab-2", second_tab_x, tabs.y),
            Some(("workspace-2".into(), "tab-3".into(), 2))
        );
        let first_tab_x = tabs.x + tabs.width / 4;
        assert_eq!(
            tab_drop_target(&snapshot, area, false, "tab-3", first_tab_x, tabs.y),
            Some(("workspace-2".into(), "tab-2".into(), 0))
        );
        let collapsed_tabs = main_areas_with_sidebar(area, true).tabs;
        let collapsed_second_x = collapsed_tabs.x + collapsed_tabs.width * 3 / 4;
        assert!(tab_drop_target(
            &snapshot,
            area,
            true,
            "tab-2",
            collapsed_second_x,
            collapsed_tabs.y,
        )
        .is_some());
    }

    #[test]
    fn narrow_tab_strip_keeps_the_active_tab_in_a_minimum_width_window() {
        let (first, widths) = tab_window(30, 6, 4);
        assert_eq!(first, 3);
        assert_eq!(widths.len(), 2);
        assert!(widths.iter().all(|width| *width >= MIN_TAB_WIDTH));
    }

    #[test]
    fn priority_sort_matches_herdr_agent_status_order() {
        assert!(
            agent_state_priority(crate::detect::AgentDisplayState::Blocked)
                > agent_state_priority(crate::detect::AgentDisplayState::Done)
        );
        assert!(
            agent_state_priority(crate::detect::AgentDisplayState::Done)
                > agent_state_priority(crate::detect::AgentDisplayState::Working)
        );
        assert!(
            agent_state_priority(crate::detect::AgentDisplayState::Working)
                > agent_state_priority(crate::detect::AgentDisplayState::Idle)
        );
        assert!(
            agent_state_priority(crate::detect::AgentDisplayState::Idle)
                > agent_state_priority(crate::detect::AgentDisplayState::Unknown)
        );
    }

    fn agent_pane(pane_id: &str, agent: &str, state: &str) -> crate::server::session::PaneView {
        serde_json::from_value(serde_json::json!({
            "pane_id": pane_id,
            "command": "powershell.exe",
            "args": [],
            "cwd": "C:/",
            "status": "Running",
            "scrollback_bytes": 0,
            "agent": agent,
            "agent_state": state
        }))
        .unwrap()
    }

    fn plain_pane(pane_id: &str) -> crate::server::session::PaneView {
        serde_json::from_value(serde_json::json!({
            "pane_id": pane_id,
            "command": "powershell.exe",
            "args": [],
            "cwd": "C:/",
            "status": "Running",
            "scrollback_bytes": 0,
            "agent": null,
            "agent_state": null
        }))
        .unwrap()
    }

    fn sample_snapshot() -> SessionSnapshot {
        SessionSnapshot {
            version: 1,
            spaces: vec![SpaceView {
                space_id: "space-1".into(),
                name: "Default".into(),
                workspaces: vec![
                    WorkspaceView {
                        workspace_id: "workspace-1".into(),
                        name: "Current project".into(),
                        repository_path: None,
                        branch: None,
                        is_linked_worktree: false,
                        worktree_group: None,
                        tabs: vec![TabView {
                            tab_id: "tab-1".into(),
                            name: "Main".into(),
                            layout: None,
                            focused_pane_id: None,
                            zoomed: false,
                        }],
                        active_tab_id: "tab-1".into(),
                        tokens: std::collections::HashMap::new(),
                    },
                    WorkspaceView {
                        workspace_id: "workspace-2".into(),
                        name: "Docs".into(),
                        repository_path: None,
                        branch: None,
                        is_linked_worktree: false,
                        worktree_group: None,
                        tabs: vec![
                            TabView {
                                tab_id: "tab-2".into(),
                                name: "Main".into(),
                                layout: None,
                                focused_pane_id: None,
                                zoomed: false,
                            },
                            TabView {
                                tab_id: "tab-3".into(),
                                name: "Activity".into(),
                                layout: Some(LayoutNode::pane("pane-1").split(
                                    SplitDirection::Horizontal,
                                    0.5,
                                    "pane-2",
                                )),
                                focused_pane_id: Some("pane-2".into()),
                                zoomed: false,
                            },
                        ],
                        active_tab_id: "tab-3".into(),
                        tokens: std::collections::HashMap::new(),
                    },
                ],
                active_workspace_id: Some("workspace-2".into()),
            }],
            active_space_id: "space-1".into(),
            panes: vec![plain_pane("pane-1"), plain_pane("pane-2")],
            focused_pane_id: Some("pane-2".into()),
            popup_pane_id: None,
            popup_width: 0,
            popup_height: 0,
            popup_width_spec: None,
            popup_height_spec: None,
            overlay_pane_id: None,
            overlay_previous_focus: None,
            overlay_previous_zoomed: false,
            event_sequence: 0,
        }
    }

    #[test]
    fn workspace_picker_highlight_is_distinct_from_the_active_workspace() {
        let snapshot = sample_snapshot();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let sidebar = main_areas(Rect::new(0, 0, 100, 30)).sidebar;
        terminal
            .draw(|frame| {
                super::render_sidebar_with_scroll_sort_and_navigation(
                    frame,
                    &snapshot,
                    sidebar,
                    false,
                    0,
                    false,
                    Some(("space-1", "workspace-1")),
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer.cell((sidebar.x + 5, sidebar.y + 2)).unwrap().bg,
            ratatui::style::Color::Rgb(30, 30, 46)
        );
        assert_ne!(
            buffer.cell((sidebar.x + 5, sidebar.y + 3)).unwrap().bg,
            ratatui::style::Color::Rgb(30, 30, 46)
        );
        assert_eq!(
            snapshot.spaces[0].active_workspace_id.as_deref(),
            Some("workspace-2"),
            "previewing a workspace must not switch it"
        );
    }

    fn click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn right_click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn clicks_target_spaces_workspaces_tabs_and_panes() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 100, 30);
        let main = main_areas(area);
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                click(main.sidebar.x + 1, main.sidebar.y + 1)
            ),
            Some(ClickTarget::Space("space-1".into()))
        );
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                click(main.sidebar.x + 1, main.sidebar.y + 4)
            ),
            Some(ClickTarget::Workspace {
                space_id: "space-1".into(),
                workspace_id: "workspace-2".into(),
            })
        );
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                click(main.tabs.x + main.tabs.width * 3 / 4, main.tabs.y)
            ),
            Some(ClickTarget::Tab("tab-3".into()))
        );
        assert_eq!(
            hit_test(&snapshot, area, click(main.tabs.right() - 2, main.tabs.y)),
            Some(ClickTarget::NewTab)
        );
        let [first, second] = split_areas(main.panes, SplitDirection::Horizontal, 0.5);
        let pane_sizes = pane_sizes(&snapshot, main.panes);
        assert_eq!(pane_sizes.len(), 2);
        assert_eq!(pane_sizes[0].pane_id, "pane-1");
        assert_eq!(pane_sizes[0].cols, first.width.saturating_sub(3).max(1));
        assert_eq!(pane_sizes[0].rows, first.height.saturating_sub(2).max(1));
        assert_eq!(pane_sizes[1].pane_id, "pane-2");
        assert_eq!(pane_sizes[1].cols, second.width.saturating_sub(3).max(1));
        assert_eq!(pane_sizes[1].rows, second.height.saturating_sub(2).max(1));
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                click(first.x + first.width / 2, first.y + first.height / 2)
            ),
            Some(ClickTarget::Pane("pane-1".into()))
        );
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                click(second.x + second.width / 2, second.y + second.height / 2)
            ),
            Some(ClickTarget::Pane("pane-2".into()))
        );
    }

    #[test]
    fn compact_sidebar_matches_herdr_width_and_keeps_mouse_navigation() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 100, 30);
        let main = super::super::layout::main_areas_with_sidebar(area, true);
        assert_eq!(main.sidebar.width, 4);
        assert_eq!(
            hit_test_with_sidebar(
                &snapshot,
                area,
                click(main.sidebar.right() - 2, main.sidebar.bottom() - 1),
                true,
            ),
            Some(ClickTarget::SidebarToggle)
        );
        assert_eq!(
            hit_test_with_sidebar(
                &snapshot,
                area,
                click(main.sidebar.x + 1, main.sidebar.y + 3),
                true,
            ),
            Some(ClickTarget::Workspace {
                space_id: "space-1".into(),
                workspace_id: "workspace-2".into(),
            })
        );

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_sidebar_with_collapsed(frame, &snapshot, main.sidebar, true);
            })
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("S"));
        assert!(content.contains("W"));
        assert!(content.contains(">"));
    }

    #[test]
    fn long_sidebar_lists_scroll_and_scrollbar_click_selects_a_viewport() {
        let mut snapshot = sample_snapshot();
        let template = snapshot.spaces[0].workspaces[0].clone();
        for index in 0..8 {
            let mut workspace = template.clone();
            workspace.workspace_id = format!("workspace-extra-{index}");
            workspace.name = format!("Extra {index}");
            snapshot.spaces[0].workspaces.push(workspace);
        }

        let area = Rect::new(0, 0, 80, 8);
        let main = super::super::layout::main_areas_with_sidebar(area, false);
        let body = super::sidebar_body(main.sidebar);
        let max_scroll = super::sidebar_scroll_max(&snapshot, area, false);
        assert_eq!(max_scroll, 15);
        assert_eq!(
            hit_test_with_sidebar_scroll(
                &snapshot,
                area,
                click(body.right() - 1, body.y + 2),
                false,
                0,
            ),
            Some(ClickTarget::SidebarScroll(4))
        );
        let track_x = body.right() - 1;
        assert_eq!(
            super::sidebar_scroll_thumb_grab_offset(&snapshot, area, false, 0, track_x, body.y),
            Some(0)
        );
        assert_eq!(
            super::sidebar_scroll_thumb_grab_offset(&snapshot, area, false, 0, track_x, body.y - 1,),
            None
        );
        assert_eq!(
            super::sidebar_scroll_offset_from_drag_row(&snapshot, area, false, body.y, 0,),
            0
        );
        assert_eq!(
            super::sidebar_scroll_offset_from_drag_row(
                &snapshot,
                area,
                false,
                body.bottom() - 1,
                0,
            ),
            max_scroll
        );
        assert_eq!(
            hit_test_with_sidebar_scroll(&snapshot, area, click(body.x, body.y), false, 5),
            Some(ClickTarget::Workspace {
                space_id: "space-1".into(),
                workspace_id: "workspace-extra-0".into(),
            })
        );

        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_sidebar_with_scroll(frame, &snapshot, main.sidebar, false, 15))
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Extra 7"));
        assert!(content.contains("▕"));
    }

    #[test]
    fn zoomed_focused_pane_uses_full_area_and_hides_other_split_handles() {
        let mut snapshot = sample_snapshot();
        let tab = &mut snapshot.spaces[0].workspaces[1].tabs[1];
        tab.zoomed = true;
        let area = Rect::new(0, 0, 100, 30);
        let main = main_areas(area);

        let panes = pane_rectangles(&snapshot, main.panes);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pane_id, "pane-2");
        assert_eq!(panes[0].rect, main.panes);
        assert!(split_handles(&snapshot, main.panes).is_empty());
        assert_eq!(pane_sizes(&snapshot, main.panes).len(), 1);
        assert_eq!(
            hit_test(&snapshot, area, click(main.panes.x + 1, main.panes.y + 1)),
            Some(ClickTarget::Pane("pane-2".into()))
        );
    }

    #[test]
    fn sidebar_and_tab_strip_render_active_names() {
        let mut snapshot = sample_snapshot();
        snapshot.spaces[0].workspaces[0].branch = Some("main".into());
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let areas = main_areas(Rect::new(0, 0, 100, 30));
        terminal
            .draw(|frame| {
                render_sidebar(frame, &snapshot, areas.sidebar);
                render_tabs(frame, &snapshot, areas.tabs);
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Default"));
        assert!(content.contains("Current project"));
        assert!(content.contains("main"));
        assert!(content.contains("Docs"));
        assert!(content.contains("Activity"));
        assert!(content.contains("+"));
    }

    #[test]
    fn sidebar_marks_linked_worktree_children() {
        let mut snapshot = sample_snapshot();
        snapshot.spaces[0].workspaces[0].worktree_group = Some("repo".into());
        let workspace = &mut snapshot.spaces[0].workspaces[1];
        workspace.branch = Some("worktree/feature".into());
        workspace.is_linked_worktree = true;
        workspace.worktree_group = Some("repo".into());
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let sidebar = main_areas(Rect::new(0, 0, 100, 30)).sidebar;
        terminal
            .draw(|frame| render_sidebar(frame, &snapshot, sidebar))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("└─ ● ↳ feature"));
    }

    #[test]
    fn sidebar_shows_agent_identity_and_state_for_each_pane() {
        let mut snapshot = sample_snapshot();
        snapshot.panes = vec![
            agent_pane("pane-1", "codex", "working"),
            agent_pane("pane-2", "open_code", "blocked"),
            agent_pane("pane-other", "codex", "idle"),
        ];
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let sidebar = main_areas(Rect::new(0, 0, 100, 30)).sidebar;
        terminal
            .draw(|frame| render_sidebar(frame, &snapshot, sidebar))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Codex"));
        assert!(content.contains("OpenCode"));
        assert!(content.contains("W"));
        assert!(content.contains("!"));
        assert!(!content.contains("Idle"));
    }

    #[test]
    fn sidebar_highlights_the_focused_agent_row() {
        let mut snapshot = sample_snapshot();
        snapshot.panes = vec![
            agent_pane("pane-1", "codex", "working"),
            agent_pane("pane-2", "open_code", "blocked"),
        ];
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let sidebar = main_areas(Rect::new(0, 0, 100, 30)).sidebar;
        terminal
            .draw(|frame| render_sidebar(frame, &snapshot, sidebar))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rows = super::sidebar_rows_with_collapsed(&snapshot, false, &HashSet::new());
        let visual = super::sidebar_visual_rows(&rows, false, &crate::config::load().sidebar);
        let focused_row = visual
            .iter()
            .enumerate()
            .find_map(|(index, (row, _))| {
                matches!(
                    row.and_then(|row| rows.get(row)),
                    Some(super::SidebarRow::Agent { pane, .. }) if pane.pane_id == "pane-2"
                )
                .then_some(index)
            })
            .expect("focused agent row exists");
        let focused_y = super::sidebar_body(sidebar).y + focused_row as u16;
        assert_eq!(
            buffer.cell((sidebar.x + 8, focused_y)).unwrap().bg,
            ratatui::style::Color::Rgb(30, 30, 46)
        );
    }

    #[test]
    fn sidebar_orders_blocked_agents_before_working_agents() {
        let mut snapshot = sample_snapshot();
        snapshot.panes = vec![
            agent_pane("pane-1", "codex", "working"),
            agent_pane("pane-2", "open_code", "blocked"),
        ];
        let area = Rect::new(0, 0, 100, 30);
        let sidebar = main_areas(area).sidebar;
        assert_eq!(
            hit_test_with_sidebar_scroll_and_sort(
                &snapshot,
                area,
                click(sidebar.x + 3, sidebar.y + 7),
                false,
                0,
                true,
            ),
            Some(ClickTarget::Agent {
                space_id: "space-1".into(),
                workspace_id: "workspace-2".into(),
                tab_id: "tab-3".into(),
                pane_id: "pane-2".into(),
            })
        );
        assert_eq!(
            hit_test_with_sidebar_scroll_and_sort(
                &snapshot,
                area,
                click(sidebar.x + 3, sidebar.y + 9),
                false,
                0,
                true,
            ),
            Some(ClickTarget::Agent {
                space_id: "space-1".into(),
                workspace_id: "workspace-2".into(),
                tab_id: "tab-3".into(),
                pane_id: "pane-1".into(),
            })
        );
        assert_eq!(
            hit_test_with_sidebar_scroll_and_sort(
                &snapshot,
                area,
                click(sidebar.x + 3, sidebar.y),
                false,
                0,
                true,
            ),
            Some(ClickTarget::ToggleAgentSort)
        );
        assert_eq!(
            hit_test_with_sidebar_scroll_and_sort(
                &snapshot,
                area,
                click(sidebar.x + 3, sidebar.y),
                false,
                0,
                false,
            ),
            Some(ClickTarget::ToggleAgentSort)
        );
    }

    #[test]
    fn priority_sidebar_renders_a_global_agent_section_in_state_order() {
        let mut snapshot = sample_snapshot();
        snapshot.panes = vec![
            agent_pane("pane-1", "codex", "working"),
            agent_pane("pane-2", "open_code", "blocked"),
        ];
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let sidebar = main_areas(Rect::new(0, 0, 100, 30)).sidebar;
        terminal
            .draw(|frame| {
                super::render_sidebar_with_scroll_and_sort(
                    frame, &snapshot, sidebar, false, 0, true,
                )
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("priority"));
        assert!(content.contains("Agents"));
        assert!(content.find("OpenCode").unwrap() < content.find("Codex").unwrap());
    }

    #[test]
    fn workspace_sidebar_uses_the_highest_priority_agent_state() {
        let mut snapshot = sample_snapshot();
        snapshot.panes = vec![
            agent_pane("pane-1", "codex", "working"),
            agent_pane("pane-2", "open_code", "blocked"),
        ];
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let sidebar = main_areas(Rect::new(0, 0, 100, 30)).sidebar;
        terminal
            .draw(|frame| {
                super::render_sidebar_with_scroll_and_sort(
                    frame, &snapshot, sidebar, false, 0, false,
                )
            })
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("! Docs"), "workspace status: {content}");
    }

    #[test]
    fn agent_sidebar_renders_custom_state_label_and_summary_token() {
        let mut snapshot = sample_snapshot();
        let mut pane = agent_pane("pane-1", "codex", "working");
        pane.state_labels
            .insert("working".into(), "Indexing".into());
        pane.tokens.insert("summary".into(), "12 files".into());
        pane.tokens.insert("model".into(), "gpt-5".into());
        assert_eq!(
            super::visible_metadata_tokens(&pane.tokens),
            vec![("summary", "12 files"), ("model", "gpt-5")]
        );
        snapshot.panes = vec![pane];
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let sidebar = main_areas(Rect::new(0, 0, 100, 30)).sidebar;
        terminal
            .draw(|frame| {
                super::render_sidebar_with_scroll_and_sort(
                    frame, &snapshot, sidebar, false, 0, false,
                )
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("12 files"));
    }

    #[test]
    fn configured_sidebar_tokens_render_selected_values() {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("model".into(), "gpt-5".into());
        let line = super::render_space_token_row(
            &["workspace".into(), "$model".into(), "branch".into()],
            "Current project",
            Some("main"),
            None,
            &metadata,
            true,
            "  ",
            Color::White,
            Color::Gray,
            Color::DarkGray,
            &crate::config::Config::default(),
        );
        let content = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(content.contains("Current project"));
        assert!(content.contains("gpt-5"));
        assert!(content.contains("main"));
    }

    #[test]
    fn sidebar_row_gaps_are_scrollable_but_not_clickable() {
        let snapshot = sample_snapshot();
        let rows = super::sidebar_rows_with_collapsed(&snapshot, false, &HashSet::new());
        let mut config = crate::config::SidebarConfig::default();
        config.spaces.row_gap = 2;
        let visual = super::sidebar_visual_rows(&rows, false, &config);
        assert!(visual.iter().any(|(row, _)| row.is_none()));
        assert_eq!(visual.iter().filter(|(row, _)| row.is_none()).count(), 2);
    }

    #[test]
    fn clicking_sidebar_agent_selects_its_workspace_tab_and_pane() {
        let mut snapshot = sample_snapshot();
        snapshot.panes = vec![agent_pane("pane-1", "codex", "working")];
        let area = Rect::new(0, 0, 100, 30);
        let sidebar = main_areas(area).sidebar;
        let target = (sidebar.y..sidebar.bottom()).find_map(|row| {
            match hit_test(&snapshot, area, click(sidebar.x + 3, row)) {
                Some(target @ ClickTarget::Agent { .. }) => Some(target),
                _ => None,
            }
        });
        assert_eq!(
            target,
            Some(ClickTarget::Agent {
                space_id: "space-1".into(),
                workspace_id: "workspace-2".into(),
                tab_id: "tab-3".into(),
                pane_id: "pane-1".into(),
            })
        );
    }

    #[test]
    fn split_border_hit_targets_the_matching_tree_path() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 100, 30);
        let main = main_areas(area);
        let split = super::super::layout::split_handles(&snapshot, main.panes)
            .into_iter()
            .next()
            .expect("sample layout has one split");
        assert_eq!(split.path, Vec::<bool>::new());
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                click(
                    split.hit_rect.x,
                    split.hit_rect.y + split.hit_rect.height / 2
                )
            ),
            Some(ClickTarget::SplitBorder(Vec::new()))
        );
    }

    #[test]
    fn right_click_targets_workspace_tab_and_pane_context_menus() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 100, 30);
        let main = main_areas(area);
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                right_click(main.sidebar.x + 1, main.sidebar.y + 4)
            ),
            Some(ClickTarget::Workspace {
                space_id: "space-1".into(),
                workspace_id: "workspace-2".into(),
            })
        );
        assert_eq!(
            hit_test(&snapshot, area, right_click(main.tabs.x + 2, main.tabs.y)),
            Some(ClickTarget::Tab("tab-2".into()))
        );
        assert_eq!(
            hit_test(
                &snapshot,
                area,
                right_click(main.panes.x + 2, main.panes.y + 2)
            ),
            Some(ClickTarget::Pane("pane-1".into()))
        );
    }

    #[test]
    fn sidebar_footer_targets_new_workspace_and_global_menu() {
        let snapshot = sample_snapshot();
        let area = Rect::new(0, 0, 100, 30);
        let sidebar = main_areas(area).sidebar;
        let footer_y = sidebar.bottom() - 1;
        assert_eq!(
            hit_test(&snapshot, area, click(sidebar.x + 1, footer_y)),
            Some(ClickTarget::NewWorkspace)
        );
        assert_eq!(
            hit_test(&snapshot, area, click(sidebar.right() - 4, footer_y)),
            Some(ClickTarget::GlobalMenu)
        );
    }

    #[test]
    fn nested_split_handles_keep_paths_for_their_own_boundaries() {
        let mut snapshot = sample_snapshot();
        snapshot.spaces[0].workspaces[1].tabs[1].layout = Some(
            LayoutNode::pane("pane-1")
                .split(SplitDirection::Horizontal, 0.5, "pane-2")
                .split(SplitDirection::Vertical, 0.5, "pane-3"),
        );
        let area = Rect::new(0, 0, 100, 30);
        let panes = main_areas(area).panes;
        let handles = super::super::layout::split_handles(&snapshot, panes);
        assert_eq!(
            handles
                .iter()
                .map(|handle| handle.path.clone())
                .collect::<Vec<_>>(),
            vec![Vec::<bool>::new(), vec![false]]
        );
        assert_ne!(handles[0].area, handles[1].area);
    }

    #[test]
    fn small_terminal_geometry_stays_inside_the_pane_area_with_many_splits() {
        let mut snapshot = sample_snapshot();
        let mut layout = LayoutNode::pane("pane-1");
        for number in 2..=8 {
            let direction = if number % 2 == 0 {
                SplitDirection::Horizontal
            } else {
                SplitDirection::Vertical
            };
            layout = layout.split(direction, 0.5, format!("pane-{number}"));
        }
        for number in 3..=8 {
            snapshot.panes.push(plain_pane(&format!("pane-{number}")));
        }
        snapshot.spaces[0].workspaces[1].tabs[1].layout = Some(layout);
        let area = Rect::new(0, 0, 20, 6);
        let pane_area = main_areas(area).panes;
        let panes = super::super::layout::pane_rectangles(&snapshot, pane_area);
        let handles = super::super::layout::split_handles(&snapshot, pane_area);
        assert_eq!(panes.len(), 8);
        assert_eq!(handles.len(), 7);
        assert!(panes.iter().all(|pane| {
            pane.rect.x >= pane_area.x
                && pane.rect.y >= pane_area.y
                && pane.rect.right() <= pane_area.right()
                && pane.rect.bottom() <= pane_area.bottom()
        }));
        assert!(handles.iter().all(|handle| {
            handle.hit_rect.x >= pane_area.x
                && handle.hit_rect.y >= pane_area.y
                && handle.hit_rect.right() <= pane_area.right()
                && handle.hit_rect.bottom() <= pane_area.bottom()
        }));
    }
}
