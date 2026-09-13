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

pub(in crate::detect) fn claude_mcp_elicitation_prompt(recent: &str) -> bool {
    let recent = recent.to_ascii_lowercase();
    recent.contains("esc to cancel")
        && recent.lines().any(is_mcp_request_header)
        && recent.lines().any(is_accept_or_decline_choice)
}

fn is_mcp_request_header(line: &str) -> bool {
    let line = line.trim();
    for (opening, closing) in [("mcp server \"", '"'), ("mcp server “", '”')] {
        if let Some(value) = line.strip_prefix(opening) {
            let Some((server_name, suffix)) = value.split_once(closing) else {
                return false;
            };
            return !server_name.is_empty() && suffix.trim() == "requests your input";
        }
    }
    false
}

fn is_accept_or_decline_choice(line: &str) -> bool {
    let line = line.trim_start();
    let line = line.strip_prefix('❯').unwrap_or(line).trim_start();
    ["accept", "decline"].iter().any(|choice| {
        line.strip_prefix(choice).is_some_and(|suffix| {
            suffix
                .chars()
                .next()
                .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_')
        })
    })
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
    use super::{claude_mcp_elicitation_prompt, claude_should_skip_state_update};

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

    #[test]
    fn mcp_input_request_needs_header_choice_and_cancel_footer() {
        for prompt in [
            "MCP server \"docs\" requests your input\n❯ Accept\nDecline\nEsc to cancel",
            "MCP server “docs” requests your input\nAccept\n❯ Decline\nEsc to cancel",
        ] {
            assert!(claude_mcp_elicitation_prompt(prompt), "{prompt}");
        }
        assert!(!claude_mcp_elicitation_prompt(
            "MCP server \"docs\" requests your input\nEnter to continue\nEsc to cancel"
        ));
        assert!(!claude_mcp_elicitation_prompt(
            "MCP server \"docs\" requests your input\n❯ Accept\nDecline"
        ));
        assert!(!claude_mcp_elicitation_prompt(
            "MCP server \"docs\" status\n❯ Accept\nDecline\nEsc to cancel"
        ));
    }
}
