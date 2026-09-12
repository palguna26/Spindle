use super::context_menu::{ContextMenu, ContextMenuAction, ContextMenuTarget};
use super::input::{action, is_prefix, Action};
use super::palette::{move_selection, Command};
use super::prompt::{PromptResult, RenamePrompt, RenameTarget};
use super::renderer;
use super::selection::TextSelection;
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
use std::collections::HashMap;
use std::io::{self, stdout};
use std::time::{Duration, Instant};

const SPLIT_DRAG_INTERVAL: Duration = Duration::from_millis(33);
const WHEEL_SCROLL_LINES: usize = 3;
const MAX_SCROLLBACK_ROWS: usize = 4096;
const ACTION_ERROR_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupErrorAction {
    Retry,
    Detach,
    Ignore,
}

fn startup_error_action(key: KeyCode) -> StartupErrorAction {
    match key {
        KeyCode::Char('r') | KeyCode::Enter => StartupErrorAction::Retry,
        KeyCode::Esc | KeyCode::Char('q') => StartupErrorAction::Detach,
        _ => StartupErrorAction::Ignore,
    }
}

struct SplitDrag {
    path: Vec<bool>,
    direction: SplitDirection,
    area: Rect,
    grab_offset: i32,
    last_sent_at: Option<Instant>,
}

struct PaneMouseCapture {
    pane_id: String,
    rect: Rect,
    button: MouseButton,
}

#[derive(Default)]
struct MouseState {
    sidebar_collapsed: bool,
    agent_priority_sort: bool,
    sidebar_scroll: usize,
    sidebar_scroll_drag: Option<u16>,
    preferences_path: std::path::PathBuf,
    split_drag: Option<SplitDrag>,
    pane_capture: Option<PaneMouseCapture>,
    selection: Option<TextSelection>,
    last_click: Option<PaneClick>,
    scroll_offsets: HashMap<String, usize>,
    scrollback_views: HashMap<String, CachedScrollbackView>,
}

struct PaneClick {
    pane_id: String,
    row: u16,
    col: u16,
    at: Instant,
}

struct CachedScrollbackView {
    bytes: Vec<u8>,
    rows: u16,
    cols: u16,
    offset: usize,
    screen: String,
}

impl PaneClick {
    fn is_double_click_for(&self, pane_id: &str, row: u16, col: u16, now: Instant) -> bool {
        self.pane_id == pane_id
            && now.duration_since(self.at) <= Duration::from_millis(350)
            && self.row.abs_diff(row) <= 1
            && self.col.abs_diff(col) <= 1
    }
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

pub fn run(
    address: impl Into<String>,
    state_dir: impl AsRef<std::path::Path>,
) -> Result<(), ClientError> {
    let client = ControlClient::connect(address)?;
    let preferences_path = state_dir.as_ref().join("client.json");
    let preferences = super::preferences::load(&preferences_path);
    let terminal_size = size().map_err(ClientError::Io)?;
    client.attach_with_terminal(
        terminal_size.0,
        terminal_size.1,
        vec!["mouse".into(), "alternate_screen".into()],
    )?;
    let mut terminal = setup_terminal().map_err(ClientError::Io)?;
    let startup_error = ensure_active_default_pane(&client, terminal_size)
        .err()
        .map(startup_error_message);
    let result = event_loop(
        &mut terminal,
        &client,
        startup_error,
        preferences.sidebar_collapsed,
        preferences.agent_priority_sort,
        &preferences_path,
    );
    restore_terminal(&mut terminal).map_err(ClientError::Io)?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut output = stdout();
    execute!(
        output,
        EnterAlternateScreen,
        EnableMouseCapture,
        crossterm::style::Print("\x1b[?1002h\x1b[?1003h")
    )?;
    Terminal::new(CrosstermBackend::new(output))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        crossterm::style::Print("\x1b[?1003l\x1b[?1002l"),
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: &ControlClient,
    mut startup_error: Option<String>,
    sidebar_collapsed: bool,
    agent_priority_sort: bool,
    preferences_path: &std::path::Path,
) -> Result<(), ClientError> {
    let mut prefix_active = false;
    let mut palette_selected = 0;
    let mut palette_open = false;
    let mut rename_prompt: Option<RenamePrompt> = None;
    let mut context_menu: Option<ContextMenu> = None;
    let mut help_open = false;
    let mut last_pane_sizes = None;
    let mut mouse_state = MouseState {
        sidebar_collapsed,
        agent_priority_sort,
        preferences_path: preferences_path.to_path_buf(),
        ..MouseState::default()
    };
    let mut was_connected = true;
    let mut snapshot = current_snapshot(client)?;
    let mut action_error: Option<(String, Instant)> = None;
    loop {
        if action_error
            .as_ref()
            .is_some_and(|(_, expires_at)| Instant::now() >= *expires_at)
        {
            action_error = None;
        }
        let terminal_size = size().map_err(ClientError::Io)?;
        let mut connected = match current_snapshot(client) {
            Ok(current) => {
                snapshot = current;
                true
            }
            Err(_) => false,
        };
        apply_scrollback_views(
            &mut snapshot,
            &mut mouse_state.scroll_offsets,
            &mut mouse_state.scrollback_views,
        );
        if snapshot_has_focused_pane(&snapshot) {
            startup_error = None;
        } else if connected && startup_error.is_none() {
            startup_error = ensure_active_default_pane(client, terminal_size)
                .err()
                .map(startup_error_message);
        }
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
        mouse_state.sidebar_scroll =
            mouse_state
                .sidebar_scroll
                .min(renderer::sidebar_scroll_max_with_sort(
                    &snapshot,
                    area,
                    mouse_state.sidebar_collapsed,
                    mouse_state.agent_priority_sort,
                ));
        let pane_sizes = renderer::pane_sizes(
            &snapshot,
            renderer::pane_content_area_with_sidebar(area, mouse_state.sidebar_collapsed),
        );
        if connected && last_pane_sizes.as_ref() != Some(&pane_sizes) {
            resize_panes(client, &pane_sizes)?;
            last_pane_sizes = Some(pane_sizes);
        }
        let focused_pane_scrolled = snapshot
            .focused_pane_id
            .as_ref()
            .and_then(|pane_id| mouse_state.scroll_offsets.get(pane_id))
            .is_some_and(|offset| *offset > 0);
        let show_host_cursor = !focused_pane_scrolled
            && mouse_state.selection.is_none()
            && !palette_open
            && !help_open
            && rename_prompt.is_none()
            && context_menu.is_none()
            && startup_error.is_none();
        terminal
            .draw(|frame| {
                renderer::render_with_sidebar_scroll_and_cursor_and_agent_sort(
                    frame,
                    &snapshot,
                    connected,
                    mouse_state.sidebar_collapsed,
                    mouse_state.sidebar_scroll,
                    show_host_cursor,
                    mouse_state.agent_priority_sort,
                );
                if let Some(selection) = &mouse_state.selection {
                    renderer::render_selection_with_sidebar(
                        frame,
                        &snapshot,
                        selection,
                        mouse_state.sidebar_collapsed,
                    );
                }
                if palette_open {
                    renderer::render_palette(frame, palette_selected);
                }
                if help_open {
                    renderer::render_help(frame);
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
                if let Some(menu) = &context_menu {
                    renderer::render_context_menu(frame, menu);
                }
                if let Some(error) = &startup_error {
                    renderer::render_startup_error(frame, error);
                } else if let Some((error, _)) = &action_error {
                    renderer::render_action_error(frame, error);
                }
            })
            .map_err(ClientError::Io)?;
        if !event::poll(Duration::from_millis(100)).map_err(ClientError::Io)? {
            continue;
        }
        let input = event::read().map_err(ClientError::Io)?;
        let key = match input {
            Event::Mouse(mouse) => {
                if rename_prompt.is_some() {
                    continue;
                }
                if startup_error.is_some() {
                    continue;
                }
                if help_open {
                    help_open = false;
                    continue;
                }
                if let Err(error) = handle_mouse(
                    client,
                    &snapshot,
                    mouse,
                    terminal_size,
                    &mut mouse_state,
                    &mut context_menu,
                    &mut rename_prompt,
                ) {
                    record_action_error(&mut action_error, "handle mouse action", Err(error));
                }
                continue;
            }
            Event::Key(key) => key,
            _ => continue,
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if startup_error.is_some() {
            match startup_error_action(key.code) {
                StartupErrorAction::Retry => {
                    startup_error = ensure_active_default_pane(client, terminal_size)
                        .err()
                        .map(startup_error_message);
                }
                StartupErrorAction::Detach => {
                    let _ = client.detach();
                    break;
                }
                StartupErrorAction::Ignore => {}
            }
            continue;
        }
        mouse_state.selection = None;
        mouse_state.last_click = None;
        mouse_state.scroll_offsets.clear();
        if let Some(prompt) = &mut rename_prompt {
            match prompt.apply_key(key.code) {
                PromptResult::Continue => {}
                PromptResult::Cancel => rename_prompt = None,
                PromptResult::Submit(name) => {
                    let target = prompt.target;
                    rename_prompt = None;
                    record_action_error(
                        &mut action_error,
                        "save name",
                        submit_rename(client, &snapshot, target, name, terminal_size),
                    );
                }
            }
            continue;
        }
        if let Some(menu) = context_menu.as_mut() {
            match key.code {
                KeyCode::Esc => context_menu = None,
                KeyCode::Up => menu.move_selection(-1),
                KeyCode::Down => menu.move_selection(1),
                KeyCode::Enter => {
                    let selected = menu.selected;
                    if let Some(action) = menu.items().get(selected).map(|(_, action)| *action) {
                        let menu = context_menu.take().expect("menu exists");
                        match activate_context_menu(client, &snapshot, menu, action, terminal_size)
                        {
                            Ok(prompt) => rename_prompt = prompt.map(RenamePrompt::new),
                            Err(error) => record_action_error(
                                &mut action_error,
                                "run pane menu action",
                                Err(error),
                            ),
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if help_open {
            help_open = false;
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
                match execute_action(command.action(), client, &snapshot, terminal_size) {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(error) => record_action_error(
                        &mut action_error,
                        "run command palette action",
                        Err(error),
                    ),
                }
            }
            continue;
        }
        if is_prefix(key) {
            prefix_active = true;
            continue;
        }
        if !prefix_active
            && key.modifiers.is_empty()
            && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
        {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                if let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) {
                    if pane.plain_page_keys_use_host_scrollback() {
                        let lines = usize::from(pane.rows.saturating_sub(1).max(1));
                        let offset = mouse_state
                            .scroll_offsets
                            .entry(pane_id.to_owned())
                            .or_default();
                        *offset =
                            adjust_scrollback_offset(*offset, key.code == KeyCode::PageUp, lines);
                    } else if let Some(bytes) = page_key_bytes(key.code) {
                        let _ = client.interactive_request(
                            "page-key-input",
                            "send_input",
                            json!({ "pane_id": pane_id, "bytes": bytes }),
                        );
                    }
                    continue;
                }
            }
        }
        let pressed = action(prefix_active, key);
        if pressed == Action::CommandPalette {
            palette_open = true;
            palette_selected = 0;
            prefix_active = false;
            continue;
        }
        if pressed == Action::Help {
            help_open = true;
            prefix_active = false;
            continue;
        }
        if pressed == Action::ToggleSidebar {
            mouse_state.sidebar_collapsed = !mouse_state.sidebar_collapsed;
            mouse_state.selection = None;
            mouse_state.last_click = None;
            let _ = super::preferences::store(
                &mouse_state.preferences_path,
                super::preferences::ClientPreferences {
                    sidebar_collapsed: mouse_state.sidebar_collapsed,
                    agent_priority_sort: mouse_state.agent_priority_sort,
                },
            );
            prefix_active = false;
            continue;
        }
        match pressed {
            Action::Detach => {
                let result = client
                    .detach()
                    .and_then(|response| require_server_success(&response, "detach"));
                match result {
                    Ok(()) => break,
                    Err(error) => record_action_error(&mut action_error, "detach", Err(error)),
                }
            }
            Action::NewTab => {
                let result = if active_workspace(&snapshot).is_some() {
                    request_action(
                        client,
                        "new-tab",
                        "create_tab",
                        json!({ "name": "Activity" }),
                        "create tab",
                    )
                    .and_then(|()| ensure_active_default_pane(client, terminal_size))
                } else {
                    create_workspace_from_current_directory(client, terminal_size)
                };
                record_action_error(&mut action_error, "create tab", result);
            }
            Action::NewPane => {
                record_action_error(
                    &mut action_error,
                    "create pane",
                    request_action(
                        client,
                        "new-pane",
                        "create_pane",
                        pane_request_for_snapshot(&snapshot, terminal_size),
                        "create pane",
                    ),
                );
            }
            Action::ClosePane => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    let result = request_action(
                        client,
                        "keyboard-close-pane",
                        "close_pane",
                        json!({ "pane_id": pane_id }),
                        "close pane",
                    )
                    .and_then(|()| ensure_active_default_pane(client, terminal_size));
                    record_action_error(&mut action_error, "close pane", result);
                }
            }
            Action::CloseTab => {
                if let Some(tab_id) = active_tab_id(&snapshot) {
                    let result = request_action(
                        client,
                        "close-tab",
                        "close_tab",
                        json!({ "id": tab_id }),
                        "close tab",
                    )
                    .and_then(|()| ensure_active_default_pane(client, terminal_size));
                    record_action_error(&mut action_error, "close tab", result);
                }
            }
            Action::NextTab | Action::PreviousTab => {
                if let Some(tab_id) = adjacent_tab_id(&snapshot, matches!(pressed, Action::NextTab))
                {
                    let result = request_action(
                        client,
                        "switch-tab",
                        "switch_tab",
                        json!({ "id": tab_id }),
                        "switch tab",
                    )
                    .and_then(|()| ensure_active_default_pane(client, terminal_size));
                    record_action_error(&mut action_error, "switch tab", result);
                }
            }
            Action::NextSpace | Action::PreviousSpace => {
                if let Some(space_id) =
                    adjacent_space_id(&snapshot, matches!(pressed, Action::NextSpace))
                {
                    let result = request_action(
                        client,
                        "switch-space",
                        "switch_space",
                        json!({ "id": space_id }),
                        "switch space",
                    )
                    .and_then(|()| ensure_active_default_pane(client, terminal_size));
                    record_action_error(&mut action_error, "switch space", result);
                }
            }
            Action::NextWorkspace => {
                if let Some(workspace_id) = adjacent_workspace_id(&snapshot) {
                    let result = request_action(
                        client,
                        "switch-workspace",
                        "switch_workspace",
                        json!({ "id": workspace_id }),
                        "switch workspace",
                    )
                    .and_then(|()| ensure_active_default_pane(client, terminal_size));
                    record_action_error(&mut action_error, "switch workspace", result);
                }
            }
            Action::StopFocusedPane => {
                if let Some(ref pane_id) = snapshot.focused_pane_id {
                    record_action_error(
                        &mut action_error,
                        "stop pane",
                        request_action(
                            client,
                            "stop-pane",
                            "stop_pane",
                            json!({ "pane_id": pane_id }),
                            "stop pane",
                        ),
                    );
                }
            }
            Action::RestartFocusedPane => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    record_action_error(
                        &mut action_error,
                        "restart pane",
                        request_action(
                            client,
                            "restart-pane",
                            "restart_pane",
                            json!({ "pane_id": pane_id }),
                            "restart pane",
                        ),
                    );
                }
            }
            Action::FocusNext => {
                record_action_error(
                    &mut action_error,
                    "focus next pane",
                    request_action(
                        client,
                        "focus-next",
                        "focus_next",
                        json!({}),
                        "focus next pane",
                    ),
                );
            }
            Action::FocusPrevious => {
                record_action_error(
                    &mut action_error,
                    "focus previous pane",
                    request_action(
                        client,
                        "focus-previous",
                        "focus_previous",
                        json!({}),
                        "focus previous pane",
                    ),
                );
            }
            Action::FocusLeft | Action::FocusRight | Action::FocusUp | Action::FocusDown => {
                record_action_error(
                    &mut action_error,
                    "focus pane",
                    request_action(
                        client,
                        "focus-direction",
                        "focus_direction",
                        json!({ "direction": focus_direction_name(pressed) }),
                        "focus pane",
                    ),
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
                record_action_error(
                    &mut action_error,
                    "split pane",
                    request_action(client, "split", "split_pane", request, "split pane"),
                );
            }
            Action::ResizeSmaller | Action::ResizeLarger => {
                if let Some(ref pane_id) = snapshot.focused_pane_id {
                    let delta = if matches!(pressed, Action::ResizeLarger) {
                        0.05
                    } else {
                        -0.05
                    };
                    record_action_error(
                        &mut action_error,
                        "resize pane",
                        request_action(
                            client,
                            "resize",
                            "resize_pane",
                            json!({ "pane_id": pane_id, "delta": delta }),
                            "resize pane",
                        ),
                    );
                }
            }
            Action::ToggleZoom => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    record_action_error(
                        &mut action_error,
                        "toggle pane zoom",
                        request_action(
                            client,
                            "toggle-zoom",
                            "toggle_pane_zoom",
                            json!({ "pane_id": pane_id }),
                            "toggle pane zoom",
                        ),
                    );
                }
            }
            Action::ToggleSidebar => {}
            Action::ToggleRightClickPassthrough => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    record_action_error(
                        &mut action_error,
                        "toggle right-click passthrough",
                        request_action(
                            client,
                            "toggle-right-click",
                            "toggle_right_click_passthrough",
                            json!({ "pane_id": pane_id }),
                            "toggle right-click passthrough",
                        ),
                    );
                }
            }
            Action::ClearPaneName => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    record_action_error(
                        &mut action_error,
                        "clear pane name",
                        request_action(
                            client,
                            "clear-pane-name",
                            "rename_pane",
                            json!({ "pane_id": pane_id, "label": "" }),
                            "clear pane name",
                        ),
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
            Action::Help => {}
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
    mouse_state: &mut MouseState,
    context_menu: &mut Option<ContextMenu>,
    rename_prompt: &mut Option<RenamePrompt>,
) -> Result<(), ClientError> {
    let area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
    if matches!(
        mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    ) {
        if renderer::sidebar_scroll_region(
            area,
            mouse_state.sidebar_collapsed,
            mouse.column,
            mouse.row,
        ) {
            let max_scroll = renderer::sidebar_scroll_max_with_sort(
                snapshot,
                area,
                mouse_state.sidebar_collapsed,
                mouse_state.agent_priority_sort,
            );
            mouse_state.sidebar_scroll = if mouse.kind == MouseEventKind::ScrollUp {
                mouse_state.sidebar_scroll.saturating_sub(1)
            } else {
                mouse_state.sidebar_scroll.saturating_add(1).min(max_scroll)
            };
            return Ok(());
        }
        if forward_mouse_to_pane(
            client,
            snapshot,
            area,
            mouse,
            &mut mouse_state.pane_capture,
            mouse_state.sidebar_collapsed,
        )? {
            return Ok(());
        }
        if let Some(renderer::ClickTarget::Pane(pane_id)) =
            renderer::hit_test_with_sidebar(snapshot, area, mouse, mouse_state.sidebar_collapsed)
        {
            if let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) {
                if !pane.alternate_screen {
                    let offset = mouse_state.scroll_offsets.entry(pane_id).or_default();
                    *offset = adjust_scrollback_offset(
                        *offset,
                        mouse.kind == MouseEventKind::ScrollUp,
                        WHEEL_SCROLL_LINES,
                    );
                }
            }
            return Ok(());
        }
    }
    if mouse.kind == MouseEventKind::Down(MouseButton::Right) {
        mouse_state.split_drag = None;
        mouse_state.sidebar_scroll_drag = None;
        mouse_state.selection = None;
        mouse_state.last_click = None;
        if context_menu.is_some() {
            *context_menu = None;
        } else if forward_mouse_to_pane(
            client,
            snapshot,
            area,
            mouse,
            &mut mouse_state.pane_capture,
            mouse_state.sidebar_collapsed,
        )? {
            return Ok(());
        } else {
            let target = renderer::hit_test_with_sidebar_scroll_and_sort(
                snapshot,
                area,
                mouse,
                mouse_state.sidebar_collapsed,
                mouse_state.sidebar_scroll,
                mouse_state.agent_priority_sort,
            );
            let agent_pane = if let Some(renderer::ClickTarget::Agent {
                space_id,
                workspace_id,
                tab_id,
                pane_id,
            }) = &target
            {
                activate_sidebar_agent(client, space_id, workspace_id, tab_id, pane_id)?;
                Some(pane_id.clone())
            } else {
                None
            };
            *context_menu = target.and_then(|target| {
                let target = match target {
                    renderer::ClickTarget::Agent { pane_id, .. } => {
                        renderer::ClickTarget::Pane(pane_id)
                    }
                    target => target,
                };
                let has_manual_label = match &target {
                    renderer::ClickTarget::Pane(pane_id) => snapshot
                        .panes
                        .iter()
                        .find(|pane| pane.pane_id == *pane_id)
                        .is_some_and(|pane| pane.label.is_some()),
                    _ => false,
                };
                ContextMenu::from_target(target, mouse.column, mouse.row).map(|mut menu| {
                    menu.has_manual_label = has_manual_label;
                    menu.source_pane_id = agent_pane
                        .clone()
                        .or_else(|| snapshot.focused_pane_id.clone());
                    menu
                })
            });
        }
        return Ok(());
    }
    if let Some(menu) = context_menu.as_ref() {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let action = menu.action_at(area, mouse.column, mouse.row);
            if let Some(action) = action {
                let menu = context_menu.take().expect("menu exists");
                *rename_prompt =
                    activate_context_menu(client, snapshot, menu, action, terminal_size)?
                        .map(RenamePrompt::new);
            } else {
                *context_menu = None;
            }
        }
        return Ok(());
    }
    if mouse.kind == MouseEventKind::Down(MouseButton::Left)
        && mouse
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
    {
        if let Some(url) = visible_web_url_at_point(
            snapshot,
            renderer::pane_content_area_with_sidebar(area, mouse_state.sidebar_collapsed),
            mouse.column,
            mouse.row,
        ) {
            let _ = super::links::open_web_url(&url);
            return Ok(());
        }
    }
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        mouse_state.sidebar_scroll_drag = None;
        if let Some(grab_row_offset) = renderer::sidebar_scroll_thumb_grab_offset_with_sort(
            snapshot,
            area,
            mouse_state.sidebar_collapsed,
            mouse_state.sidebar_scroll,
            mouse.column,
            mouse.row,
            mouse_state.agent_priority_sort,
        ) {
            mouse_state.sidebar_scroll_drag = Some(grab_row_offset);
            return Ok(());
        }
        let pane_area =
            renderer::pane_content_area_with_sidebar(area, mouse_state.sidebar_collapsed);
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
            mouse_state.pane_capture = None;
            mouse_state.last_click = None;
            mouse_state.split_drag = Some(SplitDrag {
                path: handle.path,
                direction: handle.direction,
                area: handle.area,
                grab_offset: i32::from(handle.pos) - i32::from(pointer),
                last_sent_at: None,
            });
            return Ok(());
        }
        mouse_state.split_drag = None;
        mouse_state.selection = None;
        if begin_text_selection(
            client,
            snapshot,
            area,
            mouse,
            mouse_state,
            mouse_state.sidebar_collapsed,
        )? {
            return Ok(());
        }
    }
    if matches!(
        mouse.kind,
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
    ) && mouse_state.sidebar_scroll_drag.is_some()
    {
        mouse_state.sidebar_scroll = renderer::sidebar_scroll_offset_from_drag_row_with_sort(
            snapshot,
            area,
            mouse_state.sidebar_collapsed,
            mouse.row,
            mouse_state.sidebar_scroll_drag.unwrap_or_default(),
            mouse_state.agent_priority_sort,
        );
        if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
            mouse_state.sidebar_scroll_drag = None;
        }
        return Ok(());
    }
    if matches!(
        mouse.kind,
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
    ) && mouse_state.split_drag.is_some()
    {
        let releasing = mouse.kind == MouseEventKind::Up(MouseButton::Left);
        if let Some(drag) = mouse_state.split_drag.as_mut() {
            let now = Instant::now();
            let due = drag
                .last_sent_at
                .is_none_or(|last| now.duration_since(last) >= SPLIT_DRAG_INTERVAL);
            if releasing || due {
                request_action(
                    client,
                    "mouse-set-split-ratio",
                    "set_split_ratio",
                    json!({ "path": drag.path, "ratio": drag.ratio_at(mouse) }),
                    "resize split",
                )?;
                drag.last_sent_at = Some(now);
            }
            if releasing {
                mouse_state.split_drag = None;
            }
        }
        return Ok(());
    }
    if matches!(
        mouse.kind,
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
    ) && mouse_state.selection.is_some()
    {
        let mut selection = mouse_state.selection.take().expect("selection exists");
        if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
            selection.word_selection = false;
            selection.drag(mouse.column, mouse.row);
            mouse_state.selection = Some(selection);
        } else {
            if !selection.word_selection {
                selection.drag(mouse.column, mouse.row);
            }
            let text = selection.text(
                snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == selection.pane_id)
                    .map(|pane| pane.screen.as_str())
                    .unwrap_or_default(),
            );
            if selection.has_range() && !text.is_empty() {
                let _ = super::clipboard::copy_text(&text);
            }
        }
        return Ok(());
    }
    if forward_mouse_to_pane(
        client,
        snapshot,
        area,
        mouse,
        &mut mouse_state.pane_capture,
        mouse_state.sidebar_collapsed,
    )? {
        return Ok(());
    }
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return Ok(());
    }
    let Some(target) = renderer::hit_test_with_sidebar_scroll_and_sort(
        snapshot,
        area,
        mouse,
        mouse_state.sidebar_collapsed,
        mouse_state.sidebar_scroll,
        mouse_state.agent_priority_sort,
    ) else {
        return Ok(());
    };
    match target {
        renderer::ClickTarget::SidebarToggle => {
            mouse_state.sidebar_collapsed = !mouse_state.sidebar_collapsed;
            mouse_state.selection = None;
            mouse_state.last_click = None;
            let _ = super::preferences::store(
                &mouse_state.preferences_path,
                super::preferences::ClientPreferences {
                    sidebar_collapsed: mouse_state.sidebar_collapsed,
                    agent_priority_sort: mouse_state.agent_priority_sort,
                },
            );
        }
        renderer::ClickTarget::ToggleAgentSort => {
            mouse_state.agent_priority_sort = !mouse_state.agent_priority_sort;
            mouse_state.sidebar_scroll = 0;
            let _ = super::preferences::store(
                &mouse_state.preferences_path,
                super::preferences::ClientPreferences {
                    sidebar_collapsed: mouse_state.sidebar_collapsed,
                    agent_priority_sort: mouse_state.agent_priority_sort,
                },
            );
        }
        renderer::ClickTarget::SidebarScroll(offset) => {
            mouse_state.sidebar_scroll = offset;
        }
        renderer::ClickTarget::SplitBorder(_) => {}
        renderer::ClickTarget::Space(space_id) => {
            request_action(
                client,
                "mouse-switch-space",
                "switch_space",
                json!({ "id": space_id }),
                "switch space",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
        }
        renderer::ClickTarget::Workspace {
            space_id,
            workspace_id,
        } => {
            request_action(
                client,
                "mouse-switch-workspace-space",
                "switch_space",
                json!({ "id": space_id }),
                "switch space",
            )?;
            request_action(
                client,
                "mouse-switch-workspace",
                "switch_workspace",
                json!({ "id": workspace_id }),
                "switch workspace",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
        }
        renderer::ClickTarget::Agent {
            space_id,
            workspace_id,
            tab_id,
            pane_id,
        } => activate_sidebar_agent(client, &space_id, &workspace_id, &tab_id, &pane_id)?,
        renderer::ClickTarget::Tab(tab_id) => {
            request_action(
                client,
                "mouse-switch-tab",
                "switch_tab",
                json!({ "id": tab_id }),
                "switch tab",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
        }
        renderer::ClickTarget::Pane(pane_id) => {
            request_action(
                client,
                "mouse-focus-pane",
                "focus_pane",
                json!({ "pane_id": pane_id }),
                "focus pane",
            )?;
        }
    }
    Ok(())
}

fn activate_sidebar_agent(
    client: &ControlClient,
    space_id: &str,
    workspace_id: &str,
    tab_id: &str,
    pane_id: &str,
) -> Result<(), ClientError> {
    for (request_id, operation, payload, action) in [
        (
            "mouse-agent-switch-space",
            "switch_space",
            json!({ "id": space_id }),
            "switch to agent space",
        ),
        (
            "mouse-agent-switch-workspace",
            "switch_workspace",
            json!({ "id": workspace_id }),
            "switch to agent workspace",
        ),
        (
            "mouse-agent-switch-tab",
            "switch_tab",
            json!({ "id": tab_id }),
            "switch to agent tab",
        ),
        (
            "mouse-agent-focus-pane",
            "focus_pane",
            json!({ "pane_id": pane_id }),
            "focus agent pane",
        ),
    ] {
        request_action(client, request_id, operation, payload, action)?;
    }
    Ok(())
}

fn apply_scrollback_views(
    snapshot: &mut SessionSnapshot,
    scroll_offsets: &mut HashMap<String, usize>,
    cached_views: &mut HashMap<String, CachedScrollbackView>,
) {
    scroll_offsets.retain(|pane_id, _| snapshot.panes.iter().any(|pane| &pane.pane_id == pane_id));
    for pane in &mut snapshot.panes {
        let Some(offset) = scroll_offsets.get_mut(&pane.pane_id) else {
            continue;
        };
        if pane.alternate_screen || pane.scrollback.is_empty() {
            *offset = 0;
            continue;
        }
        if let Some(cached) = cached_views.get(&pane.pane_id) {
            if cached.bytes == pane.scrollback
                && cached.rows == pane.rows
                && cached.cols == pane.cols
                && cached.offset == *offset
            {
                pane.screen.clone_from(&cached.screen);
                continue;
            }
        }
        let mut parser =
            vt100::Parser::new(pane.rows.max(1), pane.cols.max(1), MAX_SCROLLBACK_ROWS);
        parser.process(&pane.scrollback);
        parser.set_scrollback(*offset);
        let screen = parser.screen();
        *offset = screen.scrollback();
        pane.screen = screen.contents();
        cached_views.insert(
            pane.pane_id.clone(),
            CachedScrollbackView {
                bytes: pane.scrollback.clone(),
                rows: pane.rows,
                cols: pane.cols,
                offset: *offset,
                screen: pane.screen.clone(),
            },
        );
    }
    scroll_offsets.retain(|_, offset| *offset > 0);
    cached_views.retain(|pane_id, _| scroll_offsets.contains_key(pane_id));
}

fn adjust_scrollback_offset(current: usize, toward_history: bool, lines: usize) -> usize {
    if toward_history {
        current.saturating_add(lines.max(1))
    } else {
        current.saturating_sub(lines.max(1))
    }
}

fn forward_mouse_to_pane(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    capture: &mut Option<PaneMouseCapture>,
    sidebar_collapsed: bool,
) -> Result<bool, ClientError> {
    let captured = match mouse.kind {
        MouseEventKind::Drag(button) | MouseEventKind::Up(button) => capture
            .as_ref()
            .filter(|capture| capture.button == button)
            .map(|capture| (capture.pane_id.clone(), capture.rect)),
        _ => None,
    };
    let target = if matches!(mouse.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_)) {
        captured
    } else {
        renderer::pane_rectangles(
            snapshot,
            renderer::pane_content_area_with_sidebar(area, sidebar_collapsed),
        )
        .into_iter()
        .find(|pane| {
            let inner = Rect::new(
                pane.rect.x.saturating_add(1),
                pane.rect.y.saturating_add(1),
                pane.rect.width.saturating_sub(2),
                pane.rect.height.saturating_sub(2),
            );
            mouse.column >= inner.x
                && mouse.column < inner.right()
                && mouse.row >= inner.y
                && mouse.row < inner.bottom()
        })
        .map(|pane| (pane.pane_id, pane.rect))
    };
    let Some((pane_id, rect)) = target else {
        clear_mouse_capture(capture, mouse.kind);
        return Ok(false);
    };
    let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) else {
        clear_mouse_capture(capture, mouse.kind);
        return Ok(false);
    };
    let right_click = mouse.kind == MouseEventKind::Down(MouseButton::Right);
    let reports_kind = match mouse.kind {
        MouseEventKind::Down(_) => pane.mouse_reporting,
        MouseEventKind::Up(_) => pane.mouse_release,
        MouseEventKind::Drag(_) => pane.mouse_motion,
        MouseEventKind::Moved => pane.mouse_any_motion,
        MouseEventKind::ScrollUp
        | MouseEventKind::ScrollDown
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => pane.mouse_reporting,
    };
    if !reports_kind || (right_click && !pane.right_click_passthrough) {
        clear_mouse_capture(capture, mouse.kind);
        return Ok(false);
    }

    let x = mouse
        .column
        .saturating_sub(rect.x.saturating_add(1))
        .saturating_add(1)
        .clamp(1, pane.cols.max(1));
    let y = mouse
        .row
        .saturating_sub(rect.y.saturating_add(1))
        .saturating_add(1)
        .clamp(1, pane.rows.max(1));
    let Some(bytes) =
        crate::terminal::encode_mouse_event(mouse, x, y, pane.sgr_mouse, pane.utf8_mouse)
    else {
        clear_mouse_capture(capture, mouse.kind);
        return Ok(false);
    };

    if matches!(mouse.kind, MouseEventKind::Down(_)) {
        request_action(
            client,
            "mouse-focus-terminal-pane",
            "focus_pane",
            json!({ "pane_id": pane_id }),
            "focus pane",
        )?;
    }
    let response = client.interactive_request(
        "mouse-terminal-input",
        "send_input",
        json!({ "pane_id": pane_id, "bytes": bytes }),
    )?;
    require_server_success(&response, "send terminal mouse input")?;

    match mouse.kind {
        MouseEventKind::Down(button) => {
            *capture = Some(PaneMouseCapture {
                pane_id,
                rect,
                button,
            });
        }
        MouseEventKind::Up(button)
            if capture
                .as_ref()
                .is_some_and(|capture| capture.button == button) =>
        {
            *capture = None;
        }
        _ => {}
    }
    Ok(true)
}

fn begin_text_selection(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    mouse_state: &mut MouseState,
    sidebar_collapsed: bool,
) -> Result<bool, ClientError> {
    let Some(pane) = renderer::pane_rectangles(
        snapshot,
        renderer::pane_content_area_with_sidebar(area, sidebar_collapsed),
    )
    .into_iter()
    .find(|pane| {
        let inner = Rect::new(
            pane.rect.x.saturating_add(1),
            pane.rect.y.saturating_add(1),
            pane.rect.width.saturating_sub(2),
            pane.rect.height.saturating_sub(2),
        );
        mouse.column >= inner.x
            && mouse.column < inner.right()
            && mouse.row >= inner.y
            && mouse.row < inner.bottom()
    }) else {
        mouse_state.last_click = None;
        return Ok(false);
    };
    let Some(pane_snapshot) = snapshot
        .panes
        .iter()
        .find(|candidate| candidate.pane_id == pane.pane_id)
    else {
        mouse_state.last_click = None;
        return Ok(false);
    };
    if pane_snapshot.mouse_reporting {
        mouse_state.last_click = None;
        return Ok(false);
    }
    let inner = Rect::new(
        pane.rect.x.saturating_add(1),
        pane.rect.y.saturating_add(1),
        pane.rect.width.saturating_sub(2),
        pane.rect.height.saturating_sub(2),
    );
    request_action(
        client,
        "mouse-focus-selection-pane",
        "focus_pane",
        json!({ "pane_id": pane.pane_id.clone() }),
        "focus pane for selection",
    )?;
    let row = mouse.row.saturating_sub(inner.y);
    let col = mouse.column.saturating_sub(inner.x);
    let now = Instant::now();
    let double_click = mouse_state
        .last_click
        .as_ref()
        .is_some_and(|last| last.is_double_click_for(&pane.pane_id, row, col, now));
    if double_click {
        let word = pane_snapshot
            .screen
            .lines()
            .nth(usize::from(row))
            .and_then(|line| super::selection::word_range(line, col));
        mouse_state.selection = Some(match word {
            Some((start, end)) => TextSelection::word(pane.pane_id.clone(), inner, row, start, end),
            None => TextSelection::new(pane.pane_id.clone(), inner, mouse.column, mouse.row),
        });
        mouse_state.last_click = None;
    } else {
        mouse_state.selection = Some(TextSelection::new(
            pane.pane_id.clone(),
            inner,
            mouse.column,
            mouse.row,
        ));
        mouse_state.last_click = Some(PaneClick {
            pane_id: pane.pane_id,
            row,
            col,
            at: now,
        });
    }
    Ok(true)
}

fn visible_web_url_at_point(
    snapshot: &SessionSnapshot,
    pane_area: Rect,
    column: u16,
    row: u16,
) -> Option<String> {
    let pane_rect = renderer::pane_rectangles(snapshot, pane_area)
        .into_iter()
        .find(|pane| {
            let inner = Rect::new(
                pane.rect.x.saturating_add(1),
                pane.rect.y.saturating_add(1),
                pane.rect.width.saturating_sub(2),
                pane.rect.height.saturating_sub(2),
            );
            column >= inner.x && column < inner.right() && row >= inner.y && row < inner.bottom()
        })?;
    let pane = snapshot
        .panes
        .iter()
        .find(|candidate| candidate.pane_id == pane_rect.pane_id)?;
    let inner_x = pane_rect.rect.x.saturating_add(1);
    let inner_y = pane_rect.rect.y.saturating_add(1);
    super::links::web_url_at_cell(
        &pane.screen,
        row.saturating_sub(inner_y),
        column.saturating_sub(inner_x),
    )
}

fn clear_mouse_capture(capture: &mut Option<PaneMouseCapture>, kind: MouseEventKind) {
    match kind {
        MouseEventKind::Up(button)
            if capture
                .as_ref()
                .is_some_and(|capture| capture.button == button) =>
        {
            *capture = None;
        }
        _ => {}
    }
}

fn activate_context_menu(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    menu: ContextMenu,
    action: ContextMenuAction,
    terminal_size: (u16, u16),
) -> Result<Option<RenameTarget>, ClientError> {
    let source_pane_id = menu.source_pane_id.clone();
    match menu.target {
        ContextMenuTarget::Workspace { space_id, id } => {
            request_action(
                client,
                "context-switch-space",
                "switch_space",
                json!({ "id": space_id }),
                "switch space",
            )?;
            request_action(
                client,
                "context-switch-workspace",
                "switch_workspace",
                json!({ "id": id }),
                "switch workspace",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
            match action {
                ContextMenuAction::Activate => Ok(None),
                ContextMenuAction::NewTab => {
                    create_context_tab(client, terminal_size)?;
                    Ok(None)
                }
                ContextMenuAction::Rename => Ok(Some(RenameTarget::Workspace)),
                ContextMenuAction::Close => Ok(Some(RenameTarget::DeleteWorkspace)),
                _ => Ok(None),
            }
        }
        ContextMenuTarget::Tab(tab_id) => {
            let Some((space_id, workspace_id)) = tab_context_ids(snapshot, &tab_id) else {
                return Ok(None);
            };
            request_action(
                client,
                "context-tab-space",
                "switch_space",
                json!({ "id": space_id }),
                "switch space",
            )?;
            request_action(
                client,
                "context-tab-workspace",
                "switch_workspace",
                json!({ "id": workspace_id }),
                "switch workspace",
            )?;
            request_action(
                client,
                "context-activate-tab",
                "switch_tab",
                json!({ "id": tab_id }),
                "switch tab",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
            match action {
                ContextMenuAction::Activate => Ok(None),
                ContextMenuAction::NewTab => {
                    create_context_tab(client, terminal_size)?;
                    Ok(None)
                }
                ContextMenuAction::Rename => Ok(Some(RenameTarget::Tab)),
                ContextMenuAction::Close => {
                    request_action(
                        client,
                        "context-close-tab",
                        "close_tab",
                        json!({ "id": tab_id }),
                        "close tab",
                    )?;
                    ensure_active_default_pane(client, terminal_size)?;
                    Ok(None)
                }
                _ => Ok(None),
            }
        }
        ContextMenuTarget::Pane(pane_id) => {
            request_action(
                client,
                "context-focus-pane",
                "focus_pane",
                json!({ "pane_id": pane_id }),
                "focus pane",
            )?;
            match action {
                ContextMenuAction::Focus => Ok(None),
                ContextMenuAction::Rename => Ok(Some(RenameTarget::Pane)),
                ContextMenuAction::ClearPaneName => {
                    request_action(
                        client,
                        "context-clear-pane-name",
                        "rename_pane",
                        json!({ "pane_id": pane_id, "label": "" }),
                        "clear pane name",
                    )?;
                    Ok(None)
                }
                ContextMenuAction::SwapWithFocusedPane => {
                    if let Some(source_pane_id) = source_pane_id {
                        request_action(
                            client,
                            "context-swap-panes",
                            "swap_panes",
                            json!({
                                "source_pane_id": source_pane_id,
                                "target_pane_id": pane_id
                            }),
                            "swap panes",
                        )?;
                    }
                    Ok(None)
                }
                ContextMenuAction::SplitRight | ContextMenuAction::SplitDown => {
                    let mut request = pane_request_for_snapshot(snapshot, terminal_size);
                    request["direction"] = json!(if action == ContextMenuAction::SplitRight {
                        "horizontal"
                    } else {
                        "vertical"
                    });
                    request_action(
                        client,
                        "context-split-pane",
                        "split_pane",
                        request,
                        "split pane",
                    )?;
                    Ok(None)
                }
                ContextMenuAction::Zoom => {
                    request_action(
                        client,
                        "context-zoom-pane",
                        "toggle_pane_zoom",
                        json!({ "pane_id": pane_id }),
                        "toggle pane zoom",
                    )?;
                    Ok(None)
                }
                ContextMenuAction::ToggleRightClickPassthrough => {
                    request_action(
                        client,
                        "context-toggle-right-click",
                        "toggle_right_click_passthrough",
                        json!({ "pane_id": pane_id }),
                        "toggle right-click passthrough",
                    )?;
                    Ok(None)
                }
                ContextMenuAction::Stop => {
                    request_action(
                        client,
                        "context-stop-pane",
                        "stop_pane",
                        json!({ "pane_id": pane_id }),
                        "stop pane",
                    )?;
                    Ok(None)
                }
                ContextMenuAction::Restart => {
                    request_action(
                        client,
                        "context-restart-pane",
                        "restart_pane",
                        json!({ "pane_id": pane_id }),
                        "restart pane",
                    )?;
                    Ok(None)
                }
                ContextMenuAction::Close => {
                    request_action(
                        client,
                        "context-close-pane",
                        "close_pane",
                        json!({ "pane_id": pane_id }),
                        "close pane",
                    )?;
                    ensure_active_default_pane(client, terminal_size)?;
                    Ok(None)
                }
                ContextMenuAction::Activate | ContextMenuAction::NewTab => Ok(None),
            }
        }
    }
}

fn create_context_tab(
    client: &ControlClient,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    request_action(
        client,
        "context-new-tab",
        "create_tab",
        json!({ "name": "Activity" }),
        "create tab",
    )?;
    ensure_active_default_pane(client, terminal_size)
}

fn tab_context_ids(snapshot: &SessionSnapshot, tab_id: &str) -> Option<(String, String)> {
    snapshot.spaces.iter().find_map(|space| {
        space.workspaces.iter().find_map(|workspace| {
            workspace
                .tabs
                .iter()
                .any(|tab| tab.tab_id == tab_id)
                .then(|| (space.space_id.clone(), workspace.workspace_id.clone()))
        })
    })
}

#[cfg(test)]
fn reconnect_requires_reattach(was_connected: bool, connected: bool) -> bool {
    connected && !was_connected
}

fn snapshot_has_focused_pane(snapshot: &SessionSnapshot) -> bool {
    let Some(focused) = snapshot.focused_pane_id.as_deref() else {
        return false;
    };
    let Some(workspace) = active_workspace(snapshot) else {
        return false;
    };
    let Some(tab) = workspace
        .tabs
        .iter()
        .find(|tab| tab.tab_id == workspace.active_tab_id)
    else {
        return false;
    };
    tab.layout
        .as_ref()
        .is_some_and(|layout| layout.pane_ids().contains(&focused))
        && snapshot.panes.iter().any(|pane| pane.pane_id == focused)
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
                let action_name = if operation == "delete_workspace" {
                    "delete workspace"
                } else {
                    "delete space"
                };
                request_action(
                    client,
                    &format!("confirm-{operation}"),
                    operation,
                    json!({ "id": id }),
                    action_name,
                )?;
                ensure_active_default_pane(client, terminal_size)?;
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
            request_action(
                client,
                "create-workspace",
                "create_workspace",
                json!({ "name": name, "repository_path": repository_path }),
                "create workspace",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
            return Ok(());
        }
        RenameTarget::Space => ("rename_space", Some(snapshot.active_space_id.clone())),
        RenameTarget::CreateSpace => {
            request_action(
                client,
                "create-space",
                "create_space",
                json!({ "name": name }),
                "create space",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
            return Ok(());
        }
        RenameTarget::DeleteWorkspace | RenameTarget::DeleteSpace => unreachable!(),
        RenameTarget::SwitchWorkspace => {
            if let Some(id) = workspace_id_by_name(snapshot, &name) {
                request_action(
                    client,
                    "switch-workspace-by-name",
                    "switch_workspace",
                    json!({ "id": id }),
                    "switch workspace",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            return Ok(());
        }
    };
    if let Some(id) = id {
        let action_name = match operation {
            "rename_pane" => "rename pane",
            "rename_tab" => "rename tab",
            "rename_workspace" => "rename workspace",
            "rename_space" => "rename space",
            _ => "rename item",
        };
        request_action(
            client,
            &format!("rename-{operation}"),
            operation,
            json!({ "id": id, "name": name }),
            action_name,
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
            space.active_workspace_id.as_ref().and_then(|workspace_id| {
                space
                    .workspaces
                    .iter()
                    .find(|workspace| &workspace.workspace_id == workspace_id)
            })
        })
}

fn active_workspace_id(snapshot: &SessionSnapshot) -> Option<String> {
    snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == snapshot.active_space_id)
        .and_then(|space| space.active_workspace_id.clone())
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
    let index = space.workspaces.iter().position(|workspace| {
        Some(workspace.workspace_id.as_str()) == space.active_workspace_id.as_deref()
    })?;
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
            let response = client.detach()?;
            require_server_success(&response, "detach")?;
            Ok(true)
        }
        Action::NewTab => {
            if active_workspace(snapshot).is_some() {
                request_action(
                    client,
                    "palette-new-tab",
                    "create_tab",
                    json!({ "name": "Activity" }),
                    "create tab",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            } else {
                create_workspace_from_current_directory(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::NewPane => {
            request_action(
                client,
                "palette-new-pane",
                "create_pane",
                pane_request_for_snapshot(snapshot, terminal_size),
                "create pane",
            )?;
            Ok(false)
        }
        Action::ClosePane => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                request_action(
                    client,
                    "palette-close-pane",
                    "close_pane",
                    json!({ "pane_id": pane_id }),
                    "close pane",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::CloseTab => {
            if let Some(tab_id) = active_tab_id(snapshot) {
                request_action(
                    client,
                    "palette-close-tab",
                    "close_tab",
                    json!({ "id": tab_id }),
                    "close tab",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::NextTab | Action::PreviousTab => {
            if let Some(tab_id) = adjacent_tab_id(snapshot, matches!(pressed, Action::NextTab)) {
                request_action(
                    client,
                    "palette-switch-tab",
                    "switch_tab",
                    json!({ "id": tab_id }),
                    "switch tab",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::NextSpace | Action::PreviousSpace => {
            if let Some(space_id) =
                adjacent_space_id(snapshot, matches!(pressed, Action::NextSpace))
            {
                request_action(
                    client,
                    "palette-switch-space",
                    "switch_space",
                    json!({ "id": space_id }),
                    "switch space",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::NextWorkspace => {
            if let Some(workspace_id) = adjacent_workspace_id(snapshot) {
                request_action(
                    client,
                    "palette-switch-workspace",
                    "switch_workspace",
                    json!({ "id": workspace_id }),
                    "switch workspace",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::StopFocusedPane => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                request_action(
                    client,
                    "palette-stop-pane",
                    "stop_pane",
                    json!({ "pane_id": pane_id }),
                    "stop pane",
                )?;
            }
            Ok(false)
        }
        Action::RestartFocusedPane => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                request_action(
                    client,
                    "palette-restart-pane",
                    "restart_pane",
                    json!({ "pane_id": pane_id }),
                    "restart pane",
                )?;
            }
            Ok(false)
        }
        Action::FocusNext => {
            request_action(
                client,
                "palette-focus-next",
                "focus_next",
                json!({}),
                "focus next pane",
            )?;
            Ok(false)
        }
        Action::FocusPrevious => {
            request_action(
                client,
                "palette-focus-previous",
                "focus_previous",
                json!({}),
                "focus previous pane",
            )?;
            Ok(false)
        }
        Action::FocusLeft | Action::FocusRight | Action::FocusUp | Action::FocusDown => {
            request_action(
                client,
                "palette-focus-direction",
                "focus_direction",
                json!({ "direction": focus_direction_name(pressed) }),
                "focus pane",
            )?;
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
            request_action(client, "palette-split", "split_pane", request, "split pane")?;
            Ok(false)
        }
        Action::ResizeSmaller | Action::ResizeLarger => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                let delta = if matches!(pressed, Action::ResizeLarger) {
                    0.05
                } else {
                    -0.05
                };
                request_action(
                    client,
                    "palette-resize",
                    "resize_pane",
                    json!({ "pane_id": pane_id, "delta": delta }),
                    "resize pane",
                )?;
            }
            Ok(false)
        }
        Action::ToggleRightClickPassthrough => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                request_action(
                    client,
                    "palette-toggle-right-click",
                    "toggle_right_click_passthrough",
                    json!({ "pane_id": pane_id }),
                    "toggle right-click passthrough",
                )?;
            }
            Ok(false)
        }
        Action::ClearPaneName => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                request_action(
                    client,
                    "palette-clear-pane-name",
                    "rename_pane",
                    json!({ "pane_id": pane_id, "label": "" }),
                    "clear pane name",
                )?;
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
    if active_workspace(&snapshot).is_none() {
        return create_workspace_from_current_directory(client, terminal_size);
    }
    let response = client.request(
        "ensure-default-pane",
        "ensure_active_pane",
        pane_request_for_snapshot(&snapshot, terminal_size),
    )?;
    require_server_success(&response, "start the default shell")?;
    let updated = current_snapshot(client)?;
    if snapshot_has_focused_pane(&updated) {
        Ok(())
    } else {
        Err(ClientError::Server(
            "shell creation completed, but the active tab still has no usable pane".into(),
        ))
    }
}

fn require_server_success<T>(
    response: &crate::protocol::Response<T>,
    operation: &str,
) -> Result<(), ClientError> {
    if response.ok {
        return Ok(());
    }

    let detail = response
        .error
        .as_ref()
        .map(|error| format!("{}: {}", error.code, error.message))
        .unwrap_or_else(|| "the server rejected the request without details".into());
    Err(ClientError::Server(format!("{operation} failed: {detail}")))
}

fn request_action<T: serde::Serialize>(
    client: &ControlClient,
    request_id: &str,
    operation: &str,
    payload: T,
    action_name: &str,
) -> Result<(), ClientError> {
    let response = client.request(request_id, operation, payload)?;
    require_server_success(&response, action_name)
}

fn record_action_error(
    action_error: &mut Option<(String, Instant)>,
    action_name: &str,
    result: Result<(), ClientError>,
) {
    if let Err(error) = result {
        let message = match error {
            ClientError::Server(message) => message,
            error => format!("{action_name} failed: {}", startup_error_message(error)),
        };
        *action_error = Some((message, Instant::now() + ACTION_ERROR_DURATION));
    }
}

fn create_workspace_from_current_directory(
    client: &ControlClient,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    let repository_path = std::env::current_dir()
        .map_err(ClientError::Io)?
        .to_string_lossy()
        .into_owned();
    request_action(
        client,
        "create-workspace-after-close",
        "create_workspace",
        json!({ "name": "Current project", "repository_path": repository_path }),
        "create workspace",
    )?;
    ensure_active_default_pane(client, terminal_size)
}

fn startup_error_message(error: ClientError) -> String {
    match error {
        ClientError::Io(error) => error.to_string(),
        ClientError::Frame(error) => format!("{error:?}"),
        ClientError::Json(error) => error.to_string(),
        ClientError::Server(message) => message,
    }
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

fn page_key_bytes(code: KeyCode) -> Option<Vec<u8>> {
    match code {
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
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
    let workspace = space.workspaces.iter().find(|workspace| {
        Some(workspace.workspace_id.as_str()) == space.active_workspace_id.as_deref()
    })?;
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
    let workspace = space.workspaces.iter().find(|workspace| {
        Some(workspace.workspace_id.as_str()) == space.active_workspace_id.as_deref()
    })?;
    Some(workspace.active_tab_id.clone())
}

#[cfg(test)]
mod tests {
    use super::{
        active_tab_id, active_workspace, adjacent_space_id, adjacent_tab_id, adjacent_workspace_id,
        adjust_scrollback_offset, apply_scrollback_views, current_snapshot,
        ensure_active_default_pane, key_code_bytes, page_key_bytes, pane_size,
        reconnect_requires_reattach, record_action_error, renderer, require_server_success,
        snapshot_has_focused_pane, startup_error_action, visible_web_url_at_point,
        workspace_id_by_name, CachedScrollbackView, ControlClient, PaneClick, SplitDirection,
        SplitDrag, StartupErrorAction,
    };
    use crate::protocol::{ProtocolError, Response, PROTOCOL_VERSION};
    use crate::server::session::Session;
    use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use std::time::{Duration, Instant};

    #[test]
    fn scrollback_offset_shows_history_and_returns_to_live_screen() {
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "cols": 20,
                "rows": 2,
                "status": "Running",
                "scrollback_bytes": 24,
                "scrollback": b"one\r\ntwo\r\nthree\r\nfour".to_vec()
            }))
            .unwrap(),
        );
        let mut offsets = std::collections::HashMap::from([("pane-1".into(), 1)]);
        let mut cached_views = std::collections::HashMap::<String, CachedScrollbackView>::new();

        apply_scrollback_views(&mut snapshot, &mut offsets, &mut cached_views);
        assert!(snapshot.panes[0].screen.contains("three"));
        assert!(!snapshot.panes[0].screen.contains("four"));

        offsets.insert("pane-1".into(), 0);
        apply_scrollback_views(&mut snapshot, &mut offsets, &mut cached_views);
        assert!(snapshot.panes[0].screen.contains("four"));
        assert!(!offsets.contains_key("pane-1"));
    }

    #[test]
    fn common_keys_encode_for_a_pty() {
        assert_eq!(key_code_bytes(KeyCode::Enter), Some(vec![b'\r']));
        assert_eq!(key_code_bytes(KeyCode::Left), Some(b"\x1b[D".to_vec()));
        assert_eq!(page_key_bytes(KeyCode::PageUp), Some(b"\x1b[5~".to_vec()));
        assert_eq!(page_key_bytes(KeyCode::PageDown), Some(b"\x1b[6~".to_vec()));
        assert_eq!(pane_size((120, 40)), (90, 36));
        assert_eq!(pane_size((0, 0)), (1, 1));
    }

    #[test]
    fn page_scrolling_advances_a_viewport_and_clamps_at_live_output() {
        assert_eq!(adjust_scrollback_offset(5, true, 23), 28);
        assert_eq!(adjust_scrollback_offset(5, false, 23), 0);
        assert_eq!(adjust_scrollback_offset(usize::MAX, true, 1), usize::MAX);
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
    fn pane_double_click_matches_herdr_time_and_cell_tolerance() {
        let now = Instant::now();
        let first = PaneClick {
            pane_id: "pane-1".into(),
            row: 4,
            col: 8,
            at: now,
        };
        assert!(first.is_double_click_for("pane-1", 5, 9, now + Duration::from_millis(350)));
        assert!(!first.is_double_click_for("pane-2", 4, 8, now));
        assert!(!first.is_double_click_for("pane-1", 6, 8, now));
        assert!(!first.is_double_click_for("pane-1", 4, 8, now + Duration::from_millis(351)));
    }

    #[test]
    fn startup_error_supports_retry_and_clean_detach() {
        assert_eq!(
            startup_error_action(KeyCode::Enter),
            StartupErrorAction::Retry
        );
        assert_eq!(
            startup_error_action(KeyCode::Char('r')),
            StartupErrorAction::Retry
        );
        assert_eq!(
            startup_error_action(KeyCode::Esc),
            StartupErrorAction::Detach
        );
        assert_eq!(
            startup_error_action(KeyCode::Char('q')),
            StartupErrorAction::Detach
        );
        assert_eq!(
            startup_error_action(KeyCode::Char('x')),
            StartupErrorAction::Ignore
        );
    }

    #[test]
    fn ctrl_click_hit_testing_maps_window_coordinates_to_the_visible_pane_url() {
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "scrollback_bytes": 0,
                "screen": "See https://example.test",
            }))
            .unwrap(),
        );
        let tab = &mut snapshot.spaces[0].workspaces[0].tabs[0];
        tab.layout = Some(crate::model::layout::LayoutNode::Pane {
            pane_id: "pane-1".into(),
        });
        tab.focused_pane_id = Some("pane-1".into());
        snapshot.focused_pane_id = Some("pane-1".into());

        let pane_area = renderer::pane_content_area(Rect::new(0, 0, 100, 30));
        assert_eq!(
            visible_web_url_at_point(&snapshot, pane_area, pane_area.x + 5, pane_area.y + 1)
                .as_deref(),
            Some("https://example.test")
        );
        assert_eq!(
            visible_web_url_at_point(&snapshot, pane_area, pane_area.x, pane_area.y),
            None,
            "pane borders do not activate links"
        );
    }

    #[test]
    fn shell_start_failure_keeps_the_server_error_for_the_ui() {
        let response = Response {
            version: PROTOCOL_VERSION,
            request_id: "ensure-default-pane".into(),
            ok: false,
            payload: None::<serde_json::Value>,
            error: Some(ProtocolError {
                code: "pane_start_failed".into(),
                message: "powershell.exe was not found".into(),
            }),
        };

        let error = require_server_success(&response, "start the default shell").unwrap_err();
        match error {
            super::ClientError::Server(message) => {
                assert!(message.contains("pane_start_failed"));
                assert!(message.contains("powershell.exe was not found"));
            }
            other => panic!("expected server error, got {other:?}"),
        }
    }

    #[test]
    fn startup_restores_a_workspace_and_shell_after_the_last_workspace_was_closed() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let state_dir = std::env::temp_dir().join(format!(
            "spindle-client-startup-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&state_dir).unwrap();
        let server_state = state_dir.clone();
        let server_thread = std::thread::spawn(move || {
            crate::server::run(&server_state).expect("test server should exit cleanly")
        });
        let endpoint = state_dir.join("server.endpoint");
        let mut address = None;
        for _ in 0..80 {
            if let Ok(found) = std::fs::read_to_string(&endpoint) {
                if ControlClient::connect(found.trim()).is_ok() {
                    address = Some(found.trim().to_owned());
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let result = (|| -> Result<(), String> {
            let address = address.as_deref().ok_or("test server did not start")?;
            let client = ControlClient::connect(address).map_err(|error| format!("{error:?}"))?;
            client
                .attach_with_terminal(100, 30, vec!["mouse".into()])
                .map_err(|error| format!("{error:?}"))?;
            let deleted = client
                .request(
                    "close-last-workspace",
                    "delete_workspace",
                    serde_json::json!({ "id": "workspace-1" }),
                )
                .map_err(|error| format!("{error:?}"))?;
            if !deleted.ok {
                return Err("the test's last workspace could not be closed".into());
            }

            ensure_active_default_pane(&client, (100, 30)).map_err(|error| format!("{error:?}"))?;
            let snapshot = current_snapshot(&client).map_err(|error| format!("{error:?}"))?;
            if active_workspace(&snapshot).is_none() || !snapshot_has_focused_pane(&snapshot) {
                return Err(
                    "startup left the attached session without a workspace and shell".into(),
                );
            }
            Ok(())
        })();
        if let Some(address) = address {
            if let Ok(client) = ControlClient::connect(address) {
                let _ = client.request("stop-test-server", "stop_server", serde_json::json!({}));
            }
            let _ = server_thread.join();
        } else {
            drop(server_thread);
        }
        let _ = std::fs::remove_dir_all(&state_dir);
        result.expect("attaching to an empty session should restore a usable workspace");
    }

    #[test]
    fn action_failures_are_kept_for_a_short_visible_window() {
        let mut action_error = None;
        record_action_error(
            &mut action_error,
            "split pane",
            Err(super::ClientError::Server("pane is missing".into())),
        );

        let (message, expires_at) = action_error.expect("action error should be visible");
        assert_eq!(message, "pane is missing");
        assert!(expires_at > Instant::now());
        assert!(expires_at <= Instant::now() + super::ACTION_ERROR_DURATION);
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

    #[test]
    fn stale_focus_does_not_hide_a_shell_start_error() {
        let mut snapshot = crate::server::session::Session::default()
            .snapshot()
            .clone();
        snapshot.focused_pane_id = Some("pane-missing".into());
        assert!(!snapshot_has_focused_pane(&snapshot));

        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-missing",
                "command": "powershell.exe",
                "args": [],
                "cwd": "C:/",
                "status": "Running",
                "scrollback_bytes": 0
            }))
            .unwrap(),
        );
        assert!(!snapshot_has_focused_pane(&snapshot));
        snapshot.spaces[0].workspaces[0].tabs[0].layout =
            Some(crate::model::layout::LayoutNode::pane("pane-missing"));
        assert!(snapshot_has_focused_pane(&snapshot));
    }
}
