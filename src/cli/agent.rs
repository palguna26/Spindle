use super::Project;
use crate::server::session::SessionSnapshot;
use std::io;
use std::time::{Duration, Instant};

pub(super) fn run_agent_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => agent_list(project),
        [command, pane_id] if command == "get" => agent_get(project, pane_id),
        [command, pane_id] if command == "focus" => agent_focus(project, pane_id),
        [command, args @ ..] if command == "wait" => agent_wait(project, args),
        [command, args @ ..] if command == "read" => agent_read(project, args),
        [command, args @ ..] if command == "send-keys" => agent_send_keys(project, args),
        [command, args @ ..] if command == "prompt" => agent_prompt(project, args),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle agent <list|get PANE_ID>",
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

fn agent_get(project: &Project, pane_id: &str) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let row = agent_rows(&snapshot)
        .into_iter()
        .find(|row| row["pane_id"].as_str() == Some(pane_id))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no detected agent in pane '{pane_id}'"),
            )
        })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&row).map_err(io::Error::other)?
    );
    Ok(())
}

fn agent_focus(project: &Project, pane_id: &str) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let row = agent_rows(&snapshot)
        .into_iter()
        .find(|row| row["pane_id"].as_str() == Some(pane_id))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no detected agent in pane '{pane_id}'"),
            )
        })?;
    let response = super::send_command_with_payload(
        project,
        "focus_pane",
        serde_json::json!({ "pane_id": pane_id }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the agent focus request".into()),
        ));
    }
    let mut row = row;
    row["focused"] = serde_json::json!(true);
    println!(
        "{}",
        serde_json::to_string_pretty(&row).map_err(io::Error::other)?
    );
    Ok(())
}

fn agent_wait(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(pane_id) = args.first() else {
        return Err(io::Error::other(
            "usage: spindle agent wait <pane-id> [--until STATE]... [--timeout MS]",
        ));
    };
    let mut states = Vec::new();
    let mut timeout = Duration::from_secs(30);
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--until" => {
                let Some(state) = args.get(index + 1) else {
                    return Err(io::Error::other("--until requires a state"));
                };
                if !matches!(state.as_str(), "unknown" | "idle" | "working" | "blocked" | "done")
                {
                    return Err(io::Error::other(format!("invalid agent state: {state}")));
                }
                states.push(state.as_str());
                index += 2;
            }
            "--timeout" => {
                let Some(raw) = args.get(index + 1) else {
                    return Err(io::Error::other("--timeout requires milliseconds"));
                };
                let milliseconds = raw
                    .parse::<u64>()
                    .map_err(|_| io::Error::other(format!("invalid timeout: {raw}")))?;
                timeout = Duration::from_millis(milliseconds);
                index += 2;
            }
            option => return Err(io::Error::other(format!("unknown option: {option}"))),
        }
    }
    let default_states = ["idle", "done", "blocked"];
    let wanted = if states.is_empty() {
        &default_states[..]
    } else {
        &states[..]
    };
    let deadline = Instant::now() + timeout;
    loop {
        let snapshot = get_snapshot(project)?;
        let row = agent_rows(&snapshot)
            .into_iter()
            .find(|row| row["pane_id"].as_str() == Some(pane_id.as_str()))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("agent in pane '{pane_id}' is not running"),
                )
            })?;
        if row["state"].as_str().is_some_and(|state| wanted.contains(&state)) {
            println!(
                "{}",
                serde_json::to_string_pretty(&row).map_err(io::Error::other)?
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("timed out waiting for agent in pane '{pane_id}'"),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn agent_read(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(pane_id) = args.first() else {
        return Err(io::Error::other(
            "usage: spindle agent read <pane-id> [pane read options]",
        ));
    };
    let snapshot = get_snapshot(project)?;
    if !agent_rows(&snapshot)
        .iter()
        .any(|row| row["pane_id"].as_str() == Some(pane_id.as_str()))
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no detected agent in pane '{pane_id}'"),
        ));
    }
    let mut pane_args = vec!["read".to_owned(), pane_id.clone()];
    pane_args.extend(args.iter().skip(1).cloned());
    super::pane::run_pane_command(project, &pane_args)
}

fn agent_send_keys(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(pane_id) = args.first() else {
        return Err(io::Error::other(
            "usage: spindle agent send-keys <pane-id> <key>...",
        ));
    };
    if args.len() < 2 {
        return Err(io::Error::other(
            "usage: spindle agent send-keys <pane-id> <key>...",
        ));
    }
    let snapshot = get_snapshot(project)?;
    if !agent_rows(&snapshot)
        .iter()
        .any(|row| row["pane_id"].as_str() == Some(pane_id.as_str()))
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no detected agent in pane '{pane_id}'"),
        ));
    }
    let mut pane_args = vec!["send-keys".to_owned(), pane_id.clone()];
    pane_args.extend(args.iter().skip(1).cloned());
    super::pane::run_pane_command(project, &pane_args)
}

fn agent_prompt(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(pane_id) = args.first() else {
        return Err(io::Error::other(
            "usage: spindle agent prompt <pane-id> <text>...",
        ));
    };
    if args.len() < 2 {
        return Err(io::Error::other(
            "usage: spindle agent prompt <pane-id> <text>...",
        ));
    }
    if args.iter().skip(1).any(|arg| arg.starts_with("--")) {
        return Err(io::Error::other(
            "agent prompt options are not supported; use agent wait separately",
        ));
    }
    let snapshot = get_snapshot(project)?;
    let row = agent_rows(&snapshot)
        .into_iter()
        .find(|row| row["pane_id"].as_str() == Some(pane_id.as_str()))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no detected agent in pane '{pane_id}'"),
            )
        })?;
    if row["state"].as_str() == Some("blocked") {
        return Err(io::Error::other(format!(
            "agent in pane '{pane_id}' is blocked and needs interactive input"
        )));
    }
    let mut pane_args = vec!["run".to_owned(), pane_id.clone()];
    pane_args.extend(args.iter().skip(1).cloned());
    super::pane::run_pane_command(project, &pane_args)
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
    println!("Usage: spindle agent <list|get|focus|wait|read|send-keys|prompt PANE_ID [OPTIONS]>");
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
