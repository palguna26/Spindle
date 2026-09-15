use std::io;

pub(crate) fn launch_for_url(
    url: &str,
    pane_id: Option<&str>,
    pane_cwd: Option<&str>,
) -> io::Result<bool> {
    for (registration, manifest) in crate::plugin::installed()? {
        if !registration.enabled
            || !manifest.enabled
            || !crate::plugin::supports_windows(manifest.platforms.as_deref())
        {
            continue;
        }
        for handler in &manifest.link_handlers {
            if !crate::plugin::supports_windows(handler.platforms.as_deref()) {
                continue;
            }
            let Ok(pattern) = regex::Regex::new(&handler.pattern) else {
                continue;
            };
            if !pattern.is_match(url) {
                continue;
            }
            let Some(action) = manifest.actions.iter().find(|a| {
                a.id == handler.action && crate::plugin::supports_windows(a.platforms.as_deref())
            }) else {
                continue;
            };
            let Some(command) = action.command.first() else {
                continue;
            };
            let (config_dir, state_dir) = crate::plugin::ensure_user_dirs(&manifest.id)?;
            let context = link_context(&manifest.id, &handler.id, url, pane_id, pane_cwd);
            let action_args = action.command.iter().skip(1).cloned().collect::<Vec<_>>();
            let child = crate::plugin_command::command_for_argv_in_dir(
                command,
                &action_args,
                &registration.path,
            )
            .env("SPINDLE_PLUGIN_ID", &manifest.id)
            .env("SPINDLE_PLUGIN_ROOT", &registration.path)
            .env("SPINDLE_PLUGIN_CONFIG_DIR", &config_dir)
            .env("SPINDLE_PLUGIN_STATE_DIR", &state_dir)
            .env("SPINDLE_PLUGIN_CONTEXT_JSON", &context)
            .env("SPINDLE_PLUGIN_CLICKED_URL", url)
            .env("SPINDLE_PLUGIN_LINK_HANDLER_ID", &handler.id)
            .env("SPINDLE_PLUGIN_PANE_ID", pane_id.unwrap_or_default())
            .env("SPINDLE_PLUGIN_CWD", pane_cwd.unwrap_or_default())
            .env("HERDR_PLUGIN_ID", &manifest.id)
            .env("HERDR_PLUGIN_ROOT", &registration.path)
            .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
            .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
            .env("HERDR_PLUGIN_CONTEXT_JSON", &context)
            .env("HERDR_PLUGIN_CLICKED_URL", url)
            .env("HERDR_PLUGIN_LINK_HANDLER_ID", &handler.id)
            .env("HERDR_PLUGIN_PANE_ID", pane_id.unwrap_or_default())
            .env("HERDR_PLUGIN_CWD", pane_cwd.unwrap_or_default())
            .spawn()?;
            let _ = crate::plugin::record_launch(&manifest.id, "link", &handler.id, child.id());
            return Ok(true);
        }
    }
    Ok(false)
}

fn link_context(
    plugin_id: &str,
    handler_id: &str,
    url: &str,
    pane_id: Option<&str>,
    pane_cwd: Option<&str>,
) -> String {
    serde_json::json!({
        "source": "link",
        "plugin_id": plugin_id,
        "link_handler_id": handler_id,
        "url": url,
        "pane_id": pane_id,
        "cwd": pane_cwd,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::link_context;

    #[test]
    fn link_context_contains_the_invoking_pane_and_cwd() {
        let context = link_context(
            "example.links",
            "issue",
            "https://example.test/42",
            Some("pane-7"),
            Some("C:/repo"),
        );
        let value: serde_json::Value = serde_json::from_str(&context).unwrap();
        assert_eq!(value["pane_id"], "pane-7");
        assert_eq!(value["cwd"], "C:/repo");
    }

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

    #[test]
    fn platform_filters_match_windows_manifest_values() {
        assert!(crate::plugin::supports_windows(Some(&["windows".into()])));
        assert!(!crate::plugin::supports_windows(Some(&["linux".into()])));
        assert!(crate::plugin::supports_windows(None));
    }
}
