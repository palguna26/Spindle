//! Small manifest evaluator shared by screen-based agent detectors.
//!
//! The regions and rule priority follow Herdr's `src/detect/manifest.rs`.
//! Codex is the first migrated manifest; other agents still use their
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

pub(crate) fn detect_codex(input: DetectionInput<'_>) -> Option<AgentState> {
    let mut matched = None;
    for rule in CODEX_RULES {
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
    use super::{detect_codex, DetectionInput};
    use crate::detect::AgentState;

    fn detect(screen: &str, title: &str) -> Option<AgentState> {
        detect_codex(DetectionInput {
            screen,
            osc_title: title,
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
}
