use super::Project;
use crate::server::session::{PaneView, SessionSnapshot};
use std::io;
use std::time::{Duration, Instant};

pub(super) fn run_pane_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command, args @ ..] if command == "list" => pane_list_command(project, args),
        [command] if command == "current" => pane_current(project, None),
        [command, id] if command == "current" => pane_current(project, Some(id)),
        [command, id] if command == "get" => pane_get(project, id),
        [command, options @ ..]
            if command == "focus" && options.first().is_some_and(|arg| arg.starts_with('-')) =>
        {
            pane_focus(project, options)
        }
        [command, id] if command == "focus" => pane_mutation(project, "focus_pane", id),
        [command, id, label @ ..] if command == "rename" && !label.is_empty() => {
            pane_rename(project, id, &label.join(" "))
        }
        [command, id] if command == "stop" => pane_mutation(project, "stop_pane", id),
        [command, id] if command == "restart" => pane_mutation(project, "restart_pane", id),
        [command, args @ ..] if command == "zoom" => pane_zoom_command(project, args),
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
        [command, args @ ..] if command == "read" => pane_read_command(project, args),
        [command, args @ ..] if command == "swap" => pane_swap_command(project, args),
        [command, args @ ..] if command == "move" => pane_move_command(project, args),
        [command, id, options @ ..] if command == "wait-output" => {
            pane_wait_output(project, id, options)
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle pane <list|current|get|focus|rename|stop|restart|zoom|close|send-text|send-keys|run|read|swap|wait-output|split|resize>",
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

fn pane_list_command(project: &Project, args: &[String]) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let workspace_id = parse_list_workspace(args).map_err(io::Error::other)?;
    let pane_ids = workspace_id
        .as_deref()
        .map(|id| workspace_pane_ids(&snapshot, id))
        .unwrap_or_else(|| active_pane_ids(&snapshot));
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

fn parse_list_workspace(args: &[String]) -> Result<Option<String>, String> {
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() == 2 && args[0] == "--workspace" {
        if args[1].is_empty() {
            return Err("workspace ID cannot be empty".into());
        }
        return Ok(Some(args[1].clone()));
    }
    Err("usage: spindle pane list [--workspace <id>]".into())
}

fn workspace_pane_ids(snapshot: &SessionSnapshot, workspace_id: &str) -> Vec<String> {
    snapshot
        .spaces
        .iter()
        .flat_map(|space| &space.workspaces)
        .find(|workspace| workspace.workspace_id == workspace_id)
        .map(|workspace| {
            workspace
                .tabs
                .iter()
                .flat_map(|tab| {
                    tab.layout
                        .as_ref()
                        .into_iter()
                        .flat_map(|layout| layout.pane_ids())
                })
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
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

fn pane_current(project: &Project, requested_id: Option<&str>) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let Some(id) = requested_id.or(snapshot.focused_pane_id.as_deref()) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no focused pane"));
    };
    pane_get(project, id)
}

fn pane_focus(project: &Project, args: &[String]) -> io::Result<()> {
    let direction = parse_focus_direction(args).map_err(io::Error::other)?;
    pane_mutation_with_payload(
        project,
        "focus_direction",
        serde_json::json!({ "direction": direction }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoomMode {
    Toggle,
    On,
    Off,
}

fn pane_zoom_command(project: &Project, args: &[String]) -> io::Result<()> {
    let (requested_id, mode) = parse_zoom_options(args).map_err(io::Error::other)?;
    let snapshot = get_snapshot(project)?;
    let pane_id = requested_id
        .or_else(|| snapshot.focused_pane_id.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no focused pane"))?;
    let zoomed = active_tab_zoomed(&snapshot);
    let desired = match mode {
        ZoomMode::Toggle => !zoomed,
        ZoomMode::On => true,
        ZoomMode::Off => false,
    };
    if desired == zoomed && mode != ZoomMode::Toggle {
        println!(
            "{}",
            serde_json::json!({ "pane_id": pane_id, "zoomed": zoomed })
        );
        return Ok(());
    }
    pane_mutation(project, "toggle_pane_zoom", &pane_id)
}

fn parse_zoom_options(args: &[String]) -> Result<(Option<String>, ZoomMode), String> {
    let mut pane_id = None;
    let mut mode = ZoomMode::Toggle;
    let mut mode_seen = false;
    for arg in args {
        match arg.as_str() {
            "--toggle" | "--on" | "--off" => {
                if mode_seen {
                    return Err("provide only one of --toggle, --on, or --off".into());
                }
                mode = match arg.as_str() {
                    "--toggle" => ZoomMode::Toggle,
                    "--on" => ZoomMode::On,
                    _ => ZoomMode::Off,
                };
                mode_seen = true;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}"));
            }
            value if pane_id.is_none() => pane_id = Some(value.to_owned()),
            _ => return Err("only one pane ID may be provided".into()),
        }
    }
    Ok((pane_id, mode))
}

fn active_tab_zoomed(snapshot: &SessionSnapshot) -> bool {
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
    else {
        return false;
    };
    let Some(workspace_id) = space.active_workspace_id.as_deref() else {
        return false;
    };
    space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
        .and_then(|workspace| {
            workspace
                .tabs
                .iter()
                .find(|tab| tab.tab_id == workspace.active_tab_id)
        })
        .is_some_and(|tab| tab.zoomed)
}

fn pane_swap_command(project: &Project, args: &[String]) -> io::Result<()> {
    let options = parse_swap_options(args).map_err(io::Error::other)?;
    let snapshot = get_snapshot(project)?;
    let (source, target) = match options {
        SwapOptions::Explicit { source, target } => (source, target),
        SwapOptions::Direction(direction) => {
            let source = snapshot
                .focused_pane_id
                .clone()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no focused pane"))?;
            let target = active_layout(&snapshot)
                .and_then(|layout| layout.directional_pane(&source, direction))
                .map(str::to_owned)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "no pane in that direction")
                })?;
            (source, target)
        }
    };
    pane_mutation_with_payload(
        project,
        "swap_panes",
        serde_json::json!({ "source_pane_id": source, "target_pane_id": target }),
    )
}

fn pane_move_command(project: &Project, args: &[String]) -> io::Result<()> {
    let (pane_id, label) = parse_move_options(args).map_err(io::Error::other)?;
    pane_mutation_with_payload(
        project,
        "move_pane",
        serde_json::json!({ "pane_id": pane_id, "name": label }),
    )
}

fn parse_move_options(args: &[String]) -> Result<(&str, String), String> {
    let Some(pane_id) = args.first().map(String::as_str) else {
        return Err("usage: spindle pane move <id> --new-tab [--label TEXT]".into());
    };
    if args.get(1).map(String::as_str) != Some("--new-tab") {
        return Err("usage: spindle pane move <id> --new-tab [--label TEXT]".into());
    }
    if args.len() == 2 {
        return Ok((pane_id, "Moved pane".into()));
    }
    if args.len() >= 4 && args[2] == "--label" {
        let label = args[3..].join(" ");
        if !label.trim().is_empty() {
            return Ok((pane_id, label));
        }
    }
    Err("usage: spindle pane move <id> --new-tab [--label TEXT]".into())
}

enum SwapOptions {
    Direction(crate::model::layout::FocusDirection),
    Explicit { source: String, target: String },
}

fn parse_swap_options(args: &[String]) -> Result<SwapOptions, String> {
    if args.len() == 2 && args[0] == "--direction" {
        let direction = match args[1].as_str() {
            "left" => crate::model::layout::FocusDirection::Left,
            "right" => crate::model::layout::FocusDirection::Right,
            "up" => crate::model::layout::FocusDirection::Up,
            "down" => crate::model::layout::FocusDirection::Down,
            value => return Err(format!("invalid swap direction: {value}")),
        };
        return Ok(SwapOptions::Direction(direction));
    }
    if args.len() == 4 && args[0] == "--source-pane" && args[2] == "--target-pane" {
        if args[1].is_empty() || args[3].is_empty() {
            return Err("pane IDs cannot be empty".into());
        }
        return Ok(SwapOptions::Explicit {
            source: args[1].clone(),
            target: args[3].clone(),
        });
    }
    Err("usage: spindle pane swap --direction left|right|up|down | --source-pane ID --target-pane ID".into())
}

fn active_layout(snapshot: &SessionSnapshot) -> Option<&crate::model::layout::LayoutNode> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace_id = space.active_workspace_id.as_deref()?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)?;
    workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)?
        .layout
        .as_ref()
}

fn parse_focus_direction(args: &[String]) -> Result<&str, String> {
    if args.len() != 2 || args[0] != "--direction" {
        return Err("usage: spindle pane focus --direction left|right|up|down".into());
    }
    if !matches!(args[1].as_str(), "left" | "right" | "up" | "down") {
        return Err(format!("invalid focus direction: {}", args[1]));
    }
    Ok(args[1].as_str())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadSource {
    Visible,
    Recent,
}

struct ReadOptions {
    source: ReadSource,
    lines: Option<usize>,
}

fn pane_read_command(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(id) = args.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: spindle pane read <id> [--source visible|recent] [--lines N]",
        ));
    };
    let options = parse_read_options(&args[1..]).map_err(io::Error::other)?;
    pane_read(project, id, options)
}

fn parse_read_options(args: &[String]) -> Result<ReadOptions, String> {
    let mut source = ReadSource::Visible;
    let mut lines = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--source" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --source".into());
                };
                source = match value.as_str() {
                    "visible" => ReadSource::Visible,
                    "recent" => ReadSource::Recent,
                    _ => return Err(format!("invalid read source: {value}")),
                };
                index += 2;
            }
            "--lines" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --lines".into());
                };
                lines = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| format!("invalid line count: {value}"))?,
                );
                index += 2;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(ReadOptions { source, lines })
}

fn pane_read(project: &Project, id: &str, options: ReadOptions) -> io::Result<()> {
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
    let output = match options.source {
        ReadSource::Visible => pane.screen.clone(),
        ReadSource::Recent if pane.scrollback.is_empty() => pane.screen.clone(),
        ReadSource::Recent => String::from_utf8_lossy(&pane.scrollback).into_owned(),
    };
    if let Some(lines) = options.lines {
        let content: Vec<_> = output.lines().collect();
        let start = content.len().saturating_sub(lines);
        println!("{}", content[start..].join("\n"));
    } else {
        print!("{output}");
        if !output.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

struct WaitOptions {
    needle: String,
    timeout: Duration,
    lines: Option<usize>,
}

fn pane_wait_output(project: &Project, id: &str, args: &[String]) -> io::Result<()> {
    let options = parse_wait_options(args)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let deadline = Instant::now() + options.timeout;
    loop {
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
        if pane.screen.contains(&options.needle) {
            if let Some(lines) = options.lines {
                let content: Vec<_> = pane.screen.lines().collect();
                let start = content.len().saturating_sub(lines);
                println!("{}", content[start..].join("\n"));
            } else {
                print!("{}", pane.screen);
            }
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "timed out waiting for '{id}' to contain {:?}",
                    options.needle
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn parse_wait_options(args: &[String]) -> Result<WaitOptions, String> {
    let mut needle = None;
    let mut timeout = Duration::from_secs(10);
    let mut lines = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--match" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --match".into());
                };
                needle = Some(value.clone());
                index += 2;
            }
            "--timeout" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --timeout".into());
                };
                let milliseconds = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid timeout: {value}"))?;
                timeout = Duration::from_millis(milliseconds);
                index += 2;
            }
            "--lines" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --lines".into());
                };
                lines = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| format!("invalid line count: {value}"))?,
                );
                index += 2;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    let Some(needle) = needle else {
        return Err(
            "usage: spindle pane wait-output <id> --match TEXT [--timeout MS] [--lines N]".into(),
        );
    };
    Ok(WaitOptions {
        needle,
        timeout,
        lines,
    })
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
    println!("Usage: spindle pane <list|current|get|focus|rename|stop|restart|zoom|close|send-text|send-keys|run|read|swap|move|wait-output|split|resize>");
    println!("  list [--workspace <id>]  list panes in a workspace");
    println!("  current [<id>]   show the focused or requested pane");
    println!("  get <id>         show a pane as JSON");
    println!("  focus <id>       focus a pane");
    println!("  focus --direction left|right|up|down  focus a neighboring pane");
    println!("  rename <id> ...  rename a pane");
    println!("  stop <id>        stop a pane process");
    println!("  restart <id>     restart a pane process");
    println!("  zoom [<id>] [--toggle|--on|--off]  control pane zoom");
    println!("  close <id>       close a pane");
    println!("  send-text <id> <text>  send text to a pane");
    println!("  send-keys <id> <key>...  send keys (Enter, arrows, ctrl-x)");
    println!("  run <id> <command>  send a command followed by Enter");
    println!("  read <id> [--source visible|recent] [--lines N]  read pane output");
    println!(
        "  swap --direction left|right|up|down | --source-pane ID --target-pane ID  swap panes"
    );
    println!("  move <id> --new-tab [--label TEXT]  move a pane to a new tab");
    println!("  wait-output <id> --match TEXT [--timeout MS] [--lines N]  wait for output");
    println!("  split <direction> [command args...]  split with a new pane");
    println!("  resize <id> <delta>  resize the pane layout by a ratio delta");
}

#[cfg(test)]
mod tests {
    use super::{
        format_pane_list, parse_focus_direction, parse_list_workspace, parse_move_options,
        parse_read_options, parse_swap_options, parse_zoom_options, ReadSource, SwapOptions,
        ZoomMode,
    };
    use crate::server::session::Session;
    use std::time::Duration;

    #[test]
    fn pane_list_handles_an_empty_tab() {
        let session = Session::default();
        let snapshot = session.snapshot();
        let output = format_pane_list(&snapshot.panes, &[], snapshot.focused_pane_id.as_deref());
        assert_eq!(output, "No panes.\n");
    }

    #[test]
    fn wait_output_requires_a_match_and_parses_limits() {
        let args = vec![
            "--match".into(),
            "ready".into(),
            "--timeout".into(),
            "250".into(),
            "--lines".into(),
            "3".into(),
        ];
        let options = super::parse_wait_options(&args).unwrap();
        assert_eq!(options.needle, "ready");
        assert_eq!(options.timeout, Duration::from_millis(250));
        assert_eq!(options.lines, Some(3));
    }

    #[test]
    fn focus_direction_matches_herdr_cli_shape() {
        let args = vec!["--direction".into(), "right".into()];
        assert_eq!(parse_focus_direction(&args), Ok("right"));
        let invalid = vec!["--direction".into(), "diagonal".into()];
        assert!(parse_focus_direction(&invalid).is_err());
    }

    #[test]
    fn read_options_support_herdr_visible_and_recent_sources() {
        let args = vec![
            "--source".into(),
            "recent".into(),
            "--lines".into(),
            "4".into(),
        ];
        let options = parse_read_options(&args).unwrap();
        assert_eq!(options.source, ReadSource::Recent);
        assert_eq!(options.lines, Some(4));
        assert!(parse_read_options(&["--source".into(), "detection".into()]).is_err());
    }

    #[test]
    fn pane_list_accepts_herdr_workspace_selector() {
        assert_eq!(
            parse_list_workspace(&["--workspace".into(), "workspace-2".into()]),
            Ok(Some("workspace-2".into()))
        );
        assert!(parse_list_workspace(&["--workspace".into()]).is_err());
    }

    #[test]
    fn zoom_options_match_herdr_explicit_modes() {
        let args = vec!["pane-2".into(), "--on".into()];
        assert_eq!(
            parse_zoom_options(&args),
            Ok((Some("pane-2".into()), ZoomMode::On))
        );
        assert_eq!(parse_zoom_options(&[]), Ok((None, ZoomMode::Toggle)));
        assert!(parse_zoom_options(&["--on".into(), "--off".into()]).is_err());
    }

    #[test]
    fn swap_options_support_directional_and_explicit_forms() {
        assert!(matches!(
            parse_swap_options(&["--direction".into(), "left".into()]),
            Ok(SwapOptions::Direction(
                crate::model::layout::FocusDirection::Left
            ))
        ));
        assert!(matches!(
            parse_swap_options(&[
                "--source-pane".into(),
                "pane-1".into(),
                "--target-pane".into(),
                "pane-2".into(),
            ]),
            Ok(SwapOptions::Explicit { .. })
        ));
    }

    #[test]
    fn move_options_support_new_tab_and_label() {
        let args = vec![
            "pane-1".into(),
            "--new-tab".into(),
            "--label".into(),
            "Review".into(),
        ];
        assert_eq!(parse_move_options(&args), Ok(("pane-1", "Review".into())));
        assert!(parse_move_options(&["pane-1".into()]).is_err());
    }
}
