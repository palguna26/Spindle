pub(in crate::detect) fn cursor_permission_required(recent: &str, bottom_eight: &str) -> bool {
    let write_file_approval = bottom_eight.contains("write to this file?")
        && bottom_eight.contains("proceed (y)")
        && ["reject & propose changes", "esc or n or p", "add write("]
            .iter()
            .any(|signal| bottom_eight.contains(signal));

    let approval_prompt = (recent.contains("waiting for approval")
        && recent.contains("run this command?")
        && ["run (once) (y)", "skip (esc or n)"]
            .iter()
            .any(|signal| recent.contains(signal)))
        || recent.contains("(y) (enter)")
        || recent.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("allow ") && line.contains("(y)")
        })
        || recent.contains("keep (n)")
        || recent.contains("skip (esc or n)")
        || recent.lines().any(|line| {
            let line = line
                .trim_start()
                .strip_prefix('→')
                .unwrap_or(line)
                .trim_start();
            line.starts_with("run ") && line.contains("(y)")
        });

    write_file_approval || approval_prompt
}

pub(in crate::detect) fn cursor_is_working(
    bottom_six: &str,
    bottom_five: &str,
    bottom_eight: &str,
) -> bool {
    bottom_six.contains("ctrl+c to stop")
        || bottom_five.lines().any(has_background_task_count)
        || bottom_eight.lines().any(has_activity_spinner)
}

fn has_background_task_count(line: &str) -> bool {
    let words = line.to_ascii_lowercase();
    let words: Vec<_> = words.split_whitespace().collect();
    words.windows(3).any(|phrase| {
        phrase[0].parse::<u32>().is_ok_and(|count| count > 0)
            && phrase[1] == "background"
            && matches!(phrase[2], "task" | "tasks")
    })
}

fn has_activity_spinner(line: &str) -> bool {
    let line = line.trim_start();
    let spinner_end = match line.chars().next() {
        Some('⬡' | '⬢') => line.chars().next().map(char::len_utf8),
        Some(first) if is_braille_spinner(first) => line
            .char_indices()
            .take_while(|(_, character)| is_braille_spinner(*character))
            .map(|(index, character)| index + character.len_utf8())
            .last(),
        _ => None,
    };
    let Some(spinner_end) = spinner_end else {
        return false;
    };
    let activity = line[spinner_end..].trim_start();
    let word = activity
        .chars()
        .take_while(|character| character.is_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    word.ends_with("ing")
}

fn is_braille_spinner(character: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&character)
}

pub(in crate::detect) fn cursor_agent_node_argv(argv: &[String]) -> bool {
    let (Some(runtime), Some(script)) = (argv.first(), argv.get(1)) else {
        return false;
    };
    let Some((runtime_parent, runtime_name)) = parent_and_basename(runtime) else {
        return false;
    };
    let Some((script_parent, script_name)) = parent_and_basename(script) else {
        return false;
    };
    if !runtime_name.eq_ignore_ascii_case("node.exe")
        || !script_name.eq_ignore_ascii_case("index.js")
        || !runtime_parent.eq_ignore_ascii_case(script_parent)
    {
        return false;
    }

    let mut tail = runtime_parent
        .rsplit(['/', '\\'])
        .filter(|component| !component.is_empty());
    let (Some(version), Some(versions), Some(package)) = (tail.next(), tail.next(), tail.next())
    else {
        return false;
    };
    package.eq_ignore_ascii_case("cursor-agent")
        && versions.eq_ignore_ascii_case("versions")
        && !version.trim().is_empty()
}

fn parent_and_basename(path: &str) -> Option<(&str, &str)> {
    let split = path.rfind(['/', '\\'])?;
    let parent = path[..split].trim_end_matches(['/', '\\']);
    let basename = &path[split + 1..];
    (!parent.is_empty() && !basename.is_empty()).then_some((parent, basename))
}
