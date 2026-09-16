use super::Project;
use crate::server::session::SessionSnapshot;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;

pub(super) fn run_workspace_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => workspace_list(project),
        [command, options @ ..] if command == "create" => workspace_create(project, options),
        [command, workspace_id] if command == "get" => workspace_get(project, workspace_id),
        [command, workspace_id] if command == "focus" => workspace_focus(project, workspace_id),
        [command, workspace_id, options @ ..] if command == "report-metadata" => {
            workspace_report_metadata(project, workspace_id, options)
        }
        [command, workspace_id, label @ ..] if command == "rename" && !label.is_empty() => {
            workspace_rename(project, workspace_id, &label.join(" "))
        }
        [command, workspace_id, index] if command == "move" => {
            let insert_index = index.parse::<usize>().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "workspace move index must be a non-negative integer",
                )
            })?;
            workspace_move(project, workspace_id, insert_index)
        }
        [command, workspace_id] if command == "close" => {
            workspace_close(project, workspace_id, false)
        }
        [command, workspace_id, flag] if command == "close" && flag == "--group" => {
            workspace_close(project, workspace_id, true)
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle workspace <list|create|get <workspace_id>|focus <workspace_id>|report-metadata <workspace_id>|move <workspace_id> <insert-index>|rename <workspace_id> <label>|close <workspace_id> [--group]>",
            ))
        }
    }
}

fn workspace_report_metadata(
    project: &Project,
    workspace_id: &str,
    args: &[String],
) -> io::Result<()> {
    let mut source = None;
    let mut tokens = serde_json::Map::new();
    let mut ttl_ms = None;
    let mut seq = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--source" | "--token" | "--clear-token" | "--ttl-ms" | "--seq" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| io::Error::other(format!("{} requires a value", args[index])))?;
                match args[index].as_str() {
                    "--source" => source = Some(value.clone()),
                    "--token" => {
                        let (key, token) = value
                            .split_once('=')
                            .ok_or_else(|| io::Error::other("--token must be KEY=VALUE"))?;
                        tokens.insert(key.to_owned(), serde_json::Value::String(token.to_owned()));
                    }
                    "--clear-token" => {
                        tokens.insert(value.clone(), serde_json::Value::Null);
                    }
                    "--ttl-ms" => {
                        let ttl = value.parse::<u64>().map_err(|_| {
                            io::Error::other("--ttl-ms must be an unsigned integer")
                        })?;
                        if ttl == 0 || ttl > 86_400_000 {
                            return Err(io::Error::other(
                                "--ttl-ms must be between 1 and 86400000",
                            ));
                        }
                        ttl_ms = Some(ttl);
                    }
                    "--seq" => {
                        seq =
                            Some(value.parse::<u64>().map_err(|_| {
                                io::Error::other("--seq must be an unsigned integer")
                            })?);
                    }
                    _ => unreachable!(),
                }
                index += 2;
            }
            option => return Err(io::Error::other(format!("unknown option: {option}"))),
        }
    }
    let source = source.ok_or_else(|| io::Error::other("missing required --source"))?;
    if tokens.is_empty() {
        return Err(io::Error::other("provide metadata to set or clear"));
    }
    let response = super::send_command_with_payload(
        project,
        "report_workspace_metadata",
        serde_json::json!({
            "workspace_id": workspace_id,
            "source": source,
            "tokens": tokens,
            "ttl_ms": ttl_ms,
            "seq": seq,
        }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the workspace metadata report".into()),
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

fn workspace_create(project: &Project, args: &[String]) -> io::Result<()> {
    let mut name = "Workspace".to_owned();
    let mut cwd = None;
    // Herdr creates the workspace without changing focus unless --focus is given.
    let mut focus = false;
    let mut env = BTreeMap::new();
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
            "--label" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --label"));
                };
                name = value.clone();
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
            "--env" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --env"));
                };
                let (key, parsed) = super::parse_env_assignment(value)?;
                env.insert(key, parsed);
                index += 2;
            }
            value if !value.starts_with('-') && name == "Workspace" => {
                name = value.to_owned();
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(io::Error::other(format!("unknown option: {value}")));
            }
            value => return Err(io::Error::other(format!("unexpected argument: {value}"))),
        }
    }
    let cwd = cwd.or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
    });
    let before = get_snapshot(project)?;
    let previous_workspace_id = before
        .spaces
        .iter()
        .find(|space| space.space_id == before.active_space_id)
        .and_then(|space| space.active_workspace_id.clone());
    let response = super::send_command_with_payload(
        project,
        "create_workspace",
        serde_json::json!({
            "name": name,
            "repository_path": cwd.clone(),
        }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the workspace create request".into()),
        ));
    }
    let snapshot = get_snapshot(project)?;
    let repository_path = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .and_then(|space| {
            space.active_workspace_id.as_deref().and_then(|id| {
                space
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == id)
            })
        })
        .and_then(|workspace| workspace.repository_path.clone())
        .or(cwd);
    let pane = super::send_command_with_payload(
        project,
        "ensure_active_pane",
        serde_json::json!({
            "command": "powershell.exe",
            "args": ["-NoLogo", "-NoProfile"],
            "cwd": repository_path,
            "env": env,
            "cols": 80,
            "rows": 24,
        }),
    )?;
    if !pane.ok {
        return Err(io::Error::other(
            pane.error
                .map(|error| error.message)
                .unwrap_or_else(|| "workspace was created but its shell could not start".into()),
        ));
    }
    if !focus {
        if let Some(previous_workspace_id) = previous_workspace_id {
            let restore = super::send_command_with_payload(
                project,
                "focus_workspace",
                serde_json::json!({ "id": previous_workspace_id }),
            )?;
            if !restore.ok {
                return Err(io::Error::other(
                    "workspace was created, but the previous workspace could not be restored",
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

fn workspace_close(project: &Project, workspace_id: &str, group: bool) -> io::Result<()> {
    let response = super::send_command_with_payload(
        project,
        "delete_workspace",
        serde_json::json!({ "id": workspace_id, "group": group }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the workspace close request".into()),
        ));
    }
    println!("closed workspace: {workspace_id}");
    Ok(())
}

fn workspace_focus(project: &Project, workspace_id: &str) -> io::Result<()> {
    let response = super::send_command_with_payload(
        project,
        "focus_workspace",
        serde_json::json!({ "id": workspace_id }),
    )?;
    if !response.ok {
        let message = response
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "server rejected the workspace focus request".into());
        return Err(io::Error::other(message));
    }
    println!("focused workspace: {workspace_id}");
    Ok(())
}

fn workspace_get(project: &Project, workspace_id: &str) -> io::Result<()> {
    let response = super::send_command(project, "get_snapshot")?;
    if !response.ok {
        let message = response
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "server rejected the workspace lookup request".into());
        return Err(io::Error::other(message));
    }
    let snapshot: SessionSnapshot = serde_json::from_value(
        response
            .payload
            .ok_or_else(|| io::Error::other("server returned no session snapshot"))?,
    )
    .map_err(io::Error::other)?;
    let Some((space, workspace)) = snapshot.spaces.iter().find_map(|space| {
        space
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
            .map(|workspace| (space, workspace))
    }) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("workspace '{workspace_id}' does not exist"),
        ));
    };
    let active = space.space_id == snapshot.active_space_id
        && space.active_workspace_id.as_deref() == Some(workspace_id);
    println!("workspace: {}", workspace.workspace_id);
    println!("name: {}", workspace.name);
    println!("space: {}", space.name);
    println!(
        "repository: {}",
        workspace.repository_path.as_deref().unwrap_or("-")
    );
    println!("branch: {}", workspace.branch.as_deref().unwrap_or("-"));
    println!("tabs: {}", workspace.tabs.len());
    println!("active: {}", if active { "yes" } else { "no" });
    Ok(())
}

fn workspace_rename(project: &Project, workspace_id: &str, name: &str) -> io::Result<()> {
    let response = super::send_command_with_payload(
        project,
        "rename_workspace",
        serde_json::json!({ "id": workspace_id, "name": name }),
    )?;
    if !response.ok {
        let message = response
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "server rejected the workspace rename request".into());
        return Err(io::Error::other(message));
    }
    println!("renamed workspace {workspace_id}: {name}");
    Ok(())
}

fn workspace_move(project: &Project, workspace_id: &str, insert_index: usize) -> io::Result<()> {
    let response = super::send_command_with_payload(
        project,
        "move_workspace",
        serde_json::json!({ "id": workspace_id, "insert_index": insert_index }),
    )?;
    if !response.ok {
        let message = response
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "server rejected the workspace move request".into());
        return Err(io::Error::other(message));
    }
    println!("moved workspace {workspace_id} to insert index {insert_index}");
    Ok(())
}

fn workspace_list(project: &Project) -> io::Result<()> {
    let response = super::send_command(project, "get_snapshot")?;
    if !response.ok {
        let message = response
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "server rejected the workspace list request".into());
        return Err(io::Error::other(message));
    }
    let snapshot: SessionSnapshot = serde_json::from_value(
        response
            .payload
            .ok_or_else(|| io::Error::other("server returned no session snapshot"))?,
    )
    .map_err(io::Error::other)?;
    print!("{}", format_workspace_list(&snapshot));
    Ok(())
}

fn format_workspace_list(snapshot: &SessionSnapshot) -> String {
    let mut output = String::new();
    for space in &snapshot.spaces {
        for workspace in &space.workspaces {
            let active = space.space_id == snapshot.active_space_id
                && space.active_workspace_id.as_deref() == Some(&workspace.workspace_id);
            let marker = if active { '*' } else { ' ' };
            let _ = writeln!(
                output,
                "{marker} {}\t{}\t[{}]",
                workspace.workspace_id, workspace.name, space.name
            );
        }
    }
    if output.is_empty() {
        output.push_str("No workspaces.\n");
    }
    output
}

fn print_help() {
    println!("Usage: spindle workspace <list|create|get <workspace_id>|focus <workspace_id>|report-metadata <workspace_id>|move <workspace_id> <insert-index>|rename <workspace_id> <label>|close <workspace_id> [--group]>");
    println!("  list    list workspaces in the current project session");
    println!(
        "  create  create a workspace and start its PowerShell pane (--cwd, --label, --env, --focus|--no-focus)"
    );
    println!("  get     show a workspace by ID");
    println!("  focus   focus a workspace by ID");
    println!("  report-metadata  report workspace tokens (--source, --token, --clear-token, --ttl-ms, --seq)");
    println!("  move    reorder a workspace within its space");
    println!("  rename  rename a workspace by ID");
    println!("  close   close a workspace by ID");
}

#[cfg(test)]
mod tests {
    use super::format_workspace_list;
    use crate::server::session::Session;

    #[test]
    fn workspace_list_marks_the_active_workspace_and_includes_spaces() {
        let mut session = Session::default();
        session.create_workspace("Build".into()).unwrap();
        let output = format_workspace_list(session.snapshot());

        assert!(output.contains("  workspace-1\tCurrent project\t[Default]"));
        assert!(output.contains("* workspace-2\tBuild\t[Default]"));
    }

    #[test]
    fn workspace_list_handles_an_empty_session() {
        let mut session = Session::default();
        session.delete_workspace("workspace-1").unwrap();

        assert_eq!(
            format_workspace_list(session.snapshot()),
            "No workspaces.\n"
        );
    }

    #[test]
    fn workspace_env_assignments_match_herdr_rules() {
        assert_eq!(
            super::super::parse_env_assignment("SPINDLE_MODE=dev").unwrap(),
            ("SPINDLE_MODE".into(), "dev".into())
        );
        assert!(super::super::parse_env_assignment("missing-separator").is_err());
        assert!(super::super::parse_env_assignment("=empty-key").is_err());
        assert!(super::super::parse_env_assignment("KEY=bad\0value").is_err());
        assert!(super::super::parse_env_assignment("KE\0Y=value").is_err());
    }
}
