mod emulator;
mod mouse;

pub use emulator::{TerminalEmulator, TerminalSnapshot};
pub use mouse::encode_mouse_event;
