use serde::{Deserialize, Serialize};
use vt100::Parser;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSnapshot {
    pub rows: u16,
    pub cols: u16,
    pub contents: String,
    pub cursor: (u16, u16),
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub alternate_screen: bool,
}

pub struct TerminalEmulator {
    parser: Parser,
    rows: u16,
    cols: u16,
}

impl TerminalEmulator {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        Self {
            parser: Parser::new(rows, cols, scrollback),
            rows,
            cols,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.set_size(rows, cols);
        self.rows = rows;
        self.cols = cols;
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let screen = self.parser.screen();
        TerminalSnapshot {
            rows: self.rows,
            cols: self.cols,
            contents: screen.contents(),
            cursor: screen.cursor_position(),
            title: screen.title().into(),
            alternate_screen: screen.alternate_screen(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalEmulator;

    #[test]
    fn plain_output_reaches_the_screen() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        terminal.process(b"hello");
        assert!(terminal.snapshot().contents.starts_with("hello"));
    }

    #[test]
    fn ansi_cursor_movement_is_interpreted() {
        let mut terminal = TerminalEmulator::new(3, 8, 4);
        terminal.process(b"one\x1b[2;3Htwo");
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cursor, (1, 5));
        assert!(snapshot.contents.contains("one"));
        assert!(snapshot.contents.contains("two"));
    }

    #[test]
    fn resize_updates_dimensions() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        terminal.resize(4, 12);
        assert_eq!(terminal.snapshot().rows, 4);
        assert_eq!(terminal.snapshot().cols, 12);
    }

    #[test]
    fn title_and_alternate_screen_are_tracked() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        terminal.process(b"\x1b]0;Spindle\x07\x1b[?1049h");
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.title, "Spindle");
        assert!(snapshot.alternate_screen);

        terminal.process(b"\x1b[?1049l");
        assert!(!terminal.snapshot().alternate_screen);
    }
}
