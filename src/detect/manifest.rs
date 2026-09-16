//! Small manifest evaluator shared by screen-based agent detectors.
//!
//! The regions and rule priority follow Herdr's `src/detect/manifest.rs`.
//! Codex, OpenCode, Gemini, Cline, Copilot, Pi, and Qoder CLI are migrated first; other agents still use their
//! compatibility detectors until their rules are moved here.

use super::{agents, AgentState};
use regex::Regex;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DetectionInput<'a> {
    pub(crate) screen: &'a str,
    pub(crate) osc_title: &'a str,
    pub(crate) _osc_progress: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct Rule {
    priority: i32,
    state: AgentState,
    region: Region,
    matcher: Matcher,
}

#[derive(Debug, Clone, Copy)]
enum Region {
    OscTitle,
    TopNonEmpty(usize),
    BottomNonEmpty(usize),
    AfterLastPrompt,
    WholeRecent,
    WholeRecentWithoutCurrentPrompt,
}

#[derive(Debug, Clone, Copy)]
enum Matcher {
    Contains(&'static [&'static str]),
    All(&'static [&'static str]),
    Any(&'static [&'static [&'static str]]),
    Regex(&'static str),
    LineRegex(&'static str),
    TrustDirectory,
    OpenCodePermission,
    CopilotSelection,
    QoderPermission,
}

const CODEX_RULES: &[Rule] = &[
    Rule {
        priority: 1100,
        state: AgentState::Blocked,
        region: Region::OscTitle,
        matcher: Matcher::Contains(&["action required"]),
    },
    Rule {
        priority: 1050,
        state: AgentState::Working,
        region: Region::OscTitle,
        matcher: Matcher::Regex(r"(?:^| )[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏](?: |$)"),
    },
    Rule {
        priority: 1000,
        state: AgentState::Unknown,
        region: Region::AfterLastPrompt,
        matcher: Matcher::All(&[
            "↑/↓ to scroll",
            "pgup/pgdn to",
            "home/end to jump",
            "q to quit",
        ]),
    },
    Rule {
        priority: 950,
        state: AgentState::Blocked,
        region: Region::TopNonEmpty(20),
        matcher: Matcher::TrustDirectory,
    },
    Rule {
        priority: 950,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(20),
        matcher: Matcher::All(&[
            "update available!",
            "update now",
            "skip until next version",
            "press enter to continue",
        ]),
    },
    Rule {
        priority: 900,
        state: AgentState::Blocked,
        region: Region::AfterLastPrompt,
        matcher: Matcher::Any(&[
            &["press enter to confirm or esc to cancel"],
            &["enter to submit answer"],
            &["enter to submit all"],
            &["allow command?"],
        ]),
    },
    Rule {
        priority: 600,
        state: AgentState::Blocked,
        region: Region::WholeRecentWithoutCurrentPrompt,
        matcher: Matcher::Any(&[
            &["[y/n]"],
            &["yes (y)"],
            &["do you want to", "yes"],
            &["do you want to", "❯"],
            &["would you like to", "yes"],
            &["would you like to", "❯"],
        ]),
    },
    Rule {
        priority: 500,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(3),
        matcher: Matcher::LineRegex(r"^[•◦]\s+working \([^)]*esc to interrupt\)(?: · .*)?$"),
    },
    Rule {
        priority: 100,
        state: AgentState::Idle,
        region: Region::OscTitle,
        matcher: Matcher::Regex(r"\S"),
    },
];

const OPENCODE_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::OpenCodePermission,
    },
    Rule {
        priority: 110,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Any(&[
            &["esc to interrupt"],
            &["ctrl+c to interrupt"],
            &["press esc to interrupt"],
        ]),
    },
    Rule {
        priority: 105,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::LineRegex(r"(?i).*opencode.*esc (again to )?interrupt"),
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Regex(r"(■|⬝){4,}"),
    },
];

const GEMINI_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::Any(&[
            &["│ apply this change"],
            &["│ allow execution"],
            &["yes", "waiting for user confirmation"],
            &["yes", "│ do you want to proceed"],
            &["yes", "do you want to proceed?"],
        ]),
    },
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::LineRegex(r"(?i)^\s*❯.*(yes|allow)"),
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Contains(&["esc to cancel"]),
    },
];

const CLINE_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::Any(&[
            &["let cline use this tool"],
            &["[act mode]", "execute command?", "yes"],
            &["[act mode]", "use this tool?", "yes"],
            &["[plan mode]", "execute command?", "yes"],
            &["[plan mode]", "use this tool?", "yes"],
        ]),
    },
    Rule {
        priority: -10,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Regex(r"(?s).+"),
    },
];

const COPILOT_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::CopilotSelection,
    },
    Rule {
        priority: 110,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(6),
        matcher: Matcher::LineRegex(r"^\s*◎\s+waiting for background agents(?:\s|·|$)"),
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Any(&[
            &["esc to cancel"],
            &["esc cancel"],
            &["esc again to cancel"],
            &["esc interrupt"],
        ]),
    },
];

const PI_RULES: &[Rule] = &[Rule {
    priority: 100,
    state: AgentState::Working,
    region: Region::WholeRecent,
    matcher: Matcher::Contains(&["working..."]),
}];

const QODER_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::QoderPermission,
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Contains(&["(esc to cancel,"]),
    },
    Rule {
        priority: 90,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::LineRegex(r"^\s*[\u2800-\u28FF]\s+.*\p{Alphabetic}"),
    },
];

pub(crate) fn detect_codex(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, CODEX_RULES)
}

pub(crate) fn detect_opencode(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, OPENCODE_RULES)
}

pub(crate) fn detect_gemini(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, GEMINI_RULES)
}

pub(crate) fn detect_cline(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, CLINE_RULES)
}

pub(crate) fn detect_copilot(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, COPILOT_RULES)
}

pub(crate) fn detect_pi(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, PI_RULES)
}

pub(crate) fn detect_qoder(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, QODER_RULES)
}

fn detect_rules(input: DetectionInput<'_>, rules: &[Rule]) -> Option<AgentState> {
    let mut matched = None;
    for rule in rules {
        let region = region(input, rule.region);
        if matcher_matches(rule.matcher, &region)
            && matches!(rule.region, Region::BottomNonEmpty(3))
            && region.contains("■ conversation interrupted")
        {
            continue;
        }
        if matcher_matches(rule.matcher, &region)
            && matched.is_none_or(|previous: &Rule| rule.priority > previous.priority)
        {
            matched = Some(rule);
        }
    }
    matched.map(|rule| rule.state)
}

fn region(input: DetectionInput<'_>, region: Region) -> String {
    let text = match region {
        Region::OscTitle => input.osc_title.to_owned(),
        Region::TopNonEmpty(limit) => top_nonempty_lines(input.screen, limit),
        Region::BottomNonEmpty(limit) => recent_nonempty_lines(input.screen, limit),
        Region::AfterLastPrompt => agents::codex_after_last_prompt_marker(input.screen),
        Region::WholeRecent => recent_nonempty_lines(input.screen, 20),
        Region::WholeRecentWithoutCurrentPrompt => {
            if agents::codex_has_current_prompt_marker(input.screen) {
                String::new()
            } else {
                recent_nonempty_lines(input.screen, 20)
            }
        }
    };
    text.to_ascii_lowercase()
}

fn matcher_matches(matcher: Matcher, text: &str) -> bool {
    match matcher {
        Matcher::Contains(values) => values.iter().all(|value| text.contains(value)),
        Matcher::All(values) => values.iter().all(|value| text.contains(value)),
        Matcher::Any(groups) => groups
            .iter()
            .any(|group| group.iter().all(|value| text.contains(value))),
        Matcher::Regex(pattern) => Regex::new(pattern).is_ok_and(|regex| regex.is_match(text)),
        Matcher::LineRegex(pattern) => {
            Regex::new(pattern).is_ok_and(|regex| text.lines().any(|line| regex.is_match(line)))
        }
        Matcher::TrustDirectory => {
            text.lines()
                .next()
                .is_some_and(|line| line.starts_with("> you are in "))
                && text.contains("do you trust the contents of this directory?")
        }
        Matcher::OpenCodePermission => {
            text.contains("△ permission required")
                || (text.contains("esc dismiss")
                    && (text.contains("enter confirm")
                        || text.contains("enter submit")
                        || text.contains("enter toggle"))
                    && (text.contains("↑↓ select") || text.contains("⇆ tab")))
        }
        Matcher::CopilotSelection => {
            (text.contains("esc to cancel") || text.contains("esc cancel"))
                && (text.contains("enter to select")
                    || text.contains("enter to confirm")
                    || text.contains("enter to submit")
                    || text.contains("enter accept"))
        }
        Matcher::QoderPermission => {
            text.contains("permission required")
                || text.contains("allow once or always?")
                || text.contains("asking user")
                || text.contains("enter your response")
                || text.contains("review your answers:")
                || text.contains("shell awaiting input")
                || (text.contains("waiting for user confirmation")
                    && ["yes", "no", "allow", "reject"]
                        .iter()
                        .any(|signal| text.contains(signal)))
                || (text.contains("awaiting approval")
                    && ["allow", "reject"]
                        .iter()
                        .any(|signal| text.contains(signal)))
        }
    }
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

fn top_nonempty_lines(screen: &str, limit: usize) -> String {
    screen
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(limit)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        detect_cline, detect_codex, detect_copilot, detect_gemini, detect_opencode, detect_pi,
        detect_qoder, DetectionInput,
    };
    use crate::detect::AgentState;

    fn detect(screen: &str, title: &str) -> Option<AgentState> {
        detect_codex(DetectionInput {
            screen,
            osc_title: title,
            _osc_progress: "",
        })
    }

    fn detect_opencode_state(screen: &str) -> Option<AgentState> {
        detect_opencode(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_gemini_state(screen: &str) -> Option<AgentState> {
        detect_gemini(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_cline_state(screen: &str) -> Option<AgentState> {
        detect_cline(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_copilot_state(screen: &str) -> Option<AgentState> {
        detect_copilot(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_pi_state(screen: &str) -> Option<AgentState> {
        detect_pi(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_qoder_state(screen: &str) -> Option<AgentState> {
        detect_qoder(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    #[test]
    fn codex_manifest_prioritizes_osc_title_blocker() {
        assert_eq!(detect("", "Action Required"), Some(AgentState::Blocked));
    }

    #[test]
    fn codex_manifest_ignores_stale_weak_blockers_after_a_prompt() {
        assert_eq!(detect("Run this command? [y/n]\n› ready", ""), None);
    }

    #[test]
    fn codex_manifest_matches_startup_update_in_bottom_region() {
        assert_eq!(
            detect(
                "Update available!\nUpdate now\nSkip until next version\nPress enter to continue",
                ""
            ),
            Some(AgentState::Blocked)
        );
    }

    #[test]
    fn opencode_manifest_requires_permission_controls() {
        assert_eq!(
            detect_opencode_state("△ Permission required"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_opencode_state("Esc dismiss · Enter confirm · ↑↓ select"),
            Some(AgentState::Blocked)
        );
        assert_eq!(detect_opencode_state("Esc dismiss · Enter confirm"), None);
    }

    #[test]
    fn opencode_manifest_matches_interrupt_and_progress_rules() {
        assert_eq!(
            detect_opencode_state("Press esc to interrupt"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_opencode_state("build ■■■■"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_opencode_state("build ■■■"), None);
    }

    #[test]
    fn gemini_manifest_matches_confirmation_and_cancel_rules() {
        assert_eq!(
            detect_gemini_state("│ Apply this change"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_gemini_state("❯ Allow this command"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_gemini_state("Thinking · Esc to cancel"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_gemini_state("ordinary output"), None);
    }

    #[test]
    fn cline_manifest_matches_tool_permissions_and_visible_output() {
        assert_eq!(
            detect_cline_state("[Act Mode] Execute command? Yes"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_cline_state("[Plan Mode] Use this tool? Yes"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_cline_state("ordinary visible Cline output"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_cline_state(""), None);
    }

    #[test]
    fn copilot_manifest_matches_selection_and_background_rules() {
        assert_eq!(
            detect_copilot_state("Esc to cancel · Enter to select"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_copilot_state("◎ Waiting for background agents"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_copilot_state("Esc again to cancel"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_copilot_state("Enter to select"), None);
    }

    #[test]
    fn pi_manifest_requires_the_complete_working_literal() {
        assert_eq!(detect_pi_state("Working..."), Some(AgentState::Working));
        assert_eq!(detect_pi_state("working"), None);
    }

    #[test]
    fn qoder_manifest_matches_permission_cancel_and_spinner_rules() {
        assert_eq!(
            detect_qoder_state("Waiting for user confirmation · Allow"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_qoder_state("(Esc to cancel, press q to quit)"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_qoder_state("⠋ Thinking"), Some(AgentState::Working));
        assert_eq!(detect_qoder_state("⠋ 123"), None);
    }
}
