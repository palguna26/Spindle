use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Ghostty,
    Iterm2,
    Kitty,
    WezTerm,
}

fn detect_backend() -> Option<Backend> {
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let term = std::env::var("TERM").ok();
    match term_program.as_deref() {
        Some("ghostty") => return Some(Backend::Ghostty),
        Some("iTerm.app") => return Some(Backend::Iterm2),
        Some("WezTerm") => return Some(Backend::WezTerm),
        _ => {}
    }
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return Some(Backend::Kitty);
    }
    match term.as_deref() {
        Some("xterm-ghostty") => Some(Backend::Ghostty),
        Some("xterm-kitty") => Some(Backend::Kitty),
        Some(term) if term.contains("wezterm") => Some(Backend::WezTerm),
        _ => None,
    }
}

pub(crate) fn show(message: &str) -> io::Result<bool> {
    let Some(backend) = detect_backend() else {
        return Ok(false);
    };
    let (title, body) = split_message(message);
    let sequence = match backend {
        Backend::Kitty => build_osc99(title, body),
        Backend::Ghostty | Backend::Iterm2 | Backend::WezTerm => build_osc9(title, body),
    };
    let sequence = if std::env::var_os("TMUX").is_some() {
        wrap_tmux(&sequence)
    } else {
        sequence
    };
    let mut stdout = io::stdout();
    stdout.write_all(&sequence)?;
    stdout.flush()?;
    Ok(true)
}

fn split_message(message: &str) -> (&str, Option<&str>) {
    match message.split_once(": ") {
        Some((title, body)) if !title.is_empty() && !body.is_empty() => (title, Some(body)),
        _ => (message, None),
    }
}

fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|ch| *ch != '\u{1b}' && *ch != '\u{7}' && *ch != '\u{9c}')
        .map(|ch| match ch {
            '\n' | '\r' | '\t' => ' ',
            _ => ch,
        })
        .collect()
}

fn build_osc9(title: &str, body: Option<&str>) -> Vec<u8> {
    let message = body
        .filter(|body| !body.is_empty())
        .map(|body| format!("{title}: {body}"))
        .unwrap_or_else(|| title.to_owned());
    format!("\x1b]9;{}\x1b\\", sanitize(&message)).into_bytes()
}

fn build_osc99(title: &str, body: Option<&str>) -> Vec<u8> {
    let title = sanitize(title);
    match body.filter(|body| !body.is_empty()) {
        Some(body) => format!(
            "\x1b]99;i=1:d=0;{title}\x1b\\\x1b]99;i=1:p=body;{}\x1b\\",
            sanitize(body)
        )
        .into_bytes(),
        None => format!("\x1b]99;;{title}\x1b\\").into_bytes(),
    }
}

fn wrap_tmux(sequence: &[u8]) -> Vec<u8> {
    let mut wrapped = Vec::with_capacity(sequence.len() + 16);
    wrapped.extend_from_slice(b"\x1bPtmux;");
    for &byte in sequence {
        if byte == b'\x1b' {
            wrapped.push(b'\x1b');
        }
        wrapped.push(byte);
    }
    wrapped.extend_from_slice(b"\x1b\\");
    wrapped
}

#[cfg(test)]
mod tests {
    use super::{build_osc99, build_osc9, sanitize, split_message, wrap_tmux};

    #[test]
    fn splits_title_and_body_like_herdr() {
        assert_eq!(split_message("codex finished: pane-1"), ("codex finished", Some("pane-1")));
        assert_eq!(split_message("codex finished"), ("codex finished", None));
    }

    #[test]
    fn sanitizes_terminal_control_bytes() {
        assert_eq!(sanitize("a\n\tb\x1bc\x7f"), "a  bc\x7f");
    }

    #[test]
    fn builds_herdr_notification_sequences() {
        assert_eq!(build_osc9("done", Some("pane-1")), b"\x1b]9;done: pane-1\x1b\\");
        assert_eq!(
            build_osc99("done", Some("pane-1")),
            b"\x1b]99;i=1:d=0;done\x1b\\\x1b]99;i=1:p=body;pane-1\x1b\\"
        );
    }

    #[test]
    fn wraps_and_escapes_tmux_passthrough() {
        assert_eq!(wrap_tmux(b"\x1b]9;hi\x1b\\"), b"\x1bPtmux;\x1b\x1b]9;hi\x1b\x1b\\\x1b\\");
    }
}
