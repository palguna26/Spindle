use super::Project;
use crate::server::session::SessionSnapshot;
use std::io;

pub(super) fn run_agent_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => agent_list(project),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle agent list",
            ))
        }
    }
}

fn agent_list(project: &Project) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let agents = agent_rows(&snapshot);
    println!(
        "{}",
        serde_json::to_string_pretty(&agents).map_err(io::Error::other)?
    );
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

fn agent_rows(snapshot: &SessionSnapshot) -> Vec<serde_json::Value> {
    snapshot
        .spaces
        .iter()
        .flat_map(|space| {
            space.workspaces.iter().flat_map(move |workspace| {
                workspace.tabs.iter().flat_map(move |tab| {
                    let focused = tab.focused_pane_id.as_deref();
                    tab.layout.as_ref().into_iter().flat_map(move |layout| {
                        layout.pane_ids().into_iter().filter_map(move |pane_id| {
                            let pane = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)?;
                            let agent = pane.agent?;
                            Some(serde_json::json!({
                                "pane_id": pane.pane_id,
                                "agent": agent.label(),
                                "agent_kind": agent,
                                "state": pane.agent_display_state().label(),
                                "workspace_id": workspace.workspace_id,
                                "tab_id": tab.tab_id,
                                "focused": focused == Some(pane.pane_id.as_str()),
                                "cwd": pane.cwd,
                                "title": pane.title,
                            }))
                        })
                    })
                })
            })
        })
        .collect()
}

fn print_help() {
    println!("Usage: spindle agent list");
}

#[cfg(test)]
mod tests {
    use super::agent_rows;
    use crate::server::session::Session;

    #[test]
    fn agent_list_reports_detected_agents_across_spaces() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let mut snapshot = session.snapshot().clone();
        snapshot.spaces[1].workspaces[0].tabs[0].layout = Some(
            crate::model::layout::LayoutNode::pane("pane-1").split(
                crate::model::layout::Direction::Horizontal,
                0.5,
                "pane-2",
            ),
        );
        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "scrollback_bytes": 0,
                "agent": "codex",
                "agent_state": "working"
            }))
            .unwrap(),
        );
        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-2",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "scrollback_bytes": 0,
                "agent": "claude",
                "agent_state": "blocked"
            }))
            .unwrap(),
        );

        let rows = agent_rows(&snapshot);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["agent"], "Codex");
        assert_eq!(rows[0]["state"], "working");
        assert_eq!(
            rows[0]["workspace_id"],
            snapshot.spaces[1].workspaces[0].workspace_id
        );
        assert_eq!(rows[1]["agent"], "Claude");
        assert_eq!(rows[1]["state"], "blocked");
    }
}
