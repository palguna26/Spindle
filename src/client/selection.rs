use ratatui::layout::Rect;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextSelection {
    pub(crate) pane_id: String,
    pub(crate) inner: Rect,
    pub(crate) anchor: (u16, u16),
    pub(crate) cursor: (u16, u16),
    pub(crate) word_selection: bool,
}

impl TextSelection {
    pub(crate) fn new(pane_id: String, inner: Rect, x: u16, y: u16) -> Self {
        let point = point_in(inner, x, y);
        Self {
            pane_id,
            inner,
            anchor: point,
            cursor: point,
            word_selection: false,
        }
    }

    pub(crate) fn drag(&mut self, x: u16, y: u16) {
        self.cursor = point_in(self.inner, x, y);
    }

    pub(crate) fn word(
        pane_id: String,
        inner: Rect,
        row: u16,
        start_col: u16,
        end_col: u16,
    ) -> Self {
        Self {
            pane_id,
            inner,
            anchor: (row, start_col),
            cursor: (row, end_col),
            word_selection: true,
        }
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

pub(crate) fn word_range(line: &str, column: u16) -> Option<(u16, u16)> {
    let mut cells = Vec::new();
    let mut current = 0u16;
    for character in line.chars() {
        let width = UnicodeWidthChar::width(character).unwrap_or(0) as u16;
        let start = if width == 0 {
            current.saturating_sub(1)
        } else {
            current
        };
        let end = if width == 0 {
            start
        } else {
            current.saturating_add(width).saturating_sub(1)
        };
        cells.push((start, end, character));
        current = current.saturating_add(width);
    }
    let index = cells
        .iter()
        .position(|(start, end, _)| column >= *start && column <= *end)?;

    let starts_with = |start: usize, prefix: &str| {
        prefix
            .chars()
            .enumerate()
            .all(|(offset, ch)| cells.get(start + offset).is_some_and(|cell| cell.2 == ch))
    };
    let url_start = (0..cells.len()).find(|&start| {
        (starts_with(start, "https://") || starts_with(start, "http://"))
            && index >= start
            && cells[start..]
                .iter()
                .position(|cell| cell.2.is_whitespace())
                .is_none_or(|length| index < start + length)
    });
    if let Some(start) = url_start {
        let mut end = start;
        while end + 1 < cells.len() && !cells[end + 1].2.is_whitespace() {
            end += 1;
        }
        while end >= start
            && matches!(
                cells[end].2,
                '"' | '\'' | '`' | '.' | ',' | ';' | ':' | '!' | '?'
            )
        {
            if end == start {
                return None;
            }
            end -= 1;
        }
        if index <= end {
            return Some((cells[start].0, cells[end].1));
        }
    }

    for quote in ['"', '\'', '`'] {
        let mut opening = None;
        for (at, cell) in cells.iter().enumerate() {
            if cell.2 != quote {
                continue;
            }
            let escaped = (0..at)
                .rev()
                .take_while(|&idx| cells[idx].2 == '\\')
                .count()
                % 2
                == 1;
            if escaped {
                continue;
            }
            if let Some(start) = opening.take() {
                if index > start
                    && index < at
                    && cells[start + 1..at].iter().any(|cell| cell.2 == '/')
                {
                    return Some((cells[start + 1].0, cells[at - 1].1));
                }
            } else {
                opening = Some(at);
            }
        }
    }

    let is_separator = |ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '|' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '!'
            )
    };
    if is_separator(cells[index].2) {
        return None;
    }
    let mut first = index;
    while first > 0 && !is_separator(cells[first - 1].2) {
        first -= 1;
    }
    let mut last = index;
    while last + 1 < cells.len() && !is_separator(cells[last + 1].2) {
        last += 1;
    }
    while first <= last && matches!(cells[first].2, '(' | '[' | '{' | '<' | '"' | '\'' | '`') {
        first += 1;
    }
    while first <= last
        && matches!(
            cells[last].2,
            ')' | ']' | '}' | '>' | '"' | '\'' | '`' | '.' | ',' | ';' | ':' | '!' | '?'
        )
    {
        if last == 0 {
            return None;
        }
        last -= 1;
    }
    if first <= last {
        Some((cells[first].0, cells[last].1))
    } else {
        None
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
    use super::{word_range, TextSelection};
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
            word_selection: false,
        };
        assert_eq!(selection.text("A界B"), "界");
    }

    #[test]
    fn word_selection_uses_cell_columns_and_punctuation_boundaries() {
        assert_eq!(word_range("run café_path!", 7), Some((4, 12)));
        assert_eq!(word_range("run café_path!", 13), None);
        assert_eq!(word_range("a界b", 1), Some((0, 3)));
        let selection = TextSelection::word("pane-1".into(), Rect::new(0, 0, 20, 1), 0, 4, 12);
        assert!(selection.word_selection);
        assert_eq!(selection.text("run café_path!"), "café_path");
        assert_eq!(
            word_range("see https://example.com/a-b.", 12),
            Some((4, 26))
        );
        assert_eq!(
            word_range("cat \"/tmp/build output/log.txt\"", 16),
            Some((5, 29))
        );
        assert_eq!(word_range("a,b", 1), None);
    }
}
