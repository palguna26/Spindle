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
        [command, id, text @ ..] if command == "send-text" && !text.is_empty() => {
            let text = text.join(" ");
            pane_send_input(project, id, text.as_bytes())
        }
        [command, id, text @ ..] if command == "run" && !text.is_empty() => {
            let mut bytes = text.join(" ").into_bytes();
            bytes.push(b'\r');
            pane_send_input(project, id, &bytes)
        }
        [command, id, keys @ ..] if command == "send-keys" && !keys.is_empty() => pane_send_input(
            project,
            id,
            &keys
                .iter()
                .map(|key| key_bytes(key))
                .collect::<io::Result<Vec<_>>>()?
                .concat(),
        ),
        [command, direction] if command == "split" => pane_split(project, direction, None),
        [command, direction, command_args @ ..] if command == "split" => {
            pane_split(project, direction, Some(command_args))
        }
        [command, id, delta] if command == "resize" => pane_resize(project, id, delta),
        [command, id] if command == "read" => pane_read(project, id, None),
        [command, id, flag, lines] if command == "read" && flag == "--lines" => {
            let lines = lines.parse::<usize>().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid line count: {lines}"),
                )
            })?;
            pane_read(project, id, Some(lines))
        }
        [command, source, target] if command == "swap" => pane_mutation_with_payload(
            project,
            "swap_panes",
            serde_json::json!({ "source_pane_id": source, "target_pane_id": target }),
        ),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle pane <list|current|get|focus|rename|stop|restart|zoom|close|send-text|send-keys|run|read|swap|split|resize>",
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

fn pane_read(project: &Project, id: &str, lines: Option<usize>) -> io::Result<()> {
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
    if let Some(lines) = lines {
        let content: Vec<_> = pane.screen.lines().collect();
        let start = content.len().saturating_sub(lines);
        println!("{}", content[start..].join("\n"));
    } else {
        print!("{}", pane.screen);
        if !pane.screen.ends_with('\n') {
            println!();
        }
    }
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

fn pane_send_input(project: &Project, id: &str, bytes: &[u8]) -> io::Result<()> {
    pane_mutation_with_payload(
        project,
        "send_input",
        serde_json::json!({ "pane_id": id, "bytes": bytes }),
    )
}

fn key_bytes(key: &str) -> io::Result<Vec<u8>> {
    let bytes = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => vec![b'\r'],
        "tab" => vec![b'\t'],
        "backspace" | "bs" => vec![8],
        "escape" | "esc" => vec![27],
        "left" => b"\x1b[D".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        value if value.starts_with("ctrl-") && value.len() == 6 => {
            let byte = value.as_bytes()[5];
            if !byte.is_ascii_lowercase() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid key: {key}"),
                ));
            }
            vec![byte - b'a' + 1]
        }
        value if value.chars().count() == 1 => value.as_bytes().to_vec(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported key: {key}"),
            ))
        }
    };
    Ok(bytes)
}

fn pane_split(
    project: &Project,
    direction: &str,
    command_args: Option<&[String]>,
) -> io::Result<()> {
    if !matches!(direction, "horizontal" | "vertical") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "split direction must be horizontal or vertical",
        ));
    }
    let (command, args) = match command_args {
        Some(args) if !args.is_empty() => (args[0].clone(), args[1..].to_vec()),
        _ => (
            "powershell.exe".into(),
            vec!["-NoLogo".into(), "-NoProfile".into()],
        ),
    };
    let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
    pane_mutation_with_payload(
        project,
        "split_pane",
        serde_json::json!({
            "direction": direction,
            "command": command,
            "args": args,
            "cwd": cwd,
            "cols": 80,
            "rows": 24
        }),
    )
}

fn pane_resize(project: &Project, id: &str, raw_delta: &str) -> io::Result<()> {
    let delta = raw_delta.parse::<f32>().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid resize amount: {raw_delta}"),
        )
    })?;
    if !delta.is_finite() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "resize amount must be finite",
        ));
    }
    pane_mutation_with_payload(
        project,
        "resize_pane",
        serde_json::json!({ "pane_id": id, "delta": delta }),
    )
}

fn print_help() {
    println!("Usage: spindle pane <list|current|get|focus|rename|stop|restart|zoom|close|send-text|send-keys|run|read|swap|split|resize>");
    println!("  list             list panes in the active tab");
    println!("  current          show the focused pane");
    println!("  get <id>         show a pane as JSON");
    println!("  focus <id>       focus a pane");
    println!("  rename <id> ...  rename a pane");
    println!("  stop <id>        stop a pane process");
    println!("  restart <id>     restart a pane process");
    println!("  zoom <id>        toggle pane zoom");
    println!("  close <id>       close a pane");
    println!("  send-text <id> <text>  send text to a pane");
    println!("  send-keys <id> <key>...  send keys (Enter, arrows, ctrl-x)");
    println!("  run <id> <command>  send a command followed by Enter");
    println!("  read <id> [--lines N]  read visible pane output");
    println!("  swap <source> <target>  swap two panes");
    println!("  split <direction> [command args...]  split with a new pane");
    println!("  resize <id> <delta>  resize the pane layout by a ratio delta");
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
