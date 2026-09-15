use serde::Deserialize;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    actions: Vec<Action>,
    #[serde(default)]
    link_handlers: Vec<LinkHandler>,
}

#[derive(Debug, Deserialize)]
struct Action {
    id: String,
    command: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LinkHandler {
    id: String,
    pattern: String,
    action: String,
}

fn default_enabled() -> bool {
    true
}

pub(crate) fn launch_for_url(url: &str) -> io::Result<bool> {
    for (root, manifest) in installed_manifests()? {
        if !manifest.enabled {
            continue;
        }
        for handler in &manifest.link_handlers {
            let Ok(pattern) = regex::Regex::new(&handler.pattern) else {
                continue;
            };
            if !pattern.is_match(url) {
                continue;
            }
            let Some(action) = manifest
                .actions
                .iter()
                .find(|action| action.id == handler.action)
            else {
                continue;
            };
            let Some(command) = action.command.first() else {
                continue;
            };
            std::process::Command::new(command)
                .args(action.command.iter().skip(1))
                .current_dir(&root)
                .env("SPINDLE_PLUGIN_CLICKED_URL", url)
                .env("SPINDLE_PLUGIN_LINK_HANDLER_ID", &handler.id)
                .env("HERDR_PLUGIN_CLICKED_URL", url)
                .env("HERDR_PLUGIN_LINK_HANDLER_ID", &handler.id)
                .spawn()?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn installed_manifests() -> io::Result<Vec<(PathBuf, Manifest)>> {
    let Some(app_data) = std::env::var_os("APPDATA") else {
        return Ok(Vec::new());
    };
    let root = PathBuf::from(app_data).join("Spindle").join("plugins");
    let mut paths = std::fs::read_dir(&root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths
        .into_iter()
        .filter_map(|path| {
            let manifest_path = path.join("herdr-plugin.toml");
            let content = std::fs::read_to_string(manifest_path).ok()?;
            let manifest = toml::from_str(&content).ok()?;
            Some((path, manifest))
        })
        .collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::Manifest;

    #[test]
    fn herdr_manifest_shape_loads_link_handlers_and_actions() {
        let manifest: Manifest = toml::from_str(
            r#"
enabled = true
[[actions]]
id = "open"
command = ["tool", "--open"]
[[link_handlers]]
id = "issue"
title = "Open issue"
pattern = "^https://example\\.test/"
action = "open"
"#,
        )
        .unwrap();

        assert!(manifest.enabled);
        assert_eq!(manifest.actions[0].command, ["tool", "--open"]);
        assert!(regex::Regex::new(&manifest.link_handlers[0].pattern)
            .unwrap()
            .is_match("https://example.test/42"));
    }
}
