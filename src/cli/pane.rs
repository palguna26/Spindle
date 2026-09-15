use super::Project;
use crate::server::session::{PaneView, SessionSnapshot};
use std::io;

pub(super) fn run_pane_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => pane_list(project),
        [command] if command == "current" => pane_current(project),
        [command, id] if command == "get" => pane_get(project, id),
        [command, id] if command == "focus" => pane_mutation(project, "focus_pane", id),
        [command, id, label @ ..] if command == "rename" && !label.is_empty() => {
            pane_rename(project, id, &label.join(" "))
        }
        [command, id] if command == "stop" => pane_mutation(project, "stop_pane", id),
        [command, id] if command == "restart" => pane_mutation(project, "restart_pane", id),
        [command, id] if command == "zoom" => pane_mutation(project, "toggle_pane_zoom", id),
        [command, id] if command == "close" => pane_mutation(project, "close_pane", id),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle pane <list|current|get|focus|rename|stop|restart|zoom|close>",
            ))
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

fn active_pane_ids(snapshot: &SessionSnapshot) -> Vec<String> {
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    else {
        return Vec::new();
    };
    let Some(workspace_id) = space.active_workspace_id.as_deref() else {
        return Vec::new();
    };
    let Some(workspace) = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
    else {
        return Vec::new();
    };
    let Some(tab) = workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)
    else {
        return Vec::new();
    };
    tab.layout
        .as_ref()
        .map(|layout| layout.pane_ids().into_iter().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn pane_list(project: &Project) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let pane_ids = active_pane_ids(&snapshot);
    print!(
        "{}",
        format_pane_list(
            &snapshot.panes,
            &pane_ids,
            snapshot.focused_pane_id.as_deref()
        )
    );
    Ok(())
}

fn format_pane_list(panes: &[PaneView], pane_ids: &[String], focused_id: Option<&str>) -> String {
    let mut output = String::new();
    for pane in panes.iter().filter(|pane| pane_ids.contains(&pane.pane_id)) {
        let marker = if Some(pane.pane_id.as_str()) == focused_id {
            '*'
        } else {
            ' '
        };
        let label = pane.label.as_deref().unwrap_or(&pane.title);
        output.push_str(&format!(
            "{marker} {}\t{}\t{}\n",
            pane.pane_id,
            label,
            format_args!("{:?}", pane.status)
        ));
    }
    if output.is_empty() {
        output.push_str("No panes.\n");
    }
    output
}

fn pane_current(project: &Project) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let Some(id) = snapshot.focused_pane_id.as_deref() else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no focused pane"));
    };
    pane_get(project, id)
}

fn pane_get(project: &Project, id: &str) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == id)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("pane '{id}' does not exist"),
            )
        })?;
    println!(
        "{}",
        serde_json::to_string_pretty(pane).map_err(io::Error::other)?
    );
    Ok(())
}

fn pane_rename(project: &Project, id: &str, label: &str) -> io::Result<()> {
    pane_mutation_with_payload(
        project,
        "rename_pane",
        serde_json::json!({ "id": id, "name": label }),
    )
}

fn pane_mutation(project: &Project, operation: &str, id: &str) -> io::Result<()> {
    pane_mutation_with_payload(project, operation, serde_json::json!({ "pane_id": id }))
}

fn pane_mutation_with_payload(
    project: &Project,
    operation: &str,
    payload: serde_json::Value,
) -> io::Result<()> {
    let response = super::send_command_with_payload(project, operation, payload)?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the pane request".into()),
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
    println!("Usage: spindle pane <list|current|get|focus|rename|stop|restart|zoom|close>");
    println!("  list             list panes in the active tab");
    println!("  current          show the focused pane");
    println!("  get <id>         show a pane as JSON");
    println!("  focus <id>       focus a pane");
    println!("  rename <id> ...  rename a pane");
    println!("  stop <id>        stop a pane process");
    println!("  restart <id>     restart a pane process");
    println!("  zoom <id>        toggle pane zoom");
    println!("  close <id>       close a pane");
}

#[cfg(test)]
mod tests {
    use super::format_pane_list;
    use crate::server::session::Session;

    #[test]
    fn pane_list_handles_an_empty_tab() {
        let session = Session::default();
        let snapshot = session.snapshot();
        let output = format_pane_list(&snapshot.panes, &[], snapshot.focused_pane_id.as_deref());
        assert_eq!(output, "No panes.\n");
    }
}
