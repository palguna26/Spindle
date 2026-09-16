pub(in crate::detect) fn codex_should_skip_state_update(screen: &str) -> bool {
    let lines: Vec<_> = screen.lines().collect();
    let after_prompt = lines
        .iter()
        .rposition(|line| *line == "›" || line.starts_with("› "))
        .map(|index| lines[index + 1..].join("\n"))
        .unwrap_or_else(|| screen.to_owned())
        .to_ascii_lowercase();

    [
        "↑/↓ to scroll",
        "pgup/pgdn to",
        "home/end to jump",
        "q to quit",
    ]
    .iter()
    .all(|signal| after_prompt.contains(signal))
        && ["esc to edit prev", "esc/← to edit prev"]
            .iter()
            .any(|signal| after_prompt.contains(signal))
}

pub(in crate::detect) fn codex_after_last_prompt_marker(screen: &str) -> String {
    let lines: Vec<_> = screen.lines().collect();
    lines
        .iter()
        .rposition(|line| *line == "›" || line.starts_with("› "))
        .map(|index| lines[index + 1..].join("\n"))
        .unwrap_or_else(|| screen.to_owned())
}

pub(in crate::detect) fn codex_has_current_prompt_marker(screen: &str) -> bool {
    let lines: Vec<_> = screen.lines().collect();
    let Some(index) = lines
        .iter()
        .rposition(|line| *line == "›" || line.starts_with("› "))
    else {
        return false;
    };
    !lines[index + 1..].iter().any(|line| {
        line.starts_with('•')
            || line.starts_with('■')
            || line.starts_with('✗')
            || line.starts_with('✓')
    })
}

#[cfg(test)]
mod tests {
    use super::{
        codex_after_last_prompt_marker, codex_has_current_prompt_marker,
        codex_should_skip_state_update,
    };

    #[test]
    fn transcript_viewer_after_the_latest_prompt_preserves_agent_state() {
        let screen = "• Working (4s · esc to interrupt)\n› transcript\n\
            ↑/↓ to scroll · pgup/pgdn to move · home/end to jump · q to quit · esc to edit prev\n";

        assert!(codex_should_skip_state_update(screen));
    }

    #[test]
    fn transcript_viewer_cues_before_the_latest_prompt_do_not_match() {
        let screen =
            "↑/↓ to scroll · pgup/pgdn to move · home/end to jump · q to quit · esc to edit prev\n\
            › next prompt\n";

        assert!(!codex_should_skip_state_update(screen));
    }

    #[test]
    fn incomplete_transcript_viewer_footer_does_not_suppress_detection() {
        let screen =
            "› transcript\n↑/↓ to scroll · pgup/pgdn to move · q to quit · esc to edit prev\n";

        assert!(!codex_should_skip_state_update(screen));
    }

    #[test]
    fn blocker_regions_follow_the_current_prompt() {
        let screen = "Run this command? [y/n]\n› ready\n";
        assert_eq!(codex_after_last_prompt_marker(screen), "");
        assert!(codex_has_current_prompt_marker(screen));
    }

    #[test]
    fn a_working_block_marker_disables_the_current_prompt_region() {
        let screen = "› ready\n• Working (2s)\n";
        assert!(!codex_has_current_prompt_marker(screen));
    }
}
