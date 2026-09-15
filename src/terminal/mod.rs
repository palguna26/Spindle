mod emulator;
mod mouse;

pub use emulator::{HyperlinkCell, TerminalEmulator, TerminalSnapshot};
pub use mouse::encode_mouse_event;
