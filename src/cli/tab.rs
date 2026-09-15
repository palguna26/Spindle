use super::Project;
use crate::server::session::{SessionSnapshot, TabView};
use std::collections::BTreeMap;
use std::io;

pub(super) fn run_tab_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command, options @ ..] if command == "list" => tab_list(project, options),
        [command, options @ ..] if command == "create" => tab_create(project, options),
        [command, id] if command == "get" => tab_get(project, id),
        [command, id] if command == "focus" => {
            send_mutation(project, "switch_tab", serde_json::json!({ "id": id }))
        }
        [command, id, label @ ..] if command == "rename" && !label.is_empty() => send_mutation(
            project,
            "rename_tab",
            serde_json::json!({ "id": id, "name": label.join(" ") }),
        ),
        [command, id] if command == "close" => {
            send_mutation(project, "close_tab", serde_json::json!({ "id": id }))
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(io::ErrorKind::InvalidInput, "usage: spindle tab <list|create [label]|get <id>|focus <id>|rename <id> <label>|close <id>>"))
        }
    }
}

fn get_snapshot(project: &Project) -> io::Result<SessionSnapshot> {
    let response = super::send_command(project, "get_snapshot")?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the snapshot request".into()),
        ));
    }
    serde_json::from_value(
        response
            .payload
            .ok_or_else(|| io::Error::other("server returned no session snapshot"))?,
    )
    .map_err(io::Error::other)
}

fn tab_list(project: &Project, args: &[String]) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let workspace_id = match args {
        [] => active_workspace_id(&snapshot)
            .ok_or_else(|| io::Error::other("active workspace does not exist"))?,
        [flag, id] if flag == "--workspace" => id.clone(),
        _ => {
            return Err(io::Error::other(
                "usage: spindle tab list [--workspace <workspace_id>]",
            ));
        }
    };
    let workspace = find_workspace(&snapshot, &workspace_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "workspace does not exist"))?;
    print!(
        "{}",
        format_tab_list(
            workspace.workspace_id.as_str(),
            &workspace.tabs,
            &workspace.active_tab_id
        )
    );
    Ok(())
}

fn format_tab_list(workspace_id: &str, tabs: &[TabView], active_id: &str) -> String {
    let mut output = String::new();
    for tab in tabs {
        let marker = if tab.tab_id == active_id { '*' } else { ' ' };
        output.push_str(&format!(
            "{marker} {}\t{}\t[{workspace_id}]\n",
            tab.tab_id, tab.name
        ));
    }
    if output.is_empty() {
        output.push_str("No tabs.\n");
    }
    output
}

fn tab_create(project: &Project, args: &[String]) -> io::Result<()> {
    let mut name = "Main".to_owned();
    let mut cwd = None;
    let mut workspace_id = None;
    let mut env = BTreeMap::new();
    let mut focus = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--cwd" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --cwd"));
                };
                cwd = Some(value.clone());
                index += 2;
            }
            "--workspace" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --workspace"));
                };
                workspace_id = Some(value.clone());
                index += 2;
            }
            "--label" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --label"));
                };
                name = value.clone();
                index += 2;
            }
            "--env" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --env"));
                };
                let (key, value) = super::parse_env_assignment(value)?;
                env.insert(key, value);
                index += 2;
            }
            "--focus" => {
                focus = true;
                index += 1;
            }
            "--no-focus" => {
                focus = false;
                index += 1;
            }
            value if !value.starts_with('-') && name == "Main" => {
                name = value.to_owned();
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(io::Error::other(format!("unknown option: {value}")));
            }
            value => return Err(io::Error::other(format!("unexpected argument: {value}"))),
        }
    }

    let before = get_snapshot(project)?;
    let previous_workspace_id = active_workspace_id(&before);
    let previous_tab_id = active_tab_id(&before);
    if let Some(target_workspace_id) = &workspace_id {
        let response = super::send_command_with_payload(
            project,
            "switch_workspace",
            serde_json::json!({ "id": target_workspace_id }),
        )?;
        if !response.ok {
            return Err(io::Error::other(
                response
                    .error
                    .map(|error| error.message)
                    .unwrap_or_else(|| "server rejected the workspace focus request".into()),
            ));
        }
    }
    let response = super::send_command_with_payload(
        project,
        "create_tab",
        serde_json::json!({ "name": name }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the tab create request".into()),
        ));
    }
    let snapshot = get_snapshot(project)?;
    let repository_path =
        active_workspace(&snapshot).and_then(|workspace| workspace.repository_path.clone());
    let pane = super::send_command_with_payload(
        project,
        "ensure_active_pane",
        serde_json::json!({
            "command": "powershell.exe",
            "args": ["-NoLogo", "-NoProfile"],
            "cwd": cwd.or(repository_path).or_else(|| std::env::current_dir().ok().map(|path| path.to_string_lossy().into_owned())),
            "env": env,
            "cols": 80,
            "rows": 24,
        }),
    )?;
    if !pane.ok {
        return Err(io::Error::other(
            pane.error
                .map(|error| error.message)
                .unwrap_or_else(|| "tab was created but its shell could not start".into()),
        ));
    }
    if !focus {
        if let Some(previous_workspace_id) = previous_workspace_id {
            let restore = super::send_command_with_payload(
                project,
                "switch_workspace",
                serde_json::json!({ "id": previous_workspace_id }),
            )?;
            if !restore.ok {
                return Err(io::Error::other(
                    "tab was created, but the previous workspace could not be restored",
                ));
            }
        }
        if let Some(previous_tab_id) = previous_tab_id {
            let restore = super::send_command_with_payload(
                project,
                "switch_tab",
                serde_json::json!({ "id": previous_tab_id }),
            )?;
            if !restore.ok {
                return Err(io::Error::other(
                    "tab was created, but the previous tab could not be restored",
                ));
            }
        }
    }
    if let Some(payload) = response.payload {
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(io::Error::other)?
        );
    }
    Ok(())
}

fn active_tab_id(snapshot: &SessionSnapshot) -> Option<String> {
    active_workspace(snapshot).map(|workspace| workspace.active_tab_id.clone())
}

fn active_workspace_id(snapshot: &SessionSnapshot) -> Option<String> {
    active_workspace(snapshot).map(|workspace| workspace.workspace_id.clone())
}

fn find_workspace<'a>(
    snapshot: &'a SessionSnapshot,
    workspace_id: &str,
) -> Option<&'a crate::server::session::WorkspaceView> {
    snapshot.spaces.iter().find_map(|space| {
        space
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
    })
}

fn active_workspace(snapshot: &SessionSnapshot) -> Option<&crate::server::session::WorkspaceView> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace_id = space.active_workspace_id.as_deref()?;
    space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
}

fn tab_get(project: &Project, id: &str) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let Some((space, workspace, tab, active)) = snapshot.spaces.iter().find_map(|space| {
        space.workspaces.iter().find_map(|workspace| {
            workspace
                .tabs
                .iter()
                .find(|tab| tab.tab_id == id)
                .map(|tab| {
                    (
                        space.name.as_str(),
                        workspace.workspace_id.as_str(),
                        tab,
                        workspace.active_tab_id == tab.tab_id
                            && space.space_id == snapshot.active_space_id,
                    )
                })
        })
    }) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("tab '{id}' does not exist"),
        ));
    };
    println!("tab: {}", tab.tab_id);
    println!("name: {}", tab.name);
    println!("workspace: {workspace}");
    println!("space: {space}");
    println!("active: {}", if active { "yes" } else { "no" });
    Ok(())
}

fn send_mutation(project: &Project, operation: &str, payload: serde_json::Value) -> io::Result<()> {
    let response = super::send_command_with_payload(project, operation, payload)?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the tab request".into()),
        ));
    }
    if let Some(payload) = response.payload {
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(io::Error::other)?
        );
    }
    Ok(())
}

fn print_help() {
    println!("Usage: spindle tab <list [--workspace ID]|create [label] [--label TEXT] [--workspace ID] [--cwd PATH] [--env KEY=VALUE] [--focus|--no-focus]|get <id>|focus <id>|rename <id> <label>|close <id>>");
    println!("  list             list tabs in the active or selected workspace");
    println!("  create [label]   create a tab and start its PowerShell pane (--label, --workspace, --cwd, --env, --focus|--no-focus)");
    println!("  get <id>         show a tab");
    println!("  focus <id>       focus a tab in the active workspace");
    println!("  rename <id> ...  rename a tab");
    println!("  close <id>       close a tab");
}

#[cfg(test)]
mod tests {
    use super::{find_workspace, format_tab_list};
    use crate::server::session::Session;

    #[test]
    fn tab_list_marks_the_active_tab() {
        let mut session = Session::default();
        let tab = session.create_tab("Logs".into()).unwrap();
        let tabs = &session.snapshot().spaces[0].workspaces[0].tabs;
        let active = tab["tab_id"].as_str().unwrap();
        let output = format_tab_list("workspace-1", tabs, active);
        assert!(output.contains("* tab-workspace-1-1\tLogs\t[workspace-1]"));
    }

    #[test]
    fn tab_env_assignments_match_herdr_rules() {
        assert_eq!(
            super::super::parse_env_assignment("SPINDLE_TAB=dev").unwrap(),
            ("SPINDLE_TAB".into(), "dev".into())
        );
        assert!(super::super::parse_env_assignment("missing-separator").is_err());
        assert!(super::super::parse_env_assignment("=empty-key").is_err());
    }

    #[test]
    fn tab_workspace_selector_finds_inactive_workspace_without_changing_snapshot() {
        let mut session = Session::default();
        let workspace = session.create_workspace("Build".into()).unwrap();
        let workspace_id = workspace["workspace_id"].as_str().unwrap();
        let snapshot = session.snapshot();

        assert_eq!(
            find_workspace(snapshot, workspace_id).unwrap().name,
            "Build"
        );
        assert_eq!(
            snapshot.spaces[0].active_workspace_id.as_deref(),
            Some("workspace-2")
        );
    }
}
