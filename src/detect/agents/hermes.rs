pub(in crate::detect) fn hermes_permission_required(bottom_fourteen: &str) -> bool {
    let dangerous_approval = (bottom_fourteen.contains("dangerous")
        || bottom_fourteen.contains("approval")
        || (bottom_fourteen.contains("allow once") && bottom_fourteen.contains("deny"))
        || bottom_fourteen.lines().any(|line| {
            let line = line.trim_start();
            let line = line.strip_prefix('▸').unwrap_or(line).trim_start();
            line.strip_prefix('>')
                .unwrap_or(line)
                .trim_start()
                .starts_with("1. allow")
        }))
        && [
            "enter confirm",
            "enter to confirm",
            "↑/↓ to select",
            "show full command",
        ]
        .iter()
        .any(|signal| bottom_fourteen.contains(signal));

    let clarification = (bottom_fourteen.contains("hermes needs your")
        || bottom_fourteen.lines().any(|line| {
            let line = line.trim_start();
            let line = line.strip_prefix('▸').unwrap_or(line).trim_start();
            let line = line.strip_prefix('>').unwrap_or(line).trim_start();
            line.strip_prefix("ask ")
                .is_some_and(|rest| !rest.trim().is_empty())
        })
        || bottom_fourteen.contains("type your answer"))
        && [
            "enter confirm",
            "enter to confirm",
            "enter send",
            "press enter",
            "↑/↓ select",
            "↑/↓ to select",
            "other (type",
        ]
        .iter()
        .any(|signal| bottom_fourteen.contains(signal));

    let credential = bottom_fourteen.contains("sudo password")
        || bottom_fourteen.contains("skill setup")
        || (bottom_fourteen.contains("🔑") && bottom_fourteen.contains("for "));

    let confirmation = ((bottom_fourteen.contains("approve once")
        && bottom_fourteen.contains("cancel"))
        || (bottom_fourteen.contains("start a new session")
            && bottom_fourteen.contains("keep going")))
        && [
            "enter to confirm",
            "enter confirm",
            "type 1/2/3",
            "y/n quick",
        ]
        .iter()
        .any(|signal| bottom_fourteen.contains(signal));

    dangerous_approval || clarification || credential || confirmation
}

pub(in crate::detect) fn hermes_is_priority_working(recent: &str, title: &str) -> bool {
    title_has_status(title, '⏳')
        || ["msg=interrupt", "ctrl+c to interrupt"]
            .iter()
            .any(|signal| recent.contains(signal))
}

pub(in crate::detect) fn hermes_title_blocked(title: &str) -> bool {
    title_has_status(title, '⚠')
}

pub(in crate::detect) fn hermes_is_working(bottom_five: &str) -> bool {
    bottom_five.contains("ctrl+c cancel")
}

pub(in crate::detect) fn hermes_is_idle(title: &str) -> bool {
    title_has_status(title, '✓')
}

fn title_has_status(title: &str, status: char) -> bool {
    let title = title.trim_start();
    let Some(rest) = title.strip_prefix(status) else {
        return false;
    };
    let rest = rest
        .strip_prefix('\u{fe0e}')
        .or_else(|| rest.strip_prefix('\u{fe0f}'))
        .unwrap_or(rest);
    rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace)
}
