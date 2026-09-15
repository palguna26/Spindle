use std::io;

pub(crate) fn launch_for_url(url: &str) -> io::Result<bool> {
    for (registration, manifest) in crate::plugin::installed()? {
        if !registration.enabled || !manifest.enabled {
            continue;
        }
        for handler in &manifest.link_handlers {
            let Ok(pattern) = regex::Regex::new(&handler.pattern) else {
                continue;
            };
            if !pattern.is_match(url) {
                continue;
            }
            let Some(action) = manifest.actions.iter().find(|a| a.id == handler.action) else {
                continue;
            };
            let Some(command) = action.command.first() else {
                continue;
            };
            std::process::Command::new(command)
                .args(action.command.iter().skip(1))
                .current_dir(&registration.path)
                .env("SPINDLE_PLUGIN_ID", &manifest.id)
                .env("SPINDLE_PLUGIN_ROOT", &registration.path)
                .env("SPINDLE_PLUGIN_CLICKED_URL", url)
                .env("SPINDLE_PLUGIN_LINK_HANDLER_ID", &handler.id)
                .env("HERDR_PLUGIN_ID", &manifest.id)
                .env("HERDR_PLUGIN_ROOT", &registration.path)
                .env("HERDR_PLUGIN_CLICKED_URL", url)
                .env("HERDR_PLUGIN_LINK_HANDLER_ID", &handler.id)
                .spawn()?;
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn herdr_manifest_shape_loads_link_handlers_and_actions() {
        let manifest = toml::from_str::<crate::plugin::Manifest>(
            r#"
id = "example.links"
name = "Links"
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
        assert_eq!(manifest.id, "example.links");
        assert_eq!(manifest.actions[0].command, ["tool", "--open"]);
    }
}
