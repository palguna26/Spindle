use super::Project;
use crate::server::session::SessionSnapshot;
use std::fmt::Write as _;
use std::io;

pub(super) fn run_workspace_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => workspace_list(project),
        [command, workspace_id] if command == "focus" => workspace_focus(project, workspace_id),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle workspace <list|focus <workspace_id>>",
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
    println!("Usage: spindle workspace <list|focus <workspace_id>>");
    println!("  list    list workspaces in the current project session");
    println!("  focus   focus a workspace by ID");
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
