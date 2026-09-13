use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const MAX_HISTORY_BYTES: usize = 64 * 1024;
const MAX_HISTORY_ROWS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Point {
    pub(crate) row: usize,
    pub(crate) col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionKind {
    Character,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CopySelection {
    pub(crate) anchor: Point,
    pub(crate) kind: SelectionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyResult {
    Handled,
    Exit,
    Copy,
}

#[derive(Debug, Clone)]
pub(crate) struct CopyMode {
    pub(crate) pane_id: String,
    pub(crate) rows: Vec<String>,
    pub(crate) cursor: Point,
    pub(crate) viewport_top: usize,
    pub(crate) viewport_height: usize,
    pub(crate) selection: Option<CopySelection>,
    pub(crate) search_query: String,
    pub(crate) search_prompt: bool,
    pub(crate) search_matches: Vec<Point>,
    pub(crate) search_index: Option<usize>,
    search_reverse: bool,
    pub(crate) saved_offset: usize,
    bytes: Vec<u8>,
    dimensions: (u16, u16),
    pinned: bool,
}

impl CopyMode {
    pub(crate) fn new(
        pane_id: String,
        bytes: &[u8],
        rows: u16,
        cols: u16,
        viewport_height: u16,
        saved_offset: usize,
        cursor: (u16, u16),
    ) -> Self {
        let terminal_rows = rows;
        let rows = history_rows(bytes, terminal_rows, cols);
        let viewport_height = usize::from(viewport_height.max(1));
        let max_top = rows.len().saturating_sub(viewport_height);
        let viewport_top = max_top.saturating_sub(saved_offset);
        let cursor = Point {
            row: (viewport_top
                + if saved_offset == 0 {
                    usize::from(cursor.1)
                } else {
                    viewport_height.saturating_sub(1)
                })
            .min(rows.len().saturating_sub(1)),
            col: usize::from(cursor.0).min(usize::from(cols.saturating_sub(1))),
        };
        Self {
            pane_id,
            rows,
            cursor,
            viewport_top,
            viewport_height,
            selection: None,
            search_query: String::new(),
            search_prompt: false,
            search_matches: Vec::new(),
            search_index: None,
            search_reverse: false,
            saved_offset,
            bytes: bytes
                .iter()
                .rev()
                .take(MAX_HISTORY_BYTES)
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            dimensions: (terminal_rows, cols),
            pinned: saved_offset == 0,
        }
    }

    pub(crate) fn refresh(&mut self, bytes: &[u8], rows: u16, cols: u16) {
        if self.bytes == bytes && self.dimensions == (rows, cols) {
            return;
        }
        let was_pinned = self.pinned;
        let old_len = self.rows.len();
        let old_cursor = self.cursor;
        let appended = self.dimensions == (rows, cols) && bytes.starts_with(&self.bytes);
        self.rows = history_rows(bytes, rows, cols);
        self.viewport_height = usize::from(rows.max(1));
        self.bytes = bytes
            .iter()
            .rev()
            .take(MAX_HISTORY_BYTES)
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        self.dimensions = (rows, cols);
        if appended && self.rows.len() >= old_len {
            let delta = self.rows.len() - old_len;
            self.cursor.row = self.cursor.row.saturating_add(delta);
            self.viewport_top = if was_pinned {
                self.rows.len().saturating_sub(self.viewport_height)
            } else {
                self.viewport_top
            };
        } else {
            self.selection = None;
            self.search_matches.clear();
            self.search_index = None;
            self.cursor.row = old_cursor.row.min(self.rows.len().saturating_sub(1));
            self.viewport_top = self
                .viewport_top
                .min(self.rows.len().saturating_sub(self.viewport_height));
        }
        self.clamp_cursor();
        self.reveal_cursor();
    }

    pub(crate) fn key(&mut self, key: KeyEvent) -> KeyResult {
        if self.search_prompt {
            match key.code {
                KeyCode::Esc => self.search_prompt = false,
                KeyCode::Enter => {
                    self.search_prompt = false;
                    self.search();
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.search_query.push(ch)
                }
                _ => {}
            }
            return KeyResult::Handled;
        }
        if key.code == KeyCode::Esc {
            if self.selection.is_some() || !self.search_query.is_empty() {
                self.selection = None;
                self.search_query.clear();
                self.search_matches.clear();
                self.search_index = None;
                return KeyResult::Handled;
            }
            return KeyResult::Exit;
        }
        if key.code == KeyCode::Enter || key.code == KeyCode::Char('y') {
            return KeyResult::Copy;
        }
        if key.code == KeyCode::Char('q') {
            return KeyResult::Exit;
        }
        if key.code == KeyCode::Char('/') || key.code == KeyCode::Char('?') {
            self.search_prompt = true;
            self.search_reverse = key.code == KeyCode::Char('?');
            self.search_query.clear();
            return KeyResult::Handled;
        }
        if key.code == KeyCode::Char('v') || key.code == KeyCode::Char(' ') {
            self.selection = Some(CopySelection {
                anchor: self.cursor,
                kind: SelectionKind::Character,
            });
            return KeyResult::Handled;
        }
        if key.code == KeyCode::Char('V') {
            self.selection = Some(CopySelection {
                anchor: self.cursor,
                kind: SelectionKind::Line,
            });
            return KeyResult::Handled;
        }
        let (row_delta, col_delta) = match key.code {
            KeyCode::Up | KeyCode::Char('k') => (-1, 0),
            KeyCode::Down | KeyCode::Char('j') => (1, 0),
            KeyCode::Left | KeyCode::Char('h') => (0, -1),
            KeyCode::Right | KeyCode::Char('l') => (0, 1),
            KeyCode::PageUp => (-(self.viewport_height as isize), 0),
            KeyCode::PageDown => (self.viewport_height as isize, 0),
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                (self.viewport_height as isize, 0)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                (-((self.viewport_height / 2).max(1) as isize), 0)
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ((self.viewport_height / 2).max(1) as isize, 0)
            }
            KeyCode::Home | KeyCode::Char('0') => {
                self.cursor.col = 0;
                self.after_move();
                return KeyResult::Handled;
            }
            KeyCode::Char('^') => {
                self.cursor.col = self.rows[self.cursor.row]
                    .chars()
                    .take_while(|ch| ch.is_whitespace())
                    .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
                    .sum();
                self.after_move();
                return KeyResult::Handled;
            }
            KeyCode::End | KeyCode::Char('$') => {
                self.cursor.col = self.last_nonblank_col(self.cursor.row);
                self.after_move();
                return KeyResult::Handled;
            }
            KeyCode::Char('g') => {
                self.cursor.row = 0;
                self.after_move();
                return KeyResult::Handled;
            }
            KeyCode::Char('G') => {
                self.cursor.row = self.rows.len().saturating_sub(1);
                self.after_move();
                return KeyResult::Handled;
            }
            KeyCode::Char('n') => {
                self.repeat_search(false);
                return KeyResult::Handled;
            }
            KeyCode::Char('N') => {
                self.repeat_search(true);
                return KeyResult::Handled;
            }
            KeyCode::Char('{') => {
                self.move_paragraph(-1);
                return KeyResult::Handled;
            }
            KeyCode::Char('}') => {
                self.move_paragraph(1);
                return KeyResult::Handled;
            }
            KeyCode::Char('w') => {
                self.move_word(1, false, false);
                return KeyResult::Handled;
            }
            KeyCode::Char('W') => {
                self.move_word(1, true, false);
                return KeyResult::Handled;
            }
            KeyCode::Char('b') => {
                self.move_word(-1, false, false);
                return KeyResult::Handled;
            }
            KeyCode::Char('B') => {
                self.move_word(-1, true, false);
                return KeyResult::Handled;
            }
            KeyCode::Char('e') => {
                self.move_word(1, false, true);
                return KeyResult::Handled;
            }
            KeyCode::Char('E') => {
                self.move_word(1, true, true);
                return KeyResult::Handled;
            }
            _ => return KeyResult::Handled,
        };
        self.cursor.row = self
            .cursor
            .row
            .saturating_add_signed(row_delta)
            .min(self.rows.len().saturating_sub(1));
        self.cursor.col = self
            .cursor
            .col
            .saturating_add_signed(col_delta)
            .min(self.line_width(self.cursor.row).saturating_sub(1));
        self.after_move();
        KeyResult::Handled
    }

    pub(crate) fn scroll_offset(&self) -> usize {
        self.rows
            .len()
            .saturating_sub(self.viewport_top + self.viewport_height)
    }

    pub(crate) fn selected_text(&self) -> String {
        let Some(selection) = self.selection else {
            let Some(index) = self.search_index else {
                return String::new();
            };
            let Some(point) = self.search_matches.get(index) else {
                return String::new();
            };
            return self.rows.get(point.row).map_or_else(String::new, |line| {
                slice_cells(
                    line,
                    point.col,
                    point
                        .col
                        .saturating_add(UnicodeWidthStr::width(self.search_query.as_str())),
                )
            });
        };
        let (start, end) = if selection.anchor <= self.cursor {
            (selection.anchor, self.cursor)
        } else {
            (self.cursor, selection.anchor)
        };
        let mut text = String::new();
        for row in start.row..=end.row {
            if row > start.row {
                text.push('\n');
            }
            let Some(line) = self.rows.get(row) else {
                continue;
            };
            let (left, right) = match selection.kind {
                SelectionKind::Line => (0, usize::MAX),
                SelectionKind::Character => (
                    if row == start.row { start.col } else { 0 },
                    if row == end.row {
                        end.col.saturating_add(1)
                    } else {
                        usize::MAX
                    },
                ),
            };
            text.push_str(&slice_cells(line, left, right));
        }
        text.trim_end().to_owned()
    }

    fn after_move(&mut self) {
        self.clamp_cursor();
        self.reveal_cursor();
        self.pinned = self.cursor.row.saturating_add(1) >= self.rows.len();
    }

    fn reveal_cursor(&mut self) {
        if self.cursor.row < self.viewport_top {
            self.viewport_top = self.cursor.row;
        }
        if self.cursor.row >= self.viewport_top + self.viewport_height {
            self.viewport_top = self.cursor.row + 1 - self.viewport_height;
        }
    }

    fn clamp_cursor(&mut self) {
        self.cursor.row = self.cursor.row.min(self.rows.len().saturating_sub(1));
        self.cursor.col = self
            .cursor
            .col
            .min(self.line_width(self.cursor.row).saturating_sub(1));
    }

    fn line_width(&self, row: usize) -> usize {
        self.rows.get(row).map_or(1, |line| {
            line.chars()
                .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
                .sum::<usize>()
                .max(1)
        })
    }

    fn last_nonblank_col(&self, row: usize) -> usize {
        self.rows.get(row).map_or(0, |line| {
            let mut col = 0;
            let mut last = 0;
            for ch in line.chars() {
                let width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if !ch.is_whitespace() {
                    last = col;
                }
                col += width;
            }
            last
        })
    }

    fn search(&mut self) {
        self.search_matches.clear();
        self.search_index = None;
        if self.search_query.is_empty() {
            return;
        }
        let sensitive = self.search_query.chars().any(char::is_uppercase);
        let needle = if sensitive {
            self.search_query.clone()
        } else {
            self.search_query.to_lowercase()
        };
        for (row, line) in self.rows.iter().enumerate() {
            let mut haystack = String::new();
            let mut columns = Vec::new();
            let mut col = 0;
            for ch in line.chars() {
                let folded = if sensitive {
                    ch.to_string()
                } else {
                    ch.to_lowercase().to_string()
                };
                columns.extend(std::iter::repeat_n(col, folded.len()));
                haystack.push_str(&folded);
                col += UnicodeWidthChar::width(ch).unwrap_or(0);
            }
            let mut from = 0;
            while let Some(offset) = haystack[from..].find(&needle) {
                let index = from + offset;
                self.search_matches.push(Point {
                    row,
                    col: columns.get(index).copied().unwrap_or(col),
                });
                from = index + needle.len().max(1);
                if from >= haystack.len() {
                    break;
                }
            }
        }
        let index = if self.search_reverse {
            self.search_matches
                .iter()
                .rposition(|point| *point <= self.cursor)
                .or_else(|| self.search_matches.len().checked_sub(1))
        } else {
            self.search_matches
                .iter()
                .position(|point| *point >= self.cursor)
                .or_else(|| (!self.search_matches.is_empty()).then_some(0))
        };
        if let Some(index) = index {
            self.search_index = Some(index);
            self.cursor = self.search_matches[index];
            self.after_move();
        }
    }

    fn repeat_search(&mut self, reverse: bool) {
        if self.search_matches.is_empty() {
            self.search();
            return;
        }
        let index = self.search_index.unwrap_or(0);
        let next = if reverse ^ self.search_reverse {
            index
                .checked_sub(1)
                .unwrap_or(self.search_matches.len() - 1)
        } else {
            (index + 1) % self.search_matches.len()
        };
        self.search_index = Some(next);
        self.cursor = self.search_matches[next];
        self.after_move();
    }

    fn move_paragraph(&mut self, direction: isize) {
        let mut row = self.cursor.row;
        loop {
            let next = row.saturating_add_signed(direction);
            if next == row || next >= self.rows.len() {
                break;
            }
            row = next;
            if self.rows[row].trim().is_empty() {
                break;
            }
        }
        self.cursor.row = row;
        self.after_move();
    }

    fn move_word(&mut self, direction: isize, big: bool, end: bool) {
        let mut cells = Vec::new();
        for (row, line) in self.rows.iter().enumerate() {
            let mut col = 0;
            for ch in line.chars() {
                let cell_end = col + UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
                cells.push((Point { row, col }, cell_end, ch));
                col = cell_end;
            }
            cells.push((
                Point {
                    row: row + 1,
                    col: 0,
                },
                1,
                '\n',
            ));
        }
        if cells.is_empty() {
            return;
        }
        let class = |ch: char| {
            if ch.is_whitespace() {
                0
            } else if big || !ch.is_ascii() || ch.is_alphanumeric() || ch == '_' {
                1
            } else {
                2
            }
        };
        let current = cells
            .iter()
            .position(|(point, end_col, _)| {
                (point.row == self.cursor.row
                    && self.cursor.col >= point.col
                    && self.cursor.col < *end_col)
                    || *point > self.cursor
            })
            .unwrap_or(cells.len() - 1);
        let target = if direction > 0 {
            let mut index = current;
            if end {
                index += 1;
                while index < cells.len() && class(cells[index].2) == 0 {
                    index += 1;
                }
                if index == cells.len() {
                    index -= 1;
                }
                let word_class = class(cells[index].2);
                while index + 1 < cells.len() && class(cells[index + 1].2) == word_class {
                    index += 1;
                }
            } else {
                let current_class = class(cells[index].2);
                index += 1;
                if current_class != 0 {
                    while index < cells.len() && class(cells[index].2) == current_class {
                        index += 1;
                    }
                }
                while index < cells.len() && class(cells[index].2) == 0 {
                    index += 1;
                }
                if index == cells.len() {
                    index -= 1;
                }
            }
            index
        } else {
            let mut index = current.saturating_sub(1);
            while index > 0 && class(cells[index].2) == 0 {
                index -= 1;
            }
            let wanted = class(cells[index].2);
            while index > 0 && class(cells[index - 1].2) == wanted {
                index -= 1;
            }
            index
        };
        self.cursor = cells[target].0;
        self.after_move();
    }
}

fn history_rows(bytes: &[u8], rows: u16, cols: u16) -> Vec<String> {
    let bytes = &bytes[bytes.len().saturating_sub(MAX_HISTORY_BYTES)..];
    let mut parser = vt100::Parser::new(rows.max(1), cols.max(1), MAX_HISTORY_ROWS);
    parser.process(bytes);
    parser.screen_mut().set_scrollback(usize::MAX);
    let oldest = parser.screen().contents();
    let max_offset = parser.screen().scrollback();
    let mut lines: Vec<String> = oldest.lines().map(str::to_owned).collect();
    for offset in (0..max_offset).rev() {
        parser.screen_mut().set_scrollback(offset);
        if let Some(line) = parser.screen().contents().lines().last() {
            lines.push(line.to_owned());
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if lines.len() > MAX_HISTORY_ROWS {
        lines.drain(..lines.len() - MAX_HISTORY_ROWS);
    }
    lines
}

fn slice_cells(text: &str, lower: usize, upper: usize) -> String {
    let mut result = String::new();
    let mut col = 0;
    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if col + width > lower && col < upper {
            result.push(ch);
        }
        col += width;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{CopyMode, CopySelection, KeyResult, Point, SelectionKind};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn mode(text: &str) -> CopyMode {
        CopyMode::new("pane-1".into(), text.as_bytes(), 2, 20, 2, 0, (0, 1))
    }

    #[test]
    fn prefix_opened_mode_can_move_and_copy_a_unicode_range() {
        let mut mode = mode("first\r\nA界B\r\n");
        mode.cursor = Point { row: 2, col: 1 };
        mode.selection = Some(CopySelection {
            anchor: mode.cursor,
            kind: SelectionKind::Character,
        });
        mode.cursor.col = 2;
        assert_eq!(mode.selected_text(), "界");
        assert_eq!(mode.key(key(KeyCode::Enter)), KeyResult::Copy);
    }

    #[test]
    fn search_is_case_insensitive_without_uppercase_and_repeats() {
        let mut mode = mode("alpha ALPHA alpha\r\nlast\r\n");
        assert_eq!(mode.key(key(KeyCode::Char('/'))), KeyResult::Handled);
        for ch in "alpha".chars() {
            mode.key(key(KeyCode::Char(ch)));
        }
        mode.key(key(KeyCode::Enter));
        assert_eq!(mode.search_matches.len(), 3);
        assert_eq!(mode.cursor, mode.search_matches[0]);
        mode.key(key(KeyCode::Char('n')));
        assert_eq!(mode.cursor, mode.search_matches[1]);
        mode.key(key(KeyCode::Char('N')));
        assert_eq!(mode.cursor, mode.search_matches[0]);
    }

    #[test]
    fn line_selection_includes_each_selected_line() {
        let mut mode = mode("first\r\nsecond\r\nthird\r\n");
        mode.selection = Some(CopySelection {
            anchor: Point { row: 1, col: 0 },
            kind: SelectionKind::Line,
        });
        mode.cursor = Point { row: 2, col: 0 };
        assert_eq!(mode.selected_text(), "second\nthird");
    }

    #[test]
    fn moving_into_history_pins_view_while_new_output_arrives() {
        let mut mode = mode("one\r\ntwo\r\nthree\r\nfour\r\n");
        mode.key(key(KeyCode::PageUp));
        let top = mode.viewport_top;
        mode.refresh(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n", 2, 20);
        assert_eq!(mode.viewport_top, top);
    }

    #[test]
    fn retained_history_is_bounded_and_eviction_clears_selection() {
        let mut mode = mode("a\r\nb\r\nc\r\nd\r\n");
        mode.selection = Some(CopySelection {
            anchor: mode.cursor,
            kind: SelectionKind::Character,
        });
        let replaced = vec![b'x'; 64 * 1024 + 10];
        mode.refresh(&replaced, 2, 20);
        assert!(mode.selection.is_none());
        assert!(mode.rows.len() <= 4096);
    }

    #[test]
    fn backward_search_starts_at_the_nearest_earlier_match_and_enter_copies_it() {
        let mut mode = mode("alpha middle alpha\r\nlast\r\n");
        mode.key(key(KeyCode::Char('?')));
        for ch in "alpha".chars() {
            mode.key(key(KeyCode::Char(ch)));
        }
        mode.key(key(KeyCode::Enter));
        assert_eq!(mode.cursor, mode.search_matches[1]);
        assert_eq!(mode.selected_text(), "alpha");
    }

    #[test]
    fn word_motions_follow_herdr_tmux_separators_and_end_positions() {
        let mut mode = mode("foo.bar baz\r\n");
        mode.cursor = Point { row: 0, col: 0 };
        mode.key(key(KeyCode::Char('w')));
        assert_eq!(mode.cursor.col, 3);
        mode.key(key(KeyCode::Char('e')));
        assert_eq!(mode.cursor.col, 6);
        mode.cursor.col = 0;
        mode.key(key(KeyCode::Char('W')));
        assert_eq!(mode.cursor.col, 8);
        mode.cursor.col = 0;
        mode.key(key(KeyCode::Char('E')));
        assert_eq!(mode.cursor.col, 6);
    }

    #[test]
    fn line_end_motion_skips_trailing_whitespace() {
        let mut mode = mode("text   \r\n");
        mode.cursor.col = 0;
        mode.key(key(KeyCode::Char('$')));
        assert_eq!(mode.cursor.col, 3);
    }

    #[test]
    fn word_end_skips_to_the_next_word_when_already_at_its_end() {
        let mut mode = mode("foo.bar baz\r\n");
        mode.cursor = Point { row: 0, col: 2 };
        mode.key(key(KeyCode::Char('e')));
        assert_eq!(mode.cursor.col, 3);
    }
}
