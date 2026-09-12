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
    #[serde(default)]
    pub mouse_reporting: bool,
    #[serde(default)]
    pub mouse_release: bool,
    #[serde(default)]
    pub mouse_motion: bool,
    #[serde(default)]
    pub mouse_any_motion: bool,
    #[serde(default)]
    pub sgr_mouse: bool,
    #[serde(default)]
    pub utf8_mouse: bool,
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
        let mouse_mode = screen.mouse_protocol_mode();
        let mouse_encoding = screen.mouse_protocol_encoding();
        TerminalSnapshot {
            rows: self.rows,
            cols: self.cols,
            contents: screen.contents(),
            cursor: screen.cursor_position(),
            title: screen.title().into(),
            alternate_screen: screen.alternate_screen(),
            mouse_reporting: mouse_mode != vt100::MouseProtocolMode::None,
            mouse_release: matches!(
                mouse_mode,
                vt100::MouseProtocolMode::PressRelease
                    | vt100::MouseProtocolMode::ButtonMotion
                    | vt100::MouseProtocolMode::AnyMotion
            ),
            mouse_motion: matches!(
                mouse_mode,
                vt100::MouseProtocolMode::ButtonMotion | vt100::MouseProtocolMode::AnyMotion
            ),
            mouse_any_motion: mouse_mode == vt100::MouseProtocolMode::AnyMotion,
            sgr_mouse: mouse_encoding == vt100::MouseProtocolEncoding::Sgr,
            utf8_mouse: mouse_encoding == vt100::MouseProtocolEncoding::Utf8,
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

    #[test]
    fn mouse_reporting_modes_are_tracked() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        terminal.process(b"\x1b[?1002h\x1b[?1006h");
        let snapshot = terminal.snapshot();
        assert!(snapshot.mouse_reporting);
        assert!(snapshot.mouse_motion);
        assert!(!snapshot.mouse_any_motion);
        assert!(snapshot.sgr_mouse);
        assert!(!snapshot.utf8_mouse);

        terminal.process(b"\x1b[?1002l\x1b[?1006l");
        assert!(!terminal.snapshot().mouse_reporting);
        terminal.process(b"\x1b[?1003h");
        assert!(terminal.snapshot().mouse_any_motion);
    }
}
