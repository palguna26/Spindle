use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Detach,
    NewTab,
    NewPane,
    ClosePane,
    CloseTab,
    NextTab,
    PreviousTab,
    EnterCopyMode,
    NextSpace,
    PreviousSpace,
    NextWorkspace,
    WorkspacePicker,
    SessionNavigator,
    SwitchWorkspaceByName,
    StopFocusedPane,
    RestartFocusedPane,
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
    SplitHorizontal,
    SplitVertical,
    ResizeSmaller,
    ResizeLarger,
    ToggleZoom,
    ToggleSidebar,
    ToggleRightClickPassthrough,
    Help,
    CommandPalette,
    Send(KeyCode),
}

pub fn action(prefix_active: bool, key: KeyEvent) -> Action {
    if prefix_active {
        return match key.code {
            KeyCode::Char('d') => Action::Detach,
            KeyCode::Char('q') => Action::Detach,
            KeyCode::Char('c') => Action::NewTab,
            KeyCode::Char('N') => Action::CreateWorkspace,
            KeyCode::Char('n') => Action::NextTab,
            KeyCode::Char('p') => Action::PreviousTab,
            KeyCode::Char('x') => Action::ClosePane,
            KeyCode::Char('X') => Action::CloseTab,
            KeyCode::Char(']') => Action::NextTab,
            KeyCode::Char('[') => Action::EnterCopyMode,
            KeyCode::Char('}') => Action::NextSpace,
            KeyCode::Char('{') => Action::PreviousSpace,
            KeyCode::Char('w') => Action::WorkspacePicker,
            KeyCode::Char('g') => Action::SessionNavigator,
            KeyCode::Char('s') => Action::StopFocusedPane,
            KeyCode::Char('r') => Action::RestartFocusedPane,
            KeyCode::Char('o') => Action::FocusNext,
            KeyCode::Char('O') => Action::FocusPrevious,
            KeyCode::Char('h') => Action::FocusLeft,
            KeyCode::Char('j') => Action::FocusDown,
            KeyCode::Char('k') => Action::FocusUp,
            KeyCode::Char('l') => Action::FocusRight,
            KeyCode::Left => Action::FocusLeft,
            KeyCode::Right => Action::FocusRight,
            KeyCode::Up => Action::FocusUp,
            KeyCode::Down => Action::FocusDown,
            KeyCode::Char('"') => Action::SplitHorizontal,
            KeyCode::Char('%') => Action::SplitVertical,
            KeyCode::Char('-') => Action::SplitHorizontal,
            KeyCode::Char('v') => Action::SplitVertical,
            KeyCode::Char('<') => Action::ResizeSmaller,
            KeyCode::Char('>') => Action::ResizeLarger,
            KeyCode::Char('z') => Action::ToggleZoom,
            KeyCode::Char('b') => Action::ToggleSidebar,
            KeyCode::Char('?') => Action::Help,
            KeyCode::Char(':') => Action::CommandPalette,
            _ => Action::None,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
        return Action::None;
    }
    Action::Send(key.code)
}

pub fn is_prefix(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b')
}

#[cfg(test)]
mod tests {
    use super::{action, is_prefix, Action};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn prefix_commands_are_discoverable() {
        let prefix = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert!(is_prefix(prefix));
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
            Action::Detach
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Action::NewTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT)),
            Action::CreateWorkspace
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            Action::ClosePane
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT)),
            Action::CloseTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            Action::NextTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE)),
            Action::WorkspacePicker
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            Action::SessionNavigator
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
            Action::PreviousTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE)),
            Action::EnterCopyMode
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
            Action::ToggleZoom
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)),
            Action::ToggleSidebar
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            Action::Help
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char(':'), KeyModifiers::SHIFT)),
            Action::CommandPalette
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
            Action::FocusNext
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)),
            Action::NextTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            Action::StopFocusedPane
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            Action::RestartFocusedPane
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)),
            Action::SplitVertical
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE)),
            Action::SplitHorizontal
        );
        for (key, expected) in [
            ('h', Action::FocusLeft),
            ('j', Action::FocusDown),
            ('k', Action::FocusUp),
            ('l', Action::FocusRight),
            ('O', Action::FocusPrevious),
            ('q', Action::Detach),
        ] {
            assert_eq!(
                action(true, KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                expected
            );
        }
    }

    #[test]
    fn bare_q_is_sent_to_the_focused_pane() {
        assert_eq!(
            action(false, KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            Action::Send(KeyCode::Char('q'))
        );
    }
}
