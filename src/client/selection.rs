use ratatui::layout::Rect;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextSelection {
    pub(crate) pane_id: String,
    pub(crate) inner: Rect,
    pub(crate) anchor: (u16, u16),
    pub(crate) cursor: (u16, u16),
}

impl TextSelection {
    pub(crate) fn new(pane_id: String, inner: Rect, x: u16, y: u16) -> Self {
        let point = point_in(inner, x, y);
        Self {
            pane_id,
            inner,
            anchor: point,
            cursor: point,
        }
    }

    pub(crate) fn drag(&mut self, x: u16, y: u16) {
        self.cursor = point_in(self.inner, x, y);
    }

    pub(crate) fn has_range(&self) -> bool {
        self.anchor != self.cursor
    }

    pub(crate) fn ordered(&self) -> ((u16, u16), (u16, u16)) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    pub(crate) fn text(&self, screen: &str) -> String {
        let ((start_row, start_col), (end_row, end_col)) = self.ordered();
        let lines = screen.lines().collect::<Vec<_>>();
        let mut selected = String::new();
        for row in start_row..=end_row {
            if row > start_row {
                selected.push('\n');
            }
            let Some(line) = lines.get(usize::from(row)) else {
                continue;
            };
            let lower = if row == start_row { start_col } else { 0 };
            let upper = if row == end_row {
                end_col.saturating_add(1)
            } else {
                self.inner.width
            };
            let mut column = 0u16;
            for character in line.chars() {
                let width = UnicodeWidthChar::width(character).unwrap_or(0).max(1) as u16;
                let character_end = column.saturating_add(width);
                if character_end > lower && column < upper {
                    selected.push(character);
                }
                column = character_end;
                if column >= upper {
                    break;
                }
            }
        }
        selected
    }
}

fn point_in(area: Rect, x: u16, y: u16) -> (u16, u16) {
    (
        y.saturating_sub(area.y).min(area.height.saturating_sub(1)),
        x.saturating_sub(area.x).min(area.width.saturating_sub(1)),
    )
}

#[cfg(test)]
mod tests {
    use super::TextSelection;
    use ratatui::layout::Rect;

    #[test]
    fn selection_clamps_to_pane_and_copies_in_reading_order() {
        let mut selection = TextSelection::new("pane-1".into(), Rect::new(4, 2, 8, 3), 5, 2);
        selection.drag(100, 100);
        assert_eq!(selection.anchor, (0, 1));
        assert_eq!(selection.cursor, (2, 7));
        assert_eq!(selection.text("hello\nwide\nlast"), "ello\nwide\nlast");
        selection.drag(4, 2);
        assert_eq!(selection.text("hello"), "he");
    }

    #[test]
    fn selection_keeps_wide_characters_whole() {
        let selection = TextSelection {
            pane_id: "pane-1".into(),
            inner: Rect::new(0, 0, 8, 1),
            anchor: (0, 1),
            cursor: (0, 1),
        };
        assert_eq!(selection.text("A界B"), "界");
    }
}
