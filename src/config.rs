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
    #[serde(default)]
    ui: UiConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct UiConfig {
    sidebar_width: u16,
    sidebar_min_width: u16,
    sidebar_max_width: u16,
    mobile_width_threshold: u16,
    sidebar_start_collapsed: bool,
    prompt_new_tab_name: bool,
    prompt_new_workspace_name: bool,
    copy_on_select: bool,
    mouse_capture: bool,
    host_cursor: HostCursorMode,
    mouse_scroll_lines: u16,
    confirm_close: bool,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HostCursorMode {
    #[default]
    Auto,
    Native,
    Drawn,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            sidebar_width: 26,
            sidebar_min_width: 18,
            sidebar_max_width: 36,
            mobile_width_threshold: 64,
            sidebar_start_collapsed: false,
            prompt_new_tab_name: true,
            prompt_new_workspace_name: false,
            copy_on_select: true,
            mouse_capture: true,
            host_cursor: HostCursorMode::Auto,
            mouse_scroll_lines: 3,
            confirm_close: true,
        }
    }
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

impl Default for BindingValue {
    fn default() -> Self {
        Self::One(String::new())
    }
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
    #[serde(default)]
    command: Vec<CustomCommandFile>,
    #[serde(flatten)]
    bindings: BTreeMap<String, BindingValue>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct CustomCommandFile {
    key: BindingValue,
    command: String,
    #[serde(rename = "type")]
    action_type: String,
    description: Option<String>,
    width: Option<u16>,
    height: Option<u16>,
}

#[derive(Debug, Clone)]
pub(crate) struct CustomCommand {
    pub(crate) bindings: Vec<String>,
    pub(crate) command: String,
    pub(crate) action_type: String,
    pub(crate) description: Option<String>,
    pub(crate) width: Option<u16>,
    pub(crate) height: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub prefix: Option<String>,
    pub bindings: BTreeMap<String, Vec<String>>,
    pub(crate) custom_commands: Vec<CustomCommand>,
    pub theme_name: Option<String>,
    pub notifications_enabled: bool,
    pub(crate) notification_delivery: NotificationDelivery,
    pub(crate) notification_delay_seconds: u64,
    pub(crate) notification_sound: bool,
    pub(crate) sidebar_width: u16,
    pub(crate) sidebar_min_width: u16,
    pub(crate) sidebar_max_width: u16,
    pub(crate) mobile_width_threshold: u16,
    pub(crate) sidebar_start_collapsed: bool,
    pub(crate) prompt_new_tab_name: bool,
    pub(crate) prompt_new_workspace_name: bool,
    pub(crate) copy_on_select: bool,
    pub(crate) mouse_capture: bool,
    pub(crate) host_cursor: HostCursorMode,
    pub(crate) mouse_scroll_lines: usize,
    pub(crate) confirm_close: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: None,
            bindings: BTreeMap::new(),
            custom_commands: Vec::new(),
            theme_name: None,
            notifications_enabled: true,
            notification_delivery: NotificationDelivery::Herdr,
            notification_delay_seconds: 1,
            notification_sound: true,
            sidebar_width: 26,
            sidebar_min_width: 18,
            sidebar_max_width: 36,
            mobile_width_threshold: 64,
            sidebar_start_collapsed: false,
            prompt_new_tab_name: true,
            prompt_new_workspace_name: false,
            copy_on_select: true,
            mouse_capture: true,
            host_cursor: HostCursorMode::Auto,
            mouse_scroll_lines: 3,
            confirm_close: true,
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
        custom_commands: file
            .keys
            .command
            .into_iter()
            .map(|command| CustomCommand {
                bindings: command.key.values().map(str::to_owned).collect(),
                command: command.command,
                action_type: command.action_type,
                description: command.description,
                width: command.width,
                height: command.height,
            })
            .collect(),
        theme_name: file.theme.name,
        notifications_enabled: file.notifications.enabled,
        notification_delivery: file.notifications.delivery,
        notification_delay_seconds: file.notifications.delay_seconds.min(3600),
        notification_sound: file.notifications.sound,
        sidebar_width: file.ui.sidebar_width,
        sidebar_min_width: file.ui.sidebar_min_width,
        sidebar_max_width: file.ui.sidebar_max_width,
        mobile_width_threshold: file.ui.mobile_width_threshold,
        sidebar_start_collapsed: file.ui.sidebar_start_collapsed,
        prompt_new_tab_name: file.ui.prompt_new_tab_name,
        prompt_new_workspace_name: file.ui.prompt_new_workspace_name,
        copy_on_select: file.ui.copy_on_select,
        mouse_capture: file.ui.mouse_capture,
        host_cursor: file.ui.host_cursor,
        mouse_scroll_lines: usize::from(file.ui.mouse_scroll_lines.max(1)),
        confirm_close: file.ui.confirm_close,
    }
}

pub(crate) fn sidebar_bounds(config: &Config) -> (u16, u16) {
    if config.sidebar_min_width <= config.sidebar_max_width {
        (config.sidebar_min_width, config.sidebar_max_width)
    } else {
        (18, 36)
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
navigate_workspace_up = "up"
navigate_workspace_down = "down"
session_navigator = "prefix+g"
settings = "prefix+s"
reload_config = "prefix+shift+r"
open_notification_target = "prefix+o"
create_workspace = "prefix+shift+n"
rename_workspace = "prefix+shift+w"
delete_workspace = "prefix+shift+d"

# Add custom commands with `[[keys.command]]`; for example:
# key = "prefix+alt+g"
# type = "shell"
# command = "git status"

[theme]
name = "terminal"

[notifications]
enabled = true
delivery = "herdr"
delay_seconds = 1
sound = true

[ui]
sidebar_width = 26
sidebar_min_width = 18
sidebar_max_width = 36
mobile_width_threshold = 64
sidebar_start_collapsed = false
prompt_new_tab_name = true
prompt_new_workspace_name = false
copy_on_select = true
mouse_capture = true
host_cursor = "auto"
mouse_scroll_lines = 3
confirm_close = true
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
    use super::{load_from, upsert_section_key, Config, HostCursorMode, NotificationDelivery};

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
    fn loads_herdr_style_custom_shell_commands() {
        let path = std::env::temp_dir().join(format!(
            "spindle-custom-command-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[[keys.command]]\nkey = \"prefix+alt+g\"\ntype = \"shell\"\ncommand = \"echo hello\"\ndescription = \"say hello\"\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.custom_commands.len(), 1);
        assert_eq!(config.custom_commands[0].bindings, ["prefix+alt+g"]);
        assert_eq!(config.custom_commands[0].command, "echo hello");
        assert_eq!(
            config.custom_commands[0].description.as_deref(),
            Some("say hello")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_custom_plugin_action_commands() {
        let path = std::env::temp_dir().join(format!(
            "spindle-custom-plugin-command-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[[keys.command]]\nkey = \"prefix+alt+p\"\ntype = \"plugin_action\"\ncommand = \"example.plugin.open\"\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.custom_commands[0].action_type, "plugin_action");
        assert_eq!(config.custom_commands[0].command, "example.plugin.open");
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
    fn loads_herdr_sidebar_start_collapsed_setting() {
        let path = std::env::temp_dir().join(format!(
            "spindle-sidebar-start-collapsed-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\nsidebar_start_collapsed = true\n").unwrap();
        assert!(load_from(&path).sidebar_start_collapsed);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn new_tab_name_prompt_matches_herdr_default() {
        assert!(Config::default().prompt_new_tab_name);
        let path = std::env::temp_dir().join(format!(
            "spindle-prompt-new-tab-name-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\nprompt_new_tab_name = false\n").unwrap();
        assert!(!load_from(&path).prompt_new_tab_name);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn new_workspace_name_prompt_matches_herdr_default() {
        assert!(!Config::default().prompt_new_workspace_name);
        let path = std::env::temp_dir().join(format!(
            "spindle-prompt-new-workspace-name-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\nprompt_new_workspace_name = true\n").unwrap();
        assert!(load_from(&path).prompt_new_workspace_name);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn copy_on_select_matches_herdr_default() {
        assert!(Config::default().copy_on_select);
        let path = std::env::temp_dir().join(format!(
            "spindle-copy-on-select-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\ncopy_on_select = false\n").unwrap();
        assert!(!load_from(&path).copy_on_select);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn mouse_capture_matches_herdr_default() {
        assert!(Config::default().mouse_capture);
        let path =
            std::env::temp_dir().join(format!("spindle-mouse-capture-{}.toml", std::process::id()));
        std::fs::write(&path, "[ui]\nmouse_capture = false\n").unwrap();
        assert!(!load_from(&path).mouse_capture);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn host_cursor_modes_match_herdr_names_and_default() {
        assert_eq!(Config::default().host_cursor, HostCursorMode::Auto);
        let path =
            std::env::temp_dir().join(format!("spindle-host-cursor-{}.toml", std::process::id()));
        std::fs::write(&path, "[ui]\nhost_cursor = \"drawn\"\n").unwrap();
        assert_eq!(load_from(&path).host_cursor, HostCursorMode::Drawn);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn mouse_scroll_lines_matches_herdr_default_and_value() {
        assert_eq!(Config::default().mouse_scroll_lines, 3);
        let path = std::env::temp_dir().join(format!(
            "spindle-mouse-scroll-lines-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\nmouse_scroll_lines = 7\n").unwrap();
        assert_eq!(load_from(&path).mouse_scroll_lines, 7);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn confirm_close_matches_herdr_default_and_value() {
        assert!(Config::default().confirm_close);
        let path =
            std::env::temp_dir().join(format!("spindle-confirm-close-{}.toml", std::process::id()));
        std::fs::write(&path, "[ui]\nconfirm_close = false\n").unwrap();
        assert!(!load_from(&path).confirm_close);
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
    fn loads_herdr_sidebar_width_settings() {
        let path = std::env::temp_dir().join(format!(
            "spindle-sidebar-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[ui]\nsidebar_width = 30\nsidebar_min_width = 20\nsidebar_max_width = 40\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.sidebar_width, 30);
        assert_eq!(super::sidebar_bounds(&config), (20, 40));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn inverted_sidebar_bounds_use_safe_defaults() {
        let path = std::env::temp_dir().join(format!(
            "spindle-sidebar-invalid-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[ui]\nsidebar_min_width = 40\nsidebar_max_width = 20\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(super::sidebar_bounds(&config), (18, 36));
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
        assert!(document.contains("sidebar_width = 26"));
    }
}
