use super::input::{action, is_prefix, Action};
use super::palette::{move_selection, Command};
use super::prompt::{PromptResult, RenamePrompt, RenameTarget};
use super::renderer;
use super::{ClientError, ControlClient};
use crate::model::layout::Direction as SplitDirection;
use crate::server::session::SessionSnapshot;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use serde_json::json;
use std::io::{self, stdout};
use std::time::{Duration, Instant};

const SPLIT_DRAG_INTERVAL: Duration = Duration::from_millis(33);

struct SplitDrag {
    path: Vec<bool>,
    direction: SplitDirection,
    area: Rect,
    grab_offset: i32,
    last_sent_at: Option<Instant>,
}

impl SplitDrag {
    fn ratio_at(&self, mouse: MouseEvent) -> f32 {
        let (pointer, origin, extent) = match self.direction {
            SplitDirection::Horizontal => (
                i32::from(mouse.column),
                i32::from(self.area.x),
                self.area.width,
            ),
            SplitDirection::Vertical => (
                i32::from(mouse.row),
                i32::from(self.area.y),
                self.area.height,
            ),
        };
        ((pointer + self.grab_offset - origin) as f32 / f32::from(extent.max(1))).clamp(0.1, 0.9)
    }
}

pub fn run(address: impl Into<String>) -> Result<(), ClientError> {
    let client = ControlClient::connect(address)?;
    let terminal_size = size().map_err(ClientError::Io)?;
    client.attach_with_terminal(
        terminal_size.0,
        terminal_size.1,
        vec!["mouse".into(), "alternate_screen".into()],
    )?;
    ensure_active_default_pane(&client, terminal_size)?;
    let mut terminal = setup_terminal().map_err(ClientError::Io)?;
    let result = event_loop(&mut terminal, &client);
    restore_terminal(&mut terminal).map_err(ClientError::Io)?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut output = stdout();
    execute!(output, EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(output))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
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
    let mut last_pane_sizes = None;
    let mut split_drag = None;
    let mut was_connected = true;
    let mut snapshot = current_snapshot(client)?;
    loop {
        let terminal_size = size().map_err(ClientError::Io)?;
        let mut connected = match current_snapshot(client) {
            Ok(current) => {
                snapshot = current;
                true
            }
            Err(_) => false,
        };
        if connected && !was_connected {
            connected = client
                .attach_with_terminal(
                    terminal_size.0,
                    terminal_size.1,
                    vec!["mouse".into(), "alternate_screen".into()],
                )
                .is_ok();
            if connected {
                last_pane_sizes = None;
            }
        }
        was_connected = connected;
        let area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
        let pane_sizes = renderer::pane_sizes(&snapshot, renderer::pane_content_area(area));
        if connected && last_pane_sizes.as_ref() != Some(&pane_sizes) {
            resize_panes(client, &pane_sizes)?;
            last_pane_sizes = Some(pane_sizes);
        }
        terminal
            .draw(|frame| {
                renderer::render_with_connection(frame, &snapshot, connected);
                if palette_open {
                    renderer::render_palette(frame, palette_selected);
                }
                if let Some(prompt) = &rename_prompt {
                    let title = match prompt.target {
                        RenameTarget::Pane => "Rename pane",
                        RenameTarget::Tab => "Rename tab",
                        RenameTarget::Workspace => "Rename workspace",
                        RenameTarget::CreateWorkspace => "Create workspace",
                        RenameTarget::Space => "Rename space",
                        RenameTarget::CreateSpace => "Create space",
                        RenameTarget::DeleteWorkspace => "Delete workspace: type its name",
                        RenameTarget::DeleteSpace => "Delete space: type its name",
                        RenameTarget::SwitchWorkspace => "Switch workspace: type its name",
                    };
                    renderer::render_prompt(frame, title, &prompt.input);
                }
            })
            .map_err(ClientError::Io)?;
        if !event::poll(Duration::from_millis(100)).map_err(ClientError::Io)? {
            continue;
        }
        let input = event::read().map_err(ClientError::Io)?;
        let key = match input {
            Event::Mouse(mouse) => {
                handle_mouse(client, &snapshot, mouse, terminal_size, &mut split_drag)?;
                continue;
            }
            Event::Key(key) => key,
            _ => continue,
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
                    submit_rename(client, &snapshot, target, name, terminal_size)?;
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
                ensure_active_default_pane(client, terminal_size)?;
            }
            Action::NewPane => {
                let _ = client.request(
                    "new-pane",
                    "create_pane",
                    pane_request_for_snapshot(&snapshot, terminal_size),
                );
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
                    ensure_active_default_pane(client, terminal_size)?;
                }
            }
            Action::NextSpace | Action::PreviousSpace => {
                if let Some(space_id) =
                    adjacent_space_id(&snapshot, matches!(pressed, Action::NextSpace))
                {
                    let _ =
                        client.request("switch-space", "switch_space", json!({ "id": space_id }));
                    ensure_active_default_pane(client, terminal_size)?;
                }
            }
            Action::NextWorkspace => {
                if let Some(workspace_id) = adjacent_workspace_id(&snapshot) {
                    let _ = client.request(
                        "switch-workspace",
                        "switch_workspace",
                        json!({ "id": workspace_id }),
                    );
                    ensure_active_default_pane(client, terminal_size)?;
                }
            }
            Action::StopFocusedPane => {
                if let Some(ref pane_id) = snapshot.focused_pane_id {
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
            Action::FocusPrevious => {
                let _ = client.request("focus-previous", "focus_previous", json!({}));
            }
            Action::FocusLeft | Action::FocusRight | Action::FocusUp | Action::FocusDown => {
                let _ = client.request(
                    "focus-direction",
                    "focus_direction",
                    json!({ "direction": focus_direction_name(pressed) }),
                );
            }
            Action::SplitHorizontal | Action::SplitVertical => {
                let direction = if matches!(pressed, Action::SplitHorizontal) {
                    "horizontal"
                } else {
                    "vertical"
                };
                let mut request = pane_request_for_snapshot(&snapshot, terminal_size);
                request["direction"] = json!(direction);
                let _ = client.request("split", "split_pane", request);
            }
            Action::ResizeSmaller | Action::ResizeLarger => {
                if let Some(ref pane_id) = snapshot.focused_pane_id {
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
                if let Some(ref pane_id) = snapshot.focused_pane_id {
                    if let Some(bytes) = key_code_bytes(code) {
                        let _ = client.interactive_request(
                            "input",
                            "send_input",
                            json!({ "pane_id": pane_id, "bytes": bytes }),
                        );
                    }
                }
            }
            Action::None => {}
            Action::CommandPalette => {}
            Action::RenameFocusedPane
            | Action::RenameActiveTab
            | Action::RenameActiveWorkspace
            | Action::CreateWorkspace
            | Action::RenameActiveSpace
            | Action::CreateSpace
            | Action::DeleteActiveWorkspace
            | Action::DeleteActiveSpace
            | Action::SwitchWorkspaceByName => {}
        }
        prefix_active = false;
    }
    Ok(())
}

fn handle_mouse(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    mouse: MouseEvent,
    terminal_size: (u16, u16),
    split_drag: &mut Option<SplitDrag>,
) -> Result<(), ClientError> {
    let area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let pane_area = renderer::pane_content_area(area);
            if let Some(handle) = renderer::split_handles(snapshot, pane_area)
                .into_iter()
                .find(|handle| {
                    let point = (mouse.column, mouse.row);
                    point.0 >= handle.hit_rect.x
                        && point.0 < handle.hit_rect.right()
                        && point.1 >= handle.hit_rect.y
                        && point.1 < handle.hit_rect.bottom()
                })
            {
                let pointer = match handle.direction {
                    SplitDirection::Horizontal => mouse.column,
                    SplitDirection::Vertical => mouse.row,
                };
                *split_drag = Some(SplitDrag {
                    path: handle.path,
                    direction: handle.direction,
                    area: handle.area,
                    grab_offset: i32::from(handle.pos) - i32::from(pointer),
                    last_sent_at: None,
                });
                return Ok(());
            }
            *split_drag = None;
        }
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
            let releasing = mouse.kind == MouseEventKind::Up(MouseButton::Left);
            if let Some(drag) = split_drag.as_mut() {
                let now = Instant::now();
                let due = drag
                    .last_sent_at
                    .is_none_or(|last| now.duration_since(last) >= SPLIT_DRAG_INTERVAL);
                if releasing || due {
                    client.request(
                        "mouse-set-split-ratio",
                        "set_split_ratio",
                        json!({ "path": drag.path, "ratio": drag.ratio_at(mouse) }),
                    )?;
                    drag.last_sent_at = Some(now);
                }
                if releasing {
                    *split_drag = None;
                }
            }
            return Ok(());
        }
        _ => return Ok(()),
    }
    let Some(target) = renderer::hit_test(snapshot, area, mouse) else {
        return Ok(());
    };
    match target {
        renderer::ClickTarget::SplitBorder(_) => {}
        renderer::ClickTarget::Space(space_id) => {
            let response = client.request(
                "mouse-switch-space",
                "switch_space",
                json!({ "id": space_id }),
            )?;
            if response.ok {
                ensure_active_default_pane(client, terminal_size)?;
            }
        }
        renderer::ClickTarget::Workspace {
            space_id,
            workspace_id,
        } => {
            let space = client.request(
                "mouse-switch-workspace-space",
                "switch_space",
                json!({ "id": space_id }),
            )?;
            if space.ok {
                let workspace = client.request(
                    "mouse-switch-workspace",
                    "switch_workspace",
                    json!({ "id": workspace_id }),
                )?;
                if workspace.ok {
                    ensure_active_default_pane(client, terminal_size)?;
                }
            }
        }
        renderer::ClickTarget::Tab(tab_id) => {
            let response =
                client.request("mouse-switch-tab", "switch_tab", json!({ "id": tab_id }))?;
            if response.ok {
                ensure_active_default_pane(client, terminal_size)?;
            }
        }
        renderer::ClickTarget::Pane(pane_id) => {
            client.request(
                "mouse-focus-pane",
                "focus_pane",
                json!({ "pane_id": pane_id }),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
fn reconnect_requires_reattach(was_connected: bool, connected: bool) -> bool {
    connected && !was_connected
}

fn rename_target(action: Action) -> Option<RenameTarget> {
    match action {
        Action::RenameFocusedPane => Some(RenameTarget::Pane),
        Action::RenameActiveTab => Some(RenameTarget::Tab),
        Action::RenameActiveWorkspace => Some(RenameTarget::Workspace),
        Action::CreateWorkspace => Some(RenameTarget::CreateWorkspace),
        Action::RenameActiveSpace => Some(RenameTarget::Space),
        Action::CreateSpace => Some(RenameTarget::CreateSpace),
        Action::DeleteActiveWorkspace => Some(RenameTarget::DeleteWorkspace),
        Action::DeleteActiveSpace => Some(RenameTarget::DeleteSpace),
        Action::SwitchWorkspaceByName => Some(RenameTarget::SwitchWorkspace),
        _ => None,
    }
}

fn submit_rename(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    target: RenameTarget,
    name: String,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    if matches!(
        target,
        RenameTarget::DeleteWorkspace | RenameTarget::DeleteSpace
    ) {
        let (operation, id, expected_name) = match target {
            RenameTarget::DeleteWorkspace => {
                let workspace = active_workspace(snapshot);
                (
                    "delete_workspace",
                    workspace.map(|workspace| workspace.workspace_id.clone()),
                    workspace.map(|workspace| workspace.name.clone()),
                )
            }
            RenameTarget::DeleteSpace => {
                let space = snapshot
                    .spaces
                    .iter()
                    .find(|space| space.space_id == snapshot.active_space_id);
                (
                    "delete_space",
                    space.map(|space| space.space_id.clone()),
                    space.map(|space| space.name.clone()),
                )
            }
            _ => unreachable!(),
        };
        if expected_name.as_deref() == Some(name.as_str()) {
            if let Some(id) = id {
                let _ = client.request(
                    format!("confirm-{operation}"),
                    operation,
                    json!({ "id": id }),
                )?;
            }
        }
        return Ok(());
    }
    let (operation, id) = match target {
        RenameTarget::Pane => ("rename_pane", snapshot.focused_pane_id.clone()),
        RenameTarget::Tab => ("rename_tab", active_tab_id(snapshot)),
        RenameTarget::Workspace => ("rename_workspace", active_workspace_id(snapshot)),
        RenameTarget::CreateWorkspace => {
            let repository_path = std::env::current_dir()
                .map_err(ClientError::Io)?
                .to_string_lossy()
                .into_owned();
            let _ = client.request(
                "create-workspace",
                "create_workspace",
                json!({ "name": name, "repository_path": repository_path }),
            )?;
            ensure_active_default_pane(client, terminal_size)?;
            return Ok(());
        }
        RenameTarget::Space => ("rename_space", Some(snapshot.active_space_id.clone())),
        RenameTarget::CreateSpace => {
            let _ = client.request("create-space", "create_space", json!({ "name": name }))?;
            ensure_active_default_pane(client, terminal_size)?;
            return Ok(());
        }
        RenameTarget::DeleteWorkspace | RenameTarget::DeleteSpace => unreachable!(),
        RenameTarget::SwitchWorkspace => {
            if let Some(id) = workspace_id_by_name(snapshot, &name) {
                let _ = client.request(
                    "switch-workspace-by-name",
                    "switch_workspace",
                    json!({ "id": id }),
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            return Ok(());
        }
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

fn active_workspace(snapshot: &SessionSnapshot) -> Option<&crate::server::session::WorkspaceView> {
    snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .and_then(|space| {
            space
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == space.active_workspace_id)
        })
}

fn active_workspace_id(snapshot: &SessionSnapshot) -> Option<String> {
    snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .map(|space| space.active_workspace_id.clone())
}

fn workspace_id_by_name(snapshot: &SessionSnapshot, name: &str) -> Option<String> {
    snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .and_then(|space| {
            space
                .workspaces
                .iter()
                .find(|workspace| workspace.name == name)
        })
        .map(|workspace| workspace.workspace_id.clone())
}

fn adjacent_workspace_id(snapshot: &SessionSnapshot) -> Option<String> {
    let space = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)?;
    if space.workspaces.len() < 2 {
        return None;
    }
    let index = space
        .workspaces
        .iter()
        .position(|workspace| workspace.workspace_id == space.active_workspace_id)?;
    Some(
        space.workspaces[(index + 1) % space.workspaces.len()]
            .workspace_id
            .clone(),
    )
}

fn focus_direction_name(action: Action) -> &'static str {
    match action {
        Action::FocusLeft => "left",
        Action::FocusRight => "right",
        Action::FocusUp => "up",
        Action::FocusDown => "down",
        _ => unreachable!("not a directional focus action"),
    }
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
            ensure_active_default_pane(client, terminal_size)?;
            Ok(false)
        }
        Action::NewPane => {
            let _ = client.request(
                "palette-new-pane",
                "create_pane",
                pane_request_for_snapshot(snapshot, terminal_size),
            );
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
                ensure_active_default_pane(client, terminal_size)?;
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
                ensure_active_default_pane(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::NextWorkspace => {
            if let Some(workspace_id) = adjacent_workspace_id(snapshot) {
                let _ = client.request(
                    "palette-switch-workspace",
                    "switch_workspace",
                    json!({ "id": workspace_id }),
                );
                ensure_active_default_pane(client, terminal_size)?;
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
        Action::FocusPrevious => {
            let _ = client.request("palette-focus-previous", "focus_previous", json!({}));
            Ok(false)
        }
        Action::FocusLeft | Action::FocusRight | Action::FocusUp | Action::FocusDown => {
            let _ = client.request(
                "palette-focus-direction",
                "focus_direction",
                json!({ "direction": focus_direction_name(pressed) }),
            );
            Ok(false)
        }
        Action::SplitHorizontal | Action::SplitVertical => {
            let direction = if matches!(pressed, Action::SplitHorizontal) {
                "horizontal"
            } else {
                "vertical"
            };
            let mut request = pane_request_for_snapshot(snapshot, terminal_size);
            request["direction"] = json!(direction);
            let _ = client.request("palette-split", "split_pane", request);
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
        Action::RenameFocusedPane
        | Action::RenameActiveTab
        | Action::RenameActiveWorkspace
        | Action::CreateWorkspace
        | Action::RenameActiveSpace
        | Action::CreateSpace
        | Action::DeleteActiveWorkspace
        | Action::DeleteActiveSpace
        | Action::SwitchWorkspaceByName => Ok(false),
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
    pane_sizes: &[renderer::PaneSize],
) -> Result<(), ClientError> {
    for pane in pane_sizes {
        let _ = client.interactive_request(
            format!("resize-{}", pane.pane_id),
            "resize_pty",
            json!({
            "pane_id": pane.pane_id,
            "cols": pane.cols,
            "rows": pane.rows,
            "client_id": client.client_id(),
                }),
        );
    }
    Ok(())
}

fn pane_size(terminal_size: (u16, u16)) -> (u16, u16) {
    let area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
    renderer::pane_inner_size(renderer::pane_content_area(area))
}

fn pane_request(terminal_size: (u16, u16)) -> serde_json::Value {
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".into());
    let (cols, rows) = pane_size(terminal_size);
    json!({
        "command": "powershell.exe",
        "args": ["-NoLogo", "-NoProfile"],
        "cwd": cwd,
        "cols": cols,
        "rows": rows
    })
}

fn pane_request_for_snapshot(
    snapshot: &SessionSnapshot,
    terminal_size: (u16, u16),
) -> serde_json::Value {
    let mut request = pane_request(terminal_size);
    if let Some(repository_path) =
        active_workspace(snapshot).and_then(|workspace| workspace.repository_path.as_deref())
    {
        request["cwd"] = json!(repository_path);
    }
    request
}

fn ensure_active_default_pane(
    client: &ControlClient,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    let snapshot = current_snapshot(client)?;
    client.request(
        "ensure-default-pane",
        "ensure_active_pane",
        pane_request_for_snapshot(&snapshot, terminal_size),
    )?;
    Ok(())
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
    use super::{
        active_tab_id, adjacent_space_id, adjacent_tab_id, adjacent_workspace_id, key_code_bytes,
        pane_size, reconnect_requires_reattach, workspace_id_by_name, SplitDirection, SplitDrag,
    };
    use crate::server::session::Session;
    use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    #[test]
    fn common_keys_encode_for_a_pty() {
        assert_eq!(key_code_bytes(KeyCode::Enter), Some(vec![b'\r']));
        assert_eq!(key_code_bytes(KeyCode::Left), Some(b"\x1b[D".to_vec()));
        assert_eq!(pane_size((120, 40)), (90, 36));
        assert_eq!(pane_size((0, 0)), (1, 1));
    }

    #[test]
    fn split_drag_maps_pointer_position_to_a_clamped_ratio() {
        let drag = SplitDrag {
            path: Vec::new(),
            direction: SplitDirection::Horizontal,
            area: Rect::new(10, 4, 80, 20),
            grab_offset: 0,
            last_sent_at: None,
        };
        let mouse = |column| MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column,
            row: 10,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        assert!((drag.ratio_at(mouse(58)) - 0.6).abs() < f32::EPSILON);
        assert!((drag.ratio_at(mouse(0)) - 0.1).abs() < f32::EPSILON);
        assert!((drag.ratio_at(mouse(100)) - 0.9).abs() < f32::EPSILON);
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

    #[test]
    fn workspace_navigation_wraps() {
        let mut session = Session::default();
        session.create_workspace("Feature".into()).unwrap();
        let snapshot = session.snapshot().clone();
        assert_eq!(
            adjacent_workspace_id(&snapshot).as_deref(),
            Some("workspace-1")
        );
    }

    #[test]
    fn workspace_can_be_found_by_name_in_active_space() {
        let mut session = Session::default();
        let created = session.create_workspace("Feature".into()).unwrap();
        let id = created["workspace_id"].as_str().unwrap();
        assert_eq!(
            workspace_id_by_name(session.snapshot(), "Feature").as_deref(),
            Some(id)
        );
        assert_eq!(workspace_id_by_name(session.snapshot(), "Missing"), None);
    }

    #[test]
    fn reconnect_transition_requires_a_new_attach() {
        assert!(reconnect_requires_reattach(false, true));
        assert!(!reconnect_requires_reattach(true, true));
        assert!(!reconnect_requires_reattach(false, false));
    }
}
