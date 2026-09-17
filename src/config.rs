use crossterm::event::KeyModifiers;
use serde::{de, Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

mod sidebar;
pub(crate) use sidebar::SidebarConfig;

pub(crate) const THEME_NAMES: &[&str] = &[
    "catppuccin",
    "catppuccin-latte",
    "terminal",
    "tokyo-night",
    "tokyo-night-day",
    "dracula",
    "nord",
    "gruvbox",
    "gruvbox-light",
    "one-dark",
    "one-light",
    "solarized",
    "solarized-light",
    "kanagawa",
    "kanagawa-lotus",
    "rose-pine",
    "rose-pine-dawn",
    "vesper",
];

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    onboarding: Option<bool>,
    #[serde(default)]
    keys: KeysConfig,
    #[serde(default)]
    theme: ThemeConfig,
    #[serde(default)]
    notifications: NotificationsConfig,
    #[serde(default)]
    ui: UiConfig,
    #[serde(default)]
    terminal: TerminalConfig,
    #[serde(default)]
    worktrees: WorktreesConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct WorktreesConfig {
    directory: String,
}

impl Default for WorktreesConfig {
    fn default() -> Self {
        Self {
            directory: "~/.herdr/worktrees".into(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TerminalConfig {
    default_shell: Option<String>,
    shell_mode: ShellMode,
    new_cwd: NewCwd,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum NewCwd {
    #[default]
    Follow,
    Home,
    Current,
    Path(String),
}

impl<'de> Deserialize<'de> for NewCwd {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.trim() {
            "" | "follow" => Self::Follow,
            "home" => Self::Home,
            "current" => Self::Current,
            _ => Self::Path(value),
        })
    }
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShellMode {
    #[default]
    Auto,
    Login,
    NonLogin,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct UiConfig {
    #[serde(default)]
    sidebar: SidebarConfig,
    sidebar_width: u16,
    sidebar_min_width: u16,
    sidebar_max_width: u16,
    mobile_width_threshold: u16,
    sidebar_start_collapsed: bool,
    agent_panel_sort: AgentPanelSort,
    sidebar_collapsed_mode: SidebarCollapsedMode,
    tab_bar_position: TabBarPosition,
    pane_borders: PaneBorders,
    pane_outer_borders: bool,
    pane_gaps: bool,
    pane_scrollbars: bool,
    prompt_new_tab_name: bool,
    prompt_new_workspace_name: bool,
    copy_on_select: bool,
    mouse_capture: bool,
    host_cursor: HostCursorMode,
    mouse_scroll_lines: u16,
    confirm_close: bool,
    hide_tab_bar_when_single_tab: bool,
    right_click_passthrough_modifier: String,
    redraw_on_focus_gained: bool,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SidebarCollapsedMode {
    #[default]
    Compact,
    Hidden,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AgentPanelSort {
    #[default]
    #[serde(alias = "workspaces")]
    Spaces,
    Priority,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TabBarPosition {
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PaneBorders {
    #[default]
    Auto,
    Always,
    Off,
}

impl PaneBorders {
    pub(crate) fn shows_borders(self, multi_pane: bool) -> bool {
        !matches!(self, Self::Off) && (multi_pane || matches!(self, Self::Always))
    }
}

impl<'de> Deserialize<'de> for PaneBorders {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PaneBordersVisitor;
        impl<'de> de::Visitor<'de> for PaneBordersVisitor {
            type Value = PaneBorders;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("\"auto\", \"always\", \"off\", or a boolean")
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(if value {
                    PaneBorders::Auto
                } else {
                    PaneBorders::Off
                })
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "auto" => Ok(PaneBorders::Auto),
                    "always" => Ok(PaneBorders::Always),
                    "off" => Ok(PaneBorders::Off),
                    other => Err(E::invalid_value(de::Unexpected::Str(other), &self)),
                }
            }
        }
        deserializer.deserialize_any(PaneBordersVisitor)
    }
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
            sidebar: SidebarConfig::default(),
            sidebar_width: 26,
            sidebar_min_width: 18,
            sidebar_max_width: 36,
            mobile_width_threshold: 64,
            sidebar_start_collapsed: false,
            agent_panel_sort: AgentPanelSort::Spaces,
            sidebar_collapsed_mode: SidebarCollapsedMode::Compact,
            tab_bar_position: TabBarPosition::Top,
            pane_borders: PaneBorders::Auto,
            pane_outer_borders: true,
            pane_gaps: true,
            pane_scrollbars: true,
            prompt_new_tab_name: true,
            prompt_new_workspace_name: false,
            copy_on_select: true,
            mouse_capture: true,
            host_cursor: HostCursorMode::Auto,
            mouse_scroll_lines: 3,
            confirm_close: true,
            hide_tab_bar_when_single_tab: false,
            right_click_passthrough_modifier: String::new(),
            redraw_on_focus_gained: true,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct ThemeConfig {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    custom: ThemeCustomConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ThemeCustomConfig {
    accent: Option<String>,
    panel_bg: Option<String>,
    sidebar_bg: Option<String>,
    active_row_bg: Option<String>,
    selection_bg: Option<String>,
    surface0: Option<String>,
    surface_dim: Option<String>,
    overlay0: Option<String>,
    overlay1: Option<String>,
    text: Option<String>,
    subtext0: Option<String>,
    green: Option<String>,
    yellow: Option<String>,
    red: Option<String>,
    teal: Option<String>,
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
    width: Option<crate::popup_size::PopupSize>,
    height: Option<crate::popup_size::PopupSize>,
}

#[derive(Debug, Clone)]
pub(crate) struct CustomCommand {
    pub(crate) bindings: Vec<String>,
    pub(crate) command: String,
    pub(crate) action_type: String,
    pub(crate) description: Option<String>,
    pub(crate) width: Option<crate::popup_size::PopupSize>,
    pub(crate) height: Option<crate::popup_size::PopupSize>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) onboarding: Option<bool>,
    pub(crate) sidebar: SidebarConfig,
    pub prefix: Option<String>,
    pub bindings: BTreeMap<String, Vec<String>>,
    pub(crate) custom_commands: Vec<CustomCommand>,
    pub theme_name: Option<String>,
    pub(crate) theme_custom_accent: Option<String>,
    pub(crate) theme_custom_panel_bg: Option<String>,
    pub(crate) theme_custom_sidebar_bg: Option<String>,
    pub(crate) theme_custom_active_row_bg: Option<String>,
    pub(crate) theme_custom_selection_bg: Option<String>,
    pub(crate) theme_custom_surface0: Option<String>,
    pub(crate) theme_custom_surface_dim: Option<String>,
    pub(crate) theme_custom_overlay0: Option<String>,
    pub(crate) theme_custom_overlay1: Option<String>,
    pub(crate) theme_custom_text: Option<String>,
    pub(crate) theme_custom_subtext0: Option<String>,
    pub(crate) theme_custom_green: Option<String>,
    pub(crate) theme_custom_yellow: Option<String>,
    pub(crate) theme_custom_red: Option<String>,
    pub(crate) theme_custom_teal: Option<String>,
    pub notifications_enabled: bool,
    pub(crate) notification_delivery: NotificationDelivery,
    pub(crate) notification_delay_seconds: u64,
    pub(crate) notification_sound: bool,
    pub(crate) sidebar_width: u16,
    pub(crate) sidebar_min_width: u16,
    pub(crate) sidebar_max_width: u16,
    pub(crate) mobile_width_threshold: u16,
    pub(crate) sidebar_start_collapsed: bool,
    pub(crate) agent_priority_sort: bool,
    pub(crate) sidebar_collapsed_mode: SidebarCollapsedMode,
    pub(crate) tab_bar_position: TabBarPosition,
    pub(crate) pane_borders: PaneBorders,
    pub(crate) pane_outer_borders: bool,
    pub(crate) pane_gaps: bool,
    pub(crate) pane_scrollbars: bool,
    pub(crate) prompt_new_tab_name: bool,
    pub(crate) prompt_new_workspace_name: bool,
    pub(crate) copy_on_select: bool,
    pub(crate) mouse_capture: bool,
    pub(crate) host_cursor: HostCursorMode,
    pub(crate) mouse_scroll_lines: usize,
    pub(crate) confirm_close: bool,
    pub(crate) hide_tab_bar_when_single_tab: bool,
    pub(crate) right_click_passthrough_modifier: Option<KeyModifiers>,
    pub(crate) redraw_on_focus_gained: bool,
    pub(crate) default_shell: Option<String>,
    pub(crate) shell_mode: ShellMode,
    pub(crate) new_cwd: NewCwd,
    pub(crate) worktree_directory: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            onboarding: None,
            sidebar: SidebarConfig::default(),
            prefix: None,
            bindings: BTreeMap::new(),
            custom_commands: Vec::new(),
            theme_name: None,
            theme_custom_accent: None,
            theme_custom_panel_bg: None,
            theme_custom_sidebar_bg: None,
            theme_custom_active_row_bg: None,
            theme_custom_selection_bg: None,
            theme_custom_surface0: None,
            theme_custom_surface_dim: None,
            theme_custom_overlay0: None,
            theme_custom_overlay1: None,
            theme_custom_text: None,
            theme_custom_subtext0: None,
            theme_custom_green: None,
            theme_custom_yellow: None,
            theme_custom_red: None,
            theme_custom_teal: None,
            notifications_enabled: true,
            notification_delivery: NotificationDelivery::Herdr,
            notification_delay_seconds: 1,
            notification_sound: true,
            sidebar_width: 26,
            sidebar_min_width: 18,
            sidebar_max_width: 36,
            mobile_width_threshold: 64,
            sidebar_start_collapsed: false,
            agent_priority_sort: false,
            sidebar_collapsed_mode: SidebarCollapsedMode::Compact,
            tab_bar_position: TabBarPosition::Top,
            pane_borders: PaneBorders::Auto,
            pane_outer_borders: true,
            pane_gaps: true,
            pane_scrollbars: true,
            prompt_new_tab_name: true,
            prompt_new_workspace_name: false,
            copy_on_select: true,
            mouse_capture: true,
            host_cursor: HostCursorMode::Auto,
            mouse_scroll_lines: 3,
            confirm_close: true,
            hide_tab_bar_when_single_tab: false,
            right_click_passthrough_modifier: None,
            redraw_on_focus_gained: true,
            default_shell: None,
            shell_mode: ShellMode::Auto,
            new_cwd: NewCwd::Follow,
            worktree_directory: expand_tilde_path("~/.herdr/worktrees"),
        }
    }
}

pub fn path() -> PathBuf {
    if let Some(path) = config_path_override(
        std::env::var_os("SPINDLE_CONFIG_PATH"),
        std::env::var_os("HERDR_CONFIG_PATH"),
    ) {
        return path;
    }
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

pub(crate) fn remove_keybinding_config_sections(content: &str) -> (String, bool) {
    let mut result = Vec::new();
    let mut removed = false;
    let mut skipping_keys = false;
    let mut in_table = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(table_name) = table_header_name(trimmed) {
            in_table = true;
            skipping_keys = table_name == "keys" || table_name.starts_with("keys.");
            if skipping_keys {
                removed = true;
                continue;
            }
        } else if skipping_keys || (!in_table && is_top_level_keys_assignment(trimmed)) {
            removed = true;
            continue;
        }
        result.push(line);
    }

    let mut updated = result.join("\n");
    if content.ends_with('\n') || !updated.is_empty() {
        updated.push('\n');
    }
    (updated, removed)
}

fn table_header_name(trimmed: &str) -> Option<&str> {
    if let Some(name) = trimmed
        .strip_prefix("[[")
        .and_then(|value| value.strip_suffix("]]"))
    {
        return Some(name.trim());
    }
    trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .map(str::trim)
}

fn is_top_level_keys_assignment(trimmed: &str) -> bool {
    trimmed.starts_with("keys ") || trimmed.starts_with("keys=") || trimmed.starts_with("keys.")
}

fn config_path_override(
    spindle: Option<std::ffi::OsString>,
    herdr: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    spindle
        .filter(|path| !path.is_empty())
        .or_else(|| herdr.filter(|path| !path.is_empty()))
        .map(PathBuf::from)
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
    if sidebar::validate(&file.ui.sidebar).is_err() {
        return Config::default();
    }
    Config {
        onboarding: file.onboarding,
        sidebar: file.ui.sidebar,
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
        theme_custom_accent: file.theme.custom.accent,
        theme_custom_panel_bg: file.theme.custom.panel_bg,
        theme_custom_sidebar_bg: file.theme.custom.sidebar_bg,
        theme_custom_active_row_bg: file.theme.custom.active_row_bg,
        theme_custom_selection_bg: file.theme.custom.selection_bg,
        theme_custom_surface0: file.theme.custom.surface0,
        theme_custom_surface_dim: file.theme.custom.surface_dim,
        theme_custom_overlay0: file.theme.custom.overlay0,
        theme_custom_overlay1: file.theme.custom.overlay1,
        theme_custom_text: file.theme.custom.text,
        theme_custom_subtext0: file.theme.custom.subtext0,
        theme_custom_green: file.theme.custom.green,
        theme_custom_yellow: file.theme.custom.yellow,
        theme_custom_red: file.theme.custom.red,
        theme_custom_teal: file.theme.custom.teal,
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
        agent_priority_sort: matches!(file.ui.agent_panel_sort, AgentPanelSort::Priority),
        sidebar_collapsed_mode: file.ui.sidebar_collapsed_mode,
        tab_bar_position: file.ui.tab_bar_position,
        pane_borders: file.ui.pane_borders,
        pane_outer_borders: file.ui.pane_outer_borders,
        pane_gaps: file.ui.pane_gaps,
        pane_scrollbars: file.ui.pane_scrollbars,
        prompt_new_tab_name: file.ui.prompt_new_tab_name,
        prompt_new_workspace_name: file.ui.prompt_new_workspace_name,
        copy_on_select: file.ui.copy_on_select,
        mouse_capture: file.ui.mouse_capture,
        host_cursor: file.ui.host_cursor,
        mouse_scroll_lines: usize::from(file.ui.mouse_scroll_lines.max(1)),
        confirm_close: file.ui.confirm_close,
        hide_tab_bar_when_single_tab: file.ui.hide_tab_bar_when_single_tab,
        right_click_passthrough_modifier: parse_right_click_passthrough_modifier(
            &file.ui.right_click_passthrough_modifier,
        ),
        redraw_on_focus_gained: file.ui.redraw_on_focus_gained,
        default_shell: file
            .terminal
            .default_shell
            .filter(|shell| !shell.trim().is_empty()),
        shell_mode: file.terminal.shell_mode,
        new_cwd: file.terminal.new_cwd,
        worktree_directory: expand_tilde_path(&file.worktrees.directory),
    }
}

pub(crate) fn check(path: &std::path::Path) -> io::Result<Vec<String>> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    let file = match toml::from_str::<FileConfig>(&content) {
        Ok(file) => file,
        Err(error) => return Ok(vec![format!("invalid TOML: {error}")]),
    };
    let mut diagnostics = Vec::new();
    if let Err(error) = sidebar::validate(&file.ui.sidebar) {
        diagnostics.push(format!("invalid sidebar config: {error}"));
    }
    Ok(diagnostics)
}

fn expand_tilde_path(value: &str) -> PathBuf {
    if value == "~" {
        return home_directory().unwrap_or_else(|| PathBuf::from(value));
    }
    let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    else {
        let path = PathBuf::from(value);
        return if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&path))
                .unwrap_or(path)
        };
    };
    home_directory()
        .map(|home| home.join(rest))
        .unwrap_or_else(|| PathBuf::from(value))
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

pub(crate) fn complete_onboarding() -> Result<(), String> {
    let config_path = path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create config directory: {error}"))?;
    }
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read config before saving: {error}")),
    };
    std::fs::write(
        &config_path,
        upsert_top_level_bool(&content, "onboarding", false),
    )
    .map_err(|error| format!("failed to save onboarding setting: {error}"))
}

pub(crate) fn upsert_top_level_bool(content: &str, key: &str, value: bool) -> String {
    let replacement = format!("{key} = {value}");
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    let mut replaced = false;
    let mut in_section = false;
    for line in &mut lines {
        if line.trim_start().starts_with('[') {
            in_section = true;
        }
        if !in_section
            && line
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == key)
        {
            *line = replacement.clone();
            replaced = true;
        }
    }
    if !replaced {
        let insert_at = lines
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .unwrap_or(lines.len());
        lines.insert(insert_at, replacement);
    }
    let mut updated = lines.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn parse_right_click_passthrough_modifier(value: &str) -> Option<KeyModifiers> {
    let value = value.trim();
    if value.is_empty()
        || value.eq_ignore_ascii_case("off")
        || value.eq_ignore_ascii_case("none")
        || value.eq_ignore_ascii_case("disabled")
    {
        return None;
    }
    let mut modifiers = KeyModifiers::empty();
    for part in value.split('+') {
        modifiers |= match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KeyModifiers::CONTROL,
            "alt" | "option" => KeyModifiers::ALT,
            "cmd" | "command" | "super" => KeyModifiers::SUPER,
            "meta" => KeyModifiers::META,
            "hyper" => KeyModifiers::HYPER,
            _ => return None,
        };
    }
    (!modifiers.is_empty()).then_some(modifiers)
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
new_worktree = "prefix+shift+g"
rename_workspace = "prefix+shift+w"
delete_workspace = "prefix+shift+d"

# Add custom commands with `[[keys.command]]`; for example:
# key = "prefix+alt+g"
# type = "shell"
# command = "git status"

[theme]
name = "terminal"

[theme.custom]
# green = "rgb(166, 227, 161)"
# yellow = "rgb(249, 226, 175)"
# red = "rgb(243, 139, 168)"
# teal = "rgb(148, 226, 213)"

[worktrees]
# Default: ~/.herdr/worktrees/<repository>/<branch-slug>
directory = "~/.herdr/worktrees"

[notifications]
enabled = true
delivery = "herdr"
delay_seconds = 1
sound = true

[terminal]
# default_shell = "powershell.exe"
# shell_mode = "auto" # auto, login, or non_login
# new_cwd = "follow" # follow, home, current, or a fixed path

[ui]
sidebar_width = 26
sidebar_min_width = 18
sidebar_max_width = 36
mobile_width_threshold = 64
sidebar_start_collapsed = false
agent_panel_sort = "spaces"
sidebar_collapsed_mode = "compact"
tab_bar_position = "top"
pane_borders = "auto"
pane_outer_borders = true
pane_gaps = true
pane_scrollbars = true
prompt_new_tab_name = true
prompt_new_workspace_name = false
copy_on_select = true
mouse_capture = true
host_cursor = "auto"
mouse_scroll_lines = 3
confirm_close = true
hide_tab_bar_when_single_tab = false
right_click_passthrough_modifier = ""
redraw_on_focus_gained = true

# Herdr-style sidebar layouts. Tokens are rendered in the next sidebar layer.
[ui.sidebar.agents]
rows = [["state_icon", "machine", "workspace", "tab"], ["agent"]]
row_gap = 0

[ui.sidebar.spaces]
rows = [["state_icon", "workspace"], ["branch", "git_status"]]
row_gap = 0
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
    use super::{
        config_path_override, load_from, remove_keybinding_config_sections, upsert_section_key,
        upsert_top_level_bool, Config, HostCursorMode, NewCwd, NotificationDelivery, PaneBorders,
        ShellMode, SidebarCollapsedMode, TabBarPosition,
    };
    use crossterm::event::KeyModifiers;

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
    fn loads_herdr_style_terminal_default_shell() {
        let path = std::env::temp_dir().join(format!(
            "spindle-terminal-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[terminal]\ndefault_shell = \"pwsh.exe\"\n").unwrap();
        assert_eq!(load_from(&path).default_shell.as_deref(), Some("pwsh.exe"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_herdr_style_terminal_shell_mode() {
        let path = std::env::temp_dir().join(format!(
            "spindle-terminal-mode-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[terminal]\nshell_mode = \"login\"\n").unwrap();
        assert_eq!(load_from(&path).shell_mode, ShellMode::Login);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_herdr_style_terminal_new_cwd_modes() {
        let path = std::env::temp_dir().join(format!(
            "spindle-terminal-cwd-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[terminal]\nnew_cwd = \"~/Projects\"\n").unwrap();
        assert_eq!(load_from(&path).new_cwd, NewCwd::Path("~/Projects".into()));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_herdr_style_worktree_directory() {
        let path = std::env::temp_dir().join(format!(
            "spindle-worktree-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[worktrees]\ndirectory = \"C:/Projects/worktrees\"\n",
        )
        .unwrap();
        assert_eq!(
            load_from(&path).worktree_directory,
            std::path::PathBuf::from("C:/Projects/worktrees")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn removes_only_keybinding_config_sections() {
        let content = "onboarding = false\n\n[keys]\nprefix = \"ctrl+a\"\n\n[[keys.command]]\nkey = \"g\"\ncommand = \"git status\"\n\n[theme]\nname = \"nord\"\n";
        let (updated, removed) = remove_keybinding_config_sections(content);
        assert!(removed);
        assert!(updated.contains("onboarding = false"));
        assert!(updated.contains("[theme]\nname = \"nord\""));
        assert!(!updated.contains("[keys]"));
        assert!(!updated.contains("keys.command"));
        assert!(updated.parse::<toml::Value>().is_ok());
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
    fn loads_herdr_custom_theme_accent() {
        let path =
            std::env::temp_dir().join(format!("spindle-theme-custom-{}.toml", std::process::id()));
        std::fs::write(&path, "[theme.custom]\naccent = \"#010203\"\n").unwrap();
        let config = load_from(&path);
        assert_eq!(config.theme_custom_accent.as_deref(), Some("#010203"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_herdr_custom_theme_panel_background() {
        let path =
            std::env::temp_dir().join(format!("spindle-theme-panel-{}.toml", std::process::id()));
        std::fs::write(&path, "[theme.custom]\npanel_bg = \"#101112\"\n").unwrap();
        let config = load_from(&path);
        assert_eq!(config.theme_custom_panel_bg.as_deref(), Some("#101112"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_herdr_custom_sidebar_surface_colors() {
        let path = std::env::temp_dir().join(format!(
            "spindle-theme-sidebar-surface-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[theme.custom]\nsidebar_bg = \"#101112\"\nactive_row_bg = \"rgb(4, 5, 6)\"\nselection_bg = \"#070809\"\nsurface0 = \"#0a0b0c\"\nsurface_dim = \"#0c0d0e\"\noverlay0 = \"#0d0e0f\"\noverlay1 = \"#101112\"\ntext = \"#131415\"\nsubtext0 = \"#161718\"\ngreen = \"#192021\"\nyellow = \"#222324\"\nred = \"#252627\"\nteal = \"#28292a\"\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.theme_custom_sidebar_bg.as_deref(), Some("#101112"));
        assert_eq!(
            config.theme_custom_active_row_bg.as_deref(),
            Some("rgb(4, 5, 6)")
        );
        assert_eq!(config.theme_custom_selection_bg.as_deref(), Some("#070809"));
        assert_eq!(config.theme_custom_surface0.as_deref(), Some("#0a0b0c"));
        assert_eq!(config.theme_custom_surface_dim.as_deref(), Some("#0c0d0e"));
        assert_eq!(config.theme_custom_overlay0.as_deref(), Some("#0d0e0f"));
        assert_eq!(config.theme_custom_overlay1.as_deref(), Some("#101112"));
        assert_eq!(config.theme_custom_text.as_deref(), Some("#131415"));
        assert_eq!(config.theme_custom_subtext0.as_deref(), Some("#161718"));
        assert_eq!(config.theme_custom_green.as_deref(), Some("#192021"));
        assert_eq!(config.theme_custom_yellow.as_deref(), Some("#222324"));
        assert_eq!(config.theme_custom_red.as_deref(), Some("#252627"));
        assert_eq!(config.theme_custom_teal.as_deref(), Some("#28292a"));
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
    fn loads_herdr_sidebar_rows() {
        let path =
            std::env::temp_dir().join(format!("spindle-sidebar-rows-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "[ui.sidebar.spaces]\nrows = [[\"state_icon\", \"workspace\"]]\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.sidebar.spaces.rows.len(), 1);
        assert_eq!(config.sidebar.spaces.rows[0][1], "workspace");
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
    fn hide_tab_bar_matches_herdr_default_and_value() {
        assert!(!Config::default().hide_tab_bar_when_single_tab);
        let path =
            std::env::temp_dir().join(format!("spindle-hide-tab-bar-{}.toml", std::process::id()));
        std::fs::write(&path, "[ui]\nhide_tab_bar_when_single_tab = true\n").unwrap();
        assert!(load_from(&path).hide_tab_bar_when_single_tab);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn right_click_passthrough_modifier_matches_herdr_forms() {
        assert_eq!(Config::default().right_click_passthrough_modifier, None);
        let path = std::env::temp_dir().join(format!(
            "spindle-right-click-modifier-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[ui]\nright_click_passthrough_modifier = \"cmd+alt\"\n",
        )
        .unwrap();
        assert_eq!(
            load_from(&path).right_click_passthrough_modifier,
            Some(KeyModifiers::SUPER | KeyModifiers::ALT)
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn redraw_on_focus_gained_matches_herdr_default_and_value() {
        assert!(Config::default().redraw_on_focus_gained);
        let path =
            std::env::temp_dir().join(format!("spindle-redraw-focus-{}.toml", std::process::id()));
        std::fs::write(&path, "[ui]\nredraw_on_focus_gained = false\n").unwrap();
        assert!(!load_from(&path).redraw_on_focus_gained);
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
    fn sidebar_collapsed_mode_matches_herdr_default_and_hidden_value() {
        assert_eq!(
            Config::default().sidebar_collapsed_mode,
            SidebarCollapsedMode::Compact
        );
        let path = std::env::temp_dir().join(format!(
            "spindle-sidebar-collapsed-mode-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\nsidebar_collapsed_mode = \"hidden\"\n").unwrap();
        assert_eq!(
            load_from(&path).sidebar_collapsed_mode,
            SidebarCollapsedMode::Hidden
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn tab_bar_position_matches_herdr_default_and_bottom_value() {
        assert_eq!(Config::default().tab_bar_position, TabBarPosition::Top);
        let path = std::env::temp_dir().join(format!(
            "spindle-tab-bar-position-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\ntab_bar_position = \"bottom\"\n").unwrap();
        assert_eq!(load_from(&path).tab_bar_position, TabBarPosition::Bottom);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn agent_panel_sort_matches_herdr_default_and_alias() {
        assert!(!Config::default().agent_priority_sort);
        let path = std::env::temp_dir().join(format!(
            "spindle-agent-panel-sort-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[ui]\nagent_panel_sort = \"priority\"\n").unwrap();
        assert!(load_from(&path).agent_priority_sort);
        std::fs::write(&path, "[ui]\nagent_panel_sort = \"workspaces\"\n").unwrap();
        assert!(!load_from(&path).agent_priority_sort);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn pane_border_settings_match_herdr_defaults_and_values() {
        let config = Config::default();
        assert_eq!(config.pane_borders, PaneBorders::Auto);
        assert!(config.pane_outer_borders);
        assert!(config.pane_gaps);
        assert!(config.pane_scrollbars);
        let path = std::env::temp_dir().join(format!(
            "spindle-pane-border-settings-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[ui]\npane_borders = \"always\"\npane_outer_borders = false\npane_gaps = false\npane_scrollbars = false\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.pane_borders, PaneBorders::Always);
        assert!(!config.pane_outer_borders);
        assert!(!config.pane_gaps);
        assert!(!config.pane_scrollbars);
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
    fn missing_onboarding_setting_matches_herdr_first_run_default() {
        assert_eq!(Config::default().onboarding, None);
        let path =
            std::env::temp_dir().join(format!("spindle-onboarding-{}.toml", std::process::id()));
        std::fs::write(&path, "[theme]\nname = \"nord\"\n").unwrap();
        assert_eq!(load_from(&path).onboarding, None);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn config_path_override_prefers_spindle_and_accepts_herdr() {
        assert_eq!(
            config_path_override(Some("C:/spindle.toml".into()), Some("C:/herdr.toml".into())),
            Some(std::path::PathBuf::from("C:/spindle.toml"))
        );
        assert_eq!(
            config_path_override(None, Some("C:/herdr.toml".into())),
            Some(std::path::PathBuf::from("C:/herdr.toml"))
        );
        assert_eq!(config_path_override(Some("".into()), None), None);
    }

    #[test]
    fn onboarding_completion_preserves_existing_config_sections() {
        let content = "[theme]\nname = \"nord\"\n\n[ui]\nmouse_capture = true\n";
        let updated = upsert_top_level_bool(content, "onboarding", false);
        assert!(updated.starts_with("onboarding = false\n"));
        assert!(updated.contains("[theme]\nname = \"nord\""));
        assert!(updated.contains("[ui]\nmouse_capture = true"));
    }

    #[test]
    fn onboarding_update_only_replaces_the_top_level_key() {
        let content = "[ui]\nonboarding = true\n";
        let updated = upsert_top_level_bool(content, "onboarding", false);
        assert!(updated.starts_with("onboarding = false\n"));
        assert!(updated.contains("[ui]\nonboarding = true"));
    }

    #[test]
    fn default_document_exposes_settings_and_reload_bindings() {
        let document = super::default_document();
        assert!(document.contains("settings = \"prefix+s\""));
        assert!(document.contains("reload_config = \"prefix+shift+r\""));
        assert!(document.contains("sidebar_width = 26"));
    }

    #[test]
    fn custom_popup_commands_accept_percentage_dimensions() {
        let path = std::env::temp_dir().join(format!(
            "spindle-custom-popup-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[[keys.command]]\nkey = \"prefix+p\"\ncommand = \"lazygit\"\ntype = \"popup\"\nwidth = 90\nheight = \"80%\"\n",
        )
        .unwrap();
        let config = load_from(&path);
        assert_eq!(config.custom_commands.len(), 1);
        assert_eq!(
            config.custom_commands[0].width,
            Some(crate::popup_size::PopupSize::Cells(90))
        );
        assert_eq!(
            config.custom_commands[0].height,
            Some(crate::popup_size::PopupSize::Percent(80))
        );
        std::fs::remove_file(path).unwrap();
    }
}
