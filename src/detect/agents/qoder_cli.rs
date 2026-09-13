pub(in crate::detect) fn qodercli_permission_required(recent: &str) -> bool {
    [
        "permission required",
        "allow once or always?",
        "asking user",
        "enter your response",
        "review your answers:",
        "shell awaiting input",
    ]
    .iter()
    .any(|signal| recent.contains(signal))
        || (recent.contains("waiting for user confirmation")
            && ["yes", "no", "allow", "reject"]
                .iter()
                .any(|signal| recent.contains(signal)))
        || (recent.contains("awaiting approval")
            && ["allow", "reject"]
                .iter()
                .any(|signal| recent.contains(signal)))
}

pub(in crate::detect) fn qodercli_is_working(recent: &str) -> bool {
    recent.contains("(esc to cancel,")
        || recent.lines().any(|line| {
            let line = line.trim_start();
            let Some(spinner) = line.chars().next() else {
                return false;
            };
            let rest = &line[spinner.len_utf8()..];
            is_braille_spinner(spinner)
                && rest.chars().next().is_some_and(char::is_whitespace)
                && rest.chars().any(char::is_alphabetic)
        })
}

fn is_braille_spinner(character: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&character)
}
