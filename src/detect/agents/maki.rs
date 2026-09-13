use crate::detect::AgentState;

pub(in crate::detect) fn maki_state(screen: &str) -> AgentState {
    let recent = recent_nonempty_lines(screen, 20).to_lowercase();
    let bottom_three = recent_nonempty_lines(screen, 3);
    let status_line = recent_nonempty_lines(screen, 1);

    if (recent.contains("permission required")
        && (contains_all(&recent, &["y allow", "n deny"])
            || recent.contains("confirm allow")
            || recent.contains("confirm deny")
            || contains_all(&recent, &["enter deny", "esc cancel"])))
        || (recent.contains("plan complete")
            && recent.contains("enter confirm")
            && (recent.contains("space toggle parallel") || recent.contains("edit plan")))
    {
        return AgentState::Blocked;
    }

    if status_bar_mode(&status_line, true) {
        return AgentState::Working;
    }
    if status_bar_mode(&status_line, false) {
        return AgentState::Idle;
    }

    let prompt_box = bottom_three.lines().any(|line| line.starts_with("❯ "));
    let has_spinner_placeholder = bottom_three.lines().any(spinner_placeholder);
    if prompt_box && !recent.contains("queue another prompt") && !has_spinner_placeholder {
        return AgentState::Idle;
    }
    AgentState::Unknown
}

fn contains_all(text: &str, signals: &[&str]) -> bool {
    signals.iter().all(|signal| text.contains(signal))
}

fn status_bar_mode(line: &str, spinner: bool) -> bool {
    let modes = ["[BUILD]", "[PLAN]", "[BASH]"];
    if spinner {
        let Some(prefix) = spinner_prefix_end(line) else {
            return false;
        };
        modes.iter().any(|mode| line[prefix..].starts_with(mode))
    } else {
        modes
            .iter()
            .any(|mode| line.starts_with(&format!(" {mode}")))
    }
}

fn spinner_placeholder(line: &str) -> bool {
    spinner_prefix_end(line).is_some()
}

fn spinner_prefix_end(line: &str) -> Option<usize> {
    let characters: Vec<_> = line.char_indices().collect();
    let mut index = 0;
    let mut count = 0;
    loop {
        if characters
            .get(index)
            .is_none_or(|(_, character)| *character != ' ')
        {
            return None;
        }
        let (_, spinner) = *characters.get(index + 1)?;
        if !('\u{2800}'..='\u{28ff}').contains(&spinner) {
            return None;
        }
        index += 2;
        count += 1;
        if count > 2 {
            return None;
        }
        if characters
            .get(index)
            .is_none_or(|(_, character)| *character != ' ')
        {
            return None;
        }
        if characters
            .get(index + 1)
            .is_some_and(|(_, character)| ('\u{2800}'..='\u{28ff}').contains(character))
        {
            if count == 2 {
                return None;
            }
            continue;
        }
        return Some(
            characters
                .get(index + 1)
                .map_or(line.len(), |(offset, _)| *offset),
        );
    }
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
