use super::input::Action;
use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NewTab,
    NewPane,
    CloseTab,
    NextTab,
    PreviousTab,
    NextSpace,
    PreviousSpace,
    NextWorkspace,
    SwitchWorkspaceByName,
    StopFocusedPane,
    RestartFocusedPane,
    EditScrollback,
    RenameFocusedPane,
    ClearPaneName,
    RenameActiveTab,
    RenameActiveWorkspace,
    CreateWorkspace,
    RenameActiveSpace,
    CreateSpace,
    DeleteActiveWorkspace,
    DeleteActiveSpace,
    FocusNext,
    FocusPrevious,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    SwapLeft,
    SwapRight,
    SwapUp,
    SwapDown,
    SplitHorizontal,
    SplitVertical,
    ResizeSmaller,
    ResizeLarger,
    Detach,
    ToggleRightClickPassthrough,
    OpenNotificationTarget,
}

impl Command {
    pub const ALL: [Self; 38] = [
        Self::NewTab,
        Self::NewPane,
        Self::CloseTab,
        Self::NextTab,
        Self::PreviousTab,
        Self::NextSpace,
        Self::PreviousSpace,
        Self::NextWorkspace,
        Self::SwitchWorkspaceByName,
        Self::StopFocusedPane,
        Self::RestartFocusedPane,
        Self::EditScrollback,
        Self::RenameFocusedPane,
        Self::ClearPaneName,
        Self::RenameActiveTab,
        Self::RenameActiveWorkspace,
        Self::CreateWorkspace,
        Self::RenameActiveSpace,
        Self::CreateSpace,
        Self::DeleteActiveWorkspace,
        Self::DeleteActiveSpace,
        Self::FocusNext,
        Self::FocusPrevious,
        Self::FocusLeft,
        Self::FocusRight,
        Self::FocusUp,
        Self::FocusDown,
        Self::SwapLeft,
        Self::SwapRight,
        Self::SwapUp,
        Self::SwapDown,
        Self::SplitHorizontal,
        Self::SplitVertical,
        Self::ResizeSmaller,
        Self::ResizeLarger,
        Self::Detach,
        Self::ToggleRightClickPassthrough,
        Self::OpenNotificationTarget,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NewTab => "New tab",
            Self::NewPane => "New PowerShell pane",
            Self::CloseTab => "Close tab",
            Self::NextTab => "Next tab",
            Self::PreviousTab => "Previous tab",
            Self::NextSpace => "Next space",
            Self::PreviousSpace => "Previous space",
            Self::NextWorkspace => "Next workspace",
            Self::SwitchWorkspaceByName => "Switch workspace (type name)",
            Self::StopFocusedPane => "Stop focused pane",
            Self::RestartFocusedPane => "Restart focused pane",
            Self::EditScrollback => "Edit focused pane scrollback",
            Self::RenameFocusedPane => "Rename focused pane",
            Self::ClearPaneName => "Clear focused pane name",
            Self::RenameActiveTab => "Rename active tab",
            Self::RenameActiveWorkspace => "Rename active workspace",
            Self::CreateWorkspace => "Create workspace",
            Self::RenameActiveSpace => "Rename active space",
            Self::CreateSpace => "Create space",
            Self::DeleteActiveWorkspace => "Delete active workspace (type name)",
            Self::DeleteActiveSpace => "Delete active space (type name)",
            Self::FocusNext => "Focus next pane",
            Self::FocusPrevious => "Focus previous pane",
            Self::FocusLeft => "Focus pane left",
            Self::FocusRight => "Focus pane right",
            Self::FocusUp => "Focus pane up",
            Self::FocusDown => "Focus pane down",
            Self::SwapLeft => "Swap pane left",
            Self::SwapRight => "Swap pane right",
            Self::SwapUp => "Swap pane up",
            Self::SwapDown => "Swap pane down",
            Self::SplitHorizontal => "Split horizontally",
            Self::SplitVertical => "Split vertically",
            Self::ResizeSmaller => "Resize smaller",
            Self::ResizeLarger => "Resize larger",
            Self::Detach => "Detach",
            Self::ToggleRightClickPassthrough => "Toggle right-click passthrough",
            Self::OpenNotificationTarget => "Open notification target",
        }
    }

    pub fn action(self) -> Action {
        match self {
            Self::NewTab => Action::NewTab,
            Self::NewPane => Action::NewPane,
            Self::CloseTab => Action::CloseTab,
            Self::NextTab => Action::NextTab,
            Self::PreviousTab => Action::PreviousTab,
            Self::NextSpace => Action::NextSpace,
            Self::PreviousSpace => Action::PreviousSpace,
            Self::NextWorkspace => Action::NextWorkspace,
            Self::SwitchWorkspaceByName => Action::SwitchWorkspaceByName,
            Self::StopFocusedPane => Action::StopFocusedPane,
            Self::RestartFocusedPane => Action::RestartFocusedPane,
            Self::EditScrollback => Action::EditScrollback,
            Self::RenameFocusedPane => Action::RenameFocusedPane,
            Self::ClearPaneName => Action::ClearPaneName,
            Self::RenameActiveTab => Action::RenameActiveTab,
            Self::RenameActiveWorkspace => Action::RenameActiveWorkspace,
            Self::CreateWorkspace => Action::CreateWorkspace,
            Self::RenameActiveSpace => Action::RenameActiveSpace,
            Self::CreateSpace => Action::CreateSpace,
            Self::DeleteActiveWorkspace => Action::DeleteActiveWorkspace,
            Self::DeleteActiveSpace => Action::DeleteActiveSpace,
            Self::FocusNext => Action::FocusNext,
            Self::FocusPrevious => Action::FocusPrevious,
            Self::FocusLeft => Action::FocusLeft,
            Self::FocusRight => Action::FocusRight,
            Self::FocusUp => Action::FocusUp,
            Self::FocusDown => Action::FocusDown,
            Self::SwapLeft => Action::SwapLeft,
            Self::SwapRight => Action::SwapRight,
            Self::SwapUp => Action::SwapUp,
            Self::SwapDown => Action::SwapDown,
            Self::SplitHorizontal => Action::SplitHorizontal,
            Self::SplitVertical => Action::SplitVertical,
            Self::ResizeSmaller => Action::ResizeSmaller,
            Self::ResizeLarger => Action::ResizeLarger,
            Self::Detach => Action::Detach,
            Self::ToggleRightClickPassthrough => Action::ToggleRightClickPassthrough,
            Self::OpenNotificationTarget => Action::OpenNotificationTarget,
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

    #[test]
    fn pane_mouse_toggle_is_available_as_a_keyboard_command() {
        let command = Command::ToggleRightClickPassthrough;
        assert_eq!(command.label(), "Toggle right-click passthrough");
        assert_eq!(command.action(), super::Action::ToggleRightClickPassthrough);
        assert!(Command::ALL.contains(&command));
    }

    #[test]
    fn clear_pane_name_is_available_as_a_keyboard_command() {
        let command = Command::ClearPaneName;
        assert_eq!(command.label(), "Clear focused pane name");
        assert_eq!(command.action(), super::Action::ClearPaneName);
        assert!(Command::ALL.contains(&command));
    }
}
