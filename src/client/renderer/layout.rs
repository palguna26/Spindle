use crate::model::layout::{Direction as SplitDirection, LayoutNode};
use crate::server::session::SessionSnapshot;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders};

#[derive(Clone, Copy)]
pub(super) struct MainAreas {
    pub(super) sidebar: Rect,
    pub(super) tabs: Rect,
    pub(super) panes: Rect,
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
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area)[0];
    let sidebar_width = if sidebar_collapsed {
        4.min(area.width.saturating_sub(1))
    } else {
        area.width.min((area.width / 4).clamp(12, 28))
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_width), Constraint::Min(1)])
        .split(body);
    let right = columns[1];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(right);
    MainAreas {
        sidebar: columns[0],
        tabs: rows[0],
        panes: rows[1],
    }
}

pub(crate) fn pane_content_area(area: Rect) -> Rect {
    main_areas(area).panes
}

pub(crate) fn pane_content_area_with_sidebar(area: Rect, sidebar_collapsed: bool) -> Rect {
    main_areas_with_sidebar(area, sidebar_collapsed).panes
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
    pane_rectangles(snapshot, area)
        .into_iter()
        .map(|pane| {
            let (cols, rows) = pane_inner_size(pane.rect);
            PaneSize {
                pane_id: pane.pane_id,
                cols,
                rows,
            }
        })
        .collect()
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

pub(crate) fn pane_inner_size(area: Rect) -> (u16, u16) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    (inner.width.max(1), inner.height.max(1))
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
