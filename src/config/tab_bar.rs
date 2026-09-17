use serde::Deserialize;

pub(crate) const MAX_TAB_BAR_RIGHT_ENTRIES: usize = 16;
pub(crate) const MAX_TAB_BAR_COMMAND_INTERVAL_SECONDS: u64 = 31_536_000;
pub(crate) const MAX_TAB_BAR_COMMAND_TIMEOUT_SECONDS: u64 = 3_600;

fn default_datetime_format() -> String {
    "%H:%M".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TabBarRightEntryConfig {
    Zoom,
    Hostname,
    Datetime {
        #[serde(default = "default_datetime_format")]
        format: String,
    },
    Text {
        text: String,
    },
    Command {
        command: String,
        #[serde(default = "default_command_interval_seconds")]
        interval_seconds: u64,
        #[serde(default = "default_command_timeout_seconds")]
        timeout_seconds: u64,
    },
}

const fn default_command_interval_seconds() -> u64 {
    5
}

const fn default_command_timeout_seconds() -> u64 {
    2
}

pub(crate) fn tab_bar_right_diagnostics(entries: &[TabBarRightEntryConfig]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if entries.len() > MAX_TAB_BAR_RIGHT_ENTRIES {
        diagnostics.push(format!(
            "ui.tab_bar_right may contain at most {MAX_TAB_BAR_RIGHT_ENTRIES} entries; ignoring extras"
        ));
    }
    for (index, entry) in entries.iter().enumerate().take(MAX_TAB_BAR_RIGHT_ENTRIES) {
        match entry {
            TabBarRightEntryConfig::Datetime { format } if format.is_empty() => diagnostics.push(
                format!("ui.tab_bar_right[{index}] datetime format is empty; hiding entry"),
            ),
            TabBarRightEntryConfig::Datetime { format } => {
                if time::format_description::parse_strftime_owned(format).is_err() {
                    diagnostics.push(format!(
                        "ui.tab_bar_right[{index}] has an invalid datetime format; hiding entry"
                    ));
                }
            }
            TabBarRightEntryConfig::Command {
                command,
                interval_seconds,
                timeout_seconds,
            } => {
                if command.trim().is_empty() {
                    diagnostics.push(format!(
                        "ui.tab_bar_right[{index}] command is empty; hiding entry"
                    ));
                }
                if *interval_seconds == 0
                    || *interval_seconds > MAX_TAB_BAR_COMMAND_INTERVAL_SECONDS
                {
                    diagnostics.push(format!(
                        "ui.tab_bar_right[{index}] interval_seconds is outside the supported range; hiding entry"
                    ));
                }
                if *timeout_seconds == 0 || *timeout_seconds > MAX_TAB_BAR_COMMAND_TIMEOUT_SECONDS {
                    diagnostics.push(format!(
                        "ui.tab_bar_right[{index}] timeout_seconds is outside the supported range; hiding entry"
                    ));
                }
            }
            TabBarRightEntryConfig::Zoom
            | TabBarRightEntryConfig::Hostname
            | TabBarRightEntryConfig::Text { .. } => {}
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::{tab_bar_right_diagnostics, TabBarRightEntryConfig};
    use serde::Deserialize;

    #[test]
    fn tab_bar_right_entries_match_herdr_config_shape() {
        #[derive(Deserialize)]
        struct Wrapper {
            entries: Vec<TabBarRightEntryConfig>,
        }

        let parsed: Wrapper = toml::from_str(
            r#"
entries = [
  { type = "zoom" },
  { type = "hostname" },
  { type = "datetime" },
  { type = "text", text = "prod" },
  { type = "command", command = "status.ps1" },
]
"#,
        )
        .expect("parse tab bar status entries");
        let entries = parsed.entries;

        assert_eq!(entries.len(), 5);
        assert!(matches!(
            entries[2],
            TabBarRightEntryConfig::Datetime { ref format } if format == "%H:%M"
        ));
        assert!(matches!(
            entries[4],
            TabBarRightEntryConfig::Command {
                interval_seconds: 5,
                timeout_seconds: 2,
                ..
            }
        ));
    }

    #[test]
    fn invalid_tab_bar_right_entries_match_herdr_diagnostics() {
        let diagnostics = tab_bar_right_diagnostics(&[
            TabBarRightEntryConfig::Datetime {
                format: "%Q".into(),
            },
            TabBarRightEntryConfig::Command {
                command: String::new(),
                interval_seconds: 0,
                timeout_seconds: 0,
            },
        ]);
        assert_eq!(diagnostics.len(), 4);
        assert!(diagnostics[0].contains("datetime format"));
        assert!(diagnostics[1].contains("command is empty"));
    }
}
