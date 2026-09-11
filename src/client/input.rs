use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Detach,
    NewTab,
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
    Send(KeyCode),
}

pub fn action(prefix_active: bool, key: KeyEvent) -> Action {
    if prefix_active {
        return match key.code {
            KeyCode::Char('d') => Action::Detach,
            KeyCode::Char('n') => Action::NewTab,
            KeyCode::Char(']') => Action::NextTab,
            KeyCode::Char('[') => Action::PreviousTab,
            KeyCode::Char('}') => Action::NextSpace,
            KeyCode::Char('{') => Action::PreviousSpace,
            KeyCode::Char('x') => Action::StopFocusedPane,
            KeyCode::Char('o') => Action::FocusNext,
            KeyCode::Char('"') => Action::SplitHorizontal,
            KeyCode::Char('%') => Action::SplitVertical,
            KeyCode::Char('<') => Action::ResizeSmaller,
            KeyCode::Char('>') => Action::ResizeLarger,
            _ => Action::None,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
        return Action::None;
    }
    if key.code == KeyCode::Char('q') {
        Action::Detach
    } else {
        Action::Send(key.code)
    }
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
            action(true, KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            Action::NewTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
            Action::FocusNext
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)),
            Action::NextTab
        );
    }

    #[test]
    fn q_detaches_without_a_prefix() {
        assert_eq!(
            action(false, KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            Action::Detach
        );
    }
}
