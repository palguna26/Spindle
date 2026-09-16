//! Small manifest evaluator shared by screen-based agent detectors.
//!
//! The regions and rule priority follow Herdr's `src/detect/manifest.rs`.
//! Codex, OpenCode, Gemini, Cline, Copilot, Pi, Qoder CLI, and Droid are migrated first; other agents still use their
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
    OscProgress,
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
    DroidPermission,
    DroidSpinner,
    DevinTrust,
    DevinPermission,
    DevinRunningTools,
    DevinGuide,
    DevinReading,
    DevinWelcomeIdle,
    DevinLiveIdle,
    CursorWriteFile,
    CursorApproval,
    CursorSpinner,
    AmpApproval,
    AmpTitleSpinner,
    AmpStatusFooter,
    AmpTitleIdle,
    AntigravityPermission,
    #[allow(dead_code)]
    HermesDangerousApproval,
    HermesDangerousApprovalExact,
    HermesClarification,
    HermesCredential,
    HermesConfirmation,
    KiroToolPermission,
    KiroSubagentPermission,
    KiroSpinner,
    KiroIdle,
    KimiCurrentApproval,
    KimiQuestion,
    KimiLegacyApproval,
    MakiPermission,
    MakiPlanComplete,
    MakiStatusSpinner,
    MakiStatusIdle,
    MakiPromptIdle,
    MuseTrust,
    MusePickRequest,
    MuseMenuOverlay,
    MuseWorking,
    MuseApproval,
    MuseIdlePrompt,
    MuseIdleFallback,
    GrokOption,
    GrokLegacyPermission,
    GrokBackgroundChip,
    GrokTitleIdle,
    GrokSpinnerStatus,
    GrokLegacyWorking,
    GrokPromptIdle,
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

const DROID_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::DroidPermission,
    },
    Rule {
        priority: 290,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::DroidPermission,
    },
    Rule {
        priority: 110,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::DroidSpinner,
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Contains(&["esc to stop"]),
    },
];

const DEVIN_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::DevinTrust,
    },
    Rule {
        priority: 290,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::DevinPermission,
    },
    Rule {
        priority: 200,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::DevinRunningTools,
    },
    Rule {
        priority: 190,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(6),
        matcher: Matcher::DevinGuide,
    },
    Rule {
        priority: 180,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::DevinReading,
    },
    Rule {
        priority: 120,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::DevinWelcomeIdle,
    },
    Rule {
        priority: 100,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(6),
        matcher: Matcher::DevinLiveIdle,
    },
];

const CURSOR_RULES: &[Rule] = &[
    Rule {
        priority: 320,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::CursorWriteFile,
    },
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::CursorApproval,
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(6),
        matcher: Matcher::Contains(&["ctrl+c to stop"]),
    },
    Rule {
        priority: 95,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(5),
        matcher: Matcher::LineRegex(r"(?i)\b[1-9][0-9]*\s+background\s+tasks?\b"),
    },
    Rule {
        priority: 90,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::CursorSpinner,
    },
];

const AMP_RULES: &[Rule] = &[
    Rule {
        priority: 1100,
        state: AgentState::Blocked,
        region: Region::OscTitle,
        matcher: Matcher::Contains(&["plugin confirmation needed"]),
    },
    Rule {
        priority: 1050,
        state: AgentState::Working,
        region: Region::OscTitle,
        matcher: Matcher::AmpTitleSpinner,
    },
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::AmpApproval,
    },
    Rule {
        priority: 200,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(5),
        matcher: Matcher::AmpStatusFooter,
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Contains(&["esc to cancel"]),
    },
    Rule {
        priority: 50,
        state: AgentState::Idle,
        region: Region::OscTitle,
        matcher: Matcher::AmpTitleIdle,
    },
];

const ANTIGRAVITY_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::AntigravityPermission,
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::LineRegex(r"^\s*[\u2800-\u28FF]+\s+\p{Alphabetic}+\w*ing\b"),
    },
    Rule {
        priority: 90,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(5),
        matcher: Matcher::LineRegex(r"(?i)·\s*[1-9][0-9]*\s+task"),
    },
];

const KILO_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::OpenCodePermission,
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Contains(&["esc interrupt"]),
    },
];

const HERMES_RULES: &[Rule] = &[
    Rule {
        priority: 1100,
        state: AgentState::Blocked,
        region: Region::OscTitle,
        matcher: Matcher::Regex(r"^⚠[\u{fe0e}\u{fe0f}]?(?:\s|$)"),
    },
    Rule {
        priority: 1050,
        state: AgentState::Working,
        region: Region::OscTitle,
        matcher: Matcher::Regex(r"^⏳[\u{fe0e}\u{fe0f}]?(?:\s|$)"),
    },
    Rule {
        priority: 900,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(14),
        matcher: Matcher::HermesDangerousApprovalExact,
    },
    Rule {
        priority: 900,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(14),
        matcher: Matcher::HermesClarification,
    },
    Rule {
        priority: 900,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(14),
        matcher: Matcher::HermesCredential,
    },
    Rule {
        priority: 900,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(14),
        matcher: Matcher::HermesConfirmation,
    },
    Rule {
        priority: 950,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(5),
        matcher: Matcher::Any(&[&["msg=interrupt"], &["ctrl+c to interrupt"]]),
    },
    Rule {
        priority: 500,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(5),
        matcher: Matcher::Contains(&["ctrl+c cancel"]),
    },
    Rule {
        priority: 100,
        state: AgentState::Idle,
        region: Region::OscTitle,
        matcher: Matcher::Regex(r"^✓[\u{fe0e}\u{fe0f}]?(?:\s|$)"),
    },
];

const KIRO_RULES: &[Rule] = &[
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::KiroToolPermission,
    },
    Rule {
        priority: 290,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::KiroSubagentPermission,
    },
    Rule {
        priority: 200,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(5),
        matcher: Matcher::KiroIdle,
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::Contains(&["kiro is working"]),
    },
    Rule {
        priority: 90,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::KiroSpinner,
    },
];

const KIMI_RULES: &[Rule] = &[
    Rule {
        priority: 400,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::KimiCurrentApproval,
    },
    Rule {
        priority: 390,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::KimiQuestion,
    },
    Rule {
        priority: 300,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::KimiLegacyApproval,
    },
    Rule {
        priority: 120,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(3),
        matcher: Matcher::LineRegex(
            r"(?i)\bkimi[-\w.]*\s+thinking\b.*\[[1-9][0-9]*\s+agents?\s+running\]",
        ),
    },
    Rule {
        priority: 100,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::LineRegex(r"^\s*(🌕|🌖|🌗|🌘|🌑|🌒|🌓|🌔)\s*$"),
    },
    Rule {
        priority: 90,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::LineRegex(
            r"(?i)^\s*[\u2800-\u28FF]+\s*(thinking\.\.\.|working\.\.\.|using )",
        ),
    },
];

const MAKI_RULES: &[Rule] = &[
    Rule {
        priority: 980,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::MakiPermission,
    },
    Rule {
        priority: 970,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::MakiPlanComplete,
    },
    Rule {
        priority: 900,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(1),
        matcher: Matcher::MakiStatusSpinner,
    },
    Rule {
        priority: 850,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(1),
        matcher: Matcher::MakiStatusIdle,
    },
    Rule {
        priority: 840,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(3),
        matcher: Matcher::MakiPromptIdle,
    },
];

const MUSE_RULES: &[Rule] = &[
    Rule {
        priority: 970,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(12),
        matcher: Matcher::MuseTrust,
    },
    Rule {
        priority: 950,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::MusePickRequest,
    },
    Rule {
        priority: 940,
        state: AgentState::Unknown,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::MuseMenuOverlay,
    },
    Rule {
        priority: 900,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::MuseWorking,
    },
    Rule {
        priority: 850,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(8),
        matcher: Matcher::MuseApproval,
    },
    Rule {
        priority: 700,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(5),
        matcher: Matcher::MuseIdlePrompt,
    },
    Rule {
        priority: 500,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(3),
        matcher: Matcher::MuseIdleFallback,
    },
];

const GROK_RULES: &[Rule] = &[
    Rule {
        priority: 1300,
        state: AgentState::Blocked,
        region: Region::OscTitle,
        matcher: Matcher::Contains(&["action required"]),
    },
    Rule {
        priority: 1200,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::GrokOption,
    },
    Rule {
        priority: 1190,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(2),
        matcher: Matcher::All(&[":select", "ctrl+o:yolo", "ctrl+c:cancel"]),
    },
    Rule {
        priority: 1185,
        state: AgentState::Blocked,
        region: Region::BottomNonEmpty(2),
        matcher: Matcher::All(&["tab:scrollback", "shift+x:dismiss"]),
    },
    Rule {
        priority: 1180,
        state: AgentState::Blocked,
        region: Region::WholeRecent,
        matcher: Matcher::GrokLegacyPermission,
    },
    Rule {
        priority: 1170,
        state: AgentState::Working,
        region: Region::TopNonEmpty(1),
        matcher: Matcher::GrokBackgroundChip,
    },
    Rule {
        priority: 1150,
        state: AgentState::Working,
        region: Region::OscProgress,
        matcher: Matcher::Contains(&["4;1;-1"]),
    },
    Rule {
        priority: 1100,
        state: AgentState::Idle,
        region: Region::OscTitle,
        matcher: Matcher::GrokTitleIdle,
    },
    Rule {
        priority: 1000,
        state: AgentState::Working,
        region: Region::OscTitle,
        matcher: Matcher::Regex(r"\S"),
    },
    Rule {
        priority: 950,
        state: AgentState::Idle,
        region: Region::OscProgress,
        matcher: Matcher::Contains(&["4;0;0"]),
    },
    Rule {
        priority: 200,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::GrokSpinnerStatus,
    },
    Rule {
        priority: 190,
        state: AgentState::Working,
        region: Region::BottomNonEmpty(2),
        matcher: Matcher::All(&["esc:cancel", "ctrl+.:shortcuts"]),
    },
    Rule {
        priority: 120,
        state: AgentState::Working,
        region: Region::WholeRecent,
        matcher: Matcher::GrokLegacyWorking,
    },
    Rule {
        priority: 100,
        state: AgentState::Idle,
        region: Region::BottomNonEmpty(2),
        matcher: Matcher::GrokPromptIdle,
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

pub(crate) fn detect_droid(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, DROID_RULES)
}

pub(crate) fn detect_devin(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, DEVIN_RULES)
}

pub(crate) fn detect_cursor(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, CURSOR_RULES)
}

pub(crate) fn detect_amp(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, AMP_RULES)
}

pub(crate) fn detect_antigravity(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, ANTIGRAVITY_RULES)
}

pub(crate) fn detect_kilo(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, KILO_RULES)
}

pub(crate) fn detect_hermes(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, HERMES_RULES)
}

pub(crate) fn detect_kiro(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, KIRO_RULES)
}

pub(crate) fn detect_kimi(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, KIMI_RULES)
}

pub(crate) fn detect_maki(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, MAKI_RULES)
}

pub(crate) fn detect_muse(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, MUSE_RULES)
}

pub(crate) fn detect_grok(input: DetectionInput<'_>) -> Option<AgentState> {
    detect_rules(input, GROK_RULES)
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
        Region::OscProgress => input._osc_progress.to_owned(),
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
        Matcher::DroidPermission => {
            (text.contains("enter to select")
                && text.contains("esc to cancel")
                && ["â†‘â†“ to navigate", "use â†‘â†“ to navigate"]
                    .iter()
                    .any(|signal| text.contains(signal))
                && ["> yes, allow", "> no, cancel"]
                    .iter()
                    .any(|signal| text.contains(signal)))
                || (text.contains("enter select")
                    && text.contains("esc cancel")
                    && ["â†‘/â†“ navigate", "â†‘â†“ navigate"]
                        .iter()
                        .any(|signal| text.contains(signal)))
        }
        Matcher::DroidSpinner => {
            text.contains("esc to stop")
                && text.lines().any(|line| {
                    line.trim_start()
                        .chars()
                        .next()
                        .is_some_and(|ch| ('\u{2800}'..='\u{28ff}').contains(&ch))
                })
        }
        Matcher::DevinTrust => {
            text.contains("do you trust the authors of this directory?")
                && text.contains("with untrusted content.")
                && text.contains("yes, trust ")
        }
        Matcher::DevinPermission => {
            text.contains("approve once")
                && text.contains("select")
                && text.contains("confirm")
                && text.contains("esc cancel")
        }
        Matcher::DevinRunningTools => {
            text.contains("running tools")
                && text.contains("esc to interrupt")
                && !devin_blocked(text)
        }
        Matcher::DevinGuide => text.contains("guide devin while it works") && !devin_blocked(text),
        Matcher::DevinReading => {
            text.contains("reading shell ") && text.contains("timeout:") && !devin_blocked(text)
        }
        Matcher::DevinWelcomeIdle => {
            text.contains("ask devin to build")
                && text.contains("features, fix bugs")
                && text.contains("your code")
                && text.lines().any(|line| {
                    line.trim_start().starts_with('❭') && line.contains("ask devin to build")
                })
                && !devin_blocked_or_working(text)
        }
        Matcher::DevinLiveIdle => {
            text.contains("context:")
                && text.lines().any(|line| line.trim_start().starts_with('❭'))
                && !devin_blocked_or_working(text)
        }
        Matcher::CursorWriteFile => {
            text.contains("write to this file?")
                && text.contains("proceed (y)")
                && ["reject & propose changes", "esc or n or p", "add write("]
                    .iter()
                    .any(|signal| text.contains(signal))
        }
        Matcher::CursorApproval => {
            (text.contains("waiting for approval")
                && text.contains("run this command?")
                && ["run (once) (y)", "skip (esc or n)"]
                    .iter()
                    .any(|signal| text.contains(signal)))
                || text.contains("(y) (enter)")
                || text.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with("allow ") && line.contains("(y)")
                })
                || text.contains("keep (n)")
                || text.contains("skip (esc or n)")
                || text.lines().any(|line| {
                    let line = line
                        .trim_start()
                        .strip_prefix('→')
                        .unwrap_or(line)
                        .trim_start();
                    line.starts_with("run ") && line.contains("(y)")
                })
        }
        Matcher::CursorSpinner => text.lines().any(cursor_spinner_line),
        Matcher::AmpApproval => {
            text.contains("waiting for approval")
                || text.contains("invoke tool")
                || text.contains("run this command?")
                || text.contains("allow editing file:")
                || text.contains("allow creating file:")
                || text.contains("confirm tool call")
                || (text.contains("approve")
                    && [
                        "allow all for this session",
                        "allow all for every session",
                        "allow file for every session",
                        "deny with feedback",
                    ]
                    .iter()
                    .any(|signal| text.contains(signal)))
        }
        Matcher::AmpTitleSpinner => {
            Regex::new(r"^[\u{2800}-\u{28ff}] ").is_ok_and(|regex| regex.is_match(text))
        }
        Matcher::AmpStatusFooter => {
            Regex::new(r"(?i)^\s*╰\s+\S+\s+(thinking|streaming|running tools|waiting)\s+─")
                .is_ok_and(|regex| text.lines().any(|line| regex.is_match(line)))
        }
        Matcher::AmpTitleIdle => {
            text.contains(" - amp - ")
                && !Regex::new(r"^[\u{2800}-\u{28ff}] ").is_ok_and(|regex| regex.is_match(text))
                && !text.contains("plugin confirmation needed")
        }
        Matcher::AntigravityPermission => {
            text.contains("requesting permission for:")
                && (text.contains("do you want to proceed?")
                    || (text.contains("tab amend") && text.contains("edit command")))
        }
        Matcher::HermesDangerousApproval => {
            (text.contains("dangerous")
                || text.contains("approval")
                || (text.contains("allow once") && text.contains("deny"))
                || text.lines().any(|line| {
                    let line = line
                        .trim_start()
                        .trim_start_matches(['▸', '>'])
                        .trim_start();
                    line.starts_with("1. allow")
                }))
                && [
                    "enter confirm",
                    "enter to confirm",
                    "↑/↓ to select",
                    "show full command",
                ]
                .iter()
                .any(|signal| text.contains(signal))
                || (text.contains("↵ confirm")
                    && text.contains(" choose")
                    && ["approve", "reject", "revise"]
                        .iter()
                        .any(|signal| text.contains(signal))
                    && text.lines().any(|line| {
                        let line = line.trim_start().trim_start_matches('▶').trim_start();
                        line.strip_prefix("approve ")
                            .is_some_and(|question| question.trim_end().ends_with('?'))
                    }))
        }
        Matcher::HermesDangerousApprovalExact => {
            (text.contains("dangerous")
                || text.contains("approval")
                || (text.contains("allow once") && text.contains("deny"))
                || text.lines().any(|line| {
                    let line = line
                        .trim_start()
                        .trim_start_matches(['▸', '>'])
                        .trim_start();
                    line.starts_with("1. allow")
                }))
                && [
                    "enter confirm",
                    "enter to confirm",
                    "↑/↓ to select",
                    "show full command",
                ]
                .iter()
                .any(|signal| text.contains(signal))
        }
        Matcher::HermesClarification => {
            (text.contains("hermes needs your")
                || text.lines().any(|line| {
                    let line = line
                        .trim_start()
                        .trim_start_matches(['▸', '>'])
                        .trim_start();
                    line.strip_prefix("ask ")
                        .is_some_and(|rest| !rest.trim().is_empty())
                })
                || text.contains("type your answer"))
                && [
                    "enter confirm",
                    "enter to confirm",
                    "enter send",
                    "press enter",
                    "↑/↓ select",
                    "↑/↓ to select",
                    "other (type",
                ]
                .iter()
                .any(|signal| text.contains(signal))
        }
        Matcher::HermesCredential => {
            text.contains("sudo password")
                || text.contains("skill setup")
                || (text.contains("🔑") && text.contains("for "))
        }
        Matcher::HermesConfirmation => {
            ((text.contains("approve once") && text.contains("cancel"))
                || (text.contains("start a new session") && text.contains("keep going")))
                && [
                    "enter to confirm",
                    "enter confirm",
                    "type 1/2/3",
                    "y/n quick",
                ]
                .iter()
                .any(|signal| text.contains(signal))
        }
        Matcher::KiroToolPermission => {
            text.contains("requires approval")
                && [
                    "yes, single permission",
                    "trust, always allow",
                    "no (tab to edit)",
                    "esc to close",
                ]
                .iter()
                .any(|signal| text.contains(signal))
        }
        Matcher::KiroSubagentPermission => {
            text.contains("pending from subagents")
                && ["tool approval", "tool approvals"]
                    .iter()
                    .any(|signal| text.contains(signal))
                && [
                    "approve all pending",
                    "configure individually",
                    "exit (cancel subagents)",
                ]
                .iter()
                .any(|signal| text.contains(signal))
        }
        Matcher::KiroIdle => {
            text.contains("ask a question or describe a task")
                && text.contains("/copy to clipboard")
                && !text.contains("kiro is working")
                && !text.contains("esc to cancel")
        }
        Matcher::KiroSpinner => {
            text.contains("esc to cancel")
                && text.lines().any(|line| {
                    let line = line.trim_start();
                    let Some(spinner) = line.chars().next() else {
                        return false;
                    };
                    matches!(spinner, '◔' | '◑' | '◕' | '●')
                        && line[spinner.len_utf8()..]
                            .trim_start()
                            .chars()
                            .next()
                            .is_some_and(char::is_alphabetic)
                })
        }
        Matcher::KimiCurrentApproval => {
            text.contains("↵ confirm")
                && [
                    "run this command?",
                    "write this file?",
                    "apply these edits?",
                    "stop this task?",
                    "ready to build with this plan?",
                ]
                .iter()
                .any(|signal| text.contains(signal))
                && text.contains(" choose")
                && ["approve", "reject", "revise"]
                    .iter()
                    .any(|signal| text.contains(signal))
        }
        Matcher::KimiQuestion => {
            text.contains("↑↓ select")
                && text.contains("esc cancel")
                && text.lines().any(|line| {
                    let line = line.trim_start();
                    line == "question" || line.starts_with("? ")
                })
                && ["↵ choose", "↵ toggle", "↵ save"]
                    .iter()
                    .any(|signal| text.contains(signal))
        }
        Matcher::KimiLegacyApproval => {
            text.contains("requesting approval")
                && text.contains("reject")
                && ["approve once", "approve for this session"]
                    .iter()
                    .any(|signal| text.contains(signal))
                && ["1/2/3/4 choose", "↵ confirm"]
                    .iter()
                    .any(|signal| text.contains(signal))
        }
        Matcher::MakiPermission => {
            text.contains("permission required")
                && (text.contains("y allow") && text.contains("n deny")
                    || text.contains("confirm allow")
                    || text.contains("confirm deny")
                    || (text.contains("enter deny") && text.contains("esc cancel")))
        }
        Matcher::MakiPlanComplete => {
            text.contains("plan complete")
                && text.contains("enter confirm")
                && (text.contains("space toggle parallel") || text.contains("edit plan"))
        }
        Matcher::MakiStatusSpinner => {
            Regex::new(
                r"(?i)^( (?:[\u{2800}-\u{28ff}]|\u{00e2}\u{00a0}[\u{2039}\u{2122}])){1,2} \[(BUILD|PLAN|BASH)\]",
            )
                .is_ok_and(|regex| regex.is_match(text))
        }
        Matcher::MakiStatusIdle => {
            Regex::new(r"(?i)^ \[(BUILD|PLAN|BASH)\]")
                .is_ok_and(|regex| regex.is_match(text))
        }
        Matcher::MakiPromptIdle => {
            text.lines().any(|line| {
                line.starts_with("❯ ") || line.starts_with("\u{00e2}\u{009d}\u{00af} ")
            })
                && !text.contains("queue another prompt")
                && !Regex::new(
                    r"^( (?:[\u{2800}-\u{28ff}]|\u{00e2}\u{00a0}[\u{2039}\u{2122}])){1,2} ",
                )
                    .is_ok_and(|regex| text.lines().any(|line| regex.is_match(line)))
        }
        Matcher::MuseTrust => {
            text.contains("do you trust this workspace?")
                && (text.contains("trust and continue") || text.contains("use up/down"))
        }
        Matcher::MusePickRequest => {
            (text.contains("enter to select") && text.contains("tab for an optional note"))
                || (text.contains("enter to toggle") && text.contains("esc to interrupt"))
        }
        Matcher::MuseMenuOverlay => {
            (text.contains("enter confirm") && text.contains("esc go back"))
                || (text.contains("enter save") && text.contains("esc go back"))
                || (text.contains("space toggle")
                    && text.contains("esc close")
                    && text.contains("type filter"))
        }
        Matcher::MuseWorking => {
            text.contains("esc to interrupt") && !muse_pick_or_menu(text)
        }
        Matcher::MuseApproval => {
            (text.contains("allow this stage once")
                && text.contains("always allow in this workspace"))
                || (text.contains("allow once") && text.contains("allow for this session"))
                || (text.contains("yes, proceed")
                    && text.contains("yes, don't ask again this session"))
        }
        Matcher::MuseIdlePrompt => {
            muse_prompt_line(text)
                && !text.contains("esc to interrupt")
                && !muse_pick_or_menu(text)
        }
        Matcher::MuseIdleFallback => {
            text.lines().any(|line| {
                Regex::new(
                    r"(?i)^\s*\S+ (?:·|\u{00c2}\u{00b7}) (none|minimal|low|medium|high|xhigh|ultra) (?:·|\u{00c2}\u{00b7}) ",
                )
                .is_ok_and(|regex| regex.is_match(line))
            }) && !text.contains("esc to interrupt")
        }
        Matcher::GrokOption => text.lines().any(|line| {
            let line = line.trim_start();
            let mut fields = line.split_whitespace();
            let Some(gutter) = fields.next() else { return false };
            let Some(key) = fields.next() else { return false };
            let Some(choice) = fields.next() else { return false };
            !gutter.is_ascii() && key.chars().all(|c| c.is_ascii_alphanumeric())
                && matches!(choice, "(●)" | "(○)" | "(â—)" | "(â—‹)")
        }),
        Matcher::GrokLegacyPermission => {
            text.contains("yes, proceed")
                && text.contains("no, reject")
                && (text.contains("scope") && (text.contains("choose permission") || text.contains("←/→")))
        }
        Matcher::GrokBackgroundChip => {
            let line = text.trim_start();
            let mut fields = line.split_whitespace();
            let Some(marker) = fields.next() else { return false };
            let Some(count) = fields.next() else { return false };
            !marker.is_ascii() && count.parse::<u32>().is_ok_and(|count| count > 0)
                && text.contains('│')
        }
        Matcher::GrokTitleIdle => {
            (text == "grok" || text.ends_with(" - grok"))
                && !text.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
        }
        Matcher::GrokSpinnerStatus => {
            text.lines().any(|line| {
                let line = line.trim_start();
                line.contains("[stop]")
                    && line.chars().next().is_some_and(|c| ('\u{2801}'..='\u{28ff}').contains(&c))
            })
        }
        Matcher::GrokLegacyWorking => {
            text.contains("ctrl+c:cancel")
                && text.contains("ctrl+enter:interject")
                && (text.contains("waiting") || text.lines().any(|line| {
                    let line = line.trim_start();
                    line.chars().next().is_some_and(|c| ('\u{2801}'..='\u{28ff}').contains(&c))
                        && ["run", "read", "search", "list"].iter().any(|verb| line.contains(verb))
                }))
        }
        Matcher::GrokPromptIdle => {
            text.contains("ctrl+.:shortcuts")
                && !text.contains("esc:cancel")
                && !text.contains("ctrl+c:cancel")
        }
    }
}

fn muse_pick_or_menu(text: &str) -> bool {
    (text.contains("enter to select") && text.contains("tab for an optional note"))
        || (text.contains("enter to toggle") && text.contains("esc to interrupt"))
        || (text.contains("enter confirm") && text.contains("esc go back"))
        || (text.contains("enter save") && text.contains("esc go back"))
        || (text.contains("space toggle")
            && text.contains("esc close")
            && text.contains("type filter"))
}

fn muse_prompt_line(text: &str) -> bool {
    text.lines()
        .any(|line| line.starts_with("⟩ ") || line.starts_with("âŸ© "))
}

fn cursor_spinner_line(line: &str) -> bool {
    let line = line.trim_start();
    let mut chars = line.chars();
    match chars.next() {
        Some('⬡' | '⬢') => {
            let activity = chars.as_str().trim_start();
            activity
                .chars()
                .take_while(|character| character.is_alphabetic())
                .collect::<String>()
                .to_ascii_lowercase()
                .ends_with("ing")
        }
        Some(first) if ('\u{2800}'..='\u{28ff}').contains(&first) => {
            let spinner = line
                .chars()
                .take_while(|character| ('\u{2800}'..='\u{28ff}').contains(character))
                .count();
            let activity = line
                .chars()
                .skip(spinner)
                .collect::<String>()
                .trim_start()
                .to_owned();
            activity
                .chars()
                .take_while(|character| character.is_alphabetic())
                .collect::<String>()
                .to_ascii_lowercase()
                .ends_with("ing")
        }
        _ => false,
    }
}

fn devin_blocked(text: &str) -> bool {
    text.contains("approve once") && text.contains("esc cancel")
}

fn devin_blocked_or_working(text: &str) -> bool {
    devin_blocked(text)
        || (text.contains("running tools") && text.contains("esc to interrupt"))
        || text.contains("guide devin while it works")
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
        detect_amp, detect_antigravity, detect_cline, detect_codex, detect_copilot, detect_cursor,
        detect_devin, detect_droid, detect_gemini, detect_grok, detect_hermes, detect_kilo,
        detect_kimi, detect_kiro, detect_maki, detect_muse, detect_opencode, detect_pi,
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

    fn detect_droid_state(screen: &str) -> Option<AgentState> {
        detect_droid(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_devin_state(screen: &str) -> Option<AgentState> {
        detect_devin(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_cursor_state(screen: &str) -> Option<AgentState> {
        detect_cursor(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_amp_state(screen: &str, title: &str) -> Option<AgentState> {
        detect_amp(DetectionInput {
            screen,
            osc_title: title,
            _osc_progress: "",
        })
    }

    fn detect_antigravity_state(screen: &str) -> Option<AgentState> {
        detect_antigravity(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_kilo_state(screen: &str) -> Option<AgentState> {
        detect_kilo(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_hermes_state(screen: &str, title: &str) -> Option<AgentState> {
        detect_hermes(DetectionInput {
            screen,
            osc_title: title,
            _osc_progress: "",
        })
    }

    fn detect_kiro_state(screen: &str) -> Option<AgentState> {
        detect_kiro(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_kimi_state(screen: &str) -> Option<AgentState> {
        detect_kimi(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_maki_state(screen: &str) -> Option<AgentState> {
        detect_maki(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_muse_state(screen: &str) -> Option<AgentState> {
        detect_muse(DetectionInput {
            screen,
            osc_title: "",
            _osc_progress: "",
        })
    }

    fn detect_grok_state(screen: &str, title: &str, progress: &str) -> Option<AgentState> {
        detect_grok(DetectionInput {
            screen,
            osc_title: title,
            _osc_progress: progress,
        })
    }

    #[test]
    fn grok_manifest_matches_herdr_priority_rules() {
        assert_eq!(
            detect_grok_state("", "Action Required", ""),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_grok_state("┃ 2 (○) Yes, proceed", "", ""),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_grok_state("1/3:select │ Ctrl+o:yolo │ Ctrl+c:cancel", "", ""),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_grok_state("⋅ 2 │ background tasks", "", ""),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_grok_state("", "", "4;1;-1"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_grok_state("⠧ Waiting on subagent [stop]", "", ""),
            Some(AgentState::Working)
        );
        assert_eq!(detect_grok_state("", "grok", ""), Some(AgentState::Idle));
        assert_eq!(
            detect_grok_state("Ctrl+.:shortcuts", "", ""),
            Some(AgentState::Idle)
        );
        assert_eq!(detect_grok_state("", "", "4;0;0"), Some(AgentState::Idle));
    }

    #[test]
    fn muse_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_muse_state("Do you trust this workspace?\nTrust and continue"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_muse_state("Enter to select\nTab for an optional note"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_muse_state("Enter confirm\nEsc go back"),
            Some(AgentState::Unknown)
        );
        assert_eq!(
            detect_muse_state("Searching\nEsc to interrupt"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_muse_state("Allow this stage once\nAlways allow in this workspace"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_muse_state("⟩ Explain this code"),
            Some(AgentState::Idle)
        );
        assert_eq!(detect_muse_state("ordinary output"), None);
    }

    #[test]
    fn maki_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_maki_state("Permission required\nY allow\nN deny"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_maki_state("Plan complete\nEnter confirm\nEdit plan"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_maki_state(" ⠋ [BUILD] status"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_maki_state(" ⠋ ⠙ [PLAN] status"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_maki_state(" [BASH] status"), Some(AgentState::Idle));
        assert_eq!(
            detect_maki_state("Panel\n❯ Type here"),
            Some(AgentState::Idle)
        );
        assert_eq!(detect_maki_state("Panel\n❯ Queue another prompt"), None);
        assert_eq!(detect_maki_state("ordinary output"), None);
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

    #[test]
    fn droid_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_droid_state(
                "Confirm execution\nEnter to select\nEsc to cancel\nâ†‘â†“ to navigate\n> Yes, allow"
            ),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_droid_state("Choose an option\nEnter select · Esc cancel · â†‘/â†“ navigate"),
            Some(AgentState::Blocked)
        );
        assert_eq!(detect_droid_state("Esc to stop"), Some(AgentState::Working));
        assert_eq!(detect_droid_state("Working"), None);
    }

    #[test]
    fn devin_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_devin_state(
                "Do you trust the authors of this directory?\nWith untrusted content.\n❭ Yes, trust this workspace"
            ),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_devin_state("Approve once · Select · Confirm · Esc cancel"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_devin_state("Running tools · Esc to interrupt"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_devin_state("Guide Devin while it works"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_devin_state("❭ Ask Devin to build\nFeatures, fix bugs\nYour code"),
            Some(AgentState::Idle)
        );
        assert_eq!(detect_devin_state("ordinary terminal output"), None);
    }

    #[test]
    fn cursor_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_cursor_state("Write to this file?\nProceed (Y)\nReject & propose changes"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_cursor_state("Waiting for approval\nRun this command?\nRun (once) (Y)"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_cursor_state("Ctrl+C to stop"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_cursor_state("1 background task"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_cursor_state("⬡ Thinking"), Some(AgentState::Working));
        assert_eq!(detect_cursor_state("ordinary Cursor output"), None);
    }

    #[test]
    fn amp_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_amp_state("Waiting for approval", ""),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_amp_state("Approve this action\nAllow all for this session", ""),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_amp_state("Esc to cancel", ""),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_amp_state("╰ main thinking ─", ""),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_amp_state("", "⠋ amp - project"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_amp_state("", "project - amp - workspace"),
            Some(AgentState::Idle)
        );
        assert_eq!(
            detect_amp_state("", "Plugin confirmation needed - amp - workspace"),
            Some(AgentState::Blocked)
        );
    }

    #[test]
    fn antigravity_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_antigravity_state("Requesting permission for: command\nDo you want to proceed?"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_antigravity_state("⠋ Thinking"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_antigravity_state("· 2 task"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_antigravity_state("· 0 task"), None);
    }

    #[test]
    fn kilo_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_kilo_state("△ Permission required"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kilo_state("Esc dismiss\nEnter confirm\n↑↓ select"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kilo_state("Esc dismiss\nEnter submit\n⇆ Tab"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kilo_state("Esc interrupt"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_kilo_state("ordinary output"), None);
    }

    #[test]
    fn hermes_manifest_matches_herdr_priority_rules() {
        assert_eq!(
            detect_hermes_state("Dangerous command\nEnter confirm", ""),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_hermes_state("Approval needed\nShow full command", ""),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_hermes_state("Ctrl+C to interrupt", ""),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_hermes_state("Ctrl+C cancel", ""),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_hermes_state("Approval\nEnter confirm", "⏳ Working"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_hermes_state("ordinary output", "⚠ Permission needed"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_hermes_state("ordinary output", "✓ Ready"),
            Some(AgentState::Idle)
        );
    }

    #[test]
    fn kiro_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_kiro_state("Tool requires approval\nYes, single permission"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kiro_state("Pending from subagents: tool approval\nExit (cancel subagents)"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kiro_state("Kiro is working"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_kiro_state("Esc to cancel\n◑ Searching"),
            Some(AgentState::Working)
        );
        assert_eq!(
            detect_kiro_state("Ask a question or describe a task\n/copy to clipboard"),
            Some(AgentState::Idle)
        );
        assert_eq!(detect_kiro_state("ordinary output"), None);
    }

    #[test]
    fn kimi_manifest_matches_herdr_rules() {
        assert_eq!(
            detect_kimi_state("Run this command?\n↵ confirm · choose\nApprove · Reject · Revise"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kimi_state("Question\n? Which option?\n↑↓ select · esc cancel\n↵ choose"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kimi_state("Requesting approval\nApprove once · Reject\n1/2/3/4 choose"),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            detect_kimi_state("kimi-pro thinking [2 agents running]"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_kimi_state("🌔"), Some(AgentState::Working));
        assert_eq!(
            detect_kimi_state("⠋ using tools"),
            Some(AgentState::Working)
        );
        assert_eq!(detect_kimi_state("⠋ searching"), None);
    }
}
