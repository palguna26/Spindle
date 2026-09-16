use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Thumb {
    pub(super) top: u16,
    pub(super) len: u16,
}

pub(super) fn max_offset_for_pane(
    pane: &crate::server::session::PaneView,
    viewport_height: u16,
) -> usize {
    let total_rows = pane.screen.lines().count().max(1).saturating_add(
        pane.scrollback
            .iter()
            .filter(|byte| **byte == b'\n')
            .count(),
    );
    total_rows.saturating_sub(usize::from(viewport_height))
}

pub(super) fn thumb(
    max_offset: usize,
    viewport_rows: u16,
    track: Rect,
    offset: usize,
) -> Option<Thumb> {
    if max_offset == 0 || track.height == 0 {
        return None;
    }
    let track_height = usize::from(track.height);
    let total_rows = max_offset.saturating_add(usize::from(viewport_rows));
    let thumb_len = ((usize::from(viewport_rows) * track_height) as f32 / total_rows as f32)
        .round()
        .max(1.0)
        .min(track_height as f32) as usize;
    let max_thumb_top = track_height.saturating_sub(thumb_len);
    let scrolled_from_top = max_offset.saturating_sub(offset.min(max_offset));
    let thumb_top = if max_thumb_top == 0 {
        0
    } else {
        (scrolled_from_top * max_thumb_top / max_offset.max(1)) as u16
    };
    Some(Thumb {
        top: track.y.saturating_add(thumb_top),
        len: thumb_len as u16,
    })
}

fn offset_from_thumb_top(max_offset: usize, viewport_rows: u16, track: Rect, top: usize) -> usize {
    if max_offset == 0 || track.height == 0 {
        return 0;
    }
    let thumb_len = thumb(max_offset, viewport_rows, track, 0)
        .map(|thumb| usize::from(thumb.len))
        .unwrap_or(1)
        .min(usize::from(track.height));
    let max_thumb_top = usize::from(track.height).saturating_sub(thumb_len);
    if max_thumb_top == 0 {
        return 0;
    }
    let desired_top = top.min(max_thumb_top);
    let scrolled_from_top =
        ((desired_top * max_offset) as f32 / max_thumb_top as f32).round() as usize;
    max_offset.saturating_sub(scrolled_from_top)
}

pub(super) fn offset_from_row(
    max_offset: usize,
    viewport_rows: u16,
    track: Rect,
    row: u16,
) -> usize {
    let Some(thumb) = thumb(max_offset, viewport_rows, track, 0) else {
        return 0;
    };
    let clamped_row = row.clamp(track.y, track.bottom().saturating_sub(1));
    let row_offset = usize::from(clamped_row.saturating_sub(track.y));
    let thumb_center = usize::from(thumb.len) / 2;
    offset_from_thumb_top(
        max_offset,
        viewport_rows,
        track,
        row_offset.saturating_sub(thumb_center),
    )
}

pub(super) fn thumb_grab_offset(
    max_offset: usize,
    viewport_rows: u16,
    track: Rect,
    row: u16,
    offset: usize,
) -> Option<u16> {
    let thumb = thumb(max_offset, viewport_rows, track, offset)?;
    (row >= thumb.top && row < thumb.top.saturating_add(thumb.len))
        .then_some(row.saturating_sub(thumb.top))
}

pub(super) fn offset_from_drag_row(
    max_offset: usize,
    viewport_rows: u16,
    track: Rect,
    row: u16,
    grab_offset: u16,
) -> usize {
    let clamped_row = row.clamp(track.y, track.bottom().saturating_sub(1));
    let row_offset = usize::from(clamped_row.saturating_sub(track.y));
    offset_from_thumb_top(
        max_offset,
        viewport_rows,
        track,
        row_offset.saturating_sub(usize::from(grab_offset)),
    )
}

pub(super) fn render(
    buffer: &mut Buffer,
    max_offset: usize,
    viewport_rows: u16,
    track: Rect,
    focused: bool,
    offset: usize,
) {
    let Some(thumb) = thumb(max_offset, viewport_rows, track, offset) else {
        return;
    };
    let track_color = if focused {
        Color::DarkGray
    } else {
        Color::Black
    };
    let thumb_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    for row in track.y..track.bottom() {
        if let Some(cell) = buffer.cell_mut((track.x, row)) {
            cell.set_symbol("│");
            cell.set_style(Style::default().fg(track_color));
        }
    }
    for row in thumb.top..thumb.top.saturating_add(thumb.len) {
        if let Some(cell) = buffer.cell_mut((track.x, row)) {
            cell.set_symbol("█");
            cell.set_style(Style::default().fg(thumb_color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{offset_from_drag_row, offset_from_row, thumb, thumb_grab_offset};
    use ratatui::layout::Rect;

    #[test]
    fn thumb_uses_viewport_to_size_the_track_indicator() {
        let thumb = thumb(90, 10, Rect::new(0, 0, 1, 10), 90).expect("thumb");
        assert_eq!(thumb.len, 1);
    }

    #[test]
    fn track_clicks_map_top_to_history_and_bottom_to_live() {
        let track = Rect::new(0, 10, 1, 11);
        assert_eq!(offset_from_row(100, 10, track, track.y), 100);
        assert_eq!(offset_from_row(100, 10, track, track.bottom() - 1), 0);
    }

    #[test]
    fn dragging_keeps_the_thumb_grab_offset() {
        let track = Rect::new(0, 0, 1, 20);
        let grab = thumb_grab_offset(100, 10, track, 1, 100).expect("thumb grab");
        assert_eq!(offset_from_drag_row(100, 10, track, 19, grab), 0);
    }
}
