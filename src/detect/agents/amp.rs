pub(in crate::detect) fn amp_permission_required(recent: &str, title: &str) -> bool {
    title.contains("plugin confirmation needed")
        || [
            "waiting for approval",
            "invoke tool",
            "run this command?",
            "allow editing file:",
            "allow creating file:",
            "confirm tool call",
        ]
        .iter()
        .any(|signal| recent.contains(signal))
        || (recent.contains("approve")
            && [
                "allow all for this session",
                "allow all for every session",
                "allow file for every session",
                "deny with feedback",
            ]
            .iter()
            .any(|signal| recent.contains(signal)))
}

pub(in crate::detect) fn amp_is_working(recent: &str, bottom_five: &str, title: &str) -> bool {
    title_has_spinner(title)
        || bottom_five.lines().any(status_footer_is_working)
        || recent.contains("esc to cancel")
}

pub(in crate::detect) fn amp_is_idle(title: &str) -> bool {
    title.contains(" - amp - ")
        && !title_has_spinner(title)
        && !title.contains("plugin confirmation needed")
}

fn title_has_spinner(title: &str) -> bool {
    let mut characters = title.chars();
    characters.next().is_some_and(is_braille_spinner)
        && characters.next().is_some_and(char::is_whitespace)
}

fn status_footer_is_working(line: &str) -> bool {
    let Some(footer) = line.trim_start().strip_prefix('╰') else {
        return false;
    };
    let mut fields = footer.split_whitespace();
    let has_subject = fields.next().is_some();
    matches!(
        fields.next(),
        Some("thinking" | "streaming" | "running" | "waiting")
    ) && has_subject
        && fields
            .next()
            .is_some_and(|separator| separator.starts_with('─'))
}

fn is_braille_spinner(character: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&character)
}
