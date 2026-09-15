use std::io;

use crate::server::session::SessionSnapshot;

pub(crate) fn launch_action(action_arg: &str, snapshot: &SessionSnapshot) -> io::Result<()> {
    let (requested_plugin, action_id) = action_arg
        .rsplit_once('.')
        .map_or((None, action_arg), |(plugin, action)| {
            (Some(plugin), action)
        });
    let mut matches = Vec::new();
    for (registration, manifest) in crate::plugin::installed()? {
        if !registration.enabled
            || !manifest.enabled
            || !crate::plugin::supports_windows(manifest.platforms.as_deref())
            || requested_plugin.is_some_and(|id| id != manifest.id)
        {
            continue;
        }
        for action in manifest.actions {
            if action.id == action_id
                && crate::plugin::supports_windows(action.platforms.as_deref())
            {
                matches.push((registration.clone(), manifest.id.clone(), action));
            }
        }
    }
    if matches.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            if matches.is_empty() {
                format!("plugin action '{action_arg}' was not found")
            } else {
                format!("plugin action '{action_arg}' is ambiguous; use plugin.id.action")
            },
        ));
    }
    let (registration, manifest_id, action) = matches.remove(0);
    let Some(command) = action.command.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "plugin action command is empty",
        ));
    };
    let (config_dir, state_dir) = crate::plugin::ensure_user_dirs(&manifest_id)?;
    let context = action_context(&manifest_id, &action.id, snapshot)?;
    let action_args = action.command.iter().skip(1).cloned().collect::<Vec<_>>();
    let child =
        crate::plugin_command::command_for_argv_in_dir(command, &action_args, &registration.path)
            .env("SPINDLE_PLUGIN_ID", &manifest_id)
            .env("SPINDLE_PLUGIN_ROOT", &registration.path)
            .env("SPINDLE_PLUGIN_CONFIG_DIR", &config_dir)
            .env("SPINDLE_PLUGIN_STATE_DIR", &state_dir)
            .env("SPINDLE_PLUGIN_CONTEXT_JSON", &context)
            .env("SPINDLE_PLUGIN_ACTION_ID", &action.id)
            .env(
                "SPINDLE_PLUGIN_PANE_ID",
                context_field(&context, "focused_pane_id"),
            )
            .env(
                "SPINDLE_PLUGIN_CWD",
                context_field(&context, "focused_pane_cwd"),
            )
            .env("HERDR_PLUGIN_ID", &manifest_id)
            .env("HERDR_PLUGIN_ROOT", &registration.path)
            .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
            .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
            .env("HERDR_PLUGIN_CONTEXT_JSON", &context)
            .env("HERDR_PLUGIN_ACTION_ID", &action.id)
            .env("HERDR_PANE_ID", context_field(&context, "focused_pane_id"))
            .env(
                "HERDR_WORKSPACE_ID",
                context_field(&context, "workspace_id"),
            )
            .env("HERDR_TAB_ID", context_field(&context, "tab_id"))
            .env(
                "HERDR_PLUGIN_CWD",
                context_field(&context, "focused_pane_cwd"),
            )
            .spawn()?;
    let _ = crate::plugin::record_launch(&manifest_id, "palette-action", &action.id, child.id());
    Ok(())
}

fn action_context(
    plugin_id: &str,
    action_id: &str,
    snapshot: &SessionSnapshot,
) -> io::Result<String> {
    let mut context = serde_json::json!({
        "source": "palette",
        "plugin_id": plugin_id,
        "action_id": action_id,
        "cwd": std::env::current_dir()?.display().to_string(),
    });
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    else {
        return Ok(context.to_string());
    };
    let Some(workspace_id) = space.active_workspace_id.as_deref() else {
        return Ok(context.to_string());
    };
    let Some(workspace) = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
    else {
        return Ok(context.to_string());
    };
    context["workspace_id"] = serde_json::json!(workspace.workspace_id);
    context["workspace_label"] = serde_json::json!(workspace.name);
    context["workspace_cwd"] = serde_json::json!(workspace.repository_path);
    context["tab_id"] = serde_json::json!(workspace.active_tab_id);
    if let Some(tab) = workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)
    {
        context["tab_label"] = serde_json::json!(tab.name);
    }
    if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
        context["focused_pane_id"] = serde_json::json!(pane_id);
        if let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) {
            context["focused_pane_cwd"] = serde_json::json!(pane.cwd);
        }
    }
    Ok(context.to_string())
}

fn context_field(context: &str, field: &str) -> String {
    serde_json::from_str::<serde_json::Value>(context)
        .ok()
        .and_then(|value| {
            value
                .get(field)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

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
    use super::{action_context, link_context};

    #[test]
    fn action_context_contains_active_workspace_tab_and_pane() {
        let mut session = crate::server::session::Session::default();
        let workspace_id = session.create_workspace("workspace".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let tab_id = session.create_tab("tab".into()).unwrap()["tab_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let snapshot = session.snapshot();

        let context = action_context("example.plugin", "open", snapshot).unwrap();
        let value: serde_json::Value = serde_json::from_str(&context).unwrap();
        assert_eq!(value["source"], "palette");
        assert_eq!(value["plugin_id"], "example.plugin");
        assert_eq!(value["action_id"], "open");
        assert_eq!(value["workspace_id"], workspace_id);
        assert_eq!(value["tab_id"], tab_id);
    }

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
