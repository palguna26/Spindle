pub(in crate::detect) fn devin_permission_required(bottom_eight: &str) -> bool {
    (bottom_eight.contains("do you trust the authors of this directory?")
        && bottom_eight.contains("with untrusted content.")
        && bottom_eight.contains("yes, trust "))
        || (bottom_eight.contains("approve once")
            && bottom_eight.contains("select")
            && bottom_eight.contains("confirm")
            && bottom_eight.contains("esc cancel"))
}

pub(in crate::detect) fn devin_is_working(bottom_eight: &str) -> bool {
    let blocked = bottom_eight.contains("approve once") && bottom_eight.contains("esc cancel");
    !blocked
        && ((bottom_eight.contains("running tools") && bottom_eight.contains("esc to interrupt"))
            || bottom_eight.contains("guide devin while it works")
            || (bottom_eight.contains("reading shell ") && bottom_eight.contains("timeout:")))
}

pub(in crate::detect) fn devin_is_idle(screen: &str) -> bool {
    let bottom_eight = crate::detect::recent_nonempty_lines(screen, 8).to_ascii_lowercase();
    let bottom_six = crate::detect::recent_nonempty_lines(screen, 6).to_ascii_lowercase();
    let welcome_prompt = bottom_eight.contains("ask devin to build")
        && bottom_eight.contains("features, fix bugs")
        && bottom_eight.contains("your code")
        && bottom_eight
            .lines()
            .any(|line| line.trim_start().starts_with('❭') && line.contains("ask devin to build"));
    let live_prompt = bottom_six.contains("context:")
        && bottom_six
            .lines()
            .any(|line| line.trim_start().starts_with('❭'));
    let blocked = devin_permission_required(&bottom_eight);
    let working = devin_is_working(&bottom_eight);
    (welcome_prompt || live_prompt) && !blocked && !working
}
