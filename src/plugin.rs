use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

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
    pub(crate) link_handlers: Vec<LinkHandler>,
    #[serde(default)]
    pub(crate) panes: Vec<Pane>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Action {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) title: String,
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LinkHandler {
    pub(crate) id: String,
    pub(crate) pattern: String,
    pub(crate) action: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Pane {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) title: String,
    pub(crate) command: Vec<String>,
    #[serde(default = "default_pane_placement")]
    pub(crate) placement: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Registration {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_pane_placement() -> String {
    "split".into()
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

fn safe_component(id: &str) -> String {
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
