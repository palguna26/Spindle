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

pub(super) fn main_areas(area: Rect) -> MainAreas {
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area)[0];
    let sidebar_width = area.width.min((area.width / 4).clamp(12, 28));
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

pub(crate) fn pane_rectangles(snapshot: &SessionSnapshot, area: Rect) -> Vec<PaneRect> {
    let mut panes = Vec::new();
    if let Some(layout) = active_layout(snapshot) {
        collect_pane_rectangles(layout, area, &mut panes);
    }
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

pub(crate) fn pane_inner_size(area: Rect) -> (u16, u16) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    (inner.width.max(1), inner.height.max(1))
}

fn active_layout(snapshot: &SessionSnapshot) -> Option<&LayoutNode> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == space.active_workspace_id)?;
    workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)?
        .layout
        .as_ref()
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
