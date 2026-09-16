use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Theme,
    Notifications,
    Sound,
}

impl Section {
    pub(super) const ALL: [Self; 3] = [Self::Theme, Self::Notifications, Self::Sound];

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Notifications => "notifications",
            Self::Sound => "sound",
        }
    }

    fn choices(self) -> &'static [&'static str] {
        match self {
            Self::Theme => crate::config::THEME_NAMES,
            Self::Notifications => &["off", "herdr", "terminal", "system"],
            Self::Sound => &["on", "off"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Continue,
    Close,
    Saved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Settings {
    pub(super) section: Section,
    pub(super) selected: usize,
}

impl Settings {
    pub(super) fn rect(screen: Rect) -> Rect {
        let width = 58.min(screen.width.max(1));
        let height = (screen.height * 68 / 100).max(1).min(screen.height.max(1));
        Rect::new(
            screen.x + screen.width.saturating_sub(width) / 2,
            screen.y + screen.height.saturating_sub(height) / 2,
            width,
            height,
        )
    }

    pub(super) fn open() -> Self {
        let config = crate::config::load();
        let section = Section::Theme;
        Self {
            section,
            selected: selected_for(section, &config),
        }
    }

    pub(super) fn choices(&self) -> &'static [&'static str] {
        self.section.choices()
    }

    pub(super) fn move_section(&mut self, delta: isize) {
        let current = Section::ALL
            .iter()
            .position(|section| *section == self.section)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(Section::ALL.len() as isize) as usize;
        self.section = Section::ALL[next];
        self.selected = selected_for(self.section, &crate::config::load());
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        let count = self.choices().len();
        self.selected =
            (self.selected as isize + delta).clamp(0, count.saturating_sub(1) as isize) as usize;
    }

    pub(super) fn handle_key(&mut self, key: KeyCode) -> Outcome {
        match key {
            KeyCode::Esc => Outcome::Close,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.move_section(1);
                Outcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.move_section(-1);
                Outcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                Outcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                Outcome::Continue
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.save(),
            _ => Outcome::Continue,
        }
    }

    pub(super) fn handle_mouse(&mut self, screen: Rect, mouse: MouseEvent) -> Outcome {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return Outcome::Continue;
        }
        let area = Self::rect(screen);
        if mouse.column < area.x
            || mouse.column >= area.right()
            || mouse.row < area.y
            || mouse.row >= area.bottom()
        {
            return Outcome::Close;
        }
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        if mouse.row == inner.y {
            let section_width = (inner.width / Section::ALL.len() as u16).max(1);
            let index = usize::from(mouse.column.saturating_sub(inner.x) / section_width)
                .min(Section::ALL.len() - 1);
            self.section = Section::ALL[index];
            self.selected = selected_for(self.section, &crate::config::load());
        } else if mouse.row >= inner.y + 2 {
            let index = usize::from(mouse.row - (inner.y + 2));
            if index < self.choices().len() {
                self.selected = index;
                return self.save();
            }
        }
        Outcome::Continue
    }

    fn save(self) -> Outcome {
        let value = self.choices()[self.selected];
        let result = match self.section {
            Section::Theme => crate::config::write_theme(value),
            Section::Notifications => crate::config::write_notification_delivery(match value {
                "off" => crate::config::NotificationDelivery::Off,
                "terminal" => crate::config::NotificationDelivery::Terminal,
                "system" => crate::config::NotificationDelivery::System,
                _ => crate::config::NotificationDelivery::Herdr,
            }),
            Section::Sound => crate::config::write_notification_sound(value == "on"),
        };
        if result.is_ok() {
            Outcome::Saved
        } else {
            Outcome::Continue
        }
    }
}

fn selected_for(section: Section, config: &crate::config::Config) -> usize {
    let value = match section {
        Section::Theme => config.theme_name.as_deref().unwrap_or("catppuccin"),
        Section::Notifications => match config.notification_delivery {
            crate::config::NotificationDelivery::Off => "off",
            crate::config::NotificationDelivery::Herdr => "herdr",
            crate::config::NotificationDelivery::Terminal => "terminal",
            crate::config::NotificationDelivery::System => "system",
        },
        Section::Sound => {
            if config.notification_sound {
                "on"
            } else {
                "off"
            }
        }
    };
    section
        .choices()
        .iter()
        .position(|choice| *choice == value)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{Outcome, Section, Settings};
    use crossterm::event::KeyCode;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    #[test]
    fn settings_navigation_matches_herdr_sections_and_choices() {
        let mut settings = Settings::open();
        assert_eq!(settings.section, Section::Theme);
        assert!(settings.choices().contains(&"terminal"));
        settings.move_section(1);
        assert_eq!(settings.section, Section::Notifications);
        assert_eq!(settings.choices(), &["off", "herdr", "terminal", "system"]);
        assert_eq!(settings.handle_key(KeyCode::Esc), Outcome::Close);
    }

    #[test]
    fn settings_mouse_selects_a_section() {
        let screen = Rect::new(0, 0, 100, 30);
        let area = Settings::rect(screen);
        let mut settings = Settings::open();
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 30,
            row: area.y + 1,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(settings.handle_mouse(screen, mouse), Outcome::Continue);
        assert_eq!(settings.section, Section::Notifications);
    }
}
