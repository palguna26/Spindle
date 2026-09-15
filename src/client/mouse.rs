use super::copy_mode::CopyMode;
use super::selection::TextSelection;
use crate::model::layout::Direction as SplitDirection;
use crossterm::event::{MouseButton, MouseEvent};
use ratatui::layout::Rect;
use std::collections::HashMap;
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

#[derive(Default)]
pub(super) struct MouseState {
    pub(super) sidebar_collapsed: bool,
    pub(super) agent_priority_sort: bool,
    pub(super) sidebar_scroll: usize,
    pub(super) sidebar_scroll_drag: Option<u16>,
    pub(super) preferences_path: std::path::PathBuf,
    pub(super) split_drag: Option<SplitDrag>,
    pub(super) pane_capture: Option<PaneMouseCapture>,
    pub(super) selection: Option<TextSelection>,
    pub(super) last_click: Option<PaneClick>,
    pub(super) scroll_offsets: HashMap<String, usize>,
    pub(super) scrollback_views: HashMap<String, CachedScrollbackView>,
    pub(super) copy_mode: Option<CopyMode>,
    pub(super) navigation_workspace: Option<(String, String)>,
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
