use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Manifest {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) version: String,
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) actions: Vec<Action>,
    #[serde(default)]
    pub(crate) build: Vec<Build>,
    #[serde(default)]
    pub(crate) link_handlers: Vec<LinkHandler>,
    #[serde(default)]
    pub(crate) panes: Vec<Pane>,
    #[serde(default)]
    pub(crate) platforms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Build {
    pub(crate) command: Vec<String>,
    #[serde(default)]
    pub(crate) platforms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Action {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) platforms: Option<Vec<String>>,
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LinkHandler {
    pub(crate) id: String,
    pub(crate) pattern: String,
    pub(crate) action: String,
    #[serde(default)]
    pub(crate) platforms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Pane {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) title: String,
    pub(crate) command: Vec<String>,
    #[serde(default = "default_pane_placement")]
    pub(crate) placement: String,
    #[serde(default)]
    pub(crate) platforms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Registration {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) managed: bool,
    #[serde(default)]
    pub(crate) source: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LaunchLog {
    pub(crate) timestamp: u64,
    pub(crate) plugin_id: String,
    pub(crate) kind: String,
    pub(crate) command_id: String,
    pub(crate) pid: u32,
}

fn default_enabled() -> bool {
    true
}

fn default_pane_placement() -> String {
    // Herdr opens manifest panes as temporary zoomed overlays unless a
    // manifest opts into split, tab, zoomed, or popup placement.
    "overlay".into()
}

pub(crate) fn root() -> io::Result<PathBuf> {
    let Some(app_data) = std::env::var_os("APPDATA") else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "APPDATA is not set",
        ));
    };
    Ok(PathBuf::from(app_data).join("Spindle").join("plugins"))
}

pub(crate) fn managed_path(source: &str) -> io::Result<PathBuf> {
    Ok(root()?.join("github").join(safe_component(source)))
}

pub(crate) fn config_dir(id: &str) -> io::Result<PathBuf> {
    Ok(root()?.join("config").join(safe_component(id)))
}

pub(crate) fn state_dir(id: &str) -> io::Result<PathBuf> {
    Ok(root()?.join("state").join(safe_component(id)))
}

pub(crate) fn ensure_user_dirs(id: &str) -> io::Result<(PathBuf, PathBuf)> {
    let config = config_dir(id)?;
    let state = state_dir(id)?;
    std::fs::create_dir_all(&config)?;
    std::fs::create_dir_all(&state)?;
    Ok((config, state))
}

pub(crate) fn safe_component(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn supports_windows(platforms: Option<&[String]>) -> bool {
    platforms.is_none_or(|values| values.iter().any(|value| value == "windows"))
}

pub(crate) fn read_registry() -> io::Result<Vec<Registration>> {
    let Ok(content) = std::fs::read_to_string(root()?.join("registry.json")) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&content).map_err(io::Error::other)
}

pub(crate) fn write_registry(registrations: &[Registration]) -> io::Result<()> {
    let root = root()?;
    std::fs::create_dir_all(&root)?;
    let content = serde_json::to_vec_pretty(registrations).map_err(io::Error::other)?;
    std::fs::write(root.join("registry.json"), content)
}

pub(crate) fn record_launch(
    plugin_id: &str,
    kind: &str,
    command_id: &str,
    pid: u32,
) -> io::Result<()> {
    let root = root()?;
    std::fs::create_dir_all(&root)?;
    let path = root.join("launch-log.json");
    let mut entries = std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str::<Vec<LaunchLog>>(&content).ok())
        .unwrap_or_default();
    entries.push(LaunchLog {
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        plugin_id: plugin_id.into(),
        kind: kind.into(),
        command_id: command_id.into(),
        pid,
    });
    if entries.len() > 200 {
        entries.drain(..entries.len() - 200);
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&entries).map_err(io::Error::other)?,
    )
}

pub(crate) fn launch_log() -> io::Result<Vec<LaunchLog>> {
    let Ok(content) = std::fs::read_to_string(root()?.join("launch-log.json")) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&content).map_err(io::Error::other)
}

pub(crate) fn manifest_path(path: &Path) -> PathBuf {
    if path.is_file() {
        path.to_path_buf()
    } else {
        path.join("herdr-plugin.toml")
    }
}

pub(crate) fn load(path: &Path) -> io::Result<Manifest> {
    let content = std::fs::read_to_string(manifest_path(path))?;
    let manifest = toml::from_str::<Manifest>(&content).map_err(io::Error::other)?;
    if manifest.id.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "plugin manifest id cannot be empty",
        ));
    }
    Ok(manifest)
}

pub(crate) fn installed() -> io::Result<Vec<(Registration, Manifest)>> {
    let mut registrations = read_registry()?;
    registrations.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(registrations
        .into_iter()
        .filter_map(|registration| {
            load(&registration.path)
                .ok()
                .map(|manifest| (registration, manifest))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::load;

    #[test]
    fn manifest_loads_herdr_build_commands() {
        let root =
            std::env::temp_dir().join(format!("spindle-plugin-build-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("herdr-plugin.toml"),
            "id = \"example.build\"\nname = \"Build\"\nversion = \"1\"\n[[build]]\ncommand = [\"tool\", \"run\"]\nplatforms = [\"windows\"]\n",
        )
        .unwrap();
        let manifest = load(&root).unwrap();
        assert_eq!(manifest.build[0].command, ["tool", "run"]);
        assert_eq!(manifest.build[0].platforms, Some(vec!["windows".into()]));
        let _ = std::fs::remove_dir_all(root);
    }
}
