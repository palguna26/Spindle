use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

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
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self { enabled: true }
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: None,
            bindings: BTreeMap::new(),
            theme_name: None,
            notifications_enabled: true,
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
    }
}

pub fn default_document() -> &'static str {
    "[keys]\nprefix = \"ctrl+b\"\nnew_tab = \"prefix+c\"\nclose_pane = \"prefix+x\"\nclose_tab = \"prefix+shift+x\"\nnext_tab = [\"prefix+n\", \"prefix+right\"]\nprevious_tab = [\"prefix+p\", \"prefix+left\"]\nworkspace_picker = \"prefix+w\"\nsession_navigator = \"prefix+g\"\ncreate_workspace = \"prefix+shift+n\"\nrename_workspace = \"prefix+shift+w\"\ndelete_workspace = \"prefix+shift+d\"\n\n[theme]\nname = \"terminal\"\n"
}

#[cfg(test)]
mod tests {
    use super::load_from;

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
}
