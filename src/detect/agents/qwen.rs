use crate::detect::AgentState;

pub(in crate::detect) fn qwen_state(
    screen: &str,
    title: &str,
    osc_progress: &str,
) -> Option<AgentState> {
    let title = title.to_lowercase();
    let bottom_twenty = recent_nonempty_lines(screen, 20).to_lowercase();
    let bottom_eight = recent_nonempty_lines(screen, 8).to_lowercase();
    let bottom_thirty = recent_nonempty_lines(screen, 30).to_lowercase();

    if status_title(&title, '✳') {
        return Some(AgentState::Blocked);
    }
    if status_title(&title, '◐') {
        return Some(AgentState::Working);
    }
    if waiting_for_confirmation(&bottom_twenty)
        || tool_confirmation(&bottom_twenty)
        || question_dialog(&bottom_twenty)
        || folder_trust_dialog(&bottom_twenty)
    {
        return Some(AgentState::Blocked);
    }
    if cancel_hint_working(&bottom_eight) {
        return Some(AgentState::Working);
    }
    if osc_progress.starts_with("4;3")
        && osc_progress
            .chars()
            .nth(3)
            .is_none_or(|character| character == ';')
    {
        return Some(AgentState::Working);
    }
    if composer_idle(&bottom_thirty) {
        return Some(AgentState::Idle);
    }
    None
}

fn status_title(title: &str, marker: char) -> bool {
    let Some(rest) = title.strip_prefix(marker) else {
        return false;
    };
    let rest = rest.strip_prefix('\u{fe0e}').unwrap_or(rest);
    rest.starts_with(' ')
}

fn waiting_for_confirmation(bottom: &str) -> bool {
    bottom.lines().any(|line| {
        let line = line.trim();
        line.strip_prefix('⠏').is_some_and(|rest| {
            rest.chars().next().is_some_and(char::is_whitespace) && rest.trim_end().ends_with("...")
        })
    }) && [
        "waiting for user confirmation...",
        "等待用户确认...",
        "等待用戶確認...",
        "warten auf benutzerbestätigung...",
        "en attente de la confirmation de l'utilisateur...",
        "ユーザーの確認を待っています...",
        "aguardando confirmação do usuário...",
        "ожидание подтверждения от пользователя...",
        "esperant la confirmació de l'usuari...",
    ]
    .iter()
    .any(|signal| bottom.contains(signal))
}

fn tool_confirmation(bottom: &str) -> bool {
    bottom.contains("yes, allow once")
        && [
            "apply this change?",
            "allow execution of:",
            "allow execution of mcp tool",
            "do you want to proceed?",
            "shell command execution",
        ]
        .iter()
        .any(|signal| bottom.contains(signal))
}

fn question_dialog(bottom: &str) -> bool {
    bottom.lines().any(|line| {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("↑/↓") {
            return rest.contains(':') && (rest.contains("enter") || rest.contains("return"));
        }
        let line = line
            .strip_prefix('❯')
            .or_else(|| line.strip_prefix('›'))
            .map(str::trim_start)
            .unwrap_or(line);
        let line = line
            .strip_prefix("[ ]")
            .or_else(|| line.strip_prefix("[✓]"))
            .map(str::trim_start)
            .unwrap_or(line);
        let Some((number, suffix)) = line.split_once('.') else {
            return false;
        };
        !number.is_empty()
            && number.chars().all(|character| character.is_ascii_digit())
            && suffix.chars().next().is_some_and(char::is_whitespace)
    })
}

fn folder_trust_dialog(bottom: &str) -> bool {
    bottom.contains("do you trust this folder?")
        && bottom.contains("trust folder (")
        && bottom.contains("don't trust (esc)")
}

fn cancel_hint_working(bottom: &str) -> bool {
    bottom.lines().any(|line| {
        let line = line.trim();
        let Some((prefix, suffix)) = line.split_once('(') else {
            return false;
        };
        let Some(suffix) = suffix.strip_suffix(')') else {
            return false;
        };
        let Some((duration, cancel)) = suffix.split_once('·') else {
            return false;
        };
        let prefix = prefix.trim_end();
        let mut characters = prefix.chars().peekable();
        let has_spinner = characters
            .peek()
            .is_some_and(|character| ('⠁'..='⣿').contains(character));
        if has_spinner {
            characters.next();
        }
        let spinner = has_spinner && characters.next().is_some_and(char::is_whitespace);
        let dots = [".", ".."].iter().any(|dots| {
            prefix
                .strip_prefix(dots)
                .is_some_and(|rest| rest.chars().next().is_some_and(char::is_whitespace))
        });
        (prefix.is_empty() || spinner || dots)
            && cancel.trim() == "esc to cancel"
            && is_duration(duration.trim())
    })
}

fn is_duration(value: &str) -> bool {
    let value = value.trim();
    let Some(unit_start) = value.find(['m', 's']) else {
        return false;
    };
    let count = &value[..unit_start];
    if count.is_empty() || !count.chars().all(|character| character.is_ascii_digit()) {
        return false;
    }
    let rest = &value[unit_start..];
    if let Some(minutes) = rest.strip_prefix('m') {
        if minutes.is_empty() {
            return true;
        }
        let Some(seconds) = minutes.trim_start().strip_suffix('s') else {
            return false;
        };
        let seconds = seconds.trim();
        !seconds.is_empty() && seconds.chars().all(|character| character.is_ascii_digit())
    } else {
        rest == "s"
    }
}

fn composer_idle(bottom: &str) -> bool {
    bottom.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with('>')
    }) && [
        "type your message",
        "your message or @path/to/file",
        "@path/to/file",
    ]
    .iter()
    .any(|signal| bottom.contains(signal))
        || (bottom
            .lines()
            .any(|line| line.trim_start().starts_with('>'))
            && ["type", "mes", "sage", "@pat", "h/to", "/fil"]
                .iter()
                .all(|fragment| bottom.contains(fragment)))
}

fn recent_nonempty_lines(screen: &str, limit: usize) -> String {
    screen
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}
