use super::Project;
use crate::server::session::{PaneView, SessionSnapshot};
use std::io;
use std::time::{Duration, Instant};

pub(super) fn run_pane_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command, args @ ..] if command == "list" => pane_list_command(project, args),
        [command, args @ ..] if command == "current" => pane_current(project, args),
        [command, id] if command == "get" => pane_get(project, id),
        [command, options @ ..]
            if command == "focus" && options.first().is_some_and(|arg| arg.starts_with('-')) =>
        {
            pane_focus(project, options)
        }
        [command, options @ ..] if command == "neighbor" => pane_neighbor(project, options),
        [command, options @ ..] if command == "edges" => pane_edges(project, options),
        [command, options @ ..] if command == "layout" => pane_layout(project, options),
        [command, options @ ..] if command == "process-info" => pane_process_info(project, options),
        [command, options @ ..] if command == "input" => pane_input(project, options),
        [command, id] if command == "focus" => pane_mutation(project, "focus_pane", id),
        [command, id, label @ ..] if command == "rename" && !label.is_empty() => {
            let label = rename_label(label);
            pane_rename(project, id, &label)
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
        [command, options @ ..] if command == "split" && is_split_option_form(options) => {
            pane_split_options(project, options)
        }
        [command, direction] if command == "split" => pane_split(project, direction, None),
        [command, direction, command_args @ ..] if command == "split" => {
            pane_split(project, direction, Some(command_args))
        }
        [command, options @ ..]
            if command == "resize" && options.first().is_some_and(|arg| arg.starts_with('-')) =>
        {
            pane_resize_options(project, options)
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
                "usage: spindle pane <list|current|get|focus|neighbor|edges|layout|process-info|input|rename|stop|restart|zoom|close|send-text|send-keys|run|read|swap|move|wait-output|split|resize>",
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

fn all_pane_ids(snapshot: &SessionSnapshot) -> Vec<String> {
    snapshot
        .panes
        .iter()
        .map(|pane| pane.pane_id.clone())
        .collect()
}

fn pane_list_command(project: &Project, args: &[String]) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let workspace_id = parse_list_workspace(args).map_err(io::Error::other)?;
    let pane_ids = workspace_id
        .as_deref()
        .map(|id| workspace_pane_ids(&snapshot, id))
        .unwrap_or_else(|| all_pane_ids(&snapshot));
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

fn pane_current(project: &Project, args: &[String]) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let env_pane_id = std::env::var("SPINDLE_PANE_ID")
        .ok()
        .or_else(|| std::env::var("HERDR_PANE_ID").ok());
    let requested_id =
        parse_current_pane(args, env_pane_id.as_deref()).map_err(io::Error::other)?;
    let Some(id) = requested_id
        .as_deref()
        .or(snapshot.focused_pane_id.as_deref())
    else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no focused pane"));
    };
    pane_get(project, id)
}

fn parse_current_pane(
    args: &[String],
    env_pane_id: Option<&str>,
) -> Result<Option<String>, String> {
    let mut pane_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--pane" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --pane".into());
                };
                pane_id = Some(value.clone());
                index += 2;
            }
            "--current" => {
                pane_id = env_pane_id.map(str::to_owned);
                index += 1;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(pane_id.or_else(|| env_pane_id.map(str::to_owned)))
}

fn pane_focus(project: &Project, args: &[String]) -> io::Result<()> {
    let (pane_id, direction) = parse_focus_options(args).map_err(io::Error::other)?;
    pane_mutation_with_payload(
        project,
        "focus_direction",
        serde_json::json!({ "direction": direction, "pane_id": pane_id }),
    )
}

fn pane_input(project: &Project, args: &[String]) -> io::Result<()> {
    let (pane_id, right_click_passthrough) =
        parse_pane_input_options(args).map_err(io::Error::other)?;
    pane_mutation_with_payload(
        project,
        "set_right_click_passthrough",
        serde_json::json!({
            "pane_id": pane_id,
            "right_click_passthrough": right_click_passthrough,
        }),
    )
}

fn pane_neighbor(project: &Project, args: &[String]) -> io::Result<()> {
    let (pane_id, direction) = parse_neighbor_options(args).map_err(io::Error::other)?;
    let snapshot = get_snapshot(project)?;
    let pane_id = pane_id
        .or_else(|| snapshot.focused_pane_id.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no focused pane"))?;
    let layout = active_layout(&snapshot)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "active tab has no layout"))?;
    if !layout.pane_ids().contains(&pane_id.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("pane '{pane_id}' is not in the active tab"),
        ));
    }
    let neighbor_pane_id = layout
        .directional_pane(&pane_id, direction)
        .map(str::to_owned);
    println!(
        "{}",
        serde_json::json!({
            "pane_id": pane_id,
            "direction": direction_name(direction),
            "neighbor_pane_id": neighbor_pane_id,
        })
    );
    Ok(())
}

fn pane_edges(project: &Project, args: &[String]) -> io::Result<()> {
    let pane_id = parse_optional_pane_selector(args).map_err(io::Error::other)?;
    let snapshot = get_snapshot(project)?;
    let pane_id = pane_id
        .or_else(|| snapshot.focused_pane_id.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no focused pane"))?;
    let layout = active_layout(&snapshot)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "active tab has no layout"))?;
    if !layout.pane_ids().contains(&pane_id.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("pane '{pane_id}' is not in the active tab"),
        ));
    }
    println!(
        "{}",
        serde_json::json!({
            "pane_id": pane_id,
            "left": layout.directional_pane(&pane_id, crate::model::layout::FocusDirection::Left).is_none(),
            "right": layout.directional_pane(&pane_id, crate::model::layout::FocusDirection::Right).is_none(),
            "up": layout.directional_pane(&pane_id, crate::model::layout::FocusDirection::Up).is_none(),
            "down": layout.directional_pane(&pane_id, crate::model::layout::FocusDirection::Down).is_none(),
        })
    );
    Ok(())
}

fn pane_layout(project: &Project, args: &[String]) -> io::Result<()> {
    let pane_id = parse_optional_pane_selector(args).map_err(io::Error::other)?;
    let snapshot = get_snapshot(project)?;
    let pane_id = pane_id
        .or_else(|| snapshot.focused_pane_id.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no focused pane"))?;
    let layout = active_layout(&snapshot)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "active tab has no layout"))?;
    if !layout.pane_ids().contains(&pane_id.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("pane '{pane_id}' is not in the active tab"),
        ));
    }
    println!(
        "{}",
        serde_json::json!({
            "pane_id": pane_id,
            "layout": layout,
        })
    );
    Ok(())
}

fn pane_process_info(project: &Project, args: &[String]) -> io::Result<()> {
    let pane_id = parse_optional_pane_selector(args).map_err(io::Error::other)?;
    let snapshot = get_snapshot(project)?;
    let pane_id = pane_id
        .or_else(|| snapshot.focused_pane_id.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no focused pane"))?;
    let response = super::send_command_with_payload(
        project,
        "process_info",
        serde_json::json!({ "pane_id": pane_id }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected process info request".into()),
        ));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&response.payload).map_err(io::Error::other)?
    );
    Ok(())
}

fn parse_neighbor_options(
    args: &[String],
) -> Result<(Option<String>, crate::model::layout::FocusDirection), String> {
    let mut pane_id = None;
    let mut direction = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--pane" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --pane".into());
                };
                if value.is_empty() {
                    return Err("pane ID cannot be empty".into());
                }
                pane_id = Some(value.clone());
                index += 2;
            }
            "--current" => {
                pane_id = None;
                index += 1;
            }
            "--direction" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --direction".into());
                };
                direction = Some(parse_layout_direction(value)?);
                index += 2;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    let direction = direction.ok_or(
        "usage: spindle pane neighbor --direction left|right|up|down [--pane ID|--current]",
    )?;
    Ok((pane_id, direction))
}

fn parse_optional_pane_selector(args: &[String]) -> Result<Option<String>, String> {
    let env_pane_id = std::env::var("SPINDLE_PANE_ID")
        .ok()
        .or_else(|| std::env::var("HERDR_PANE_ID").ok())
        .filter(|value| !value.trim().is_empty());
    let mut pane_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--pane" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --pane".into());
                };
                pane_id = Some(value.clone());
                index += 2;
            }
            "--current" => {
                pane_id = env_pane_id.clone();
                index += 1;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(pane_id.or(env_pane_id))
}

fn parse_layout_direction(value: &str) -> Result<crate::model::layout::FocusDirection, String> {
    match value {
        "left" => Ok(crate::model::layout::FocusDirection::Left),
        "right" => Ok(crate::model::layout::FocusDirection::Right),
        "up" => Ok(crate::model::layout::FocusDirection::Up),
        "down" => Ok(crate::model::layout::FocusDirection::Down),
        _ => Err(format!("invalid pane direction: {value}")),
    }
}

fn direction_name(direction: crate::model::layout::FocusDirection) -> &'static str {
    match direction {
        crate::model::layout::FocusDirection::Left => "left",
        crate::model::layout::FocusDirection::Right => "right",
        crate::model::layout::FocusDirection::Up => "up",
        crate::model::layout::FocusDirection::Down => "down",
    }
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
    let options = parse_move_options(args).map_err(io::Error::other)?;
    let mut payload = serde_json::json!({ "pane_id": options.pane_id });
    if let Some(label) = options.label {
        payload["name"] = serde_json::json!(label);
    }
    if let Some(target_tab_id) = options.target_tab_id {
        payload["target_tab_id"] = serde_json::json!(target_tab_id);
        if let Some(target_pane_id) = options.target_pane_id {
            payload["target_pane_id"] = serde_json::json!(target_pane_id);
        }
        payload["direction"] = serde_json::json!(options.direction);
        if let Some(ratio) = options.ratio {
            payload["ratio"] = serde_json::json!(ratio);
        }
    }
    pane_mutation_with_payload(project, "move_pane", payload)
}

#[derive(Debug, PartialEq)]
struct MoveOptions {
    pane_id: String,
    label: Option<String>,
    target_tab_id: Option<String>,
    target_pane_id: Option<String>,
    direction: String,
    ratio: Option<f32>,
}

fn parse_move_options(args: &[String]) -> Result<MoveOptions, String> {
    let Some(pane_id) = args.first().map(String::as_str) else {
        return Err("usage: spindle pane move <id> --new-tab [--label TEXT] or --tab ID [--pane ID] [--split right|down]".into());
    };
    if args.get(1).map(String::as_str) == Some("--new-tab") {
        if args.len() == 2 {
            return Ok(MoveOptions {
                pane_id: pane_id.into(),
                label: Some("Moved pane".into()),
                target_tab_id: None,
                target_pane_id: None,
                direction: "right".into(),
                ratio: None,
            });
        }
        if args.len() >= 4 && args[2] == "--label" {
            let label = args[3..].join(" ");
            if !label.trim().is_empty() {
                return Ok(MoveOptions {
                    pane_id: pane_id.into(),
                    label: Some(label),
                    target_tab_id: None,
                    target_pane_id: None,
                    direction: "right".into(),
                    ratio: None,
                });
            }
        }
        return Err("usage: spindle pane move <id> --new-tab [--label TEXT]".into());
    }
    if args.get(1).map(String::as_str) != Some("--tab") || args.len() < 3 {
        return Err("usage: spindle pane move <id> --tab ID [--pane ID] [--split right|down] [--ratio FLOAT]".into());
    }
    let mut target_pane_id = None;
    let mut direction = "right";
    let mut ratio = None;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--pane" if index + 1 < args.len() => {
                target_pane_id = Some(args[index + 1].clone());
                index += 2;
            }
            "--split" if index + 1 < args.len() => {
                direction = &args[index + 1];
                if !matches!(direction, "right" | "down") {
                    return Err("--split must be right or down".into());
                }
                index += 2;
            }
            "--ratio" if index + 1 < args.len() => {
                ratio = Some(args[index + 1].parse::<f32>().map_err(|_| {
                    format!("invalid ratio: {}", args[index + 1])
                })?);
                if !ratio.is_some_and(|value| value.is_finite()) {
                    return Err("ratio must be finite".into());
                }
                index += 2;
            }
            _ => {
                return Err(
                        "usage: spindle pane move <id> --tab ID [--pane ID] [--split right|down] [--ratio FLOAT]"
                        .into(),
                )
            }
        }
    }
    Ok(MoveOptions {
        pane_id: pane_id.into(),
        label: None,
        target_tab_id: Some(args[2].clone()),
        target_pane_id,
        direction: direction.into(),
        ratio,
    })
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

fn parse_focus_options(args: &[String]) -> Result<(Option<String>, &str), String> {
    let env_pane_id = || {
        std::env::var("SPINDLE_PANE_ID")
            .ok()
            .or_else(|| std::env::var("HERDR_PANE_ID").ok())
            .filter(|value| !value.trim().is_empty())
    };
    let mut pane_id = None;
    let mut direction = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--direction" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --direction".into());
                };
                if !matches!(value.as_str(), "left" | "right" | "up" | "down") {
                    return Err(format!("invalid focus direction: {value}"));
                }
                direction = Some(value.as_str());
                index += 2;
            }
            "--pane" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --pane".into());
                };
                pane_id = Some(value.clone());
                index += 2;
            }
            "--current" => {
                pane_id = env_pane_id();
                index += 1;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    let direction = direction
        .ok_or("usage: spindle pane focus --direction left|right|up|down [--pane ID|--current]")?;
    Ok((pane_id, direction))
}

fn parse_pane_input_options(args: &[String]) -> Result<(String, bool), String> {
    let env_pane_id = std::env::var("SPINDLE_PANE_ID")
        .ok()
        .or_else(|| std::env::var("HERDR_PANE_ID").ok())
        .filter(|value| !value.trim().is_empty());
    let mut pane_id = None;
    let mut right_click_passthrough = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--pane" => {
                if pane_id.is_some() {
                    return Err("provide only one pane selector".into());
                }
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --pane".into());
                };
                pane_id = Some(value.clone());
                index += 2;
            }
            "--current" => {
                if pane_id.is_some() {
                    return Err("provide only one pane selector".into());
                }
                pane_id = Some(
                    env_pane_id
                        .clone()
                        .ok_or("--current requires a pane ID environment variable")?,
                );
                index += 1;
            }
            "--right-click" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --right-click".into());
                };
                right_click_passthrough = Some(match value.as_str() {
                    "herdr" => false,
                    "pane" => true,
                    other => return Err(format!("invalid right-click target: {other}")),
                });
                index += 2;
            }
            option if option.starts_with('-') => return Err(format!("unknown option: {option}")),
            positional => {
                if pane_id.is_some() {
                    return Err(format!("unexpected argument: {positional}"));
                }
                pane_id = Some(positional.to_owned());
                index += 1;
            }
        }
    }
    let pane_id = pane_id.ok_or(
        "usage: spindle pane input [<pane_id>|--pane ID|--current] --right-click herdr|pane",
    )?;
    let right_click_passthrough = right_click_passthrough.ok_or(
        "usage: spindle pane input [<pane_id>|--pane ID|--current] --right-click herdr|pane",
    )?;
    Ok((pane_id, right_click_passthrough))
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
    RecentUnwrapped,
    Detection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadFormat {
    Text,
    Ansi,
}

struct ReadOptions {
    source: ReadSource,
    lines: Option<usize>,
    format: ReadFormat,
}

fn pane_read_command(project: &Project, args: &[String]) -> io::Result<()> {
    let env_pane_id = std::env::var("SPINDLE_PANE_ID")
        .ok()
        .or_else(|| std::env::var("HERDR_PANE_ID").ok());
    let (requested_id, options) =
        parse_read_target(args, env_pane_id.as_deref()).map_err(io::Error::other)?;
    let snapshot = get_snapshot(project)?;
    let id = requested_id
        .or_else(|| snapshot.focused_pane_id.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no focused pane"))?;
    pane_read(project, &id, options)
}

fn parse_read_target(
    args: &[String],
    env_pane_id: Option<&str>,
) -> Result<(Option<String>, ReadOptions), String> {
    let mut pane_id = None;
    let mut options = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--pane" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --pane".into());
                };
                pane_id = Some(value.clone());
                index += 2;
            }
            "--current" => {
                pane_id = Some(
                    env_pane_id
                        .map(str::to_owned)
                        .ok_or("--current requires SPINDLE_PANE_ID or HERDR_PANE_ID")?,
                );
                index += 1;
            }
            value if !value.starts_with('-') && pane_id.is_none() => {
                pane_id = Some(value.to_owned());
                index += 1;
            }
            value if !value.starts_with('-') => {
                return Err(format!("unexpected argument: {value}"));
            }
            _ => {
                options.push(args[index].clone());
                if matches!(args[index].as_str(), "--source" | "--lines" | "--format") {
                    let Some(value) = args.get(index + 1) else {
                        return Err(format!("missing value for {}", args[index]));
                    };
                    options.push(value.clone());
                    index += 2;
                } else if matches!(args[index].as_str(), "--ansi" | "--raw") {
                    index += 1;
                } else {
                    return Err(format!("unknown option: {}", args[index]));
                }
            }
        }
    }
    Ok((pane_id, parse_read_options(&options)?))
}

fn parse_read_options(args: &[String]) -> Result<ReadOptions, String> {
    let mut source = ReadSource::Recent;
    let mut lines = None;
    let mut format = ReadFormat::Text;
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
                    "recent-unwrapped" => ReadSource::RecentUnwrapped,
                    "detection" => ReadSource::Detection,
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
            "--format" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("missing value for --format".into());
                };
                format = match value.as_str() {
                    "text" => ReadFormat::Text,
                    "ansi" => ReadFormat::Ansi,
                    _ => return Err(format!("invalid read format: {value}")),
                };
                index += 2;
            }
            "--ansi" | "--raw" => {
                format = ReadFormat::Ansi;
                index += 1;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(ReadOptions {
        source,
        lines,
        format,
    })
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
        ReadSource::Visible | ReadSource::Detection => pane.screen.clone(),
        ReadSource::Recent | ReadSource::RecentUnwrapped if pane.scrollback.is_empty() => {
            pane.screen.clone()
        }
        ReadSource::Recent | ReadSource::RecentUnwrapped => {
            String::from_utf8_lossy(&pane.scrollback).into_owned()
        }
    };
    let output = match options.format {
        ReadFormat::Text => strip_ansi(&output),
        ReadFormat::Ansi => output,
    };
    if let Some(lines) = read_line_limit(options.source, options.lines) {
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

fn read_line_limit(source: ReadSource, requested: Option<usize>) -> Option<usize> {
    requested.or_else(|| {
        matches!(source, ReadSource::Recent | ReadSource::RecentUnwrapped).then_some(80)
    })
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

fn strip_ansi(text: &str) -> String {
    let mut output = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte != 0x1b {
            output.push(byte);
            continue;
        }
        let Some(next) = bytes.next() else {
            break;
        };
        match next {
            b'[' => {
                for byte in bytes.by_ref() {
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            b']' => {
                let mut escaped = false;
                for byte in bytes.by_ref() {
                    if byte == 0x07 {
                        break;
                    }
                    if escaped {
                        escaped = false;
                        if byte == b'\\' {
                            break;
                        }
                    } else if byte == 0x1b {
                        escaped = true;
                    }
                }
            }
            _ => {}
        }
    }
    String::from_utf8_lossy(&output).into_owned()
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

fn rename_label(args: &[String]) -> String {
    if args.len() == 1 && args[0] == "--clear" {
        String::new()
    } else {
        args.join(" ")
    }
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
    if split_direction(direction).is_none() {
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
            "direction": split_direction(direction).expect("validated split direction"),
            "command": command,
            "args": args,
            "cwd": cwd,
            "cols": 80,
            "rows": 24
        }),
    )
}

fn pane_split_options(project: &Project, args: &[String]) -> io::Result<()> {
    let env_pane_id = std::env::var("SPINDLE_PANE_ID")
        .ok()
        .or_else(|| std::env::var("HERDR_PANE_ID").ok())
        .filter(|value| !value.trim().is_empty());
    let mut pane_id = None;
    let mut direction = None;
    let mut cwd = None;
    let mut env = serde_json::Map::new();
    let mut ratio = None;
    let mut focus = false;
    let mut right_click_passthrough = false;
    let mut index = 0;
    if args.first().is_some_and(|value| !value.starts_with('-')) {
        pane_id = args.first().cloned();
        index = 1;
    }
    while index < args.len() {
        match args[index].as_str() {
            "--pane" => {
                if pane_id.is_some() {
                    return Err(io::Error::other("provide only one pane selector"));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --pane"));
                };
                pane_id = Some(value.clone());
                index += 2;
            }
            "--current" => {
                if pane_id.is_some() {
                    return Err(io::Error::other("provide only one pane selector"));
                }
                pane_id = Some(env_pane_id.clone().ok_or_else(|| {
                    io::Error::other("--current requires a pane ID environment variable")
                })?);
                index += 1;
            }
            "--direction" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --direction"));
                };
                if !matches!(value.as_str(), "right" | "down" | "horizontal" | "vertical") {
                    return Err(io::Error::other(format!(
                        "invalid split direction: {value}"
                    )));
                }
                direction = Some(value.clone());
                index += 2;
            }
            "--cwd" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --cwd"));
                };
                cwd = Some(value.clone());
                index += 2;
            }
            "--ratio" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --ratio"));
                };
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| io::Error::other(format!("invalid ratio: {value}")))?;
                if !parsed.is_finite() || !(0.05..=0.95).contains(&parsed) {
                    return Err(io::Error::other(format!("invalid ratio: {value}")));
                }
                ratio = Some(parsed);
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
            "--right-click" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --right-click"));
                };
                right_click_passthrough = match value.as_str() {
                    "herdr" => false,
                    "pane" => true,
                    other => {
                        return Err(io::Error::other(format!(
                            "invalid right-click target: {other}"
                        )))
                    }
                };
                index += 2;
            }
            "--env" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --env"));
                };
                let (key, value) = value.split_once('=').ok_or_else(|| {
                    io::Error::other(format!("environment must use KEY=VALUE: {value}"))
                })?;
                if key.is_empty() {
                    return Err(io::Error::other("environment key cannot be empty"));
                }
                env.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
                index += 2;
            }
            other => return Err(io::Error::other(format!("unknown option: {other}"))),
        }
    }
    let direction = direction.ok_or_else(|| {
        io::Error::other(
            "usage: spindle pane split [--pane ID|--current] --direction right|down [--ratio FLOAT] [--cwd PATH] [--env KEY=VALUE] [--right-click herdr|pane] [--focus|--no-focus]",
        )
    })?;
    let cwd = cwd.unwrap_or(std::env::current_dir()?.to_string_lossy().into_owned());
    pane_mutation_with_payload(
        project,
        "split_pane",
        serde_json::json!({
            "direction": split_direction(&direction).expect("validated split direction"),
            "pane_id": pane_id,
            "command": "powershell.exe",
            "args": ["-NoLogo", "-NoProfile"],
            "cwd": cwd,
            "env": env,
            "ratio": ratio,
            "focus": focus,
            "right_click_passthrough": right_click_passthrough,
            "cols": 80,
            "rows": 24
        }),
    )
}

fn split_direction(value: &str) -> Option<&'static str> {
    match value {
        "right" | "horizontal" => Some("horizontal"),
        "down" | "vertical" => Some("vertical"),
        _ => None,
    }
}

fn is_split_option_form(args: &[String]) -> bool {
    args.first().is_some_and(|arg| arg.starts_with('-'))
        || args.iter().any(|arg| arg == "--direction")
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

fn pane_resize_options(project: &Project, args: &[String]) -> io::Result<()> {
    let pane_id = parse_optional_pane_selector(args).map_err(io::Error::other)?;
    let mut direction = None;
    let mut amount = 0.05_f32;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--pane" => index += 2,
            "--current" => index += 1,
            "--direction" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --direction"));
                };
                parse_layout_direction(value).map_err(io::Error::other)?;
                direction = Some(value.clone());
                index += 2;
            }
            "--amount" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::other("missing value for --amount"));
                };
                amount = value
                    .parse::<f32>()
                    .map_err(|_| io::Error::other(format!("invalid amount: {value}")))?;
                if !amount.is_finite() {
                    return Err(io::Error::other("resize amount must be finite"));
                }
                index += 2;
            }
            other => return Err(io::Error::other(format!("unknown option: {other}"))),
        }
    }
    let direction = direction.ok_or_else(|| {
        io::Error::other(
            "usage: spindle pane resize --direction left|right|up|down [--amount FLOAT] [--pane ID|--current]",
        )
    })?;
    pane_mutation_with_payload(
        project,
        "resize_pane_direction",
        serde_json::json!({
            "pane_id": pane_id,
            "direction": direction,
            "amount": amount,
        }),
    )
}

fn print_help() {
    println!("Usage: spindle pane <list|current|get|focus|neighbor|edges|layout|process-info|input|rename|stop|restart|zoom|close|send-text|send-keys|run|read|swap|move|wait-output|split|resize>");
    println!("  list [--workspace <id>]  list panes in a workspace");
    println!("  current [<id>]   show the focused or requested pane");
    println!("  get <id>         show a pane as JSON");
    println!("  focus <id>       focus a pane");
    println!(
        "  focus --direction left|right|up|down [--pane ID|--current]  focus a neighboring pane"
    );
    println!("  neighbor --direction left|right|up|down [--pane ID|--current]  inspect a neighboring pane");
    println!("  edges [--pane ID|--current]  inspect pane layout edges");
    println!("  layout [--pane ID|--current]  inspect the active pane layout");
    println!("  process-info [--pane ID|--current]  inspect the running pane process");
    println!(
        "  input [<id>|--pane ID|--current] --right-click herdr|pane  set right-click routing"
    );
    println!("  rename <id> <label>|--clear  rename or clear a pane label");
    println!("  stop <id>        stop a pane process");
    println!("  restart <id>     restart a pane process");
    println!("  zoom [<id>] [--toggle|--on|--off]  control pane zoom");
    println!("  close <id>       close a pane");
    println!("  send-text <id> <text>  send text to a pane");
    println!("  send-keys <id> <key>...  send keys (Enter, arrows, ctrl-x)");
    println!("  run <id> <command>  send a command followed by Enter");
    println!("  read [<id>|--pane ID|--current] [--source visible|recent|recent-unwrapped|detection] [--lines N] [--format text|ansi] [--ansi|--raw]  read pane output");
    println!(
        "  swap --direction left|right|up|down | --source-pane ID --target-pane ID  swap panes"
    );
    println!(
        "  move <id> --new-tab [--label TEXT] | --tab ID [--pane ID] [--split right|down]  move a pane"
    );
    println!("  wait-output <id> --match TEXT [--timeout MS] [--lines N]  wait for output");
    println!("  split <direction> [command args...] | [--pane ID|--current] --direction right|down [--ratio FLOAT] [--cwd PATH] [--env KEY=VALUE] [--right-click herdr|pane] [--focus|--no-focus]  split with a new pane");
    println!("  resize <id> <delta> | --direction left|right|up|down [--amount FLOAT] [--pane ID|--current]  resize the pane layout");
}

#[cfg(test)]
mod tests {
    use super::{
        all_pane_ids, direction_name, format_pane_list, is_split_option_form, parse_current_pane,
        parse_focus_options, parse_layout_direction, parse_list_workspace, parse_move_options,
        parse_neighbor_options, parse_optional_pane_selector, parse_pane_input_options,
        parse_read_options, parse_read_target, parse_swap_options, parse_zoom_options,
        read_line_limit, rename_label, split_direction, strip_ansi, MoveOptions, ReadFormat,
        ReadSource, SwapOptions, ZoomMode,
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
    fn pane_list_defaults_to_all_session_panes() {
        let session = Session::default();
        let mut snapshot = session.snapshot().clone();
        snapshot.panes.extend([
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-2", "command": "powershell.exe", "args": [], "cwd": "C:/",
                "status": "Running", "scrollback_bytes": 0
            }))
            .unwrap(),
        ]);
        assert_eq!(all_pane_ids(&snapshot), vec!["pane-1", "pane-2"]);
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
        assert_eq!(parse_focus_options(&args), Ok((None, "right")));
        let invalid = vec!["--direction".into(), "diagonal".into()];
        assert!(parse_focus_options(&invalid).is_err());
    }

    #[test]
    fn focus_direction_accepts_herdr_pane_selector() {
        let args = vec![
            "--pane".to_string(),
            "pane-2".to_string(),
            "--direction".to_string(),
            "left".to_string(),
        ];
        assert_eq!(
            parse_focus_options(&args),
            Ok((Some("pane-2".into()), "left"))
        );
    }

    #[test]
    fn pane_input_matches_herdr_right_click_targets() {
        let args = vec![
            "--pane".to_string(),
            "pane-2".to_string(),
            "--right-click".to_string(),
            "pane".to_string(),
        ];
        assert_eq!(parse_pane_input_options(&args), Ok(("pane-2".into(), true)));
        let invalid = vec!["--right-click".to_string(), "mouse".to_string()];
        assert!(parse_pane_input_options(&invalid).is_err());
    }

    #[test]
    fn split_direction_accepts_herdr_names() {
        assert_eq!(split_direction("right"), Some("horizontal"));
        assert_eq!(split_direction("down"), Some("vertical"));
        assert_eq!(split_direction("sideways"), None);
    }

    #[test]
    fn rename_clear_matches_herdr_cli() {
        assert_eq!(rename_label(&["--clear".into()]), "");
        assert_eq!(rename_label(&["Build".into(), "logs".into()]), "Build logs");
    }

    #[test]
    fn split_positional_pane_uses_herdr_option_form() {
        assert!(is_split_option_form(&[
            "pane-1".into(),
            "--direction".into(),
            "right".into(),
        ]));
        assert!(!is_split_option_form(&["right".into()]));
    }

    #[test]
    fn pane_neighbor_options_match_herdr_shape() {
        let (pane_id, direction) = parse_neighbor_options(&[
            "--direction".into(),
            "right".into(),
            "--pane".into(),
            "pane-2".into(),
        ])
        .unwrap();
        assert_eq!(pane_id, Some("pane-2".into()));
        assert_eq!(direction_name(direction), "right");
        assert!(parse_neighbor_options(&["--direction".into(), "diagonal".into()]).is_err());
        assert_eq!(
            parse_layout_direction("up"),
            Ok(crate::model::layout::FocusDirection::Up)
        );
        assert_eq!(
            parse_optional_pane_selector(&["--pane".into(), "pane-3".into()]),
            Ok(Some("pane-3".into()))
        );
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
        let detection = parse_read_options(&["--source".into(), "detection".into()]).unwrap();
        assert_eq!(detection.source, ReadSource::Detection);
        assert!(parse_read_options(&["--format".into(), "json".into()]).is_err());
    }

    #[test]
    fn read_options_default_to_recent_source_like_herdr() {
        let options = parse_read_options(&[]).unwrap();
        assert_eq!(options.source, ReadSource::Recent);
        assert_eq!(options.lines, None);
        assert_eq!(options.format, ReadFormat::Text);
        assert_eq!(read_line_limit(options.source, options.lines), Some(80));
    }

    #[test]
    fn read_line_limits_match_herdr_defaults_by_source() {
        assert_eq!(read_line_limit(ReadSource::Visible, None), None);
        assert_eq!(read_line_limit(ReadSource::Detection, None), None);
        assert_eq!(read_line_limit(ReadSource::Recent, Some(4)), Some(4));
    }

    #[test]
    fn read_options_accept_herdr_output_aliases() {
        let ansi = parse_read_options(&["--ansi".into()]).unwrap();
        assert_eq!(ansi.format, ReadFormat::Ansi);
        let raw = parse_read_options(&["--raw".into()]).unwrap();
        assert_eq!(raw.format, ReadFormat::Ansi);
        let unwrapped =
            parse_read_options(&["--source".into(), "recent-unwrapped".into()]).unwrap();
        assert_eq!(unwrapped.source, ReadSource::RecentUnwrapped);
        let (_, target_options) =
            parse_read_target(&["--current".into(), "--ansi".into()], Some("pane-1")).unwrap();
        assert_eq!(target_options.format, ReadFormat::Ansi);
    }

    #[test]
    fn text_reads_strip_terminal_escape_sequences_without_losing_unicode() {
        assert_eq!(strip_ansi("\x1b[31mhello\x1b[0m π"), "hello π");
        assert_eq!(strip_ansi("\x1b]0;title\x07ready"), "ready");
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
    fn current_pane_accepts_herdr_selector_forms() {
        assert_eq!(
            parse_current_pane(&["--pane".into(), "pane-2".into()], Some("pane-1")),
            Ok(Some("pane-2".into()))
        );
        assert_eq!(
            parse_current_pane(&["--current".into()], Some("pane-1")),
            Ok(Some("pane-1".into()))
        );
        assert!(parse_current_pane(&["pane-2".into()], None).is_err());
    }

    #[test]
    fn read_accepts_herdr_selector_forms() {
        let (pane_id, options) = parse_read_target(
            &[
                "--source".into(),
                "recent".into(),
                "--pane".into(),
                "pane-2".into(),
            ],
            Some("pane-1"),
        )
        .unwrap();
        assert_eq!(pane_id, Some("pane-2".into()));
        assert_eq!(options.source, ReadSource::Recent);
        assert_eq!(
            parse_read_target(&["--current".into()], Some("pane-1"))
                .unwrap()
                .0,
            Some("pane-1".into())
        );
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
        assert_eq!(
            parse_move_options(&args),
            Ok(MoveOptions {
                pane_id: "pane-1".into(),
                label: Some("Review".into()),
                target_tab_id: None,
                target_pane_id: None,
                direction: "right".into(),
                ratio: None,
            })
        );
        assert!(parse_move_options(&["pane-1".into()]).is_err());
    }

    #[test]
    fn move_options_support_existing_tab_split() {
        let args = vec![
            "pane-1".into(),
            "--tab".into(),
            "tab-2".into(),
            "--pane".into(),
            "pane-2".into(),
            "--split".into(),
            "down".into(),
            "--ratio".into(),
            "0.7".into(),
        ];
        assert_eq!(
            parse_move_options(&args),
            Ok(MoveOptions {
                pane_id: "pane-1".into(),
                label: None,
                target_tab_id: Some("tab-2".into()),
                target_pane_id: Some("pane-2".into()),
                direction: "down".into(),
                ratio: Some(0.7),
            })
        );
    }
}
