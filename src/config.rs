use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) const THEME_NAMES: &[&str] = &[
    "catppuccin",
    "terminal",
    "tokyo-night",
    "dracula",
    "nord",
    "gruvbox",
];

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    keys: KeysConfig,
    #[serde(default)]
    theme: ThemeConfig,
    #[serde(default)]
    notifications: NotificationsConfig,
}

#[derive(Debug, Deserialize, Default)]
struct ThemeConfig {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct NotificationsConfig {
    enabled: bool,
    delivery: NotificationDelivery,
    delay_seconds: u64,
    sound: bool,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NotificationDelivery {
    Herdr,
    Terminal,
    System,
    Off,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            delivery: NotificationDelivery::Herdr,
            delay_seconds: 1,
            sound: true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BindingValue {
    One(String),
    Many(Vec<String>),
}

impl BindingValue {
    fn values(&self) -> impl Iterator<Item = &str> {
        match self {
            Self::One(value) => std::slice::from_ref(value).iter().map(String::as_str),
            Self::Many(values) => values.iter().map(String::as_str),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct KeysConfig {
    #[serde(default)]
    prefix: Option<String>,
    #[serde(flatten)]
    bindings: BTreeMap<String, BindingValue>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub prefix: Option<String>,
    pub bindings: BTreeMap<String, Vec<String>>,
    pub theme_name: Option<String>,
    pub notifications_enabled: bool,
    pub(crate) notification_delivery: NotificationDelivery,
    pub(crate) notification_delay_seconds: u64,
    pub(crate) notification_sound: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: None,
            bindings: BTreeMap::new(),
            theme_name: None,
            notifications_enabled: true,
            notification_delivery: NotificationDelivery::Herdr,
            notification_delay_seconds: 1,
            notification_sound: true,
        }
    }
}

pub fn path() -> PathBuf {
    if let Ok(app_data) = std::env::var("APPDATA") {
        return PathBuf::from(app_data).join("Spindle").join("config.toml");
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile)
            .join("AppData")
            .join("Roaming")
            .join("Spindle")
            .join("config.toml");
    }
    std::env::temp_dir().join("Spindle").join("config.toml")
}

pub fn load() -> Config {
    load_from(&path())
}

pub fn load_from(path: &std::path::Path) -> Config {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Config::default();
    };
    let Ok(file) = toml::from_str::<FileConfig>(&content) else {
        return Config::default();
    };
    Config {
        prefix: file.keys.prefix,
        bindings: file
            .keys
            .bindings
            .into_iter()
            .map(|(name, values)| (name, values.values().map(str::to_owned).collect()))
            .collect(),
        theme_name: file.theme.name,
        notifications_enabled: file.notifications.enabled,
        notification_delivery: file.notifications.delivery,
        notification_delay_seconds: file.notifications.delay_seconds.min(3600),
        notification_sound: file.notifications.sound,
    }
}

pub fn default_document() -> &'static str {
    r#"[keys]
prefix = "ctrl+b"
new_tab = "prefix+c"
close_pane = "prefix+x"
close_tab = "prefix+shift+x"
next_tab = ["prefix+n", "prefix+right"]
previous_tab = ["prefix+p", "prefix+left"]
workspace_picker = "prefix+w"
session_navigator = "prefix+g"
settings = "prefix+s"
reload_config = "prefix+shift+r"
open_notification_target = "prefix+o"
create_workspace = "prefix+shift+n"
rename_workspace = "prefix+shift+w"
delete_workspace = "prefix+shift+d"

[theme]
name = "terminal"

[notifications]
enabled = true
delivery = "herdr"
delay_seconds = 1
sound = true
"#
}

pub(crate) fn write_theme(name: &str) -> Result<(), String> {
    update_section_key(&path(), "theme", "name", &format!("\"{name}\""))
}

pub(crate) fn write_notification_delivery(delivery: NotificationDelivery) -> Result<(), String> {
    let value = match delivery {
        NotificationDelivery::Herdr => "\"herdr\"",
        NotificationDelivery::Terminal => "\"terminal\"",
        NotificationDelivery::System => "\"system\"",
        NotificationDelivery::Off => "\"off\"",
    };
    update_section_key(&path(), "notifications", "delivery", value)
}

pub(crate) fn write_notification_sound(enabled: bool) -> Result<(), String> {
    update_section_key(
        &path(),
        "notifications",
        "sound",
        if enabled { "true" } else { "false" },
    )
}

fn update_section_key(
    path: &std::path::Path,
    section: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create config directory: {error}"))?;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read config before saving: {error}")),
    };
    std::fs::write(path, upsert_section_key(&content, section, key, value))
        .map_err(|error| format!("failed to save config: {error}"))
}

fn upsert_section_key(content: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("[{section}]");
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    let section_start = lines.iter().position(|line| line.trim() == header);
    if let Some(start) = section_start {
        let end = lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, line)| line.trim_start().starts_with('['))
            .map_or(lines.len(), |(index, _)| index);
        if let Some(index) = (start + 1..end).find(|index| {
            lines[*index]
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == key)
        }) {
            lines[index] = format!("{key} = {value}");
        } else {
            lines.insert(end, format!("{key} = {value}"));
        }
    } else {
        if !lines.is_empty() && !lines.last().is_some_and(String::is_empty) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.push(format!("{key} = {value}"));
    }
    let mut result = lines.join("\n");
    result.push('\n');
    result
}

#[cfg(test)]
mod tests {
    use super::{load_from, upsert_section_key, NotificationDelivery};

    #[test]
    fn loads_herdr_style_key_bindings() {
        let path = std::env::temp_dir().join(format!("spindle-config-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "[keys]\nprefix = \"ctrl+a\"\nnew_tab = \"prefix+t\"\nnext_tab = [\"prefix+n\", \"ctrl+alt+]\"]\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.prefix.as_deref(), Some("ctrl+a"));
        assert_eq!(config.bindings["new_tab"], vec!["prefix+t"]);
        assert_eq!(config.bindings["next_tab"].len(), 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_or_missing_files_use_defaults() {
        let path = std::env::temp_dir().join(format!(
            "spindle-invalid-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "not = [valid").unwrap();
        assert!(load_from(&path).bindings.is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_theme_name_without_affecting_key_defaults() {
        let path = std::env::temp_dir().join(format!("spindle-theme-{}.toml", std::process::id()));
        std::fs::write(&path, "[theme]\nname = \"nord\"\n").unwrap();
        let config = load_from(&path);
        assert_eq!(config.theme_name.as_deref(), Some("nord"));
        assert!(config.bindings.is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_notification_switch_with_enabled_default() {
        let path =
            std::env::temp_dir().join(format!("spindle-notifications-{}.toml", std::process::id()));
        std::fs::write(&path, "[notifications]\nenabled = false\n").unwrap();
        assert!(!load_from(&path).notifications_enabled);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_herdr_notification_delivery_modes() {
        let path = std::env::temp_dir().join(format!(
            "spindle-notification-delivery-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[notifications]\ndelivery = \"terminal\"\n").unwrap();
        assert_eq!(
            load_from(&path).notification_delivery,
            NotificationDelivery::Terminal
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_and_bounds_notification_delay() {
        let path = std::env::temp_dir().join(format!(
            "spindle-notification-delay-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[notifications]\ndelay_seconds = 7\n").unwrap();
        assert_eq!(load_from(&path).notification_delay_seconds, 7);
        std::fs::write(&path, "[notifications]\ndelay_seconds = 9999\n").unwrap();
        assert_eq!(load_from(&path).notification_delay_seconds, 3600);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn upserts_values_without_removing_other_sections() {
        let content = "[keys]\nprefix = \"ctrl+b\"\n\n[theme]\nname = \"terminal\"\n\n[notifications]\nsound = true\n";
        let updated = upsert_section_key(content, "theme", "name", "\"nord\"");
        assert!(updated.contains("name = \"nord\""));
        assert!(updated.contains("prefix = \"ctrl+b\""));
        assert!(updated.contains("[notifications]"));
    }

    #[test]
    fn default_document_exposes_settings_and_reload_bindings() {
        let document = super::default_document();
        assert!(document.contains("settings = \"prefix+s\""));
        assert!(document.contains("reload_config = \"prefix+shift+r\""));
    }
}
