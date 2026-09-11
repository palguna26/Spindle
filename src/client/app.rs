use super::input::{action, is_prefix, Action};
use super::palette::{move_selection, Command};
use super::prompt::{PromptResult, RenamePrompt, RenameTarget};
use super::renderer;
use super::{ClientError, ControlClient};
use crate::server::session::SessionSnapshot;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use serde_json::json;
use std::io::{self, stdout};
use std::time::Duration;

pub fn run(address: impl Into<String>) -> Result<(), ClientError> {
    let client = ControlClient::connect(address)?;
    let terminal_size = size().map_err(ClientError::Io)?;
    client.attach_with_terminal(
        terminal_size.0,
        terminal_size.1,
        vec!["mouse".into(), "alternate_screen".into()],
    )?;
    let mut terminal = setup_terminal().map_err(ClientError::Io)?;
    let result = event_loop(&mut terminal, &client);
    restore_terminal(&mut terminal).map_err(ClientError::Io)?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(output))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: &ControlClient,
) -> Result<(), ClientError> {
    let mut prefix_active = false;
    let mut palette_selected = 0;
    let mut palette_open = false;
    let mut rename_prompt: Option<RenamePrompt> = None;
    let mut last_size = None;
    loop {
        let snapshot = current_snapshot(client)?;
        let terminal_size = size().map_err(ClientError::Io)?;
        if last_size != Some(terminal_size) {
            resize_panes(client, &snapshot, terminal_size)?;
            last_size = Some(terminal_size);
        }
        terminal
            .draw(|frame| {
                renderer::render(frame, &snapshot);
                if palette_open {
                    renderer::render_palette(frame, palette_selected);
                }
                if let Some(prompt) = &rename_prompt {
                    let title = match prompt.target {
                        RenameTarget::Pane => "Rename pane",
                        RenameTarget::Tab => "Rename tab",
                        RenameTarget::Workspace => "Rename workspace",
                    };
                    renderer::render_prompt(frame, title, &prompt.input);
                }
            })
            .map_err(ClientError::Io)?;
        if !event::poll(Duration::from_millis(100)).map_err(ClientError::Io)? {
            continue;
        }
        let Event::Key(key) = event::read().map_err(ClientError::Io)? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(prompt) = &mut rename_prompt {
            match prompt.apply_key(key.code) {
                PromptResult::Continue => {}
                PromptResult::Cancel => rename_prompt = None,
                PromptResult::Submit(name) => {
                    let target = prompt.target;
                    rename_prompt = None;
                    submit_rename(client, &snapshot, target, name)?;
                }
            }
            continue;
        }
        if palette_open {
            if let Some(next) = move_selection(palette_selected, key.code) {
                palette_selected = next;
            } else if key.code == KeyCode::Esc {
                palette_open = false;
            } else if key.code == KeyCode::Enter {
                let command = Command::ALL[palette_selected];
                palette_open = false;
                if let Some(target) = rename_target(command.action()) {
                    rename_prompt = Some(RenamePrompt::new(target));
                    continue;
                }
                if execute_action(command.action(), client, &snapshot, terminal_size)? {
                    break;
                }
            }
            continue;
        }
        if is_prefix(key) {
            prefix_active = true;
            continue;
        }
        let pressed = action(prefix_active, key);
        if pressed == Action::CommandPalette {
            palette_open = true;
            palette_selected = 0;
            prefix_active = false;
            continue;
        }
        match pressed {
            Action::Detach => {
                client.detach()?;
                break;
            }
            Action::NewTab => {
                client.request("new-tab", "create_tab", json!({ "name": "Activity" }))?;
            }
            Action::CloseTab => {
                if let Some(tab_id) = active_tab_id(&snapshot) {
                    let _ = client.request("close-tab", "close_tab", json!({ "id": tab_id }));
                }
            }
            Action::NextTab | Action::PreviousTab => {
                if let Some(tab_id) = adjacent_tab_id(&snapshot, matches!(pressed, Action::NextTab))
                {
                    let _ = client.request("switch-tab", "switch_tab", json!({ "id": tab_id }));
                }
            }
            Action::NextSpace | Action::PreviousSpace => {
                if let Some(space_id) =
                    adjacent_space_id(&snapshot, matches!(pressed, Action::NextSpace))
                {
                    let _ =
                        client.request("switch-space", "switch_space", json!({ "id": space_id }));
                }
            }
            Action::StopFocusedPane => {
                if let Some(pane_id) = snapshot.focused_pane_id {
                    let _ = client.request("stop-pane", "stop_pane", json!({ "pane_id": pane_id }));
                }
            }
            Action::RestartFocusedPane => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    let _ = client.request(
                        "restart-pane",
                        "restart_pane",
                        json!({ "pane_id": pane_id }),
                    );
                }
            }
            Action::FocusNext => {
                let _ = client.request("focus-next", "focus_next", json!({}));
            }
            Action::SplitHorizontal | Action::SplitVertical => {
                let direction = if matches!(pressed, Action::SplitHorizontal) {
                    "horizontal"
                } else {
                    "vertical"
                };
                let cwd = std::env::current_dir()
                    .map_err(ClientError::Io)?
                    .to_string_lossy()
                    .into_owned();
                let _ = client.request(
                    "split",
                    "split_pane",
                    json!({
                        "direction": direction,
                        "command": "powershell.exe",
                        "args": ["-NoLogo", "-NoProfile"],
                        "cwd": cwd,
                        "cols": pane_size(terminal_size).0,
                        "rows": pane_size(terminal_size).1
                    }),
                );
            }
            Action::ResizeSmaller | Action::ResizeLarger => {
                if let Some(pane_id) = snapshot.focused_pane_id {
                    let delta = if matches!(pressed, Action::ResizeLarger) {
                        0.05
                    } else {
                        -0.05
                    };
                    let _ = client.request(
                        "resize",
                        "resize_pane",
                        json!({ "pane_id": pane_id, "delta": delta }),
                    );
                }
            }
            Action::Send(code) => {
                if let Some(pane_id) = snapshot.focused_pane_id {
                    if let Some(bytes) = key_code_bytes(code) {
                        let _ = client.request(
                            "input",
                            "send_input",
                            json!({ "pane_id": pane_id, "bytes": bytes }),
                        );
                    }
                }
            }
            Action::None => {}
            Action::CommandPalette => {}
            Action::RenameFocusedPane | Action::RenameActiveTab | Action::RenameActiveWorkspace => {
            }
        }
        prefix_active = false;
    }
    Ok(())
}

fn rename_target(action: Action) -> Option<RenameTarget> {
    match action {
        Action::RenameFocusedPane => Some(RenameTarget::Pane),
        Action::RenameActiveTab => Some(RenameTarget::Tab),
        Action::RenameActiveWorkspace => Some(RenameTarget::Workspace),
        _ => None,
    }
}

fn submit_rename(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    target: RenameTarget,
    name: String,
) -> Result<(), ClientError> {
    let (operation, id) = match target {
        RenameTarget::Pane => ("rename_pane", snapshot.focused_pane_id.clone()),
        RenameTarget::Tab => ("rename_tab", active_tab_id(snapshot)),
        RenameTarget::Workspace => ("rename_workspace", active_workspace_id(snapshot)),
    };
    if let Some(id) = id {
        let _ = client.request(
            format!("rename-{}", operation),
            operation,
            json!({ "id": id, "name": name }),
        )?;
    }
    Ok(())
}

fn active_workspace_id(snapshot: &SessionSnapshot) -> Option<String> {
    snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .map(|space| space.active_workspace_id.clone())
}

fn execute_action(
    pressed: Action,
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    terminal_size: (u16, u16),
) -> Result<bool, ClientError> {
    match pressed {
        Action::Detach => {
            client.detach()?;
            Ok(true)
        }
        Action::NewTab => {
            client.request(
                "palette-new-tab",
                "create_tab",
                json!({ "name": "Activity" }),
            )?;
            Ok(false)
        }
        Action::CloseTab => {
            if let Some(tab_id) = active_tab_id(snapshot) {
                let _ = client.request("palette-close-tab", "close_tab", json!({ "id": tab_id }));
            }
            Ok(false)
        }
        Action::NextTab | Action::PreviousTab => {
            if let Some(tab_id) = adjacent_tab_id(snapshot, matches!(pressed, Action::NextTab)) {
                let _ = client.request("palette-switch-tab", "switch_tab", json!({ "id": tab_id }));
            }
            Ok(false)
        }
        Action::NextSpace | Action::PreviousSpace => {
            if let Some(space_id) =
                adjacent_space_id(snapshot, matches!(pressed, Action::NextSpace))
            {
                let _ = client.request(
                    "palette-switch-space",
                    "switch_space",
                    json!({ "id": space_id }),
                );
            }
            Ok(false)
        }
        Action::StopFocusedPane => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                let _ = client.request(
                    "palette-stop-pane",
                    "stop_pane",
                    json!({ "pane_id": pane_id }),
                );
            }
            Ok(false)
        }
        Action::RestartFocusedPane => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                let _ = client.request(
                    "palette-restart-pane",
                    "restart_pane",
                    json!({ "pane_id": pane_id }),
                );
            }
            Ok(false)
        }
        Action::FocusNext => {
            let _ = client.request("palette-focus-next", "focus_next", json!({}));
            Ok(false)
        }
        Action::SplitHorizontal | Action::SplitVertical => {
            let direction = if matches!(pressed, Action::SplitHorizontal) {
                "horizontal"
            } else {
                "vertical"
            };
            let cwd = std::env::current_dir()
                .map_err(ClientError::Io)?
                .to_string_lossy()
                .into_owned();
            let _ = client.request("palette-split", "split_pane", json!({ "direction": direction, "command": "powershell.exe", "args": ["-NoLogo", "-NoProfile"], "cwd": cwd, "cols": pane_size(terminal_size).0, "rows": pane_size(terminal_size).1 }));
            Ok(false)
        }
        Action::ResizeSmaller | Action::ResizeLarger => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                let delta = if matches!(pressed, Action::ResizeLarger) {
                    0.05
                } else {
                    -0.05
                };
                let _ = client.request(
                    "palette-resize",
                    "resize_pane",
                    json!({ "pane_id": pane_id, "delta": delta }),
                );
            }
            Ok(false)
        }
        Action::RenameFocusedPane | Action::RenameActiveTab | Action::RenameActiveWorkspace => {
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn current_snapshot(client: &ControlClient) -> Result<SessionSnapshot, ClientError> {
    let response = client.request_with_retry(
        "snapshot",
        "get_snapshot",
        json!({ "client_id": client.client_id() }),
        5,
        Duration::from_millis(50),
    )?;
    serde_json::from_value(response.payload.unwrap_or_default()).map_err(ClientError::Json)
}

fn resize_panes(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    let (cols, rows) = pane_size(terminal_size);
    for pane in &snapshot.panes {
        let _ = client.request_with_retry(
            format!("resize-{}", pane.pane_id),
            "resize_pty",
            json!({
                "pane_id": pane.pane_id,
                "cols": cols,
                "rows": rows,
                "client_id": client.client_id(),
            }),
            2,
            Duration::from_millis(20),
        );
    }
    Ok(())
}

fn pane_size(terminal_size: (u16, u16)) -> (u16, u16) {
    (
        terminal_size.0.max(1),
        terminal_size.1.saturating_sub(1).max(1),
    )
}

fn key_code_bytes(code: KeyCode) -> Option<Vec<u8>> {
    match code {
        KeyCode::Char(character) => Some(character.to_string().into_bytes()),
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![8]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![27]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        _ => None,
    }
}

fn adjacent_space_id(snapshot: &SessionSnapshot, forward: bool) -> Option<String> {
    if snapshot.spaces.len() < 2 {
        return None;
    }
    let index = snapshot
        .spaces
        .iter()
        .position(|space| space.space_id == snapshot.active_space_id)?;
    let next = if forward {
        (index + 1) % snapshot.spaces.len()
    } else {
        (index + snapshot.spaces.len() - 1) % snapshot.spaces.len()
    };
    Some(snapshot.spaces[next].space_id.clone())
}

fn adjacent_tab_id(snapshot: &SessionSnapshot, forward: bool) -> Option<String> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == space.active_workspace_id)?;
    if workspace.tabs.len() < 2 {
        return None;
    }
    let index = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == workspace.active_tab_id)?;
    let next = if forward {
        (index + 1) % workspace.tabs.len()
    } else {
        (index + workspace.tabs.len() - 1) % workspace.tabs.len()
    };
    Some(workspace.tabs[next].tab_id.clone())
}

fn active_tab_id(snapshot: &SessionSnapshot) -> Option<String> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    let workspace = space
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == space.active_workspace_id)?;
    Some(workspace.active_tab_id.clone())
}

#[cfg(test)]
mod tests {
    use super::{active_tab_id, adjacent_space_id, adjacent_tab_id, key_code_bytes, pane_size};
    use crate::server::session::Session;
    use crossterm::event::KeyCode;

    #[test]
    fn common_keys_encode_for_a_pty() {
        assert_eq!(key_code_bytes(KeyCode::Enter), Some(vec![b'\r']));
        assert_eq!(key_code_bytes(KeyCode::Left), Some(b"\x1b[D".to_vec()));
        assert_eq!(pane_size((120, 40)), (120, 39));
        assert_eq!(pane_size((0, 0)), (1, 1));
    }

    #[test]
    fn navigation_wraps_across_tabs_and_spaces() {
        let mut session = Session::default();
        let first_tab = session.create_tab("Logs".into()).unwrap();
        let first_tab_id = first_tab["tab_id"].as_str().unwrap();
        let second_space = session.create_space("Other".into()).unwrap();
        let second_space_id = second_space["space_id"].as_str().unwrap();
        session.switch_space("space-1").unwrap();
        let snapshot = session.snapshot().clone();

        assert_eq!(adjacent_tab_id(&snapshot, false).as_deref(), Some("tab-1"));
        assert_eq!(snapshot.spaces[0].workspaces[0].active_tab_id, first_tab_id);
        assert_eq!(
            adjacent_space_id(&snapshot, false).as_deref(),
            Some(second_space_id)
        );
        assert_eq!(
            adjacent_space_id(&snapshot, true).as_deref(),
            Some(second_space_id)
        );
        assert_eq!(active_tab_id(&snapshot).as_deref(), Some(first_tab_id));
    }
}
