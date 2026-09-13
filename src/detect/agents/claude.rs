pub(in crate::detect) fn claude_should_skip_state_update(screen: &str) -> bool {
    let recent = screen.to_ascii_lowercase();
    let bottom_three = recent_nonempty_lines(screen, 3).to_ascii_lowercase();
    let transcript_viewer = bottom_three.contains("showing detailed transcript")
        && (["ctrl+o", "to toggle"]
            .iter()
            .all(|signal| bottom_three.contains(signal))
            || ["ctrl+e", "show all"]
                .iter()
                .all(|signal| bottom_three.contains(signal))
            || ["ctrl+e", "collapse"]
                .iter()
                .all(|signal| bottom_three.contains(signal))
            || bottom_three.contains("↑↓ scroll")
            || bottom_three.contains("? for shortcuts"));
    let model_picker = ["select model", "enter to set as default", "esc to cancel"]
        .iter()
        .all(|signal| recent.contains(signal))
        && !recent.contains("do you want to proceed?")
        && !recent.contains("enter to select");

    transcript_viewer || model_picker
}

fn recent_nonempty_lines(screen: &str, limit: usize) -> String {
    screen
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::claude_should_skip_state_update;

    #[test]
    fn detailed_transcript_controls_suppress_status_updates() {
        for controls in [
            "Ctrl+O to toggle",
            "Ctrl+E show all",
            "Ctrl+E collapse",
            "↑↓ scroll",
            "? for shortcuts",
        ] {
            let screen = format!("Showing detailed transcript\n{controls}");
            assert!(claude_should_skip_state_update(&screen), "{controls}");
        }
    }

    #[test]
    fn model_picker_suppresses_status_without_matching_real_prompts() {
        assert!(claude_should_skip_state_update(
            "Select model\nEnter to set as default\nEsc to cancel"
        ));
        assert!(!claude_should_skip_state_update(
            "Select model\nEnter to select\nEsc to cancel"
        ));
        assert!(!claude_should_skip_state_update(
            "Select model\nEnter to set as default\nEsc to cancel\nDo you want to proceed?"
        ));
    }

    #[test]
    fn transcript_controls_must_be_in_the_last_three_nonempty_lines() {
        assert!(!claude_should_skip_state_update(
            "Showing detailed transcript\nCtrl+O to toggle\nordinary output\none\ntwo"
        ));
    }
}
