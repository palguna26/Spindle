pub(in crate::detect) fn copilot_permission_required(recent: &str) -> bool {
    let escape_hint = recent.contains("esc to cancel") || recent.contains("esc cancel");
    let confirmation_hint = [
        "enter to select",
        "enter to confirm",
        "enter to submit",
        "enter accept",
    ]
    .iter()
    .any(|hint| recent.contains(hint));
    escape_hint && confirmation_hint
}

pub(in crate::detect) fn copilot_has_cancel_hint(recent: &str) -> bool {
    [
        "esc to cancel",
        "esc cancel",
        "esc again to cancel",
        "esc interrupt",
    ]
    .iter()
    .any(|hint| recent.contains(hint))
}

pub(in crate::detect) fn copilot_background_agents_working(recent: &str) -> bool {
    recent
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(6)
        .any(|line| {
            line.trim_start()
                .strip_prefix('\u{25ce}')
                .is_some_and(|message| {
                    message
                        .trim_start()
                        .starts_with("waiting for background agents")
                })
        })
}
