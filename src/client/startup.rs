use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupErrorAction {
    Retry,
    Detach,
    Ignore,
}

pub(super) fn startup_error_action(key: KeyCode) -> StartupErrorAction {
    match key {
        KeyCode::Char('r') | KeyCode::Enter => StartupErrorAction::Retry,
        KeyCode::Esc | KeyCode::Char('q') => StartupErrorAction::Detach,
        _ => StartupErrorAction::Ignore,
    }
}
