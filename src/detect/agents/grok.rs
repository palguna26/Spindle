use crate::detect::AgentState;

pub(in crate::detect) fn grok_state(screen: &str, title: &str, osc_progress: &str) -> AgentState {
    let title = title.to_lowercase();
    let recent = recent_nonempty_lines(screen, 20).to_lowercase();
    let bottom_two = recent_nonempty_lines(screen, 2).to_lowercase();
    let top_line = screen
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .to_lowercase();

    if title.contains("action required")
        || option_dialog(&recent)
        || contains_all(&bottom_two, &[":select", "ctrl+o:yolo", "ctrl+c:cancel"])
        || contains_all(&bottom_two, &["tab:scrollback", "shift+x:dismiss"])
        || legacy_permission_scope(&recent)
    {
        return AgentState::Blocked;
    }
    if background_work_chip(&top_line) || osc_progress == "4;1;-1" {
        return AgentState::Working;
    }
    if idle_title(&title) {
        return AgentState::Idle;
    }
    if !title.trim().is_empty() {
        return AgentState::Working;
    }
    if osc_progress == "4;0;0" {
        return AgentState::Idle;
    }
    if recent.lines().any(spinner_status_line)
        || contains_all(&bottom_two, &["esc:cancel", "ctrl+.:shortcuts"])
        || legacy_working(&recent)
    {
        return AgentState::Working;
    }
    if bottom_two.contains("ctrl+.:shortcuts")
        && !bottom_two.contains("esc:cancel")
        && !bottom_two.contains("ctrl+c:cancel")
    {
        return AgentState::Idle;
    }
    AgentState::Unknown
}

fn option_dialog(recent: &str) -> bool {
    recent.lines().any(|line| {
        let Some(rest) = line.trim_start().strip_prefix('┃') else {
            return false;
        };
        let mut parts = rest.split_whitespace();
        let Some(key) = parts.next() else {
            return false;
        };
        if !key
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        {
            return false;
        }
        matches!(parts.next(), Some("(●)" | "(○)"))
            && parts.next().is_some_and(|label| !label.is_empty())
    })
}

fn legacy_permission_scope(recent: &str) -> bool {
    recent.contains("yes, proceed")
        && recent.contains("no, reject")
        && (recent.contains("use ← → to choose permission whitelist scope")
            || recent.contains("←/→:scope"))
}

fn background_work_chip(line: &str) -> bool {
    let Some((marker, rest)) = line.trim_start().split_once(char::is_whitespace) else {
        return false;
    };
    marker
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '⋅' | ':' | '⸬' | '⁙' | '.' | '·'))
        && rest
            .split_once('│')
            .is_some_and(|(count, _)| count.trim().parse::<u32>().is_ok_and(|count| count > 0))
}

fn idle_title(title: &str) -> bool {
    (title == "grok" || title.ends_with(" - grok"))
        && !title
            .chars()
            .any(|character| ('\u{2800}'..='\u{28ff}').contains(&character))
}

fn spinner_status_line(line: &str) -> bool {
    let line = line.trim_start();
    let mut characters = line.chars();
    characters
        .next()
        .is_some_and(|character| ('\u{2801}'..='\u{28ff}').contains(&character))
        && line.trim_end().ends_with("[stop]")
}

fn legacy_working(recent: &str) -> bool {
    recent.contains("ctrl+c:cancel")
        && recent.contains("ctrl+enter:interject")
        && (recent.contains("waiting")
            || recent.lines().any(|line| {
                let line = line.trim_start();
                let Some(spinner) = line.chars().next() else {
                    return false;
                };
                if !('⠁'..='⣿').contains(&spinner) {
                    return false;
                }
                let rest = &line[spinner.len_utf8()..];
                rest.starts_with(' ')
                    && ["run", "read", "search", "list"]
                        .iter()
                        .any(|verb| rest.trim_start().starts_with(verb))
            }))
}

fn contains_all(text: &str, signals: &[&str]) -> bool {
    signals.iter().all(|signal| text.contains(signal))
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
