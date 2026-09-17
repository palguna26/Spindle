use serde::Deserialize;

pub(crate) const MAX_TAB_BAR_RIGHT_ENTRIES: usize = 16;

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

#[cfg(test)]
mod tests {
    use super::TabBarRightEntryConfig;
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
}
