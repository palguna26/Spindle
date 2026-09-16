pub(in crate::detect) fn opencode_permission_required(recent: &str) -> bool {
    if recent.contains("\u{25b3} permission required") {
        return true;
    }

    recent.contains("esc dismiss")
        && (recent.contains("enter confirm")
            || recent.contains("enter submit")
            || recent.contains("enter toggle"))
        && (recent.contains("\u{2191}\u{2193} select") || recent.contains("\u{21c6} tab"))
}

pub(in crate::detect) fn opencode_interrupt_hint_working(recent: &str) -> bool {
    let lower = recent.to_ascii_lowercase();
    [
        "esc to interrupt",
        "ctrl+c to interrupt",
        "press esc to interrupt",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
        || lower.lines().any(|line| {
            line.contains("opencode")
                && (line.contains("esc to interrupt") || line.contains("esc again to interrupt"))
        })
}

pub(in crate::detect) fn opencode_progress_bar_working(recent: &str) -> bool {
    recent.lines().any(|line| {
        let mut run = 0;
        for character in line.chars() {
            if matches!(character, '■' | '⬬') {
                run += 1;
                if run >= 4 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    })
}
