use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Help,
    CommandPalette,
    ReloadConfig,
    Detach,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Continue,
    Close,
    Activate(Action),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct GlobalMenu {
    pub(super) selected: usize,
}

const ITEMS: [(&str, Action); 4] = [
    ("help", Action::Help),
    ("command palette", Action::CommandPalette),
    ("reload config", Action::ReloadConfig),
    ("detach", Action::Detach),
];

impl GlobalMenu {
    pub(super) fn items() -> &'static [(&'static str, Action)] {
        &ITEMS
    }

    pub(super) fn handle_key(&mut self, key: KeyCode) -> Outcome {
        match key {
            KeyCode::Esc => Outcome::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                Outcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(ITEMS.len() - 1);
                Outcome::Continue
            }
            KeyCode::Enter => Outcome::Activate(ITEMS[self.selected].1),
            _ => Outcome::Continue,
        }
    }

    pub(super) fn rect(sidebar: Rect, screen: Rect) -> Rect {
        let width = 18.min(sidebar.width.max(1));
        let height = (ITEMS.len() as u16 + 2).min(screen.height.max(1));
        Rect::new(
            sidebar.right().saturating_sub(width),
            sidebar.bottom().saturating_sub(1 + height),
            width,
            height,
        )
    }

    pub(super) fn hit_test(sidebar: Rect, screen: Rect, column: u16, row: u16) -> Option<usize> {
        let rect = Self::rect(sidebar, screen);
        let inner = Rect::new(
            rect.x.saturating_add(1),
            rect.y.saturating_add(1),
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
        );
        (column >= inner.x && column < inner.right() && row >= inner.y && row < inner.bottom())
            .then(|| usize::from(row.saturating_sub(inner.y)))
            .filter(|index| *index < ITEMS.len())
    }

    pub(super) fn select_at(&mut self, sidebar: Rect, screen: Rect, mouse: MouseEvent) -> Outcome {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                Self::hit_test(sidebar, screen, mouse.column, mouse.row).map_or(
                    Outcome::Close,
                    |selected| {
                        self.selected = selected;
                        Outcome::Activate(ITEMS[selected].1)
                    },
                )
            }
            MouseEventKind::Moved => {
                if let Some(selected) = Self::hit_test(sidebar, screen, mouse.column, mouse.row) {
                    self.selected = selected;
                }
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, GlobalMenu, Outcome};
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    #[test]
    fn keyboard_navigation_activates_menu_items() {
        let mut menu = GlobalMenu::default();
        assert_eq!(menu.handle_key(KeyCode::Down), Outcome::Continue);
        assert_eq!(menu.selected, 1);
        assert_eq!(
            menu.handle_key(KeyCode::Enter),
            Outcome::Activate(Action::CommandPalette)
        );
        assert_eq!(menu.handle_key(KeyCode::Esc), Outcome::Close);
    }

    #[test]
    fn menu_mouse_hit_testing_selects_a_row() {
        let sidebar = Rect::new(0, 0, 24, 20);
        let screen = Rect::new(0, 0, 80, 24);
        let rect = GlobalMenu::rect(sidebar, screen);
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 2,
            row: rect.y + 2,
            modifiers: KeyModifiers::NONE,
        };
        let mut menu = GlobalMenu::default();
        assert_eq!(
            menu.select_at(sidebar, screen, mouse),
            Outcome::Activate(Action::CommandPalette)
        );
    }
}
