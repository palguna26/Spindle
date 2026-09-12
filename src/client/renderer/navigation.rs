use crate::server::session::{SessionSnapshot, WorkspaceView};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::layout::{main_areas, pane_rectangles, split_handles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
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
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return None;
    }
    let x = mouse.column;
    let y = mouse.row;
    let main = main_areas(area);
    if contains(main.sidebar, x, y) {
        let body = Rect::new(
            main.sidebar.x.saturating_add(1),
            main.sidebar.y.saturating_add(1),
            main.sidebar.width.saturating_sub(2),
            main.sidebar.height.saturating_sub(2),
        );
        if !contains(body, x, y) {
            return None;
        }
        let rows = sidebar_rows(snapshot);
        let row = y.saturating_sub(body.y) as usize;
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
        if let Some(split) = split_handles(snapshot, main.panes)
            .into_iter()
            .find(|split| contains(split.hit_rect, x, y))
        {
            return Some(ClickTarget::SplitBorder(split.path));
        }
        let pane = pane_rectangles(snapshot, main.panes)
            .into_iter()
            .find(|pane| contains(pane.rect, x, y))?;
        return Some(ClickTarget::Pane(pane.pane_id));
    }
    None
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

pub(super) fn render_sidebar(frame: &mut Frame<'_>, snapshot: &SessionSnapshot, area: Rect) {
    let rows = sidebar_rows(snapshot);
    let lines = rows
        .iter()
        .map(|row| match row {
            SidebarRow::Space { space_id, name } => {
                let active = *space_id == snapshot.active_space_id;
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
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        if active { "● " } else { "○ " },
                        Style::default().fg(if active {
                            Color::Green
                        } else {
                            Color::DarkGray
                        }),
                    ),
                    Span::styled(
                        (*name).to_owned(),
                        Style::default().fg(if active { Color::White } else { Color::Gray }),
                    ),
                ])
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
        frame.render_widget(
            Paragraph::new(tab.name.clone())
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
    use super::super::layout::{pane_sizes, split_areas};
    use super::{hit_test, main_areas, render_sidebar, render_tabs, ClickTarget};
    use crate::model::layout::{Direction as SplitDirection, LayoutNode};
    use crate::server::session::{SessionSnapshot, SpaceView, TabView, WorkspaceView};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

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
