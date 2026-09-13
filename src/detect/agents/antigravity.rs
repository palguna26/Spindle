pub(in crate::detect) fn antigravity_permission_required(recent: &str) -> bool {
    recent.contains("requesting permission for:")
        && (recent.contains("do you want to proceed?")
            || (recent.contains("tab amend") && recent.contains("edit command")))
}

pub(in crate::detect) fn antigravity_is_working(recent: &str, bottom_five: &str) -> bool {
    recent.lines().any(has_spinner_working_line)
        || bottom_five.lines().any(has_background_task_line)
}

fn has_spinner_working_line(line: &str) -> bool {
    let mut characters = line.trim_start().chars().peekable();
    let mut spinner_count = 0;
    while characters
        .peek()
        .is_some_and(|character| ('\u{2800}'..='\u{28ff}').contains(character))
    {
        characters.next();
        spinner_count += 1;
    }
    if spinner_count == 0 || !characters.next().is_some_and(char::is_whitespace) {
        return false;
    }

    let Some(word) = characters.find(|character| !character.is_whitespace()) else {
        return false;
    };
    word.is_alphabetic()
        && std::iter::once(word)
            .chain(characters)
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect::<String>()
            .to_ascii_lowercase()
            .ends_with("ing")
}

fn has_background_task_line(line: &str) -> bool {
    let line = line.trim_start();
    let Some(rest) = line.strip_prefix('·') else {
        return false;
    };
    let mut parts = rest.split_whitespace();
    parts
        .next()
        .is_some_and(|count| count.parse::<u32>().is_ok_and(|count| count > 0))
        && parts
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("task"))
}
