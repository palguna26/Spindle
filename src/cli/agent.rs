use super::Project;
use crate::server::session::SessionSnapshot;
use std::io;
use std::time::{Duration, Instant};

const AGENT_PROMPT_EFFECT_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn run_agent_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => agent_list(project),
        [command, pane_id] if command == "get" => agent_get(project, pane_id),
        [command, pane_id] if command == "focus" => agent_focus(project, pane_id),
        [command, args @ ..] if command == "start" => agent_start(project, args),
        [command, args @ ..] if command == "wait" => agent_wait(project, args),
        [command, args @ ..] if command == "read" => agent_read(project, args),
        [command, args @ ..] if command == "send-keys" => agent_send_keys(project, args),
        [command, args @ ..] if command == "prompt" => agent_prompt(project, args),
        [command, args @ ..] if command == "rename" => agent_rename(project, args),
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
    let (_, row) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&row).map_err(io::Error::other)?
    );
    Ok(())
}

fn agent_focus(project: &Project, pane_id: &str) -> io::Result<()> {
    let snapshot = get_snapshot(project)?;
    let (resolved_pane_id, row) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
    let response = super::send_command_with_payload(
        project,
        "focus_pane",
        serde_json::json!({ "pane_id": resolved_pane_id }),
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

fn agent_start(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(name) = args.first() else {
        return Err(io::Error::other(
            "usage: spindle agent start NAME --kind KIND --pane PANE_ID [--timeout MS] [-- AGENT_ARGS...]",
        ));
    };
    let separator = args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(args.len());
    let mut kind = None;
    let mut pane_id = None;
    let mut timeout = Duration::from_secs(30);
    let mut index = 1;
    while index < separator {
        match args[index].as_str() {
            "--kind" => {
                let Some(value) = args.get(index + 1).filter(|_| index + 1 < separator) else {
                    return Err(io::Error::other("missing value for --kind"));
                };
                kind = Some(value.clone());
                index += 2;
            }
            "--pane" => {
                let Some(value) = args.get(index + 1).filter(|_| index + 1 < separator) else {
                    return Err(io::Error::other("missing value for --pane"));
                };
                pane_id = Some(value.clone());
                index += 2;
            }
            "--timeout" => {
                let Some(value) = args.get(index + 1).filter(|_| index + 1 < separator) else {
                    return Err(io::Error::other("missing value for --timeout"));
                };
                let milliseconds = value
                    .parse::<u64>()
                    .map_err(|_| io::Error::other(format!("invalid timeout: {value}")))?;
                if !(3_000..=300_000).contains(&milliseconds) {
                    return Err(io::Error::other(
                        "agent start timeout must be between 3000 and 300000 milliseconds",
                    ));
                }
                timeout = Duration::from_millis(milliseconds);
                index += 2;
            }
            option => return Err(io::Error::other(format!("unknown option: {option}"))),
        }
    }
    let kind = kind.ok_or_else(|| io::Error::other("missing required --kind"))?;
    let pane_id = pane_id.ok_or_else(|| io::Error::other("missing required --pane"))?;
    let command = agent_command(&kind)
        .ok_or_else(|| io::Error::other(format!("unsupported interactive agent kind: {kind}")))?;
    let snapshot = get_snapshot(project)?;
    if !snapshot.panes.iter().any(|pane| pane.pane_id == pane_id) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("pane '{pane_id}' does not exist"),
        ));
    }

    super::pane::run_pane_command(project, &["rename".into(), pane_id.clone(), name.clone()])?;
    let mut command_line = command.to_owned();
    if separator < args.len() {
        for argument in &args[separator + 1..] {
            command_line.push(' ');
            command_line.push_str(&shell_quote(argument));
        }
    }
    super::pane::run_pane_command(project, &["run".into(), pane_id.clone(), command_line])?;

    let deadline = Instant::now() + timeout;
    loop {
        let snapshot = get_snapshot(project)?;
        if let Some(row) = agent_rows(&snapshot)
            .into_iter()
            .find(|row| row["pane_id"].as_str() == Some(pane_id.as_str()))
        {
            match row["state"].as_str() {
                Some("blocked") => {
                    return Err(io::Error::other(format!("agent '{name}' started blocked")));
                }
                Some("idle") | Some("working") => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&row).map_err(io::Error::other)?
                    );
                    return Ok(());
                }
                _ => {}
            }
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("timed out waiting for agent '{name}' to start"),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn agent_command(kind: &str) -> Option<&'static str> {
    match kind
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "")
        .as_str()
    {
        "pi" => Some("pi"),
        "qodercli" => Some("qoder"),
        "droid" => Some("droid"),
        "kiro" => Some("kiro"),
        "cline" => Some("cline"),
        "kimi" => Some("kimi"),
        "devin" => Some("devin"),
        "cursor" => Some("cursor-agent"),
        "amp" => Some("amp"),
        "kilo" => Some("kilo"),
        "antigravity" => Some("agy"),
        "hermes" => Some("hermes"),
        "qwen" => Some("qwen"),
        "grok" => Some("grok"),
        "maki" => Some("maki"),
        "muse" => Some("muse"),
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        "githubcopilot" | "copilot" => Some("github-copilot"),
        _ => None,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
                if !matches!(
                    state.as_str(),
                    "unknown" | "idle" | "working" | "blocked" | "done"
                ) {
                    return Err(io::Error::other(format!("invalid agent state: {state}")));
                }
                states.push(state.clone());
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
        default_states
            .iter()
            .map(|state| (*state).to_owned())
            .collect::<Vec<_>>()
    } else {
        states
    };
    let deadline = Instant::now() + timeout;
    loop {
        let snapshot = get_snapshot(project)?;
        let (_, row) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
        if row["state"]
            .as_str()
            .is_some_and(|state| wanted.iter().any(|wanted| wanted == state))
        {
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
    let (resolved_pane_id, _) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
    let mut pane_args = vec!["read".to_owned(), resolved_pane_id];
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
    let (resolved_pane_id, _) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
    let mut pane_args = vec!["send-keys".to_owned(), resolved_pane_id];
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
    let (text, wait_args) = parse_prompt_options(args)?;
    let snapshot = get_snapshot(project)?;
    let (resolved_pane_id, row) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
    if row["state"].as_str() == Some("blocked") {
        return Err(io::Error::other(format!(
            "agent '{pane_id}' is blocked and needs interactive input"
        )));
    }
    let mut pane_args = vec!["run".to_owned(), resolved_pane_id];
    pane_args.extend(text);
    super::pane::run_pane_command(project, &pane_args)?;
    if let Some(wait_args) = wait_args {
        let mut wait = vec![pane_id.to_owned()];
        wait.extend(wait_args);
        agent_wait_after_prompt(project, &wait)?;
    }
    Ok(())
}

fn parse_prompt_options(args: &[String]) -> io::Result<(Vec<String>, Option<Vec<String>>)> {
    let mut text = Vec::new();
    let mut wait = false;
    let mut wait_args = Vec::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--wait" => {
                wait = true;
                index += 1;
            }
            "--until" => {
                let Some(state) = args.get(index + 1) else {
                    return Err(io::Error::other("--until requires a state"));
                };
                if !matches!(
                    state.as_str(),
                    "unknown" | "idle" | "working" | "blocked" | "done"
                ) {
                    return Err(io::Error::other(format!("invalid agent state: {state}")));
                }
                wait_args.extend(["--until".to_owned(), state.clone()]);
                index += 2;
            }
            "--timeout" => {
                let Some(timeout) = args.get(index + 1) else {
                    return Err(io::Error::other("--timeout requires milliseconds"));
                };
                timeout
                    .parse::<u64>()
                    .map_err(|_| io::Error::other(format!("invalid timeout: {timeout}")))?;
                wait_args.extend(["--timeout".to_owned(), timeout.clone()]);
                index += 2;
            }
            option if option.starts_with("--") => {
                return Err(io::Error::other(format!("unknown option: {option}")));
            }
            value => {
                if wait {
                    return Err(io::Error::other("prompt text must come before --wait"));
                }
                text.push(value.to_owned());
                index += 1;
            }
        }
    }
    if !wait_args.is_empty() && !wait {
        return Err(io::Error::other("--until and --timeout require --wait"));
    }
    if text.is_empty() {
        return Err(io::Error::other(
            "agent prompt requires text before options",
        ));
    }
    Ok((text, wait.then_some(wait_args)))
}

fn agent_wait_after_prompt(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(pane_id) = args.first() else {
        return Err(io::Error::other("usage: spindle agent wait <pane-id>"));
    };
    let (wanted, requested_timeout) = wait_options(args)?;
    // Herdr always gives a prompt five seconds to produce a first activity
    // signal, even when the caller asks for a shorter overall wait.
    let timeout = prompt_wait_timeout(requested_timeout);
    let deadline = Instant::now() + timeout;
    let activity_deadline = Instant::now() + AGENT_PROMPT_EFFECT_TIMEOUT;
    let mut activity_seen = false;

    loop {
        let snapshot = get_snapshot(project)?;
        let (_, row) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
        let state = row["state"].as_str().unwrap_or("unknown");
        activity_seen |= matches!(state, "working" | "blocked");
        if activity_seen && wanted.iter().any(|wanted| wanted == state) {
            println!(
                "{}",
                serde_json::to_string_pretty(&row).map_err(io::Error::other)?
            );
            return Ok(());
        }
        let now = Instant::now();
        if !activity_seen && now >= activity_deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "agent prompt produced no observed working or blocked state within {} ms; current state is {state}",
                    AGENT_PROMPT_EFFECT_TIMEOUT.as_millis()
                ),
            ));
        }
        if now >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("timed out waiting for agent in pane '{pane_id}'"),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn prompt_wait_timeout(requested: Duration) -> Duration {
    requested.max(AGENT_PROMPT_EFFECT_TIMEOUT)
}

fn wait_options(args: &[String]) -> io::Result<(Vec<String>, Duration)> {
    let mut states = Vec::new();
    let mut timeout = Duration::from_secs(30);
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--until" => {
                let Some(state) = args.get(index + 1) else {
                    return Err(io::Error::other("--until requires a state"));
                };
                if !matches!(
                    state.as_str(),
                    "unknown" | "idle" | "working" | "blocked" | "done"
                ) {
                    return Err(io::Error::other(format!("invalid agent state: {state}")));
                }
                states.push(state.clone());
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
    Ok((
        if states.is_empty() {
            default_states.into_iter().map(str::to_owned).collect()
        } else {
            states
        },
        timeout,
    ))
}

fn agent_rename(project: &Project, args: &[String]) -> io::Result<()> {
    let Some(pane_id) = args.first() else {
        return Err(io::Error::other(
            "usage: spindle agent rename <pane-id> <name>|--clear",
        ));
    };
    if args.len() < 2 {
        return Err(io::Error::other(
            "usage: spindle agent rename <pane-id> <name>|--clear",
        ));
    }
    let snapshot = get_snapshot(project)?;
    let (resolved_pane_id, _) = resolve_agent(&agent_rows(&snapshot), pane_id)?;
    let mut pane_args = vec!["rename".to_owned(), resolved_pane_id];
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

fn resolve_agent(
    rows: &[serde_json::Value],
    target: &str,
) -> io::Result<(String, serde_json::Value)> {
    if let Some(row) = rows
        .iter()
        .find(|row| row["pane_id"].as_str() == Some(target))
    {
        return Ok((target.to_owned(), row.clone()));
    }
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["agent"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(target))
        })
        .collect();
    match matches.as_slice() {
        [row] => Ok((
            row["pane_id"].as_str().unwrap_or_default().to_owned(),
            (*row).clone(),
        )),
        [] => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no detected agent matching '{target}'"),
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("agent name '{target}' is ambiguous; use a pane ID"),
        )),
    }
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
                            let pane =
                                snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)?;
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
    println!(
        "Usage: spindle agent <list|get|focus|start|wait|read|send-keys|prompt|rename TARGET [OPTIONS]>\nTARGET is a pane ID or a unique live agent name\nagent start NAME --kind KIND --pane PANE_ID [--timeout MS] [-- AGENT_ARGS...]\nagent prompt TARGET TEXT [--wait] [--until STATE]... [--timeout MS]"
    );
}

#[cfg(test)]
mod tests {
    use super::{
        agent_command, agent_rows, parse_prompt_options, prompt_wait_timeout, resolve_agent,
        shell_quote,
    };
    use crate::server::session::Session;
    use std::time::Duration;

    #[test]
    fn prompt_wait_keeps_herdr_five_second_activity_window() {
        assert_eq!(
            prompt_wait_timeout(Duration::from_millis(500)),
            Duration::from_secs(5)
        );
        assert_eq!(
            prompt_wait_timeout(Duration::from_secs(12)),
            Duration::from_secs(12)
        );
    }

    #[test]
    fn agent_list_reports_detected_agents_across_spaces() {
        let mut session = Session::default();
        session.create_space("Other project".into()).unwrap();
        let mut snapshot = session.snapshot().clone();
        snapshot.spaces[1].workspaces[0].tabs[0].layout =
            Some(crate::model::layout::LayoutNode::pane("pane-1").split(
                crate::model::layout::Direction::Horizontal,
                0.5,
                "pane-2",
            ));
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

    #[test]
    fn agent_target_accepts_unique_name_and_rejects_ambiguous_name() {
        let rows = vec![
            serde_json::json!({"pane_id": "pane-1", "agent": "Codex"}),
            serde_json::json!({"pane_id": "pane-2", "agent": "Claude"}),
        ];
        assert_eq!(resolve_agent(&rows, "codex").unwrap().0, "pane-1");
        assert_eq!(resolve_agent(&rows, "pane-2").unwrap().0, "pane-2");

        let duplicate = vec![
            serde_json::json!({"pane_id": "pane-1", "agent": "Codex"}),
            serde_json::json!({"pane_id": "pane-2", "agent": "Codex"}),
        ];
        let error = resolve_agent(&duplicate, "codex").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn prompt_options_match_herdr_wait_shape() {
        let args = vec![
            "codex".into(),
            "Review".into(),
            "the".into(),
            "diff".into(),
            "--wait".into(),
            "--until".into(),
            "done".into(),
            "--timeout".into(),
            "120000".into(),
        ];
        let (text, wait) = parse_prompt_options(&args).unwrap();
        assert_eq!(text, ["Review", "the", "diff"]);
        assert_eq!(wait.unwrap(), ["--until", "done", "--timeout", "120000"]);
        assert!(parse_prompt_options(&["codex".into(), "--until".into(), "done".into()]).is_err());
    }

    #[test]
    fn agent_start_uses_supported_commands_and_powershell_quoting() {
        assert_eq!(agent_command("Qoder CLI"), Some("qoder"));
        assert_eq!(agent_command("github-copilot"), Some("github-copilot"));
        assert_eq!(agent_command("not-an-agent"), None);
        assert_eq!(shell_quote("say 'yes'"), "'say ''yes'''");
    }
}
