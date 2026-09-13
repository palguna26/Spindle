use super::Project;
use crate::server::session::SessionSnapshot;
use std::fmt::Write as _;
use std::io;

pub(super) fn run_workspace_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => workspace_list(project),
        [command, workspace_id] if command == "get" => workspace_get(project, workspace_id),
        [command, workspace_id] if command == "focus" => workspace_focus(project, workspace_id),
        [command, workspace_id, label @ ..] if command == "rename" && !label.is_empty() => {
            workspace_rename(project, workspace_id, &label.join(" "))
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle workspace <list|get <workspace_id>|focus <workspace_id>|rename <workspace_id> <label>>",
            ))
        }
    }
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
    println!("Usage: spindle workspace <list|get <workspace_id>|focus <workspace_id>|rename <workspace_id> <label>>");
    println!("  list    list workspaces in the current project session");
    println!("  get     show a workspace by ID");
    println!("  focus   focus a workspace by ID");
    println!("  rename  rename a workspace by ID");
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
}
