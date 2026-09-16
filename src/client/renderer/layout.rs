use crate::model::layout::{Direction as SplitDirection, LayoutNode};
use crate::server::session::SessionSnapshot;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders};

#[derive(Clone, Copy)]
pub(super) struct MainAreas {
    pub(super) sidebar: Rect,
    pub(super) tabs: Rect,
    pub(super) panes: Rect,
    pub(super) mobile_header: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneRect {
    pub(crate) pane_id: String,
    pub(crate) rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneSize {
    pub(crate) pane_id: String,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitHandle {
    pub(crate) path: Vec<bool>,
    pub(crate) direction: SplitDirection,
    pub(crate) area: Rect,
    pub(crate) pos: u16,
    pub(crate) hit_rect: Rect,
}

pub(super) fn main_areas(area: Rect) -> MainAreas {
    main_areas_with_sidebar(area, false)
}

pub(crate) fn sidebar_area(area: Rect, sidebar_collapsed: bool) -> Rect {
    main_areas_with_sidebar(area, sidebar_collapsed).sidebar
}

pub(super) fn main_areas_with_sidebar(area: Rect, sidebar_collapsed: bool) -> MainAreas {
    let config = crate::config::load();
    main_areas_with_sidebar_and_tab_count(
        area,
        sidebar_collapsed,
        usize::MAX,
        config.tab_bar_position,
    )
}

pub(super) fn main_areas_for_snapshot(
    snapshot: &SessionSnapshot,
    area: Rect,
    sidebar_collapsed: bool,
) -> MainAreas {
    let tab_count = active_workspace(snapshot).map_or(0, |workspace| workspace.tabs.len());
    let config = crate::config::load();
    main_areas_with_sidebar_and_tab_count(
        area,
        sidebar_collapsed,
        tab_count,
        config.tab_bar_position,
    )
}

pub(super) fn main_areas_for_snapshot_with_tab_bar_position(
    snapshot: &SessionSnapshot,
    area: Rect,
    sidebar_collapsed: bool,
    tab_bar_position: crate::config::TabBarPosition,
) -> MainAreas {
    let tab_count = active_workspace(snapshot).map_or(0, |workspace| workspace.tabs.len());
    main_areas_with_sidebar_and_tab_count(area, sidebar_collapsed, tab_count, tab_bar_position)
}

fn main_areas_with_sidebar_and_tab_count(
    area: Rect,
    sidebar_collapsed: bool,
    tab_count: usize,
    tab_bar_position: crate::config::TabBarPosition,
) -> MainAreas {
    let body = area;
    let config = crate::config::load();
    if area.width <= config.mobile_width_threshold {
        let header_height = body.height.min(2);
        return MainAreas {
            sidebar: Rect::default(),
            tabs: Rect::default(),
            panes: Rect::new(
                0,
                body.y.saturating_add(header_height),
                area.width,
                body.height.saturating_sub(header_height),
            ),
            mobile_header: Rect::new(0, body.y, area.width, header_height),
        };
    }
    let (sidebar_min_width, sidebar_max_width) = crate::config::sidebar_bounds(&config);
    let sidebar_width = if sidebar_collapsed {
        match config.sidebar_collapsed_mode {
            crate::config::SidebarCollapsedMode::Compact => 4.min(area.width.saturating_sub(1)),
            crate::config::SidebarCollapsedMode::Hidden => 0,
        }
    } else {
        area.width.min(
            config
                .sidebar_width
                .clamp(sidebar_min_width, sidebar_max_width),
        )
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_width), Constraint::Min(1)])
        .split(body);
    let right = columns[1];
    let tab_bar_hidden = config.hide_tab_bar_when_single_tab && tab_count == 1;
    let tab_height = if tab_bar_hidden { 0 } else { 1 };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(match tab_bar_position {
            crate::config::TabBarPosition::Top => {
                [Constraint::Length(tab_height), Constraint::Min(1)]
            }
            crate::config::TabBarPosition::Bottom => {
                [Constraint::Min(1), Constraint::Length(tab_height)]
            }
        })
        .split(right);
    let (tabs, panes) = match tab_bar_position {
        crate::config::TabBarPosition::Top => (rows[0], rows[1]),
        crate::config::TabBarPosition::Bottom => (rows[1], rows[0]),
    };
    MainAreas {
        sidebar: columns[0],
        tabs,
        panes,
        mobile_header: Rect::default(),
    }
}

pub(super) fn mobile_switch_rect(header: Rect) -> Rect {
    let width = 10.min(header.width);
    Rect::new(
        header.right().saturating_sub(width),
        header.y,
        width,
        header.height,
    )
}

pub(crate) fn pane_content_area(area: Rect) -> Rect {
    main_areas(area).panes
}

pub(crate) fn pane_content_area_for_snapshot(
    snapshot: &SessionSnapshot,
    area: Rect,
    sidebar_collapsed: bool,
) -> Rect {
    main_areas_for_snapshot(snapshot, area, sidebar_collapsed).panes
}

pub(crate) fn pane_rectangles(snapshot: &SessionSnapshot, area: Rect) -> Vec<PaneRect> {
    let mut panes = Vec::new();
    let Some(tab) = active_tab(snapshot) else {
        return panes;
    };
    if tab.zoomed {
        if let Some(pane_id) = tab.focused_pane_id.as_ref() {
            panes.push(PaneRect {
                pane_id: pane_id.clone(),
                rect: area,
            });
        }
    } else if let Some(layout) = tab.layout.as_ref() {
        collect_pane_rectangles(layout, area, &mut panes);
    }
    panes.retain(|pane| {
        snapshot
            .panes
            .iter()
            .any(|known| known.pane_id == pane.pane_id)
    });
    panes
}

pub(crate) fn pane_sizes(snapshot: &SessionSnapshot, area: Rect) -> Vec<PaneSize> {
    let panes = pane_rectangles(snapshot, area);
    let config = crate::config::load();
    let mut sizes = panes
        .into_iter()
        .map(|pane| {
            let alternate_screen = snapshot
                .panes
                .iter()
                .find(|known| known.pane_id == pane.pane_id)
                .is_some_and(|known| known.alternate_screen);
            let borders = pane_borders_for_rect(
                pane.rect,
                &pane_rectangles(snapshot, area),
                config.pane_borders,
                config.pane_outer_borders,
                config.pane_gaps,
            );
            let (cols, rows) = pane_inner_size_with_options(
                pane.rect,
                borders,
                config.pane_scrollbars && !alternate_screen,
            );
            PaneSize {
                pane_id: pane.pane_id,
                cols,
                rows,
            }
        })
        .collect::<Vec<_>>();
    if let Some(popup_id) = snapshot.popup_pane_id.as_ref() {
        let popup = super::popup_rect_with_specs(
            area,
            snapshot.popup_width,
            snapshot.popup_height,
            snapshot.popup_width_spec,
            snapshot.popup_height_spec,
        );
        let inner = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .inner(popup);
        sizes.push(PaneSize {
            pane_id: popup_id.clone(),
            cols: inner.width,
            rows: inner.height,
        });
    }
    sizes
}

pub(crate) fn split_handles(snapshot: &SessionSnapshot, area: Rect) -> Vec<SplitHandle> {
    let mut handles = Vec::new();
    if active_tab(snapshot).is_some_and(|tab| tab.zoomed) {
        return handles;
    }
    if let Some(layout) = active_tab(snapshot).and_then(|tab| tab.layout.as_ref()) {
        collect_split_handles(layout, area, Vec::new(), &mut handles);
    }
    handles
}

pub(crate) fn pane_inner_size_with_options(
    area: Rect,
    borders: Borders,
    scrollbars: bool,
) -> (u16, u16) {
    let inner = pane_inner_area(area, borders, scrollbars);
    (inner.width.max(1), inner.height.max(1))
}

pub(crate) fn pane_inner_area(area: Rect, borders: Borders, scrollbars: bool) -> Rect {
    let inner = Block::default().borders(borders).inner(area);
    if scrollbars && inner.width > 4 {
        Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(1),
            inner.height,
        )
    } else {
        inner
    }
}

pub(crate) fn pane_borders_for_rect(
    rect: Rect,
    panes: &[PaneRect],
    mode: crate::config::PaneBorders,
    outer: bool,
    gaps: bool,
) -> Borders {
    let mut borders = if mode.shows_borders(panes.len() > 1) {
        Borders::ALL
    } else {
        Borders::NONE
    };
    if !borders.is_empty() && !outer {
        let left = panes.iter().all(|pane| pane.rect.x >= rect.x);
        let top = panes.iter().all(|pane| pane.rect.y >= rect.y);
        let right = panes.iter().all(|pane| pane.rect.right() <= rect.right());
        let bottom = panes.iter().all(|pane| pane.rect.bottom() <= rect.bottom());
        if left {
            borders.remove(Borders::LEFT);
        }
        if top {
            borders.remove(Borders::TOP);
        }
        if right {
            borders.remove(Borders::RIGHT);
        }
        if bottom {
            borders.remove(Borders::BOTTOM);
        }
    }
    if !gaps {
        let right_neighbor = panes.iter().any(|pane| {
            pane.rect.x == rect.right()
                && pane.rect.y < rect.bottom()
                && pane.rect.bottom() > rect.y
        });
        let below_neighbor = panes.iter().any(|pane| {
            pane.rect.y == rect.bottom() && pane.rect.x < rect.right() && pane.rect.right() > rect.x
        });
        if right_neighbor {
            borders.remove(Borders::RIGHT);
        }
        if below_neighbor {
            borders.remove(Borders::BOTTOM);
        }
    }
    borders
}

fn active_tab(snapshot: &SessionSnapshot) -> Option<&crate::server::session::TabView> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace_id = space.active_workspace_id.as_ref()?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| &workspace.workspace_id == workspace_id)?;
    workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)
}

fn active_workspace(snapshot: &SessionSnapshot) -> Option<&crate::server::session::WorkspaceView> {
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

fn collect_pane_rectangles(node: &LayoutNode, area: Rect, panes: &mut Vec<PaneRect>) {
    match node {
        LayoutNode::Pane { pane_id } => panes.push(PaneRect {
            pane_id: pane_id.clone(),
            rect: area,
        }),
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let [first_area, second_area] = split_areas(area, *direction, *ratio);
            collect_pane_rectangles(first, first_area, panes);
            collect_pane_rectangles(second, second_area, panes);
        }
    }
}

fn collect_split_handles(
    node: &LayoutNode,
    area: Rect,
    path: Vec<bool>,
    handles: &mut Vec<SplitHandle>,
) {
    let LayoutNode::Split {
        direction,
        ratio,
        first,
        second,
    } = node
    else {
        return;
    };

    let [first_area, second_area] = split_areas(area, *direction, *ratio);
    let pos = match direction {
        SplitDirection::Horizontal => first_area.right(),
        SplitDirection::Vertical => first_area.bottom(),
    };
    handles.push(SplitHandle {
        path: path.clone(),
        direction: *direction,
        area,
        pos,
        hit_rect: split_hit_rect(area, *direction, pos),
    });

    let mut first_path = path.clone();
    first_path.push(false);
    collect_split_handles(first, first_area, first_path, handles);
    let mut second_path = path;
    second_path.push(true);
    collect_split_handles(second, second_area, second_path, handles);
}

fn split_hit_rect(area: Rect, direction: SplitDirection, pos: u16) -> Rect {
    match direction {
        SplitDirection::Horizontal => {
            let x = pos.saturating_sub(1).max(area.x);
            let right = pos.saturating_add(1).min(area.right());
            Rect::new(x, area.y, right.saturating_sub(x), area.height)
        }
        SplitDirection::Vertical => {
            let y = pos.saturating_sub(1).max(area.y);
            let bottom = pos.saturating_add(1).min(area.bottom());
            Rect::new(area.x, y, area.width, bottom.saturating_sub(y))
        }
    }
}

pub(super) fn split_areas(area: Rect, direction: SplitDirection, ratio: f32) -> [Rect; 2] {
    let percentage = (ratio * 100.0).round() as u16;
    let layout_direction = match direction {
        SplitDirection::Horizontal => Direction::Horizontal,
        SplitDirection::Vertical => Direction::Vertical,
    };
    let areas = Layout::default()
        .direction(layout_direction)
        .constraints([
            Constraint::Percentage(percentage),
            Constraint::Percentage(100 - percentage),
        ])
        .split(area);
    [areas[0], areas[1]]
}

#[cfg(test)]
mod tests {
    use super::{
        main_areas_with_sidebar_and_tab_count, pane_borders_for_rect, pane_inner_size_with_options,
        PaneRect,
    };
    use crate::config::PaneBorders;
    use ratatui::layout::Rect;
    use ratatui::widgets::Borders;

    #[test]
    fn auto_borders_only_frame_split_panes_like_herdr() {
        let single = [PaneRect {
            pane_id: "one".into(),
            rect: Rect::new(0, 0, 10, 5),
        }];
        assert_eq!(
            pane_borders_for_rect(single[0].rect, &single, PaneBorders::Auto, true, true),
            Borders::NONE
        );

        let split = [
            PaneRect {
                pane_id: "one".into(),
                rect: Rect::new(0, 0, 5, 5),
            },
            PaneRect {
                pane_id: "two".into(),
                rect: Rect::new(5, 0, 5, 5),
            },
        ];
        assert_eq!(
            pane_borders_for_rect(split[0].rect, &split, PaneBorders::Auto, true, true),
            Borders::ALL
        );
    }

    #[test]
    fn pane_outer_and_gap_options_remove_the_matching_edges() {
        let split = [
            PaneRect {
                pane_id: "one".into(),
                rect: Rect::new(0, 0, 5, 5),
            },
            PaneRect {
                pane_id: "two".into(),
                rect: Rect::new(5, 0, 5, 5),
            },
        ];
        let borders =
            pane_borders_for_rect(split[0].rect, &split, PaneBorders::Always, false, false);
        assert!(!borders.contains(Borders::LEFT));
        assert!(!borders.contains(Borders::TOP));
        assert!(!borders.contains(Borders::BOTTOM));
        assert!(!borders.contains(Borders::RIGHT));
    }

    #[test]
    fn alternate_screen_reclaims_scrollbar_column_like_herdr() {
        let area = Rect::new(0, 0, 20, 8);
        let borders = Borders::NONE;

        let host_size = pane_inner_size_with_options(area, borders, true);
        let alternate_size = pane_inner_size_with_options(area, borders, false);

        assert_eq!(host_size.0, 19);
        assert_eq!(alternate_size.0, 20);
        assert_eq!(host_size.1, alternate_size.1);
    }

    #[test]
    fn terminal_panes_use_the_full_height_like_herdr() {
        let areas = main_areas_with_sidebar_and_tab_count(
            Rect::new(0, 0, 80, 24),
            false,
            2,
            crate::config::TabBarPosition::Top,
        );

        assert_eq!(areas.panes.y, 1);
        assert_eq!(areas.panes.height, 23);
    }
}
