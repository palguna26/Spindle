use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use crate::plugin::{self, Manifest};
use crate::protocol::Event;
use crate::server::session::SessionSnapshot;

pub(super) fn run_startup_hooks(snapshot: &SessionSnapshot, endpoint: &str) {
    let Ok(plugins) = plugin::installed() else {
        return;
    };
    for (registration, manifest) in plugins {
        if !registration.enabled
            || !manifest.enabled
            || !plugin::supports_windows(manifest.platforms.as_deref())
        {
            continue;
        }
        for (index, startup) in manifest.startup.iter().enumerate() {
            if !plugin::supports_windows(startup.platforms.as_deref()) {
                continue;
            }
            if let Err(error) = launch_startup(
                &manifest,
                &registration.path,
                startup.command.as_slice(),
                snapshot,
                endpoint,
            ) {
                eprintln!(
                    "Spindle plugin startup hook {}.{} failed: {error}",
                    manifest.id,
                    index + 1
                );
            }
        }
    }
}

pub(super) fn run_event_hook(
    event: &Event<serde_json::Value>,
    snapshot: &SessionSnapshot,
    endpoint: &str,
) {
    let Some(hook_name) = event_hook_name(event) else {
        return;
    };
    let Ok(plugins) = plugin::installed() else {
        return;
    };
    for (registration, manifest) in plugins {
        if !registration.enabled
            || !manifest.enabled
            || !plugin::supports_windows(manifest.platforms.as_deref())
        {
            continue;
        }
        for (index, hook) in manifest.events.iter().enumerate() {
            if hook.on != hook_name || !plugin::supports_windows(hook.platforms.as_deref()) {
                continue;
            }
            if let Err(error) = launch_event(
                &manifest,
                &registration.path,
                hook.command.as_slice(),
                event,
                snapshot,
                endpoint,
                hook_name,
            ) {
                eprintln!(
                    "Spindle plugin event hook {}.{} failed: {error}",
                    manifest.id,
                    index + 1
                );
            }
        }
    }
}

fn event_hook_name(event: &Event<serde_json::Value>) -> Option<&'static str> {
    match event.event.as_str() {
        "pane_created" => "pane.created",
        "tab_created" => "tab.created",
        "tab_closed" => "tab.closed",
        "tab_renamed" => "tab.renamed",
        "tab_focused" => "tab.focused",
        "workspace_focused" => "workspace.focused",
        "workspace_created" => "workspace.created",
        "workspace_updated" => "workspace.updated",
        "workspace_renamed" => "workspace.renamed",
        "workspace_closed" => "workspace.closed",
        "worktree_created" => "worktree.created",
        "worktree_opened" => "worktree.opened",
        "worktree_removed" => "worktree.removed",
        "pane_focused" => "pane.focused",
        "pane_moved" => "pane.moved",
        "pane_updated" => "pane.updated",
        "layout_updated" => "layout.updated",
        "pane_agent_detected" => "pane.agent_detected",
        "pane_agent_status_changed" => "pane.agent_status_changed",
        "pane_closed" => "pane.closed",
        "pane_status"
            if event
                .payload
                .get("status")
                .and_then(|status| status.get("Running"))
                .is_none() =>
        {
            "pane.exited"
        }
        _ => return None,
    }
    .into()
}

fn launch_event(
    manifest: &Manifest,
    root: &Path,
    argv: &[String],
    event: &Event<serde_json::Value>,
    snapshot: &SessionSnapshot,
    endpoint: &str,
    hook_name: &str,
) -> std::io::Result<()> {
    let Some(program) = argv.first() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "plugin event command is empty",
        ));
    };
    let (config_dir, state_dir) = plugin::ensure_user_dirs(&manifest.id)?;
    let mut context =
        serde_json::from_str::<serde_json::Value>(&startup_context(&manifest.id, snapshot)?)
            .map_err(std::io::Error::other)?;
    context["source"] = serde_json::json!("event");
    context["event"] = serde_json::json!(hook_name);
    context["event_payload"] = event.payload.clone();
    if hook_name.starts_with("worktree.") {
        context["worktree"] = event.payload.clone();
    }
    let context = context.to_string();
    let args = argv.iter().skip(1).cloned().collect::<Vec<_>>();
    let child = crate::plugin_command::command_for_argv_in_dir(program, &args, root)
        .envs(startup_environment(
            &manifest.id,
            root,
            &config_dir,
            &state_dir,
            &context,
            endpoint,
        ))
        .env("SPINDLE_PLUGIN_EVENT", hook_name)
        .env("HERDR_PLUGIN_EVENT", hook_name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let _ = plugin::record_launch(&manifest.id, "event", hook_name, child.id());
    Ok(())
}

fn launch_startup(
    manifest: &Manifest,
    root: &Path,
    argv: &[String],
    snapshot: &SessionSnapshot,
    endpoint: &str,
) -> std::io::Result<()> {
    let Some(program) = argv.first() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "plugin startup command is empty",
        ));
    };
    let (config_dir, state_dir) = plugin::ensure_user_dirs(&manifest.id)?;
    let context = startup_context(&manifest.id, snapshot)?;
    let args = argv.iter().skip(1).cloned().collect::<Vec<_>>();
    let child = crate::plugin_command::command_for_argv_in_dir(program, &args, root)
        .envs(startup_environment(
            &manifest.id,
            root,
            &config_dir,
            &state_dir,
            &context,
            endpoint,
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let _ = plugin::record_launch(&manifest.id, "startup", "startup", child.id());
    Ok(())
}

fn startup_environment(
    plugin_id: &str,
    root: &Path,
    config_dir: &Path,
    state_dir: &Path,
    context: &str,
    endpoint: &str,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let values = [
        ("SPINDLE_PLUGIN_ID", plugin_id.to_owned()),
        ("SPINDLE_PLUGIN_ROOT", root.display().to_string()),
        (
            "SPINDLE_PLUGIN_CONFIG_DIR",
            config_dir.display().to_string(),
        ),
        ("SPINDLE_PLUGIN_STATE_DIR", state_dir.display().to_string()),
        ("SPINDLE_PLUGIN_CONTEXT_JSON", context.to_owned()),
        ("SPINDLE_PLUGIN_EVENT", "startup".into()),
        ("SPINDLE_SOCKET_PATH", endpoint.into()),
        ("HERDR_PLUGIN_ID", plugin_id.to_owned()),
        ("HERDR_PLUGIN_ROOT", root.display().to_string()),
        ("HERDR_PLUGIN_CONFIG_DIR", config_dir.display().to_string()),
        ("HERDR_PLUGIN_STATE_DIR", state_dir.display().to_string()),
        ("HERDR_PLUGIN_CONTEXT_JSON", context.to_owned()),
        ("HERDR_PLUGIN_EVENT", "startup".into()),
        ("HERDR_SOCKET_PATH", endpoint.into()),
    ];
    env.extend(values.into_iter().map(|(name, value)| (name.into(), value)));
    for (name, field) in [
        ("SPINDLE_PANE_ID", "focused_pane_id"),
        ("SPINDLE_WORKSPACE_ID", "workspace_id"),
        ("SPINDLE_TAB_ID", "tab_id"),
        ("SPINDLE_PLUGIN_CWD", "focused_pane_cwd"),
        ("HERDR_PANE_ID", "focused_pane_id"),
        ("HERDR_WORKSPACE_ID", "workspace_id"),
        ("HERDR_TAB_ID", "tab_id"),
        ("HERDR_PLUGIN_CWD", "focused_pane_cwd"),
    ] {
        env.insert(name.into(), context_field(context, field));
    }
    env
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

fn startup_context(plugin_id: &str, snapshot: &SessionSnapshot) -> std::io::Result<String> {
    let mut context = serde_json::json!({
        "source": "startup",
        "plugin_id": plugin_id,
        "cwd": std::env::current_dir()?.display().to_string(),
    });
    if let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    {
        if let Some(workspace_id) = space.active_workspace_id.as_deref() {
            context["workspace_id"] = serde_json::json!(workspace_id);
            if let Some(workspace) = space
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == workspace_id)
            {
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
            }
        }
    }
    if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
        context["focused_pane_id"] = serde_json::json!(pane_id);
        if let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) {
            context["focused_pane_cwd"] = serde_json::json!(pane.cwd);
        }
    }
    Ok(context.to_string())
}

#[cfg(test)]
mod tests {
    use super::{event_hook_name, startup_context, startup_environment};
    use crate::protocol::Event;
    use std::path::Path;

    #[test]
    fn startup_context_and_environment_use_herdr_names() {
        let snapshot = crate::server::session::Session::default()
            .snapshot()
            .clone();
        let context = startup_context("example.startup", &snapshot).unwrap();
        let env = startup_environment(
            "example.startup",
            Path::new("plugin"),
            Path::new("config"),
            Path::new("state"),
            &context,
            "\\\\.\\pipe\\spindle",
        );
        assert_eq!(env["HERDR_PLUGIN_ID"], "example.startup");
        assert_eq!(env["HERDR_PLUGIN_EVENT"], "startup");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&context).unwrap()["source"],
            "startup"
        );
    }

    #[test]
    fn tab_rename_events_use_herdr_hook_names() {
        let event = Event {
            version: crate::protocol::PROTOCOL_VERSION,
            sequence: 1,
            event: "tab_renamed".into(),
            payload: serde_json::json!({ "tab_id": "tab-1" }),
        };
        assert_eq!(event_hook_name(&event), Some("tab.renamed"));
    }

    #[test]
    fn pane_update_events_use_herdr_hook_names() {
        let event = Event {
            version: crate::protocol::PROTOCOL_VERSION,
            sequence: 1,
            event: "pane_updated".into(),
            payload: serde_json::json!({ "pane_id": "pane-1", "label": "Shell" }),
        };
        assert_eq!(event_hook_name(&event), Some("pane.updated"));
    }

    #[test]
    fn workspace_update_events_use_herdr_hook_names() {
        let event = Event {
            version: crate::protocol::PROTOCOL_VERSION,
            sequence: 1,
            event: "workspace_updated".into(),
            payload: serde_json::json!({ "workspace_id": "workspace-1" }),
        };
        assert_eq!(event_hook_name(&event), Some("workspace.updated"));
    }

    #[test]
    fn worktree_events_use_herdr_hook_names() {
        for (event, hook) in [
            ("worktree_created", "worktree.created"),
            ("worktree_opened", "worktree.opened"),
            ("worktree_removed", "worktree.removed"),
        ] {
            let event = Event {
                version: crate::protocol::PROTOCOL_VERSION,
                sequence: 1,
                event: event.into(),
                payload: serde_json::json!({ "path": "C:/repo" }),
            };
            assert_eq!(event_hook_name(&event), Some(hook));
        }
    }
}
