use super::copy_mode::CopyMode;
use super::selection::TextSelection;
use crate::model::layout::Direction as SplitDirection;
use crate::server::session::SessionSnapshot;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub(super) struct SplitDrag {
    pub(super) path: Vec<bool>,
    pub(super) direction: SplitDirection,
    pub(super) area: Rect,
    pub(super) grab_offset: i32,
    pub(super) last_sent_at: Option<Instant>,
}

pub(super) struct PaneMouseCapture {
    pub(super) pane_id: String,
    pub(super) rect: Rect,
    pub(super) button: MouseButton,
}

pub(super) struct WorkspaceDrag {
    pub(super) space_id: String,
    pub(super) workspace_id: String,
    pub(super) drop_row: Option<u16>,
}

pub(super) struct TabDrag {
    pub(super) workspace_id: String,
    pub(super) tab_id: String,
    pub(super) insert_index: Option<usize>,
}

#[derive(Default)]
pub(super) struct MouseState {
    pub(super) sidebar_collapsed: bool,
    pub(super) agent_priority_sort: bool,
    pub(super) sidebar_scroll: usize,
    pub(super) sidebar_scroll_drag: Option<u16>,
    pub(super) preferences_path: std::path::PathBuf,
    pub(super) split_drag: Option<SplitDrag>,
    pub(super) workspace_drag: Option<WorkspaceDrag>,
    pub(super) tab_drag: Option<TabDrag>,
    pub(super) pane_capture: Option<PaneMouseCapture>,
    pub(super) selection: Option<TextSelection>,
    pub(super) last_click: Option<PaneClick>,
    pub(super) scroll_offsets: HashMap<String, usize>,
    pub(super) scrollback_views: HashMap<String, CachedScrollbackView>,
    pub(super) copy_mode: Option<CopyMode>,
    pub(super) navigation_workspace: Option<(String, String)>,
    pub(super) collapsed_worktree_groups: HashSet<String>,
}

pub(super) struct PaneClick {
    pub(super) pane_id: String,
    pub(super) row: u16,
    pub(super) col: u16,
    pub(super) at: Instant,
}

pub(super) struct CachedScrollbackView {
    pub(super) bytes: Vec<u8>,
    pub(super) rows: u16,
    pub(super) cols: u16,
    pub(super) offset: usize,
    pub(super) screen: String,
}

impl PaneClick {
    pub(super) fn is_double_click_for(
        &self,
        pane_id: &str,
        row: u16,
        col: u16,
        now: Instant,
    ) -> bool {
        self.pane_id == pane_id
            && now.duration_since(self.at) <= Duration::from_millis(350)
            && self.row.abs_diff(row) <= 1
            && self.col.abs_diff(col) <= 1
    }
}

impl SplitDrag {
    pub(super) fn ratio_at(&self, mouse: MouseEvent) -> f32 {
        let (pointer, origin, extent) = match self.direction {
            SplitDirection::Horizontal => (
                i32::from(mouse.column),
                i32::from(self.area.x),
                self.area.width,
            ),
            SplitDirection::Vertical => (
                i32::from(mouse.row),
                i32::from(self.area.y),
                self.area.height,
            ),
        };
        ((pointer + self.grab_offset - origin) as f32 / f32::from(extent.max(1))).clamp(0.1, 0.9)
    }
}

pub(super) fn pane_mouse_target(
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    capture: &Option<PaneMouseCapture>,
    sidebar_collapsed: bool,
) -> Option<(String, Rect)> {
    if let MouseEventKind::Drag(button) | MouseEventKind::Up(button) = mouse.kind {
        return capture
            .as_ref()
            .filter(|capture| capture.button == button)
            .map(|capture| (capture.pane_id.clone(), capture.rect));
    }
    if let Some(popup_id) = snapshot.popup_pane_id.as_deref() {
        let popup = super::renderer::popup_rect(
            super::renderer::pane_content_area_with_sidebar(area, sidebar_collapsed),
            snapshot.popup_width,
            snapshot.popup_height,
        );
        let inner = Rect::new(
            popup.x.saturating_add(1),
            popup.y.saturating_add(1),
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        if mouse.column >= inner.x
            && mouse.column < inner.right()
            && mouse.row >= inner.y
            && mouse.row < inner.bottom()
        {
            return Some((popup_id.to_owned(), popup));
        }
    }
    super::renderer::pane_rectangles(
        snapshot,
        super::renderer::pane_content_area_with_sidebar(area, sidebar_collapsed),
    )
    .into_iter()
    .find(|pane| {
        let inner = Rect::new(
            pane.rect.x.saturating_add(1),
            pane.rect.y.saturating_add(1),
            pane.rect.width.saturating_sub(2),
            pane.rect.height.saturating_sub(2),
        );
        mouse.column >= inner.x
            && mouse.column < inner.right()
            && mouse.row >= inner.y
            && mouse.row < inner.bottom()
    })
    .map(|pane| (pane.pane_id, pane.rect))
}

pub(super) fn should_forward_pane_mouse(
    pane: &crate::server::session::PaneView,
    kind: MouseEventKind,
) -> bool {
    let reports = match kind {
        MouseEventKind::Down(_) => pane.mouse_reporting,
        MouseEventKind::Up(_) => pane.mouse_release,
        MouseEventKind::Drag(_) => pane.mouse_motion,
        MouseEventKind::Moved => pane.mouse_any_motion,
        MouseEventKind::ScrollUp
        | MouseEventKind::ScrollDown
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => pane.mouse_reporting,
    };
    reports && (kind != MouseEventKind::Down(MouseButton::Right) || pane.right_click_passthrough)
}

pub(super) fn visible_web_url_at_point(
    snapshot: &SessionSnapshot,
    pane_area: Rect,
    column: u16,
    row: u16,
) -> Option<String> {
    let pane_rect = super::renderer::pane_rectangles(snapshot, pane_area)
        .into_iter()
        .find(|pane| {
            let inner = Rect::new(
                pane.rect.x.saturating_add(1),
                pane.rect.y.saturating_add(1),
                pane.rect.width.saturating_sub(2),
                pane.rect.height.saturating_sub(2),
            );
            column >= inner.x && column < inner.right() && row >= inner.y && row < inner.bottom()
        })?;
    let pane = snapshot
        .panes
        .iter()
        .find(|candidate| candidate.pane_id == pane_rect.pane_id)?;
    let inner_x = pane_rect.rect.x.saturating_add(1);
    let inner_y = pane_rect.rect.y.saturating_add(1);
    if let Some(link) = pane.hyperlinks.iter().find(|link| {
        link.row == row.saturating_sub(inner_y) && link.col == column.saturating_sub(inner_x)
    }) {
        if super::links::is_safe_web_url(&link.uri) {
            return Some(link.uri.clone());
        }
    }
    super::links::web_url_at_cell(
        &pane.screen,
        row.saturating_sub(inner_y),
        column.saturating_sub(inner_x),
    )
}

pub(super) fn clear_mouse_capture(capture: &mut Option<PaneMouseCapture>, kind: MouseEventKind) {
    match kind {
        MouseEventKind::Up(button)
            if capture
                .as_ref()
                .is_some_and(|capture| capture.button == button) =>
        {
            *capture = None;
        }
        _ => {}
    }
}
