use serde::{Deserialize, Serialize};

const MAX_ROWS: usize = 16;
const MAX_TOKENS_PER_ROW: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct SidebarConfig {
    #[serde(default)]
    pub(crate) agents: AgentSidebarConfig,
    #[serde(default)]
    pub(crate) spaces: SpaceSidebarConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AgentSidebarConfig {
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) row_gap: u16,
}

impl Default for AgentSidebarConfig {
    fn default() -> Self {
        Self {
            rows: vec![
                vec![
                    "state_icon".into(),
                    "machine".into(),
                    "workspace".into(),
                    "tab".into(),
                ],
                vec!["agent".into()],
            ],
            row_gap: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SpaceSidebarConfig {
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) row_gap: u16,
}

impl Default for SpaceSidebarConfig {
    fn default() -> Self {
        Self {
            rows: vec![
                vec!["state_icon".into(), "workspace".into()],
                vec!["branch".into(), "git_status".into()],
            ],
            row_gap: 0,
        }
    }
}

pub(crate) fn validate(config: &SidebarConfig) -> Result<(), String> {
    validate_rows("agents", &config.agents.rows)?;
    validate_rows("spaces", &config.spaces.rows)
}

fn validate_rows(section: &str, rows: &[Vec<String>]) -> Result<(), String> {
    if rows.len() > MAX_ROWS {
        return Err(format!(
            "sidebar {section} layouts may contain at most {MAX_ROWS} rows"
        ));
    }
    if rows.iter().any(|row| row.len() > MAX_TOKENS_PER_ROW) {
        return Err(format!(
            "sidebar {section} rows may contain at most {MAX_TOKENS_PER_ROW} tokens"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate, SidebarConfig};

    #[test]
    fn defaults_match_herdr_sidebar_row_shapes() {
        let config = SidebarConfig::default();
        assert_eq!(
            config.agents.rows,
            vec![
                vec!["state_icon", "machine", "workspace", "tab"],
                vec!["agent"]
            ]
        );
        assert_eq!(
            config.spaces.rows,
            vec![
                vec!["state_icon", "workspace"],
                vec!["branch", "git_status"]
            ]
        );
    }

    #[test]
    fn rejects_oversized_herdr_style_layouts() {
        let mut config = SidebarConfig::default();
        config.spaces.rows = vec![vec!["workspace".to_owned(); 17]];
        assert!(validate(&config).is_err());
    }

    #[test]
    fn parses_nested_sidebar_configuration() {
        let config: SidebarConfig = toml::from_str(
            r#"
            [agents]
            row_gap = 1
            rows = [["state_icon", "$model"], ["agent"]]
            [spaces]
            rows = [["state_icon", "workspace"], ["$jj_status"]]
            "#,
        )
        .unwrap();
        assert_eq!(config.agents.row_gap, 1);
        assert_eq!(config.spaces.rows[1][0], "$jj_status");
        validate(&config).unwrap();
    }
}
