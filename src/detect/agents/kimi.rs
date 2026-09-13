pub(in crate::detect) fn kimi_permission_required(recent: &str) -> bool {
    let current_approval_panel = recent.contains("↵ confirm")
        && (recent.contains("run this command?")
            || recent.contains("write this file?")
            || recent.contains("apply these edits?")
            || recent.contains("stop this task?")
            || recent.contains("ready to build with this plan?")
            || recent.lines().any(approve_question_line))
        && recent.contains(" choose")
        && ["approve", "reject", "revise"]
            .iter()
            .any(|signal| recent.contains(signal));

    let question_panel = recent.contains("↑↓ select")
        && recent.contains("esc cancel")
        && recent.lines().any(|line| {
            let line = line.trim_start();
            line == "question" || line.starts_with("? ")
        })
        && ["↵ choose", "↵ toggle", "↵ save"]
            .iter()
            .any(|signal| recent.contains(signal));

    let legacy_approval_panel = recent.contains("requesting approval")
        && recent.contains("reject")
        && ["approve once", "approve for this session"]
            .iter()
            .any(|signal| recent.contains(signal))
        && ["1/2/3/4 choose", "↵ confirm"]
            .iter()
            .any(|signal| recent.contains(signal));

    current_approval_panel || question_panel || legacy_approval_panel
}

fn approve_question_line(line: &str) -> bool {
    let line = line
        .trim_start()
        .strip_prefix('▶')
        .unwrap_or(line)
        .trim_start();
    line.strip_prefix("approve ")
        .is_some_and(|question| question.trim_end().ends_with('?'))
}

pub(in crate::detect) fn kimi_is_working(recent: &str, bottom_three: &str) -> bool {
    kimi_background_agents_working(bottom_three) || recent.lines().any(kimi_spinner_line)
}

fn kimi_background_agents_working(bottom_three: &str) -> bool {
    bottom_three.lines().any(|line| {
        let mut words = line.split_whitespace();
        let Some(agent) = words.next() else {
            return false;
        };
        let agent = agent.to_ascii_lowercase();
        if !agent.starts_with("kimi")
            || !agent[4..].chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
            || words
                .next()
                .is_none_or(|word| !word.eq_ignore_ascii_case("thinking"))
        {
            return false;
        }

        let Some((_, status)) = line.split_once('[') else {
            return false;
        };
        let Some((status, _)) = status.split_once(']') else {
            return false;
        };
        let mut status = status.split_whitespace();
        let count = status.next().and_then(|count| count.parse::<u32>().ok());
        matches!(count, Some(1..))
            && matches!(status.next(), Some("agent" | "agents"))
            && matches!(status.next(), Some(word) if word.eq_ignore_ascii_case("running"))
            && status.next().is_none()
    })
}

fn kimi_spinner_line(line: &str) -> bool {
    let line = line.trim_start();
    if matches!(line, "🌕" | "🌖" | "🌗" | "🌘" | "🌑" | "🌒" | "🌓" | "🌔") {
        return true;
    }

    let spinner_end = line
        .char_indices()
        .take_while(|(_, character)| is_braille_spinner(*character))
        .map(|(index, character)| index + character.len_utf8())
        .last();
    let Some(spinner_end) = spinner_end else {
        return false;
    };
    let activity = line[spinner_end..].trim_start();
    activity.starts_with("thinking...")
        || activity.starts_with("working...")
        || activity.starts_with("using ")
}

fn is_braille_spinner(character: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&character)
}
