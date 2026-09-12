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
        preferences_path: preferences_path.to_path_buf(),
        ..MouseState::default()
    };
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
        mouse_state.sidebar_scroll = mouse_state.sidebar_scroll.min(renderer::sidebar_scroll_max(
            &snapshot,
            area,
            mouse_state.sidebar_collapsed,
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
                renderer::render_with_sidebar_scroll_and_cursor(
                    frame,
                    &snapshot,
                    connected,
                    mouse_state.sidebar_collapsed,
                    mouse_state.sidebar_scroll,
                    show_host_cursor,
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
                handle_mouse(
                    client,
                    &snapshot,
                    mouse,
                    terminal_size,
                    &mut mouse_state,
                    &mut context_menu,
                    &mut rename_prompt,
                )?;
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
                    submit_rename(client, &snapshot, target, name, terminal_size)?;
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
                        rename_prompt =
                            activate_context_menu(client, &snapshot, menu, action, terminal_size)?
                                .map(RenamePrompt::new);
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
                },
            );
            prefix_active = false;
            continue;
        }
        match pressed {
            Action::Detach => {
                client.detach()?;
                break;
            }
            Action::NewTab => {
                if active_workspace(&snapshot).is_some() {
                    client.request("new-tab", "create_tab", json!({ "name": "Activity" }))?;
                    ensure_active_default_pane(client, terminal_size)?;
                } else {
                    create_workspace_from_current_directory(client, terminal_size)?;
                }
            }
            Action::NewPane => {
                let _ = client.request(
                    "new-pane",
                    "create_pane",
                    pane_request_for_snapshot(&snapshot, terminal_size),
                );
            }
            Action::ClosePane => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    client.request(
                        "keyboard-close-pane",
                        "close_pane",
                        json!({ "pane_id": pane_id }),
                    )?;
                    ensure_active_default_pane(client, terminal_size)?;
                }
            }
            Action::CloseTab => {
                if let Some(tab_id) = active_tab_id(&snapshot) {
                    let response =
                        client.request("close-tab", "close_tab", json!({ "id": tab_id }))?;
                    if response.ok {
                        ensure_active_default_pane(client, terminal_size)?;
                    }
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
            Action::ToggleZoom => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    let _ = client.request(
                        "toggle-zoom",
                        "toggle_pane_zoom",
                        json!({ "pane_id": pane_id }),
                    );
                }
            }
            Action::ToggleSidebar => {}
            Action::ToggleRightClickPassthrough => {
                if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                    client.request(
                        "toggle-right-click",
                        "toggle_right_click_passthrough",
                        json!({ "pane_id": pane_id }),
                    )?;
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
            let max_scroll =
                renderer::sidebar_scroll_max(snapshot, area, mouse_state.sidebar_collapsed);
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
            *context_menu = renderer::hit_test_with_sidebar_scroll(
                snapshot,
                area,
                mouse,
                mouse_state.sidebar_collapsed,
                mouse_state.sidebar_scroll,
            )
            .and_then(|target| ContextMenu::from_target(target, mouse.column, mouse.row));
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
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        mouse_state.sidebar_scroll_drag = None;
        if let Some(grab_row_offset) = renderer::sidebar_scroll_thumb_grab_offset(
            snapshot,
            area,
            mouse_state.sidebar_collapsed,
            mouse_state.sidebar_scroll,
            mouse.column,
            mouse.row,
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
        mouse_state.sidebar_scroll = renderer::sidebar_scroll_offset_from_drag_row(
            snapshot,
            area,
            mouse_state.sidebar_collapsed,
            mouse.row,
            mouse_state.sidebar_scroll_drag.unwrap_or_default(),
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
                client.request(
                    "mouse-set-split-ratio",
                    "set_split_ratio",
                    json!({ "path": drag.path, "ratio": drag.ratio_at(mouse) }),
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
    let Some(target) = renderer::hit_test_with_sidebar_scroll(
        snapshot,
        area,
        mouse,
        mouse_state.sidebar_collapsed,
        mouse_state.sidebar_scroll,
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
                },
            );
        }
        renderer::ClickTarget::SidebarScroll(offset) => {
            mouse_state.sidebar_scroll = offset;
        }
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
        client.request(
            "mouse-focus-terminal-pane",
            "focus_pane",
            json!({ "pane_id": pane_id }),
        )?;
    }
    client.interactive_request(
        "mouse-terminal-input",
        "send_input",
        json!({ "pane_id": pane_id, "bytes": bytes }),
    )?;

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
    client.request(
        "mouse-focus-selection-pane",
        "focus_pane",
        json!({ "pane_id": pane.pane_id.clone() }),
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
    match menu.target {
        ContextMenuTarget::Workspace { space_id, id } => {
            client.request(
                "context-switch-space",
                "switch_space",
                json!({ "id": space_id }),
            )?;
            client.request(
                "context-switch-workspace",
                "switch_workspace",
                json!({ "id": id }),
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
            client.request(
                "context-tab-space",
                "switch_space",
                json!({ "id": space_id }),
            )?;
            client.request(
                "context-tab-workspace",
                "switch_workspace",
                json!({ "id": workspace_id }),
            )?;
            client.request(
                "context-activate-tab",
                "switch_tab",
                json!({ "id": tab_id }),
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
                    client.request("context-close-tab", "close_tab", json!({ "id": tab_id }))?;
                    ensure_active_default_pane(client, terminal_size)?;
                    Ok(None)
                }
                _ => Ok(None),
            }
        }
        ContextMenuTarget::Pane(pane_id) => {
            client.request(
                "context-focus-pane",
                "focus_pane",
                json!({ "pane_id": pane_id }),
            )?;
            match action {
                ContextMenuAction::Focus => Ok(None),
                ContextMenuAction::Rename => Ok(Some(RenameTarget::Pane)),
                ContextMenuAction::SplitRight | ContextMenuAction::SplitDown => {
                    let mut request = pane_request_for_snapshot(snapshot, terminal_size);
                    request["direction"] = json!(if action == ContextMenuAction::SplitRight {
                        "horizontal"
                    } else {
                        "vertical"
                    });
                    client.request("context-split-pane", "split_pane", request)?;
                    Ok(None)
                }
                ContextMenuAction::Zoom => {
                    client.request(
                        "context-zoom-pane",
                        "toggle_pane_zoom",
                        json!({ "pane_id": pane_id }),
                    )?;
                    Ok(None)
                }
                ContextMenuAction::ToggleRightClickPassthrough => {
                    client.request(
                        "context-toggle-right-click",
                        "toggle_right_click_passthrough",
                        json!({ "pane_id": pane_id }),
                    )?;
                    Ok(None)
                }
                ContextMenuAction::Stop => {
                    client.request(
                        "context-stop-pane",
                        "stop_pane",
                        json!({ "pane_id": pane_id }),
                    )?;
                    Ok(None)
                }
                ContextMenuAction::Restart => {
                    client.request(
                        "context-restart-pane",
                        "restart_pane",
                        json!({ "pane_id": pane_id }),
                    )?;
                    Ok(None)
                }
                ContextMenuAction::Close => {
                    client.request(
                        "context-close-pane",
                        "close_pane",
                        json!({ "pane_id": pane_id }),
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
    client.request(
        "context-new-tab",
        "create_tab",
        json!({ "name": "Activity" }),
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
                let response = client.request(
                    format!("confirm-{operation}"),
                    operation,
                    json!({ "id": id }),
                )?;
                if response.ok {
                    ensure_active_default_pane(client, terminal_size)?;
                }
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
            client.detach()?;
            Ok(true)
        }
        Action::NewTab => {
            if active_workspace(snapshot).is_some() {
                client.request(
                    "palette-new-tab",
                    "create_tab",
                    json!({ "name": "Activity" }),
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            } else {
                create_workspace_from_current_directory(client, terminal_size)?;
            }
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
        Action::ClosePane => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                client.request(
                    "palette-close-pane",
                    "close_pane",
                    json!({ "pane_id": pane_id }),
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
            Ok(false)
        }
        Action::CloseTab => {
            if let Some(tab_id) = active_tab_id(snapshot) {
                let response =
                    client.request("palette-close-tab", "close_tab", json!({ "id": tab_id }))?;
                if response.ok {
                    ensure_active_default_pane(client, terminal_size)?;
                }
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
        Action::ToggleRightClickPassthrough => {
            if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                client.request(
                    "palette-toggle-right-click",
                    "toggle_right_click_passthrough",
                    json!({ "pane_id": pane_id }),
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
        return Ok(());
    }
    client.request(
        "ensure-default-pane",
        "ensure_active_pane",
        pane_request_for_snapshot(&snapshot, terminal_size),
    )?;
    let updated = current_snapshot(client)?;
    if snapshot_has_focused_pane(&updated) {
        Ok(())
    } else {
        Err(ClientError::Server(
            "the server accepted shell creation, but the active tab still has no usable pane"
                .into(),
        ))
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
    client.request(
        "create-workspace-after-close",
        "create_workspace",
        json!({ "name": "Current project", "repository_path": repository_path }),
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
        active_tab_id, adjacent_space_id, adjacent_tab_id, adjacent_workspace_id,
        adjust_scrollback_offset, apply_scrollback_views, key_code_bytes, page_key_bytes,
        pane_size, reconnect_requires_reattach, snapshot_has_focused_pane, startup_error_action,
        workspace_id_by_name, CachedScrollbackView, PaneClick, SplitDirection, SplitDrag,
        StartupErrorAction,
    };
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
