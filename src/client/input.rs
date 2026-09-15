use crate::config::Config;
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
    SwitchTab(usize),
    EnterCopyMode,
    NextSpace,
    PreviousSpace,
    NextWorkspace,
    WorkspacePicker,
    SessionNavigator,
    SwitchWorkspaceByName,
    StopFocusedPane,
    RestartFocusedPane,
    EditScrollback,
    EnterResizeMode,
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
    ToggleZoom,
    ToggleSidebar,
    ToggleRightClickPassthrough,
    Settings,
    Help,
    CommandPalette,
    OpenNotificationTarget,
    Send(KeyEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    action: Action,
    code: KeyCode,
    modifiers: KeyModifiers,
    prefix: bool,
}

#[derive(Debug, Clone)]
pub struct Keymap {
    prefix: (KeyCode, KeyModifiers),
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            prefix: (KeyCode::Char('b'), KeyModifiers::CONTROL),
            bindings: default_bindings(),
        }
    }
}

impl Keymap {
    pub fn from_config(config: &Config) -> Self {
        let mut keymap = Self::default();
        if let Some(prefix) = config.prefix.as_deref().and_then(parse_combo) {
            keymap.prefix = prefix;
        }
        for (name, values) in &config.bindings {
            let Some(action) = action_name(name) else {
                continue;
            };
            let parsed: Vec<_> = values
                .iter()
                .filter_map(|value| parse_binding(value))
                .collect();
            if !parsed.is_empty() {
                keymap.bindings.retain(|binding| binding.action != action);
                keymap
                    .bindings
                    .extend(parsed.into_iter().map(|(code, modifiers, prefix)| Binding {
                        action,
                        code,
                        modifiers,
                        prefix,
                    }));
            }
        }
        keymap
    }

    pub fn is_prefix(&self, key: KeyEvent) -> bool {
        key.code == self.prefix.0 && key.modifiers == self.prefix.1
    }

    pub fn action(&self, prefix_active: bool, key: KeyEvent) -> Action {
        self.bindings
            .iter()
            .find(|binding| {
                binding.prefix == prefix_active
                    && binding.code == key.code
                    && binding.modifiers == key.modifiers
            })
            .map(|binding| binding.action)
            .unwrap_or_else(|| {
                if prefix_active {
                    Action::None
                } else {
                    Action::Send(key)
                }
            })
    }

    pub fn binding_label(&self, action: Action) -> String {
        self.bindings
            .iter()
            .find(|binding| binding.action == action)
            .map(|binding| {
                let key = key_label(binding.code, binding.modifiers);
                if binding.prefix {
                    format!("prefix+{key}")
                } else {
                    key
                }
            })
            .unwrap_or_else(|| "unbound".into())
    }
}

pub fn action(prefix_active: bool, key: KeyEvent) -> Action {
    Keymap::default().action(prefix_active, key)
}

pub fn is_prefix(key: KeyEvent) -> bool {
    Keymap::default().is_prefix(key)
}

fn default_bindings() -> Vec<Binding> {
    let mut bindings: Vec<_> = [
        (Action::Detach, 'd', false),
        (Action::Detach, 'q', false),
        (Action::DeleteActiveWorkspace, 'D', true),
        (Action::NewTab, 'c', false),
        (Action::CreateWorkspace, 'N', true),
        (Action::NextTab, 'n', false),
        (Action::NextTab, ']', false),
        (Action::PreviousTab, 'p', false),
        (Action::RenameActiveTab, 'T', true),
        (Action::RenameFocusedPane, 'P', true),
        (Action::ClosePane, 'x', false),
        (Action::CloseTab, 'X', true),
        (Action::EnterCopyMode, '[', false),
        (Action::NextSpace, '}', false),
        (Action::PreviousSpace, '{', false),
        (Action::WorkspacePicker, 'w', false),
        (Action::RenameActiveWorkspace, 'W', true),
        (Action::SessionNavigator, 'g', false),
        (Action::Settings, 's', false),
        (Action::EnterResizeMode, 'r', false),
        (Action::EditScrollback, 'e', false),
        (Action::OpenNotificationTarget, 'o', false),
        (Action::FocusPrevious, 'O', true),
        (Action::FocusLeft, 'h', false),
        (Action::FocusDown, 'j', false),
        (Action::FocusUp, 'k', false),
        (Action::FocusRight, 'l', false),
        (Action::SwapLeft, 'h', true),
        (Action::SwapDown, 'j', true),
        (Action::SwapUp, 'k', true),
        (Action::SwapRight, 'l', true),
        (Action::SplitHorizontal, '-', false),
        (Action::SplitVertical, 'v', false),
        (Action::ResizeSmaller, '<', true),
        (Action::ResizeLarger, '>', true),
        (Action::ToggleZoom, 'z', false),
        (Action::ToggleSidebar, 'b', false),
        (Action::Help, '?', true),
        (Action::CommandPalette, ':', true),
    ]
    .into_iter()
    .map(|(action, code, shifted)| Binding {
        action,
        code: KeyCode::Char(code),
        modifiers: if shifted {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        },
        prefix: true,
    })
    .collect();
    bindings.extend([
        Binding {
            action: Action::FocusNext,
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::FocusPrevious,
            code: KeyCode::Tab,
            modifiers: KeyModifiers::SHIFT,
            prefix: true,
        },
        Binding {
            action: Action::FocusPrevious,
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::FocusPrevious,
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::SHIFT,
            prefix: true,
        },
        Binding {
            action: Action::FocusPrevious,
            code: KeyCode::Char('O'),
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::FocusLeft,
            code: KeyCode::Left,
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::FocusRight,
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::FocusUp,
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::FocusDown,
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::SplitHorizontal,
            code: KeyCode::Char('"'),
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
        Binding {
            action: Action::SplitVertical,
            code: KeyCode::Char('%'),
            modifiers: KeyModifiers::NONE,
            prefix: true,
        },
    ]);
    for (index, code) in ('1'..='9').enumerate() {
        bindings.push(Binding {
            action: Action::SwitchTab(index),
            code: KeyCode::Char(code),
            modifiers: KeyModifiers::NONE,
            prefix: true,
        });
    }
    bindings
}

fn action_name(name: &str) -> Option<Action> {
    Some(match name {
        "detach" => Action::Detach,
        "new_tab" => Action::NewTab,
        "new_pane" => Action::NewPane,
        "close_pane" => Action::ClosePane,
        "close_tab" => Action::CloseTab,
        "next_tab" => Action::NextTab,
        "previous_tab" => Action::PreviousTab,
        "rename_tab" => Action::RenameActiveTab,
        "rename_pane" => Action::RenameFocusedPane,
        "clear_pane_name" => Action::ClearPaneName,
        "next_space" => Action::NextSpace,
        "previous_space" => Action::PreviousSpace,
        "next_workspace" => Action::NextWorkspace,
        "workspace_picker" => Action::WorkspacePicker,
        "session_navigator" => Action::SessionNavigator,
        "stop_pane" => Action::StopFocusedPane,
        "restart_pane" => Action::RestartFocusedPane,
        "edit_scrollback" => Action::EditScrollback,
        "resize_mode" => Action::EnterResizeMode,
        "focus_next" => Action::FocusNext,
        "focus_previous" => Action::FocusPrevious,
        "focus_left" => Action::FocusLeft,
        "focus_down" => Action::FocusDown,
        "focus_up" => Action::FocusUp,
        "focus_right" => Action::FocusRight,
        "swap_left" => Action::SwapLeft,
        "swap_down" => Action::SwapDown,
        "swap_up" => Action::SwapUp,
        "swap_right" => Action::SwapRight,
        "split_horizontal" => Action::SplitHorizontal,
        "split_vertical" => Action::SplitVertical,
        "resize_smaller" => Action::ResizeSmaller,
        "resize_larger" => Action::ResizeLarger,
        "toggle_zoom" => Action::ToggleZoom,
        "toggle_sidebar" => Action::ToggleSidebar,
        "toggle_right_click_passthrough" => Action::ToggleRightClickPassthrough,
        "help" => Action::Help,
        "settings" => Action::Settings,
        "command_palette" => Action::CommandPalette,
        "open_notification_target" => Action::OpenNotificationTarget,
        "enter_copy_mode" => Action::EnterCopyMode,
        "create_workspace" => Action::CreateWorkspace,
        "rename_workspace" => Action::RenameActiveWorkspace,
        "delete_workspace" => Action::DeleteActiveWorkspace,
        _ => return None,
    })
}

fn parse_binding(value: &str) -> Option<(KeyCode, KeyModifiers, bool)> {
    let mut parts = value.trim().split('+').collect::<Vec<_>>();
    let prefix = parts.first().is_some_and(|part| *part == "prefix");
    if prefix {
        parts.remove(0);
    }
    let raw_key = parts.pop()?;
    let key = raw_key.to_ascii_lowercase();
    let mut modifiers = KeyModifiers::NONE;
    for modifier in parts {
        modifiers |= match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KeyModifiers::CONTROL,
            "alt" | "option" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            "super" | "cmd" | "command" => KeyModifiers::SUPER,
            _ => return None,
        };
    }
    let mut code = parse_key(&key)?;
    if modifiers.contains(KeyModifiers::SHIFT) {
        if let KeyCode::Char(character) = code {
            code = KeyCode::Char(character.to_ascii_uppercase());
        }
    } else if matches!(raw_key, "?" | ":" | "}" | "{" | "<" | ">" | "\"" | "%") {
        modifiers |= KeyModifiers::SHIFT;
    }
    Some((code, modifiers, prefix))
}

fn parse_combo(value: &str) -> Option<(KeyCode, KeyModifiers)> {
    parse_binding(value).map(|(code, modifiers, _)| (code, modifiers))
}

fn parse_key(value: &str) -> Option<KeyCode> {
    Some(match value {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "minus" => KeyCode::Char('-'),
        "plus" => KeyCode::Char('+'),
        "comma" => KeyCode::Char(','),
        "period" | "dot" => KeyCode::Char('.'),
        "slash" => KeyCode::Char('/'),
        "backslash" => KeyCode::Char('\\'),
        "[" | "bracketleft" => KeyCode::Char('['),
        "]" | "bracketright" => KeyCode::Char(']'),
        value if value.chars().count() == 1 => KeyCode::Char(value.chars().next()?),
        _ => return None,
    })
}

fn key_label(code: KeyCode, modifiers: KeyModifiers) -> String {
    let mut label = String::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        label.push_str("Ctrl+");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        label.push_str("Alt+");
    }
    if modifiers.contains(KeyModifiers::SUPER) {
        label.push_str("Super+");
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        label.push_str("Shift+");
    }
    match code {
        KeyCode::Char(character) => format!("{label}{character}"),
        KeyCode::Enter => format!("{label}Enter"),
        KeyCode::Esc => format!("{label}Esc"),
        KeyCode::Tab => format!("{label}Tab"),
        KeyCode::Left => format!("{label}Left"),
        KeyCode::Right => format!("{label}Right"),
        KeyCode::Up => format!("{label}Up"),
        KeyCode::Down => format!("{label}Down"),
        _ => format!("{label}key"),
    }
}

#[cfg(test)]
mod tests {
    use super::{action, is_prefix, Action, Keymap};
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::BTreeMap;

    #[test]
    fn prefix_commands_are_discoverable() {
        let prefix = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert!(is_prefix(prefix));
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
            Action::Detach
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)),
            Action::DeleteActiveWorkspace
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
            action(true, KeyEvent::new(KeyCode::Char('W'), KeyModifiers::SHIFT)),
            Action::RenameActiveWorkspace
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
            Action::OpenNotificationTarget
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Action::FocusNext
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
            Action::FocusPrevious
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT)),
            Action::FocusPrevious
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT)),
            Action::RenameActiveTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('P'), KeyModifiers::SHIFT)),
            Action::RenameFocusedPane
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
            Action::SwitchTab(0)
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)),
            Action::NextTab
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            Action::Settings
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            Action::EnterResizeMode
        );
        assert_eq!(
            action(true, KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)),
            Action::EditScrollback
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
            Action::Send(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        );
    }

    #[test]
    fn configured_prefix_and_action_override_defaults() {
        let keymap = Keymap::from_config(&Config {
            prefix: Some("ctrl+a".into()),
            bindings: BTreeMap::from([(String::from("new_tab"), vec![String::from("prefix+t")])]),
            ..Config::default()
        });
        assert!(keymap.is_prefix(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)));
        assert_eq!(
            keymap.action(false, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Action::Send(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
        );
        assert_eq!(
            keymap.action(false, KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)),
            Action::Send(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
        );
        assert_eq!(
            keymap.action(true, KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)),
            Action::NewTab
        );
    }

    #[test]
    fn invalid_configured_binding_keeps_the_default() {
        let keymap = Keymap::from_config(&Config {
            bindings: BTreeMap::from([(
                String::from("new_tab"),
                vec![String::from("prefix+not-a-key")],
            )]),
            ..Config::default()
        });
        assert_eq!(
            keymap.action(true, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Action::NewTab
        );
    }
}
