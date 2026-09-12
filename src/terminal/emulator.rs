use serde::{Deserialize, Serialize};
use vt100::Parser;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSnapshot {
    pub rows: u16,
    pub cols: u16,
    pub contents: String,
    pub cursor: (u16, u16),
    #[serde(default)]
    pub cursor_visible: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub osc_title: String,
    #[serde(default)]
    pub osc_progress: String,
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
    #[serde(default)]
    pub application_cursor: bool,
    #[serde(default)]
    pub bracketed_paste: bool,
}

pub struct TerminalEmulator {
    parser: Parser,
    rows: u16,
    cols: u16,
    query_state: QueryState,
    agent_osc: AgentOscTracker,
}

const MAX_AGENT_OSC_BYTES: usize = 1024;
const MAX_AGENT_OSC_CHARS: usize = 256;

#[derive(Default)]
struct AgentOscTracker {
    state: AgentOscState,
    latest_title: String,
    latest_progress: String,
}

#[derive(Default)]
enum AgentOscState {
    #[default]
    Ground,
    Escape,
    Body(Vec<u8>),
    EscapeInBody(Vec<u8>),
}

impl AgentOscTracker {
    fn observe(&mut self, bytes: &[u8]) {
        for byte in bytes.iter().copied() {
            self.state = match std::mem::take(&mut self.state) {
                AgentOscState::Ground if byte == 0x1b => AgentOscState::Escape,
                AgentOscState::Ground => AgentOscState::Ground,
                AgentOscState::Escape if byte == b']' => AgentOscState::Body(Vec::new()),
                AgentOscState::Escape if byte == 0x1b => AgentOscState::Escape,
                AgentOscState::Escape => AgentOscState::Ground,
                AgentOscState::Body(body) if byte == 0x07 => {
                    self.finish(&body);
                    AgentOscState::Ground
                }
                AgentOscState::Body(body) if byte == 0x1b => AgentOscState::EscapeInBody(body),
                AgentOscState::Body(mut body) if body.len() < MAX_AGENT_OSC_BYTES => {
                    body.push(byte);
                    AgentOscState::Body(body)
                }
                AgentOscState::Body(_) => AgentOscState::Ground,
                AgentOscState::EscapeInBody(body) if byte == b'\\' => {
                    self.finish(&body);
                    AgentOscState::Ground
                }
                AgentOscState::EscapeInBody(mut body) if body.len() + 1 < MAX_AGENT_OSC_BYTES => {
                    body.push(0x1b);
                    body.push(byte);
                    AgentOscState::Body(body)
                }
                AgentOscState::EscapeInBody(_) => AgentOscState::Ground,
            };
        }
    }

    fn finish(&mut self, body: &[u8]) {
        let Some(separator) = body.iter().position(|byte| *byte == b';') else {
            return;
        };
        let command = &body[..separator];
        let value: String = String::from_utf8_lossy(&body[separator + 1..])
            .chars()
            .filter(|character| !character.is_control())
            .take(MAX_AGENT_OSC_CHARS)
            .collect();
        match command {
            b"0" | b"2" => self.latest_title = value,
            b"9" => self.latest_progress = value,
            _ => {}
        }
    }
}

#[derive(Default)]
enum QueryState {
    #[default]
    Ground,
    Escape,
    Csi(Vec<u8>),
    String {
        osc: bool,
        escaped: bool,
    },
}

impl TerminalEmulator {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        Self {
            parser: Parser::new(rows, cols, scrollback),
            rows,
            cols,
            query_state: QueryState::Ground,
            agent_osc: AgentOscTracker::default(),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.agent_osc.observe(bytes);
        let query_ends = self.cursor_report_query_ends(bytes);
        let mut responses = Vec::with_capacity(query_ends.len());
        let mut start = 0;
        for end in query_ends {
            self.parser.process(&bytes[start..=end]);
            let (row, col) = self.parser.screen().cursor_position();
            responses.push(format!("\x1b[{};{}R", row + 1, col + 1).into_bytes());
            start = end + 1;
        }
        self.parser.process(&bytes[start..]);
        responses
    }

    fn cursor_report_query_ends(&mut self, bytes: &[u8]) -> Vec<usize> {
        let mut query_ends = Vec::new();
        for (index, byte) in bytes.iter().copied().enumerate() {
            self.query_state = match std::mem::take(&mut self.query_state) {
                QueryState::Ground if byte == 0x1b => QueryState::Escape,
                QueryState::Ground => QueryState::Ground,
                QueryState::Escape if byte == b'[' => QueryState::Csi(Vec::new()),
                QueryState::Escape if matches!(byte, b']' | b'P' | b'_' | b'^' | b'X') => {
                    QueryState::String {
                        osc: byte == b']',
                        escaped: false,
                    }
                }
                QueryState::Escape if byte == 0x1b => QueryState::Escape,
                QueryState::Escape => QueryState::Ground,
                QueryState::Csi(_params) if byte == 0x1b => QueryState::Escape,
                QueryState::Csi(params) if (0x40..=0x7e).contains(&byte) => {
                    if byte == b'n' && params == b"6" {
                        query_ends.push(index);
                    }
                    QueryState::Ground
                }
                QueryState::Csi(mut params) if params.len() < 16 => {
                    params.push(byte);
                    QueryState::Csi(params)
                }
                QueryState::Csi(_) => QueryState::Ground,
                QueryState::String { escaped: true, .. } if byte == b'\\' => QueryState::Ground,
                QueryState::String { osc: true, .. } if byte == 0x07 => QueryState::Ground,
                QueryState::String { osc, .. } => QueryState::String {
                    osc,
                    escaped: byte == 0x1b,
                },
            };
        }
        query_ends
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.set_size(rows, cols);
        self.rows = rows;
        self.cols = cols;
    }

    pub fn clear_agent_osc_evidence(&mut self) {
        self.agent_osc.latest_title.clear();
        self.agent_osc.latest_progress.clear();
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
            cursor_visible: !screen.hide_cursor(),
            title: screen.title().into(),
            osc_title: self.agent_osc.latest_title.clone(),
            osc_progress: self.agent_osc.latest_progress.clone(),
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
            application_cursor: screen.application_cursor(),
            bracketed_paste: screen.bracketed_paste(),
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
    fn cursor_position_queries_receive_the_current_one_based_position() {
        let mut terminal = TerminalEmulator::new(4, 8, 4);
        terminal.process(b"\x1b[2;4H");

        assert_eq!(terminal.process(b"\x1b[6n"), vec![b"\x1b[2;4R"]);
    }

    #[test]
    fn cursor_position_queries_can_span_output_chunks() {
        let mut terminal = TerminalEmulator::new(4, 8, 4);
        terminal.process(b"\x1b[3;5H\x1b[6");

        assert_eq!(terminal.process(b"n"), vec![b"\x1b[3;5R"]);
    }

    #[test]
    fn cursor_position_query_text_inside_an_osc_string_is_ignored() {
        let mut terminal = TerminalEmulator::new(4, 8, 4);

        assert!(terminal.process(b"\x1b]0;\x1b[6n\x07").is_empty());
    }

    #[test]
    fn cursor_visibility_modes_are_tracked() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        assert!(terminal.snapshot().cursor_visible);

        terminal.process(b"\x1b[?25l");
        assert!(!terminal.snapshot().cursor_visible);

        terminal.process(b"\x1b[?25h");
        assert!(terminal.snapshot().cursor_visible);
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
    fn osc_progress_is_retained_across_chunks_and_st_termination() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        terminal.process(b"\x1b]0;Claude\x07");
        assert_eq!(terminal.snapshot().osc_title, "Claude");

        terminal.process(b"\x1b]9;4;1;");
        assert_eq!(terminal.snapshot().osc_progress, "");

        terminal.process(b"\x1b\\");
        assert_eq!(terminal.snapshot().osc_progress, "4;1;");

        terminal.process(b"\x1b]9;4;0;\x07");
        assert_eq!(terminal.snapshot().osc_progress, "4;0;");
        terminal.clear_agent_osc_evidence();
        assert_eq!(terminal.snapshot().osc_title, "");
        assert_eq!(terminal.snapshot().osc_progress, "");
    }

    #[test]
    fn empty_osc_title_clears_retained_title() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        terminal.process(b"\x1b]2;Claude\x07");
        terminal.process(b"\x1b]0;\x07");
        assert_eq!(terminal.snapshot().osc_title, "");
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

    #[test]
    fn page_key_scrollback_modes_are_tracked() {
        let mut terminal = TerminalEmulator::new(2, 8, 4);
        terminal.process(b"\x1b[?1h\x1b[?2004h");
        let snapshot = terminal.snapshot();
        assert!(snapshot.application_cursor);
        assert!(snapshot.bracketed_paste);

        terminal.process(b"\x1b[?1l\x1b[?2004l");
        let snapshot = terminal.snapshot();
        assert!(!snapshot.application_cursor);
        assert!(!snapshot.bracketed_paste);
    }
}
