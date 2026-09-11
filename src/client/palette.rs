use super::input::Action;
use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    NextSpace,
    PreviousSpace,
    StopFocusedPane,
    FocusNext,
    SplitHorizontal,
    SplitVertical,
    ResizeSmaller,
    ResizeLarger,
    Detach,
}

impl Command {
    pub const ALL: [Self; 13] = [
        Self::NewTab,
        Self::CloseTab,
        Self::NextTab,
        Self::PreviousTab,
        Self::NextSpace,
        Self::PreviousSpace,
        Self::StopFocusedPane,
        Self::FocusNext,
        Self::SplitHorizontal,
        Self::SplitVertical,
        Self::ResizeSmaller,
        Self::ResizeLarger,
        Self::Detach,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NewTab => "New tab",
            Self::CloseTab => "Close tab",
            Self::NextTab => "Next tab",
            Self::PreviousTab => "Previous tab",
            Self::NextSpace => "Next space",
            Self::PreviousSpace => "Previous space",
            Self::StopFocusedPane => "Stop focused pane",
            Self::FocusNext => "Focus next pane",
            Self::SplitHorizontal => "Split horizontally",
            Self::SplitVertical => "Split vertically",
            Self::ResizeSmaller => "Resize smaller",
            Self::ResizeLarger => "Resize larger",
            Self::Detach => "Detach",
        }
    }

    pub fn action(self) -> Action {
        match self {
            Self::NewTab => Action::NewTab,
            Self::CloseTab => Action::CloseTab,
            Self::NextTab => Action::NextTab,
            Self::PreviousTab => Action::PreviousTab,
            Self::NextSpace => Action::NextSpace,
            Self::PreviousSpace => Action::PreviousSpace,
            Self::StopFocusedPane => Action::StopFocusedPane,
            Self::FocusNext => Action::FocusNext,
            Self::SplitHorizontal => Action::SplitHorizontal,
            Self::SplitVertical => Action::SplitVertical,
            Self::ResizeSmaller => Action::ResizeSmaller,
            Self::ResizeLarger => Action::ResizeLarger,
            Self::Detach => Action::Detach,
        }
    }
}

pub fn move_selection(selected: usize, key: KeyCode) -> Option<usize> {
    let count = Command::ALL.len();
    if count == 0 {
        return None;
    }
    match key {
        KeyCode::Up | KeyCode::Char('k') => Some((selected + count - 1) % count),
        KeyCode::Down | KeyCode::Char('j') => Some((selected + 1) % count),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{move_selection, Command};
    use crossterm::event::KeyCode;

    #[test]
    fn palette_selection_wraps() {
        assert_eq!(move_selection(0, KeyCode::Up), Some(Command::ALL.len() - 1));
        assert_eq!(
            move_selection(Command::ALL.len() - 1, KeyCode::Down),
            Some(0)
        );
    }
}
