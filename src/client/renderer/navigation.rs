use crate::server::session::{SessionSnapshot, WorkspaceView};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::layout::{pane_rectangles, split_handles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    SidebarToggle,
    SidebarScroll(usize),
    Space(String),
    Workspace {
        space_id: String,
        workspace_id: String,
    },
    Tab(String),
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
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
    ) {
        return None;
    }
    let x = mouse.column;
    let y = mouse.row;
    let main = super::layout::main_areas_with_sidebar(area, sidebar_collapsed);
    if contains(main.sidebar, x, y) {
        if y == main.sidebar.bottom().saturating_sub(1)
            && x == main.sidebar.right().saturating_sub(2)
        {
            return Some(ClickTarget::SidebarToggle);
        }
        let body = sidebar_body(main.sidebar);
        if !contains(body, x, y) {
            return None;
        }
        let rows = sidebar_rows(snapshot);
        let max_scroll = rows.len().saturating_sub(usize::from(body.height));
        if max_scroll > 0 && body.width > 1 && x == body.right().saturating_sub(1) {
            let row = usize::from(y.saturating_sub(body.y));
            let offset =
                row.saturating_mul(max_scroll) / usize::from(body.height.saturating_sub(1).max(1));
            return Some(ClickTarget::SidebarScroll(offset.min(max_scroll)));
        }
        let row =
            usize::from(y.saturating_sub(body.y)).saturating_add(sidebar_scroll.min(max_scroll));
        return rows.get(row).map(|row| match row {
            SidebarRow::Space { space_id, .. } => ClickTarget::Space(space_id.to_string()),
            SidebarRow::Workspace {
                space_id,
                workspace_id,
                ..
            } => ClickTarget::Workspace {
                space_id: space_id.to_string(),
                workspace_id: workspace_id.to_string(),
            },
        });
    }
    if contains(main.tabs, x, y) {
        let workspace = active_workspace(snapshot)?;
        let index = tab_index_at(main.tabs, x, workspace.tabs.len())?;
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

pub fn sidebar_scroll_max(snapshot: &SessionSnapshot, area: Rect, collapsed: bool) -> usize {
    let sidebar = super::layout::main_areas_with_sidebar(area, collapsed).sidebar;
    let body = sidebar_body(sidebar);
    sidebar_rows(snapshot)
        .len()
        .saturating_sub(usize::from(body.height))
}

pub fn sidebar_scroll_region(area: Rect, collapsed: bool, x: u16, y: u16) -> bool {
    let sidebar = super::layout::main_areas_with_sidebar(area, collapsed).sidebar;
    contains(sidebar_body(sidebar), x, y)
}

fn sidebar_body(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
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
    },
}

fn workspace_agent_counts(snapshot: &SessionSnapshot, workspace_id: &str) -> [usize; 4] {
    let Some(workspace) = snapshot
        .spaces
        .iter()
        .flat_map(|space| &space.workspaces)
        .find(|workspace| workspace.workspace_id == workspace_id)
    else {
        return [0; 4];
    };
    let pane_ids = workspace
        .tabs
        .iter()
        .filter_map(|tab| tab.layout.as_ref())
        .flat_map(|layout| layout.pane_ids())
        .collect::<Vec<_>>();
    let mut counts = [0; 4];
    for pane in &snapshot.panes {
        if !pane_ids.contains(&pane.pane_id.as_str()) || pane.agent.is_none() {
            continue;
        }
        let index = match pane
            .agent_state
            .unwrap_or(crate::detect::AgentState::Unknown)
        {
            crate::detect::AgentState::Unknown => 0,
            crate::detect::AgentState::Idle => 1,
            crate::detect::AgentState::Working => 2,
            crate::detect::AgentState::Blocked => 3,
        };
        counts[index] += 1;
    }
    counts
}

fn sidebar_rows(snapshot: &SessionSnapshot) -> Vec<SidebarRow<'_>> {
    snapshot
        .spaces
        .iter()
        .flat_map(|space| {
            std::iter::once(SidebarRow::Space {
                space_id: &space.space_id,
                name: &space.name,
            })
            .chain(
                space
                    .workspaces
                    .iter()
                    .map(|workspace| SidebarRow::Workspace {
                        space_id: &space.space_id,
                        workspace_id: &workspace.workspace_id,
                        name: &workspace.name,
                    }),
            )
        })
        .collect()
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

pub(super) fn render_sidebar_with_scroll(
    frame: &mut Frame<'_>,
    snapshot: &SessionSnapshot,
    area: Rect,
    collapsed: bool,
    scroll: usize,
) {
    let rows = sidebar_rows(snapshot);
    let body = sidebar_body(area);
    let max_scroll = rows.len().saturating_sub(usize::from(body.height));
    let start = scroll.min(max_scroll);
    let lines = rows
        .iter()
        .skip(start)
        .take(usize::from(body.height))
        .map(|row| match row {
            SidebarRow::Space { space_id, name } => {
                let active = *space_id == snapshot.active_space_id;
                if collapsed {
                    return Line::from(if active { "S " } else { "s " });
                }
                let marker = if active { "● " } else { "○ " };
                Line::from(vec![
                    Span::styled(
                        marker,
                        Style::default().fg(if active { Color::Cyan } else { Color::DarkGray }),
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
            } => {
                let space = snapshot
                    .spaces
                    .iter()
                    .find(|space| space.space_id == *space_id);
                let active = *space_id == snapshot.active_space_id
                    && space.is_some_and(|space| space.active_workspace_id == *workspace_id);
                if collapsed {
                    return Line::from(if active { "W " } else { "w " });
                }
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(
                        if active { "● " } else { "○ " },
                        Style::default().fg(if active {
                            Color::Green
                        } else {
                            Color::DarkGray
                        }),
                    ),
                ];
                let counts = workspace_agent_counts(snapshot, workspace_id);
                let labels = ["?", "I", "W", "!"];
                let colors = [Color::DarkGray, Color::Green, Color::Yellow, Color::Red];
                for (index, count) in counts.into_iter().enumerate() {
                    if count > 0 {
                        spans.push(Span::styled(
                            format!(" {}{count}", labels[index]),
                            Style::default().fg(colors[index]),
                        ));
                    }
                }
                spans.push(Span::styled(
                    (*name).to_owned(),
                    Style::default().fg(if active { Color::White } else { Color::Gray }),
                ));
                Line::from(spans)
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title("Spaces"),
        ),
        area,
    );
    if !area.is_empty() {
        let x = area.right().saturating_sub(2);
        let y = area.bottom().saturating_sub(1);
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
            cell.set_symbol(if collapsed { ">" } else { "<" });
            cell.set_fg(Color::Cyan);
        }
    }
    if max_scroll > 0 && body.width > 1 && body.height > 0 {
        render_sidebar_scrollbar(frame, body, start, max_scroll, rows.len());
    }
}

fn render_sidebar_scrollbar(
    frame: &mut Frame<'_>,
    body: Rect,
    start: usize,
    max_scroll: usize,
    row_count: usize,
) {
    let height = usize::from(body.height);
    let thumb_height = (height.saturating_mul(height) / row_count.max(1))
        .max(1)
        .min(height);
    let thumb_top = start.saturating_mul(height.saturating_sub(thumb_height)) / max_scroll.max(1);
    let x = body.right().saturating_sub(1);
    for row in 0..height {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, body.y + row as u16)) {
            let thumb = row >= thumb_top && row < thumb_top + thumb_height;
            cell.set_symbol(if thumb { "#" } else { "|" });
            cell.set_fg(if thumb { Color::Gray } else { Color::DarkGray });
        }
    }
}

pub(super) fn render_tabs(frame: &mut Frame<'_>, snapshot: &SessionSnapshot, area: Rect) {
    let Some(workspace) = active_workspace(snapshot) else {
        return;
    };
    if workspace.tabs.is_empty() {
        frame.render_widget(Paragraph::new("No tabs"), area);
        return;
    }
    let widths = equal_widths(area.width, workspace.tabs.len());
    let mut x = area.x;
    for (tab, width) in workspace.tabs.iter().zip(widths) {
        let rect = Rect::new(x, area.y, width, area.height);
        let selected = tab.tab_id == workspace.active_tab_id;
        let label = if tab.zoomed {
            format!("{} Z", tab.name)
        } else {
            tab.name.clone()
        };
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(if selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(Color::Gray)
                }),
            rect,
        );
        x = x.saturating_add(width);
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

fn tab_index_at(area: Rect, x: u16, count: usize) -> Option<usize> {
    if count == 0 || area.width == 0 {
        return None;
    }
    let widths = equal_widths(area.width, count);
    let offset = x.checked_sub(area.x)?;
    let mut edge = 0u16;
    widths.iter().position(|width| {
        let contains = offset >= edge && offset < edge.saturating_add(*width);
        edge = edge.saturating_add(*width);
        contains
    })
}

fn active_workspace(snapshot: &SessionSnapshot) -> Option<&WorkspaceView> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == space.active_workspace_id)
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}

#[cfg(test)]
mod tests {
    use super::super::layout::main_areas;
    use super::super::layout::{pane_rectangles, pane_sizes, split_areas, split_handles};
    use super::{
        hit_test, hit_test_with_sidebar, hit_test_with_sidebar_scroll, render_sidebar,
        render_sidebar_with_collapsed, render_sidebar_with_scroll, render_tabs, ClickTarget,
    };
    use crate::model::layout::{Direction as SplitDirection, LayoutNode};
    use crate::server::session::{SessionSnapshot, SpaceView, TabView, WorkspaceView};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

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
                        tabs: vec![TabView {
                            tab_id: "tab-1".into(),
                            name: "Main".into(),
                            layout: None,
                            focused_pane_id: None,
                            zoomed: false,
                        }],
                        active_tab_id: "tab-1".into(),
                    },
                    WorkspaceView {
                        workspace_id: "workspace-2".into(),
                        name: "Docs".into(),
                        repository_path: None,
                        branch: None,
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
                    },
                ],
                active_workspace_id: "workspace-2".into(),
            }],
            active_space_id: "space-1".into(),
            panes: Vec::new(),
            focused_pane_id: Some("pane-2".into()),
            event_sequence: 0,
        }
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
                click(main.sidebar.x + 1, main.sidebar.y + 3)
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
        let [first, second] = split_areas(main.panes, SplitDirection::Horizontal, 0.5);
        let pane_sizes = pane_sizes(&snapshot, main.panes);
        assert_eq!(pane_sizes.len(), 2);
        assert_eq!(pane_sizes[0].pane_id, "pane-1");
        assert_eq!(pane_sizes[0].cols, first.width.saturating_sub(2).max(1));
        assert_eq!(pane_sizes[0].rows, first.height.saturating_sub(2).max(1));
        assert_eq!(pane_sizes[1].pane_id, "pane-2");
        assert_eq!(pane_sizes[1].cols, second.width.saturating_sub(2).max(1));
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
        assert_eq!(max_scroll, 6);
        assert_eq!(
            hit_test_with_sidebar_scroll(
                &snapshot,
                area,
                click(body.right() - 1, body.y + 2),
                false,
                0,
            ),
            Some(ClickTarget::SidebarScroll(3))
        );
        assert_eq!(
            hit_test_with_sidebar_scroll(&snapshot, area, click(body.x, body.y), false, 3),
            Some(ClickTarget::Workspace {
                space_id: "space-1".into(),
                workspace_id: "workspace-extra-0".into(),
            })
        );

        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_sidebar_with_scroll(frame, &snapshot, main.sidebar, false, 6))
            .unwrap();
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Extra 7"));
        assert!(content.contains("#"));
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
        let snapshot = sample_snapshot();
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
        assert!(content.contains("Docs"));
        assert!(content.contains("Activity"));
    }

    #[test]
    fn sidebar_shows_agent_state_counts_for_each_workspace() {
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
        assert!(content.contains("W1"));
        assert!(content.contains("!1"));
        assert!(!content.contains("I1"));
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
                right_click(main.sidebar.x + 1, main.sidebar.y + 3)
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
