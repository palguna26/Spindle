use super::Project;
use crate::server::session::{SessionSnapshot, TabView};
use std::io;

pub(super) fn run_tab_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => tab_list(project),
        [command] if command == "create" => tab_create(project, "Main"),
        [command, name] if command == "create" => tab_create(project, name),
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

fn tab_list(project: &Project) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .ok_or_else(|| io::Error::other("active space does not exist"))?;
    let workspace_id = space
        .active_workspace_id
        .as_deref()
        .ok_or_else(|| io::Error::other("active workspace does not exist"))?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
        .ok_or_else(|| io::Error::other("active workspace does not exist"))?;
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

fn tab_create(project: &Project, name: &str) -> io::Result<()> {
    send_mutation(project, "create_tab", serde_json::json!({ "name": name }))
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
    println!("Usage: spindle tab <list|create [label]|get <id>|focus <id>|rename <id> <label>|close <id>>");
    println!("  list             list tabs in the active workspace");
    println!("  create [label]   create a tab in the active workspace");
    println!("  get <id>         show a tab");
    println!("  focus <id>       focus a tab in the active workspace");
    println!("  rename <id> ...  rename a tab");
    println!("  close <id>       close a tab");
}

#[cfg(test)]
mod tests {
    use super::format_tab_list;
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
}
