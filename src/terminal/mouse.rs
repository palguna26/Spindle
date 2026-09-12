use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

pub fn encode_mouse_event(
    event: MouseEvent,
    x: u16,
    y: u16,
    sgr: bool,
    utf8: bool,
) -> Option<Vec<u8>> {
    let (mut button, release) = match event.kind {
        MouseEventKind::Down(button) => (button_code(button)?, false),
        MouseEventKind::Up(button) => (button_code(button)?, true),
        MouseEventKind::Drag(button) => (button_code(button)? + 32, false),
        MouseEventKind::Moved => (35, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
    };
    button += modifier_code(event.modifiers);
    let x = u32::from(x.max(1));
    let y = u32::from(y.max(1));

    if sgr {
        let suffix = if release { 'm' } else { 'M' };
        Some(format!("\x1b[<{button};{x};{y}{suffix}").into_bytes())
    } else {
        let button = if release {
            3 + modifier_code(event.modifiers)
        } else {
            button
        };
        let encode = |value: u32| -> Option<String> {
            let value = value.checked_add(32)?;
            if utf8 {
                char::from_u32(value).map(|character| character.to_string())
            } else if value <= 255 {
                char::from_u32(value).map(|character| character.to_string())
            } else {
                None
            }
        };
        let bytes = format!("\x1b[M{}{}{}", encode(button)?, encode(x)?, encode(y)?);
        Some(bytes.into_bytes())
    }
}

fn button_code(button: MouseButton) -> Option<u32> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
    }
}

fn modifier_code(modifiers: KeyModifiers) -> u32 {
    u32::from(modifiers.contains(KeyModifiers::SHIFT)) * 4
        + u32::from(modifiers.contains(KeyModifiers::ALT)) * 8
        + u32::from(modifiers.contains(KeyModifiers::CONTROL)) * 16
}

#[cfg(test)]
mod tests {
    use super::encode_mouse_event;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn event(kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers,
        }
    }

    #[test]
    fn sgr_mouse_encodes_buttons_modifiers_and_release() {
        assert_eq!(
            encode_mouse_event(
                event(
                    MouseEventKind::Down(MouseButton::Right),
                    KeyModifiers::SHIFT
                ),
                10,
                4,
                true,
                false,
            )
            .unwrap(),
            b"\x1b[<6;10;4M"
        );
        assert_eq!(
            encode_mouse_event(
                event(MouseEventKind::Up(MouseButton::Left), KeyModifiers::NONE),
                2,
                3,
                true,
                false,
            )
            .unwrap(),
            b"\x1b[<0;2;3m"
        );
    }

    #[test]
    fn legacy_mouse_uses_one_based_coordinates_and_rejects_overflow() {
        assert_eq!(
            encode_mouse_event(
                event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                1,
                1,
                false,
                false,
            )
            .unwrap(),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
        assert!(encode_mouse_event(
            event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
            300,
            1,
            false,
            false,
        )
        .is_none());
    }
}
