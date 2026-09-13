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

#[cfg(test)]
mod tests {
    use super::codex_should_skip_state_update;

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
}
