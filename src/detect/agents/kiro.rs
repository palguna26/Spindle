pub(in crate::detect) fn kiro_permission_required(recent: &str) -> bool {
    (recent.contains("requires approval")
        && [
            "yes, single permission",
            "trust, always allow",
            "no (tab to edit)",
            "esc to close",
        ]
        .iter()
        .any(|signal| recent.contains(signal)))
        || (recent.contains("pending from subagents")
            && ["tool approval", "tool approvals"]
                .iter()
                .any(|signal| recent.contains(signal))
            && [
                "approve all pending",
                "configure individually",
                "exit (cancel subagents)",
            ]
            .iter()
            .any(|signal| recent.contains(signal)))
}

pub(in crate::detect) fn kiro_is_working(recent: &str) -> bool {
    recent.contains("kiro is working")
        || (recent.contains("esc to cancel")
            && recent.lines().any(|line| {
                let line = line.trim_start();
                let Some(spinner) = line.chars().next() else {
                    return false;
                };
                matches!(spinner, '◔' | '◑' | '◕' | '●')
                    && line[spinner.len_utf8()..]
                        .trim_start()
                        .chars()
                        .next()
                        .is_some_and(char::is_alphabetic)
            }))
}

pub(in crate::detect) fn kiro_is_idle(screen: &str) -> bool {
    let bottom_five = crate::detect::recent_nonempty_lines(screen, 5).to_ascii_lowercase();
    bottom_five.contains("ask a question or describe a task")
        && bottom_five.contains("/copy to clipboard")
        && !bottom_five.contains("kiro is working")
        && !bottom_five.contains("esc to cancel")
}
