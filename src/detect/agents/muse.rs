use crate::detect::AgentState;

pub(in crate::detect) fn muse_state(screen: &str) -> AgentState {
    let bottom_twelve = recent_nonempty_lines(screen, 12).to_lowercase();
    let bottom_eight = recent_nonempty_lines(screen, 8).to_lowercase();
    let bottom_five = recent_nonempty_lines(screen, 5).to_lowercase();
    let bottom_three = recent_nonempty_lines(screen, 3).to_lowercase();

    let workspace_trust = bottom_twelve.contains("do you trust this workspace?")
        && (bottom_twelve.contains("trust and continue") || bottom_twelve.contains("use up/down"));
    let pick_request = contains_all(
        &bottom_eight,
        &["enter to select", "tab for an optional note"],
    ) || contains_all(&bottom_eight, &["enter to toggle", "esc to interrupt"]);
    if workspace_trust || pick_request {
        return AgentState::Blocked;
    }

    if menu_overlay(&bottom_eight) {
        return AgentState::Unknown;
    }
    if bottom_eight.contains("esc to interrupt") && !pick_request {
        return AgentState::Working;
    }
    if approval_prompt(&bottom_eight) {
        return AgentState::Blocked;
    }
    if idle_prompt(&bottom_five) && !has_overlay_or_work(&bottom_five) {
        return AgentState::Idle;
    }
    if idle_status_fallback(&bottom_three) && !bottom_three.contains("esc to interrupt") {
        return AgentState::Idle;
    }
    AgentState::Unknown
}

pub(in crate::detect) fn muse_should_skip_state_update(screen: &str) -> bool {
    menu_overlay(&recent_nonempty_lines(screen, 8).to_lowercase())
}

fn menu_overlay(bottom: &str) -> bool {
    contains_all(bottom, &["enter confirm", "esc go back"])
        || contains_all(bottom, &["enter save", "esc go back"])
        || contains_all(bottom, &["space toggle", "esc close", "type filter"])
}

fn approval_prompt(bottom: &str) -> bool {
    contains_all(
        bottom,
        &["allow this stage once", "always allow in this workspace"],
    ) || contains_all(bottom, &["allow once", "allow for this session"])
        || contains_all(
            bottom,
            &["yes, proceed", "yes, don't ask again this session"],
        )
}

fn idle_prompt(bottom: &str) -> bool {
    bottom.lines().any(|line| {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix('⟩') else {
            return false;
        };
        rest.chars().all(char::is_whitespace)
            || (rest.chars().next().is_some_and(char::is_whitespace)
                && rest.chars().any(|character| !character.is_whitespace()))
    })
}

fn has_overlay_or_work(bottom: &str) -> bool {
    bottom.contains("esc to interrupt")
        || contains_all(bottom, &["enter to select", "tab for an optional note"])
        || contains_all(bottom, &["enter to toggle", "esc to interrupt"])
        || menu_overlay(bottom)
}

fn idle_status_fallback(bottom: &str) -> bool {
    bottom.lines().any(|line| {
        let Some((_, rest)) = line.trim_start().split_once(" · ") else {
            return false;
        };
        let Some((effort, suffix)) = rest.split_once(" · ") else {
            return false;
        };
        matches!(
            effort,
            "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "ultra"
        ) && !suffix.is_empty()
    })
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
