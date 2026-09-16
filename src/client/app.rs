use super::context_menu::{ContextMenu, ContextMenuAction, ContextMenuTarget};
use super::copy_mode::{CopyMode, KeyResult};
use super::global_menu::{Action as GlobalMenuAction, GlobalMenu, Outcome as GlobalMenuOutcome};
use super::input::{Action, Keymap};
#[cfg(test)]
use super::mouse::should_forward_pane_mouse;
use super::mouse::{
    clear_mouse_capture, pane_mouse_target, pane_terminal_area,
    should_forward_pane_mouse_with_modifier, visible_web_url_at_point, CachedScrollbackView,
    MouseState, PaneClick, PaneMouseCapture, PaneScrollbarDrag, SplitDrag, TabDrag, WorkspaceDrag,
};
use super::navigator::{Navigator, Outcome as NavigatorOutcome, Target as NavigatorTarget};
use super::palette::{move_selection, Command};
use super::prompt::{PromptResult, RenamePrompt, RenameTarget};
use super::renderer;
use super::scrollbar::{
    max_offset_for_pane, offset_from_drag_row, offset_from_row, thumb_grab_offset,
};
use super::selection::TextSelection;
use super::settings::{Outcome as SettingsOutcome, Settings};
use super::startup::{startup_error_action, StartupErrorAction};
use super::workspace_navigation::{
    indexed_workspace_selection, move_workspace_selection, workspace_picker_key, WorkspacePickerKey,
};
use super::worktree_actions;
use super::{ClientError, ControlClient};
use crate::model::layout::Direction as SplitDirection;
use crate::server::session::SessionSnapshot;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
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
use std::collections::VecDeque;
use std::io::{self, stdout};
use std::time::{Duration, Instant};

const SPLIT_DRAG_INTERVAL: Duration = Duration::from_millis(33);
const MAX_SCROLLBACK_ROWS: usize = 4096;
const ACTION_ERROR_DURATION: Duration = Duration::from_secs(5);

pub fn run(
    address: impl Into<String>,
    state_dir: impl AsRef<std::path::Path>,
) -> Result<(), ClientError> {
    let client = ControlClient::connect(address)?;
    let preferences_path = state_dir.as_ref().join("client.json");
    let preferences = super::preferences::load(&preferences_path);
    let config = crate::config::load();
    let terminal_size = size().map_err(ClientError::Io)?;
    client.attach_with_terminal(
        terminal_size.0,
        terminal_size.1,
        vec!["mouse".into(), "alternate_screen".into()],
    )?;
    let mut terminal = setup_terminal(config.mouse_capture).map_err(ClientError::Io)?;
    let startup_error = ensure_active_default_pane(&client, terminal_size)
        .err()
        .map(startup_error_message);
    let result = event_loop(
        &mut terminal,
        &client,
        startup_error,
        initial_sidebar_collapsed(&preferences, &config),
        preferences.agent_priority_sort,
        preferences.collapsed_worktree_groups,
        &preferences_path,
    );
    restore_terminal(&mut terminal).map_err(ClientError::Io)?;
    result
}

fn setup_terminal(mouse_capture: bool) -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;
    set_mouse_capture(&mut output, mouse_capture)?;
    Terminal::new(CrosstermBackend::new(output))
}

fn set_mouse_capture(output: &mut impl io::Write, enabled: bool) -> io::Result<()> {
    if enabled {
        execute!(
            output,
            EnableMouseCapture,
            crossterm::style::Print("\x1b[?1002h\x1b[?1003h")
        )?;
    } else {
        execute!(
            output,
            DisableMouseCapture,
            crossterm::style::Print("\x1b[?1003l\x1b[?1002l")
        )?;
    }
    Ok(())
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

fn store_client_preferences(mouse_state: &MouseState) {
    let mut collapsed_worktree_groups = mouse_state
        .collapsed_worktree_groups
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    collapsed_worktree_groups.sort_unstable();
    let _ = super::preferences::store(
        &mouse_state.preferences_path,
        super::preferences::ClientPreferences {
            sidebar_collapsed: Some(mouse_state.sidebar_collapsed),
            agent_priority_sort: mouse_state.agent_priority_sort,
            collapsed_worktree_groups,
        },
    );
}

fn initial_sidebar_collapsed(
    preferences: &super::preferences::ClientPreferences,
    config: &crate::config::Config,
) -> bool {
    preferences
        .sidebar_collapsed
        .unwrap_or(config.sidebar_start_collapsed)
}

fn should_draw_host_cursor(mode: crate::config::HostCursorMode) -> bool {
    match mode {
        crate::config::HostCursorMode::Auto => {
            crate::platform::should_draw_host_cursor_by_default()
        }
        crate::config::HostCursorMode::Native => false,
        crate::config::HostCursorMode::Drawn => true,
    }
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: &ControlClient,
    mut startup_error: Option<String>,
    sidebar_collapsed: bool,
    agent_priority_sort: bool,
    collapsed_worktree_groups: Vec<String>,
    preferences_path: &std::path::Path,
) -> Result<(), ClientError> {
    let mut prefix_active = false;
    let mut resize_mode = false;
    let mut keymap;
    let mut palette_selected = 0;
    let mut palette_open = false;
    let mut navigator: Option<Navigator> = None;
    let mut global_menu: Option<GlobalMenu> = None;
    let mut settings: Option<Settings> = None;
    let mut rename_prompt: Option<RenamePrompt> = None;
    let mut context_menu: Option<ContextMenu> = None;
    let mut onboarding_open = crate::config::load().onboarding.unwrap_or(true);
    let mut help_open = false;
    let mut last_pane_sizes = None;
    let mut mouse_state = MouseState {
        sidebar_collapsed,
        agent_priority_sort,
        preferences_path: preferences_path.to_path_buf(),
        collapsed_worktree_groups: collapsed_worktree_groups.into_iter().collect(),
        ..MouseState::default()
    };
    let mut mouse_capture = crate::config::load().mouse_capture;
    let mut was_connected = true;
    let mut snapshot = current_snapshot(client)?;
    let mut previous_pane_id: Option<String> = None;
    let mut skip_draw = false;
    let mut action_error: Option<(String, Instant)> = None;
    let mut status_notice: Option<(String, Instant)> = None;
    let mut notifications = VecDeque::new();
    let mut pending_external_notifications = VecDeque::new();
    loop {
        let config = crate::config::load();
        if config.mouse_capture != mouse_capture {
            set_mouse_capture(terminal.backend_mut(), config.mouse_capture)
                .map_err(ClientError::Io)?;
            mouse_capture = config.mouse_capture;
        }
        mouse_state.copy_on_select = config.copy_on_select;
        mouse_state.mouse_scroll_lines = config.mouse_scroll_lines;
        keymap = Keymap::from_config(&config);
        if !config.notifications_enabled {
            notifications.clear();
            pending_external_notifications.clear();
        }
        if action_error
            .as_ref()
            .is_some_and(|(_, expires_at)| Instant::now() >= *expires_at)
        {
            action_error = None;
        }
        if status_notice
            .as_ref()
            .is_some_and(|(_, expires_at)| Instant::now() >= *expires_at)
        {
            status_notice = None;
        }
        crate::client::notifications::expire(&mut notifications, Instant::now());
        if config.notification_sound {
            while let Some(kind) =
                crate::client::notifications::take_visible_sound(&mut notifications, Instant::now())
            {
                let sound = match kind {
                    crate::client::notifications::Kind::NeedsAttention => {
                        crate::platform::NotificationSound::Attention
                    }
                    crate::client::notifications::Kind::Finished => {
                        crate::platform::NotificationSound::Finished
                    }
                };
                let _ = crate::platform::play_notification_sound(sound);
            }
        }
        let terminal_size = size().map_err(ClientError::Io)?;
        let mut connected = match current_snapshot(client) {
            Ok(current) => {
                let now = Instant::now();
                if config.notifications_enabled {
                    let due = crate::client::notifications::take_due(
                        &mut pending_external_notifications,
                        now,
                    );
                    if config.notification_sound {
                        crate::client::notifications::play_sounds(&due);
                    }
                    crate::client::notifications::deliver(
                        &mut notifications,
                        due,
                        config.notification_delivery,
                        0,
                        now,
                    );
                    let events = crate::client::notifications::observe_all(&snapshot, &current);
                    if config.notification_delay_seconds > 0
                        && matches!(
                            config.notification_delivery,
                            crate::config::NotificationDelivery::Terminal
                                | crate::config::NotificationDelivery::System
                        )
                    {
                        crate::client::notifications::defer_external(
                            &mut pending_external_notifications,
                            events,
                            now,
                            config.notification_delay_seconds,
                        );
                    } else {
                        if config.notification_sound
                            && matches!(
                                config.notification_delivery,
                                crate::config::NotificationDelivery::Terminal
                                    | crate::config::NotificationDelivery::System
                            )
                        {
                            crate::client::notifications::play_sounds(&events);
                        }
                        crate::client::notifications::deliver(
                            &mut notifications,
                            events,
                            config.notification_delivery,
                            config.notification_delay_seconds,
                            now,
                        );
                    }
                }
                if snapshot.focused_pane_id != current.focused_pane_id {
                    previous_pane_id = snapshot.focused_pane_id.clone();
                }
                snapshot = current;
                true
            }
            Err(_) => false,
        };
        if let Some(copy_mode) = mouse_state.copy_mode.as_mut() {
            if snapshot.focused_pane_id.as_deref() != Some(copy_mode.pane_id.as_str()) {
                let pane_id = copy_mode.pane_id.clone();
                let saved_offset = copy_mode.saved_offset;
                mouse_state.copy_mode = None;
                mouse_state.scroll_offsets.insert(pane_id, saved_offset);
            } else if let Some(pane) = snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == copy_mode.pane_id)
            {
                copy_mode.refresh(&pane.scrollback, pane.rows, pane.cols);
                mouse_state
                    .scroll_offsets
                    .insert(copy_mode.pane_id.clone(), copy_mode.scroll_offset());
            } else {
                let pane_id = copy_mode.pane_id.clone();
                let saved_offset = copy_mode.saved_offset;
                mouse_state.copy_mode = None;
                mouse_state.scroll_offsets.insert(pane_id, saved_offset);
            }
        }
        apply_scrollback_views(
            &mut snapshot,
            &mut mouse_state.scroll_offsets,
            &mut mouse_state.scrollback_views,
        );
        if snapshot_has_focused_pane(&snapshot) {
            startup_error = None;
        } else if connected && startup_error.is_none() {
            match ensure_active_default_pane(client, terminal_size) {
                Ok(()) => {
                    // Recovery changes the server immediately. Pull the new snapshot before
                    // drawing so startup never presents Herdr's usable shell as an empty pane.
                    match current_snapshot(client) {
                        Ok(updated) => snapshot = updated,
                        Err(error) => startup_error = Some(startup_error_message(error)),
                    }
                }
                Err(error) => startup_error = Some(startup_error_message(error)),
            }
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
                .min(renderer::sidebar_scroll_max_with_sort_and_groups(
                    &snapshot,
                    area,
                    mouse_state.sidebar_collapsed,
                    mouse_state.agent_priority_sort,
                    &mouse_state.collapsed_worktree_groups,
                ));
        let pane_sizes = renderer::pane_sizes(
            &snapshot,
            renderer::pane_content_area_for_snapshot(
                &snapshot,
                area,
                mouse_state.sidebar_collapsed,
            ),
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
        let show_host_cursor = should_draw_host_cursor(config.host_cursor)
            && mouse_state.copy_mode.is_none()
            && !focused_pane_scrolled
            && mouse_state.selection.is_none()
            && !palette_open
            && navigator.is_none()
            && !help_open
            && rename_prompt.is_none()
            && context_menu.is_none()
            && !resize_mode
            && startup_error.is_none();
        if !skip_draw {
            terminal
                .draw(|frame| {
                renderer::render_with_sidebar_scroll_and_cursor_and_agent_sort_and_navigation_and_groups_with_tab_scroll(
                    frame,
                    &snapshot,
                    connected,
                    mouse_state.sidebar_collapsed,
                    mouse_state.sidebar_scroll,
                    show_host_cursor,
                    mouse_state.agent_priority_sort,
                    mouse_state
                        .navigation_workspace
                        .as_ref()
                        .map(|(space, workspace)| (space.as_str(), workspace.as_str())),
                    &mouse_state.collapsed_worktree_groups,
                    &mouse_state.scroll_offsets,
                    mouse_state.tab_scroll,
                );
                if let Some(insert_index) = mouse_state
                    .tab_drag
                    .as_ref()
                    .and_then(|drag| drag.insert_index)
                {
                    renderer::render_tab_drop_indicator(
                        frame,
                        &snapshot,
                        mouse_state.sidebar_collapsed,
                        insert_index,
                    );
                }
                if let Some(row) = mouse_state
                    .workspace_drag
                    .as_ref()
                    .and_then(|drag| drag.drop_row)
                {
                    renderer::render_workspace_drop_indicator(
                        frame,
                        mouse_state.sidebar_collapsed,
                        row,
                    );
                }
                if resize_mode {
                    renderer::render_resize_mode(frame, &snapshot, mouse_state.sidebar_collapsed);
                }
                if prefix_active {
                    renderer::render_prefix_mode(
                        frame,
                        &keymap,
                        &snapshot,
                        mouse_state.sidebar_collapsed,
                    );
                }
                if let Some(selection) = &mouse_state.selection {
                    renderer::render_selection_with_sidebar(
                        frame,
                        &snapshot,
                        selection,
                        mouse_state.sidebar_collapsed,
                    );
                }
                if let Some(copy_mode) = &mouse_state.copy_mode {
                    renderer::render_copy_mode(
                        frame,
                        &snapshot,
                        copy_mode,
                        mouse_state.sidebar_collapsed,
                    );
                }
                if palette_open {
                    renderer::render_palette(frame, palette_selected);
                }
                if let Some(navigator) = &navigator {
                    renderer::render_navigator(frame, &snapshot, navigator);
                }
                if onboarding_open {
                    renderer::render_onboarding(frame);
                } else if help_open {
                    renderer::render_help(frame, &keymap);
                }
                if let Some(prompt) = &rename_prompt {
                    let title = match prompt.target {
                        RenameTarget::Pane => "Rename pane",
                        RenameTarget::Tab => "Rename tab",
                        RenameTarget::Workspace => "Rename workspace",
                        RenameTarget::CreateWorkspace => "Create workspace",
                        RenameTarget::CreateTab => "Create tab",
                        RenameTarget::Space => "Rename space",
                        RenameTarget::CreateSpace => "Create space",
                        RenameTarget::DeleteWorkspace => "Delete workspace: type its name",
                        RenameTarget::DeleteWorkspaceGroup => {
                            "Delete workspace group: type its name"
                        }
                        RenameTarget::DeleteSpace => "Delete space: type its name",
                        RenameTarget::CreateWorktree => "Create worktree: type branch name",
                        RenameTarget::OpenWorktree => "Open worktree: type path or branch",
                        RenameTarget::RemoveWorktree => "Remove worktree checkout: type its name",
                        RenameTarget::SwitchWorkspace => "Switch workspace: type its name",
                        RenameTarget::PluginAction => "Run plugin action: type its ID",
                    };
                    renderer::render_prompt(frame, title, &prompt.input);
                }
                if let Some(menu) = &context_menu {
                    renderer::render_context_menu(frame, menu);
                }
                if let Some(menu) = &global_menu {
                    renderer::render_global_menu(frame, menu);
                }
                if let Some(open_settings) = &settings {
                    renderer::render_settings(frame, open_settings);
                }
                if let Some(error) = &startup_error {
                    renderer::render_startup_error(frame, error);
                } else if let Some((error, _)) = &action_error {
                    renderer::render_action_error(frame, error);
                } else if let Some((notice, _)) = &status_notice {
                    renderer::render_status_notice(frame, notice);
                }
                if let Some(message) =
                    crate::client::notifications::visible_message(&notifications, Instant::now())
                {
                    renderer::render_notification(frame, message);
                }
                })
                .map_err(ClientError::Io)?;
        }
        skip_draw = false;
        if !event::poll(Duration::from_millis(100)).map_err(ClientError::Io)? {
            continue;
        }
        let input = event::read().map_err(ClientError::Io)?;
        let key = match input {
            Event::Mouse(mouse) => {
                if !mouse_capture {
                    continue;
                }
                if onboarding_open {
                    let area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                        && renderer::onboarding_area(area)
                            .contains((mouse.column, mouse.row).into())
                    {
                        onboarding_open = false;
                        if let Err(error) = crate::config::complete_onboarding() {
                            status_notice = Some((error, Instant::now() + ACTION_ERROR_DURATION));
                        }
                    }
                    continue;
                }
                if let Some(open_settings) = settings.as_mut() {
                    if matches!(
                        open_settings.handle_mouse(
                            Rect::new(0, 0, terminal_size.0, terminal_size.1),
                            mouse,
                        ),
                        SettingsOutcome::Close | SettingsOutcome::Saved
                    ) {
                        settings = None;
                    }
                    continue;
                }
                if settings.is_some() {
                    continue;
                }
                if let Some(menu) = global_menu.as_mut() {
                    let sidebar = renderer::sidebar_area(
                        Rect::new(0, 0, terminal_size.0, terminal_size.1),
                        false,
                    );
                    match menu.select_at(
                        sidebar,
                        Rect::new(0, 0, terminal_size.0, terminal_size.1),
                        mouse,
                    ) {
                        GlobalMenuOutcome::Continue => {}
                        GlobalMenuOutcome::Close => global_menu = None,
                        GlobalMenuOutcome::Activate(action) => {
                            global_menu = None;
                            match action {
                                GlobalMenuAction::Settings => settings = Some(Settings::open()),
                                GlobalMenuAction::Help => help_open = true,
                                GlobalMenuAction::CommandPalette => {
                                    palette_open = true;
                                    palette_selected = 0;
                                }
                                GlobalMenuAction::ReloadConfig => {
                                    status_notice = Some((
                                        "configuration reloaded".into(),
                                        Instant::now() + ACTION_ERROR_DURATION,
                                    ));
                                }
                                GlobalMenuAction::Detach => {
                                    let _ = client.detach();
                                    break;
                                }
                            }
                        }
                    }
                    continue;
                }
                if let Some(open_navigator) = navigator.as_mut() {
                    let area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
                    let mut activate = None;
                    let mut close = false;
                    match mouse.kind {
                        MouseEventKind::Moved => {
                            if let renderer::NavigatorHit::Row { target, .. } =
                                renderer::hit_test_navigator(
                                    &snapshot,
                                    open_navigator,
                                    area,
                                    mouse.column,
                                    mouse.row,
                                )
                            {
                                open_navigator.select(target);
                            }
                        }
                        MouseEventKind::ScrollUp => open_navigator
                            .move_selection(&snapshot, -(mouse_state.mouse_scroll_lines as isize)),
                        MouseEventKind::ScrollDown => open_navigator
                            .move_selection(&snapshot, mouse_state.mouse_scroll_lines as isize),
                        MouseEventKind::Down(MouseButton::Left) => {
                            match renderer::hit_test_navigator(
                                &snapshot,
                                open_navigator,
                                area,
                                mouse.column,
                                mouse.row,
                            ) {
                                renderer::NavigatorHit::Search => open_navigator.focus_search(),
                                renderer::NavigatorHit::Close => close = true,
                                renderer::NavigatorHit::Row { target, expand } => {
                                    open_navigator.select(target.clone());
                                    if expand {
                                        open_navigator.toggle_selected(&snapshot);
                                    } else {
                                        activate = Some(target);
                                    }
                                }
                                renderer::NavigatorHit::Outside => close = true,
                                renderer::NavigatorHit::Inside => {}
                            }
                        }
                        _ => {}
                    }
                    // Mouse events are owned by the modal navigator, never the pane beneath it.
                    if let Some(target) = activate {
                        let result = match target {
                            NavigatorTarget::Menu(index) => {
                                global_menu = Some(GlobalMenu { selected: index });
                                Ok(())
                            }
                            NavigatorTarget::NewTab if config.prompt_new_tab_name => {
                                rename_prompt =
                                    Some(RenamePrompt::new_tab(next_tab_name(&snapshot)));
                                Ok(())
                            }
                            NavigatorTarget::NewWorkspace if config.prompt_new_workspace_name => {
                                rename_prompt =
                                    Some(RenamePrompt::new(RenameTarget::CreateWorkspace));
                                Ok(())
                            }
                            target => {
                                switch_navigator_target(client, &snapshot, target, terminal_size)
                            }
                        };
                        if result.is_ok() {
                            navigator = None;
                        }
                        record_action_error(&mut action_error, "open navigator target", result);
                    }
                    if close {
                        navigator = None;
                    }
                    continue;
                }
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
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && matches!(
                        renderer::hit_test_with_sidebar(
                            &snapshot,
                            Rect::new(0, 0, terminal_size.0, terminal_size.1),
                            mouse,
                            mouse_state.sidebar_collapsed,
                        ),
                        Some(renderer::ClickTarget::GlobalMenu)
                    )
                {
                    global_menu = Some(GlobalMenu::default());
                    continue;
                }
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && matches!(
                        renderer::hit_test_with_sidebar(
                            &snapshot,
                            Rect::new(0, 0, terminal_size.0, terminal_size.1),
                            mouse,
                            mouse_state.sidebar_collapsed,
                        ),
                        Some(renderer::ClickTarget::MobileSwitcher)
                    )
                {
                    navigator = Some(Navigator::new_mobile(&snapshot));
                    continue;
                }
                if let Some(mode) = mouse_state.copy_mode.take() {
                    mouse_state
                        .scroll_offsets
                        .insert(mode.pane_id, mode.saved_offset);
                }
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    mouse_state.navigation_workspace = None;
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
            Event::FocusGained => {
                skip_draw = !config.redraw_on_focus_gained;
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
        if onboarding_open {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                onboarding_open = false;
                if let Err(error) = crate::config::complete_onboarding() {
                    status_notice = Some((error, Instant::now() + ACTION_ERROR_DURATION));
                }
            }
            continue;
        }
        if mouse_state.copy_mode.is_none() {
            mouse_state.selection = None;
            mouse_state.last_click = None;
            mouse_state.scroll_offsets.clear();
        }
        if let Some(prompt) = &mut rename_prompt {
            match prompt.apply_key(key.code) {
                PromptResult::Continue => {}
                PromptResult::Cancel => rename_prompt = None,
                PromptResult::Submit(name) => {
                    let target = prompt.target;
                    rename_prompt = None;
                    if target == RenameTarget::PluginAction {
                        record_action_error(
                            &mut action_error,
                            "run plugin action",
                            super::plugins::launch_action(&name, &snapshot)
                                .map_err(ClientError::Io),
                        );
                    } else {
                        record_action_error(
                            &mut action_error,
                            "save name",
                            submit_rename(client, &snapshot, target, name, terminal_size),
                        );
                    }
                }
            }
            continue;
        }
        if let Some(menu) = global_menu.as_mut() {
            match menu.handle_key(key.code) {
                GlobalMenuOutcome::Continue => {}
                GlobalMenuOutcome::Close => global_menu = None,
                GlobalMenuOutcome::Activate(action) => {
                    global_menu = None;
                    match action {
                        GlobalMenuAction::Settings => settings = Some(Settings::open()),
                        GlobalMenuAction::Help => help_open = true,
                        GlobalMenuAction::CommandPalette => {
                            palette_open = true;
                            palette_selected = 0;
                        }
                        GlobalMenuAction::ReloadConfig => {
                            status_notice = Some((
                                "configuration reloaded".into(),
                                Instant::now() + ACTION_ERROR_DURATION,
                            ));
                        }
                        GlobalMenuAction::Detach => {
                            let _ = client.detach();
                            break;
                        }
                    }
                }
            }
            continue;
        }
        if let Some(open_settings) = settings.as_mut() {
            match open_settings.handle_key(key.code) {
                SettingsOutcome::Continue => {}
                SettingsOutcome::Close | SettingsOutcome::Saved => settings = None,
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
                        match activate_context_menu(
                            client,
                            &snapshot,
                            menu,
                            action,
                            terminal_size,
                            &mut mouse_state,
                        ) {
                            Ok(prompt) => {
                                rename_prompt =
                                    prompt.map(|target| prompt_for_target(target, &snapshot))
                            }
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
                if (!matches!(command.action(), Action::NewTab | Action::CreateWorkspace))
                    || (command.action() == Action::NewTab && config.prompt_new_tab_name)
                    || (command.action() == Action::CreateWorkspace
                        && config.prompt_new_workspace_name)
                {
                    if let Some(target) = rename_target(command.action()) {
                        rename_prompt = Some(prompt_for_target(target, &snapshot));
                        continue;
                    }
                }
                match execute_action(
                    command.action(),
                    client,
                    &snapshot,
                    previous_pane_id.as_deref(),
                    terminal_size,
                ) {
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
        if let Some(open_navigator) = navigator.as_mut() {
            match open_navigator.handle_key(key, &snapshot) {
                NavigatorOutcome::Continue => {}
                NavigatorOutcome::Close => navigator = None,
                NavigatorOutcome::Activate => {
                    let target = open_navigator.selected(&snapshot);
                    if let Some(target) = target {
                        let result = match target {
                            NavigatorTarget::Menu(index) => {
                                global_menu = Some(GlobalMenu { selected: index });
                                Ok(())
                            }
                            target => {
                                switch_navigator_target(client, &snapshot, target, terminal_size)
                            }
                        };
                        if result.is_ok() {
                            navigator = None;
                        }
                        record_action_error(&mut action_error, "open navigator target", result);
                    } else {
                        navigator = None;
                    }
                }
            }
            continue;
        }
        if mouse_state.navigation_workspace.is_some() {
            match workspace_picker_key(key, &keymap) {
                WorkspacePickerKey::Cancel => mouse_state.navigation_workspace = None,
                WorkspacePickerKey::Move(forward) => {
                    mouse_state.navigation_workspace = move_workspace_selection(
                        &snapshot,
                        mouse_state.navigation_workspace.as_ref(),
                        forward,
                    );
                }
                WorkspacePickerKey::Confirm => {
                    if let Some((space_id, workspace_id)) =
                        mouse_state.navigation_workspace.as_ref()
                    {
                        let result = switch_to_workspace(
                            client,
                            &snapshot,
                            space_id,
                            workspace_id,
                            terminal_size,
                        );
                        record_action_error(&mut action_error, "switch workspace", result);
                    }
                    mouse_state.navigation_workspace = None;
                }
                WorkspacePickerKey::Choose(index) => {
                    if let Some((space_id, workspace_id)) =
                        indexed_workspace_selection(&snapshot, index)
                    {
                        let result = switch_to_workspace(
                            client,
                            &snapshot,
                            &space_id,
                            &workspace_id,
                            terminal_size,
                        );
                        record_action_error(&mut action_error, "switch workspace", result);
                        mouse_state.navigation_workspace = None;
                    }
                }
                WorkspacePickerKey::Ignore => {}
            }
            continue;
        }
        if mouse_state.copy_mode.is_some() {
            if keymap.is_prefix(key) {
                prefix_active = true;
                continue;
            }
            if !prefix_active {
                let result = mouse_state.copy_mode.as_mut().map(|mode| mode.key(key));
                match result {
                    Some(KeyResult::Exit) => {
                        let mode = mouse_state.copy_mode.take().expect("copy mode exists");
                        mouse_state
                            .scroll_offsets
                            .insert(mode.pane_id, mode.saved_offset);
                    }
                    Some(KeyResult::Copy) => {
                        let mode = mouse_state.copy_mode.take().expect("copy mode exists");
                        let text = mode.selected_text();
                        if !text.is_empty() {
                            record_action_error(
                                &mut action_error,
                                "copy terminal selection",
                                super::clipboard::copy_text(&text).map_err(ClientError::Io),
                            );
                        }
                        mouse_state
                            .scroll_offsets
                            .insert(mode.pane_id, mode.saved_offset);
                    }
                    Some(KeyResult::Handled) => {
                        if let Some(mode) = mouse_state.copy_mode.as_ref() {
                            mouse_state
                                .scroll_offsets
                                .insert(mode.pane_id.clone(), mode.scroll_offset());
                        }
                    }
                    None => {}
                }
                continue;
            }
        }
        if resize_mode {
            if keymap.is_prefix(key) || matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                resize_mode = false;
                prefix_active = false;
                continue;
            }
            let delta = match key.code {
                KeyCode::Char('h') | KeyCode::Up | KeyCode::Left => Some(-0.05),
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Right => Some(0.05),
                _ => None,
            };
            if let (Some(delta), Some(pane_id)) = (delta, snapshot.focused_pane_id.as_deref()) {
                record_action_error(
                    &mut action_error,
                    "resize pane",
                    request_action(
                        client,
                        "resize-mode",
                        "resize_pane",
                        json!({ "pane_id": pane_id, "delta": delta }),
                        "resize pane",
                    ),
                );
            }
            continue;
        }
        if keymap.is_prefix(key) {
            prefix_active = true;
            continue;
        }
        if !prefix_active
            && key.modifiers.is_empty()
            && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
        {
            if let Some(pane_id) = input_pane_id(&snapshot) {
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
        let pressed = keymap.action(prefix_active, key);
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
        if pressed == Action::Settings {
            settings = Some(Settings::open());
            prefix_active = false;
            continue;
        }
        if pressed == Action::ReloadConfig {
            status_notice = Some((
                "configuration reloaded".into(),
                Instant::now() + ACTION_ERROR_DURATION,
            ));
            prefix_active = false;
            continue;
        }
        if pressed == Action::ToggleSidebar {
            if uses_mobile_navigation(terminal_size.0, config.mobile_width_threshold) {
                navigator = Some(Navigator::new_mobile(&snapshot));
                prefix_active = false;
                continue;
            }
            mouse_state.sidebar_collapsed = !mouse_state.sidebar_collapsed;
            mouse_state.selection = None;
            mouse_state.last_click = None;
            store_client_preferences(&mouse_state);
            prefix_active = false;
            continue;
        }
        if pressed == Action::SessionNavigator {
            navigator = Some(
                if uses_mobile_navigation(terminal_size.0, config.mobile_width_threshold) {
                    Navigator::new_mobile(&snapshot)
                } else {
                    Navigator::new(&snapshot)
                },
            );
            prefix_active = false;
            continue;
        }
        if let Action::CustomCommand(index) = pressed {
            if let Some(command) = config.custom_commands.get(index) {
                record_action_error(
                    &mut action_error,
                    command.description.as_deref().unwrap_or("custom command"),
                    super::custom_commands::run(command, &snapshot, client)
                        .map_err(ClientError::Io),
                );
            }
            prefix_active = false;
            continue;
        }
        match pressed {
            Action::EnterCopyMode => {
                if mouse_state.copy_mode.is_none() {
                    if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
                        if let Some(pane) =
                            snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)
                        {
                            if !pane.alternate_screen {
                                let offset = mouse_state
                                    .scroll_offsets
                                    .get(pane_id)
                                    .copied()
                                    .unwrap_or(0);
                                mouse_state.copy_mode = Some(CopyMode::new(
                                    pane_id.to_owned(),
                                    &pane.scrollback,
                                    pane.rows,
                                    pane.cols,
                                    pane.rows,
                                    offset,
                                    pane.cursor,
                                ));
                            }
                        }
                    }
                }
            }
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
                if config.prompt_new_tab_name && active_workspace(&snapshot).is_some() {
                    rename_prompt = Some(RenamePrompt::new_tab(next_tab_name(&snapshot)));
                    continue;
                }
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
                if let Some(pane_id) = snapshot.popup_pane_id.as_deref() {
                    record_action_error(
                        &mut action_error,
                        "close popup",
                        request_action(
                            client,
                            "keyboard-close-popup",
                            "close_popup",
                            json!({ "pane_id": pane_id }),
                            "close popup",
                        ),
                    );
                } else if let Some(pane_id) = snapshot.focused_pane_id.as_deref() {
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
            Action::MoveTabPrevious | Action::MoveTabNext => {
                if let Some((tab_id, index, tab_count)) = active_tab_position(&snapshot) {
                    let insert_index = if matches!(pressed, Action::MoveTabPrevious) {
                        index.saturating_sub(1)
                    } else {
                        (index + 2).min(tab_count)
                    };
                    record_action_error(
                        &mut action_error,
                        "move tab",
                        request_action(
                            client,
                            "move-tab-keyboard",
                            "move_tab",
                            json!({ "id": tab_id, "insert_index": insert_index }),
                            "move tab",
                        ),
                    );
                }
            }
            Action::SwitchTab(index) => {
                if let Some(tab_id) = indexed_tab_id(&snapshot, index) {
                    let result = request_action(
                        client,
                        "switch-indexed-tab",
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
            Action::PreviousAgent | Action::NextAgent => {
                if let Some(pane_id) =
                    adjacent_agent_pane_id(&snapshot, matches!(pressed, Action::NextAgent))
                {
                    let result = request_action(
                        client,
                        "switch-agent",
                        "focus_pane",
                        json!({ "pane_id": pane_id }),
                        "focus agent",
                    )
                    .and_then(|()| ensure_active_default_pane(client, terminal_size));
                    record_action_error(&mut action_error, "focus agent", result);
                }
            }
            Action::WorkspacePicker => {
                if uses_mobile_navigation(terminal_size.0, config.mobile_width_threshold) {
                    navigator = Some(Navigator::new_mobile(&snapshot));
                    prefix_active = false;
                    continue;
                }
                let active_space = snapshot
                    .spaces
                    .iter()
                    .find(|space| space.space_id == snapshot.active_space_id);
                mouse_state.navigation_workspace = active_space.and_then(|space| {
                    space
                        .active_workspace_id
                        .as_ref()
                        .filter(|workspace_id| {
                            space
                                .workspaces
                                .iter()
                                .any(|workspace| &workspace.workspace_id == *workspace_id)
                        })
                        .cloned()
                        .or_else(|| {
                            space
                                .workspaces
                                .first()
                                .map(|workspace| workspace.workspace_id.clone())
                        })
                        .map(|workspace_id| (space.space_id.clone(), workspace_id))
                });
            }
            Action::SessionNavigator => {}
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
            Action::EditScrollback => {
                let result = snapshot
                    .focused_pane_id
                    .as_deref()
                    .and_then(|pane_id| snapshot.panes.iter().find(|pane| pane.pane_id == pane_id))
                    .ok_or_else(|| ClientError::Server("no focused pane".into()))
                    .and_then(open_scrollback_in_editor);
                record_action_error(&mut action_error, "edit scrollback", result);
            }
            Action::EnterResizeMode => {
                resize_mode = true;
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
            Action::LastPane => {
                if let Some(pane_id) = last_pane_target(&snapshot, previous_pane_id.as_deref()) {
                    record_action_error(
                        &mut action_error,
                        "focus last pane",
                        request_action(
                            client,
                            "focus-last-pane",
                            "focus_pane",
                            json!({ "pane_id": pane_id }),
                            "focus last pane",
                        ),
                    );
                }
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
            Action::ResizePaneLeft
            | Action::ResizePaneDown
            | Action::ResizePaneUp
            | Action::ResizePaneRight => {
                record_action_error(
                    &mut action_error,
                    "resize pane",
                    request_action(
                        client,
                        "resize-pane-direction",
                        "resize_pane_direction",
                        json!({
                            "direction": resize_direction_name(pressed),
                            "amount": 0.05,
                        }),
                        "resize pane",
                    ),
                );
            }
            Action::SwapLeft | Action::SwapRight | Action::SwapUp | Action::SwapDown => {
                let result = snapshot
                    .focused_pane_id
                    .as_deref()
                    .and_then(|source| {
                        directional_pane_id(&snapshot, source, swap_direction_name(pressed))
                            .map(|target| (source.to_owned(), target.to_owned()))
                    })
                    .map_or_else(
                        || Err(ClientError::Server("no pane in that direction".into())),
                        |(source, target)| {
                            request_action(
                                client,
                                "swap-direction",
                                "swap_panes",
                                json!({
                                    "source_pane_id": source,
                                    "target_pane_id": target,
                                }),
                                "swap panes",
                            )
                        },
                    );
                record_action_error(&mut action_error, "swap panes", result);
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
            Action::Send(key) => {
                if let Some(pane_id) = input_pane_id(&snapshot) {
                    if let Some(bytes) = key_code_bytes(key) {
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
            Action::Settings => {}
            Action::ReloadConfig => {}
            Action::CommandPalette => {}
            Action::OpenNotificationTarget => {
                let target =
                    crate::client::notifications::visible_pane_id(&notifications, Instant::now())
                        .map(str::to_owned)
                        .ok_or_else(|| ClientError::Server("no visible notification target".into()))
                        .and_then(|pane_id| {
                            request_action(
                                client,
                                "open-notification-target",
                                "focus_pane",
                                json!({ "pane_id": pane_id }),
                                "open notification target",
                            )
                        });
                record_action_error(&mut action_error, "open notification target", target);
            }
            Action::CreateWorkspace => {
                if config.prompt_new_workspace_name {
                    rename_prompt = Some(RenamePrompt::new(RenameTarget::CreateWorkspace));
                    continue;
                }
                let result = create_workspace_from_current_directory(client, terminal_size);
                record_action_error(&mut action_error, "create workspace", result);
            }
            Action::RenameActiveWorkspace => {
                rename_prompt = Some(prompt_for_target(RenameTarget::Workspace, &snapshot));
            }
            Action::DeleteActiveWorkspace => {
                if config.confirm_close {
                    rename_prompt = Some(RenamePrompt::new(RenameTarget::DeleteWorkspace));
                } else if let Err(error) = close_active_workspace(client, &snapshot, terminal_size)
                {
                    record_action_error(&mut action_error, "close workspace", Err(error));
                }
            }
            Action::RenameFocusedPane
            | Action::RenameActiveTab
            | Action::RenameActiveSpace
            | Action::CreateSpace
            | Action::DeleteActiveSpace
            | Action::SwitchWorkspaceByName
            | Action::PluginAction
            | Action::CustomCommand(_) => {
                if let Some(target) = rename_target(pressed) {
                    rename_prompt = Some(prompt_for_target(target, &snapshot));
                }
            }
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
    if let Some(drag) = mouse_state.pane_scrollbar_drag.as_ref() {
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(pane) = snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == drag.pane_id)
                {
                    let max_offset = max_offset_for_pane(pane, drag.track.height);
                    mouse_state.scroll_offsets.insert(
                        drag.pane_id.clone(),
                        offset_from_drag_row(
                            max_offset,
                            drag.track.height,
                            drag.track,
                            mouse.row,
                            drag.grab_offset,
                        ),
                    );
                }
                return Ok(());
            }
            MouseEventKind::Up(MouseButton::Left) => {
                mouse_state.pane_scrollbar_drag = None;
                return Ok(());
            }
            _ => {}
        }
    }
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        if let Some((pane_id, track)) =
            pane_scrollbar_at(snapshot, area, mouse, mouse_state.sidebar_collapsed)
        {
            let pane = snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == pane_id)
                .expect("scrollbar target pane exists");
            let max_offset = max_offset_for_pane(pane, track.height);
            let current_offset = mouse_state
                .scroll_offsets
                .get(&pane_id)
                .copied()
                .unwrap_or(0);
            mouse_state.scroll_offsets.insert(
                pane_id.clone(),
                offset_from_row(max_offset, track.height, track, mouse.row),
            );
            mouse_state.pane_scrollbar_drag =
                thumb_grab_offset(max_offset, track.height, track, mouse.row, current_offset).map(
                    |grab_offset| PaneScrollbarDrag {
                        pane_id,
                        track,
                        grab_offset,
                    },
                );
            return Ok(());
        }
    }
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
            let max_scroll = renderer::sidebar_scroll_max_with_sort_and_groups(
                snapshot,
                area,
                mouse_state.sidebar_collapsed,
                mouse_state.agent_priority_sort,
                &mouse_state.collapsed_worktree_groups,
            );
            mouse_state.sidebar_scroll = if mouse.kind == MouseEventKind::ScrollUp {
                mouse_state.sidebar_scroll.saturating_sub(1)
            } else {
                mouse_state.sidebar_scroll.saturating_add(1).min(max_scroll)
            };
            return Ok(());
        }
        if renderer::tab_scroll_region(area, mouse_state.sidebar_collapsed, mouse.column, mouse.row)
        {
            let max_scroll =
                renderer::tab_scroll_max(snapshot, area, mouse_state.sidebar_collapsed);
            mouse_state.tab_scroll = if mouse.kind == MouseEventKind::ScrollUp {
                mouse_state.tab_scroll.saturating_sub(1)
            } else {
                mouse_state.tab_scroll.saturating_add(1).min(max_scroll)
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
                        mouse_state.mouse_scroll_lines,
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
            let target = renderer::hit_test_with_sidebar_scroll_and_sort_and_groups_and_tab_scroll(
                snapshot,
                area,
                mouse,
                mouse_state.sidebar_collapsed,
                mouse_state.sidebar_scroll,
                mouse_state.agent_priority_sort,
                &mouse_state.collapsed_worktree_groups,
                mouse_state.tab_scroll,
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
                    if let ContextMenuTarget::Workspace { space_id, id } = &menu.target {
                        menu.close_group = workspace_has_linked_children(snapshot, space_id, id);
                        if let Some(workspace) = snapshot
                            .spaces
                            .iter()
                            .find(|space| space.space_id == *space_id)
                            .and_then(|space| {
                                space
                                    .workspaces
                                    .iter()
                                    .find(|workspace| workspace.workspace_id == *id)
                            })
                        {
                            menu.is_git =
                                workspace.repository_path.is_some() || workspace.branch.is_some();
                            menu.is_linked_worktree = workspace.is_linked_worktree;
                            menu.has_worktree_children = menu.close_group;
                        }
                    }
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
                *rename_prompt = activate_context_menu(
                    client,
                    snapshot,
                    menu,
                    action,
                    terminal_size,
                    mouse_state,
                )?
                .map(|target| prompt_for_target(target, snapshot));
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
            renderer::pane_content_area_for_snapshot(snapshot, area, mouse_state.sidebar_collapsed),
            mouse.column,
            mouse.row,
        ) {
            let pane_id =
                pane_mouse_target(snapshot, area, mouse, &None, mouse_state.sidebar_collapsed)
                    .map(|(pane_id, _)| pane_id);
            let pane_cwd = pane_id.as_deref().and_then(|pane_id| {
                snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == pane_id)
                    .map(|pane| pane.cwd.as_str())
            });
            match super::plugins::launch_for_url(&url, pane_id.as_deref(), pane_cwd) {
                Ok(true) => return Ok(()),
                Ok(false) | Err(_) => {
                    super::links::open_web_url(&url).map_err(ClientError::Io)?;
                }
            }
            return Ok(());
        }
    }
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        mouse_state.sidebar_scroll_drag = None;
        if let Some(grab_row_offset) =
            renderer::sidebar_scroll_thumb_grab_offset_with_sort_and_groups(
                snapshot,
                area,
                mouse_state.sidebar_collapsed,
                mouse_state.sidebar_scroll,
                mouse.column,
                mouse.row,
                mouse_state.agent_priority_sort,
                &mouse_state.collapsed_worktree_groups,
            )
        {
            mouse_state.sidebar_scroll_drag = Some(grab_row_offset);
            return Ok(());
        }
        if let Some(renderer::ClickTarget::Workspace {
            space_id,
            workspace_id,
        }) = renderer::hit_test_with_sidebar_scroll_and_sort_and_groups(
            snapshot,
            area,
            mouse,
            mouse_state.sidebar_collapsed,
            mouse_state.sidebar_scroll,
            mouse_state.agent_priority_sort,
            &mouse_state.collapsed_worktree_groups,
        ) {
            mouse_state.workspace_drag = Some(WorkspaceDrag {
                space_id,
                workspace_id,
                drop_row: None,
            });
            return Ok(());
        }
        if let Some(renderer::ClickTarget::Tab(tab_id)) =
            renderer::hit_test_with_sidebar_scroll_and_sort_and_groups_and_tab_scroll(
                snapshot,
                area,
                mouse,
                mouse_state.sidebar_collapsed,
                mouse_state.sidebar_scroll,
                mouse_state.agent_priority_sort,
                &mouse_state.collapsed_worktree_groups,
                mouse_state.tab_scroll,
            )
        {
            if let Some(workspace) = active_workspace(snapshot) {
                mouse_state.tab_drag = Some(TabDrag {
                    workspace_id: workspace.workspace_id.clone(),
                    tab_id,
                    insert_index: None,
                });
                return Ok(());
            }
        }
        let pane_area =
            renderer::pane_content_area_for_snapshot(snapshot, area, mouse_state.sidebar_collapsed);
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
        mouse_state.sidebar_scroll =
            renderer::sidebar_scroll_offset_from_drag_row_with_sort_and_groups(
                snapshot,
                area,
                mouse_state.sidebar_collapsed,
                mouse.row,
                mouse_state.sidebar_scroll_drag.unwrap_or_default(),
                mouse_state.agent_priority_sort,
                &mouse_state.collapsed_worktree_groups,
            );
        if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
            mouse_state.sidebar_scroll_drag = None;
        }
        return Ok(());
    }
    if matches!(
        mouse.kind,
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
    ) && mouse_state.workspace_drag.is_some()
    {
        if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
            if let Some(drag) = mouse_state.workspace_drag.as_mut() {
                drag.drop_row = renderer::workspace_drop_target_with_groups(
                    snapshot,
                    area,
                    mouse_state.sidebar_scroll,
                    mouse_state.agent_priority_sort,
                    &drag.workspace_id,
                    mouse.column,
                    mouse.row,
                    &mouse_state.collapsed_worktree_groups,
                )
                .map(|(_, _, _)| mouse.row);
            }
            return Ok(());
        }
        let drag = mouse_state
            .workspace_drag
            .take()
            .expect("workspace drag exists");
        if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
            if let Some((space_id, target_workspace_id, insert_index)) =
                renderer::workspace_drop_target_with_groups(
                    snapshot,
                    area,
                    mouse_state.sidebar_scroll,
                    mouse_state.agent_priority_sort,
                    &drag.workspace_id,
                    mouse.column,
                    mouse.row,
                    &mouse_state.collapsed_worktree_groups,
                )
            {
                if space_id == drag.space_id && target_workspace_id != drag.workspace_id {
                    request_action(
                        client,
                        "mouse-move-workspace",
                        "move_workspace",
                        json!({ "id": drag.workspace_id, "insert_index": insert_index }),
                        "move workspace",
                    )?;
                    return Ok(());
                }
            }
            request_action(
                client,
                "mouse-switch-workspace-space",
                "switch_space",
                json!({ "id": drag.space_id }),
                "switch space",
            )?;
            request_action(
                client,
                "mouse-switch-workspace",
                "switch_workspace",
                json!({ "id": drag.workspace_id }),
                "switch workspace",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
        }
        return Ok(());
    }
    if matches!(
        mouse.kind,
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
    ) && mouse_state.tab_drag.is_some()
    {
        if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
            if let Some(drag) = mouse_state.tab_drag.as_mut() {
                drag.insert_index = renderer::tab_drop_target(
                    snapshot,
                    area,
                    mouse_state.sidebar_collapsed,
                    &drag.tab_id,
                    mouse.column,
                    mouse.row,
                )
                .map(|(_, _, insert_index)| insert_index);
            }
            return Ok(());
        }
        let drag = mouse_state.tab_drag.take().expect("tab drag exists");
        if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
            if let Some((workspace_id, target_tab_id, insert_index)) = renderer::tab_drop_target(
                snapshot,
                area,
                mouse_state.sidebar_collapsed,
                &drag.tab_id,
                mouse.column,
                mouse.row,
            ) {
                if workspace_id == drag.workspace_id && target_tab_id != drag.tab_id {
                    request_action(
                        client,
                        "mouse-move-tab",
                        "move_tab",
                        json!({ "id": drag.tab_id, "insert_index": insert_index }),
                        "move tab",
                    )?;
                    return Ok(());
                }
            }
            request_action(
                client,
                "mouse-switch-tab",
                "switch_tab",
                json!({ "id": drag.tab_id }),
                "switch tab",
            )?;
            ensure_active_default_pane(client, terminal_size)?;
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
            if mouse_state.copy_on_select && selection.has_range() && !text.is_empty() {
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
    let Some(target) = renderer::hit_test_with_sidebar_scroll_and_sort_and_groups_and_tab_scroll(
        snapshot,
        area,
        mouse,
        mouse_state.sidebar_collapsed,
        mouse_state.sidebar_scroll,
        mouse_state.agent_priority_sort,
        &mouse_state.collapsed_worktree_groups,
        mouse_state.tab_scroll,
    ) else {
        return Ok(());
    };
    match target {
        renderer::ClickTarget::MobileSwitcher => {}
        renderer::ClickTarget::GlobalMenu => {}
        renderer::ClickTarget::NewWorkspace => {
            if crate::config::load().prompt_new_workspace_name {
                *rename_prompt = Some(RenamePrompt::new(RenameTarget::CreateWorkspace));
            } else {
                create_workspace_from_current_directory(client, terminal_size)?;
            }
        }
        renderer::ClickTarget::NewTab => {
            if crate::config::load().prompt_new_tab_name {
                *rename_prompt = Some(RenamePrompt::new_tab(next_tab_name(snapshot)));
            } else {
                request_action(
                    client,
                    "mouse-new-tab",
                    "create_tab",
                    json!({ "name": "Activity" }),
                    "create tab",
                )?;
                ensure_active_default_pane(client, terminal_size)?;
            }
        }
        renderer::ClickTarget::TabScrollLeft => {
            mouse_state.tab_scroll = mouse_state.tab_scroll.saturating_sub(1);
        }
        renderer::ClickTarget::TabScrollRight => {
            let max_scroll =
                renderer::tab_scroll_max(snapshot, area, mouse_state.sidebar_collapsed);
            mouse_state.tab_scroll = mouse_state.tab_scroll.saturating_add(1).min(max_scroll);
        }
        renderer::ClickTarget::SidebarToggle => {
            mouse_state.sidebar_collapsed = !mouse_state.sidebar_collapsed;
            mouse_state.selection = None;
            mouse_state.last_click = None;
            store_client_preferences(mouse_state);
        }
        renderer::ClickTarget::ToggleAgentSort => {
            mouse_state.agent_priority_sort = !mouse_state.agent_priority_sort;
            mouse_state.sidebar_scroll = 0;
            store_client_preferences(mouse_state);
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
        parser.screen_mut().set_scrollback(*offset);
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

fn pane_scrollbar_at(
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    sidebar_collapsed: bool,
) -> Option<(String, Rect)> {
    let config = crate::config::load();
    if !config.pane_scrollbars {
        return None;
    }
    let pane_area = renderer::pane_content_area_for_snapshot(snapshot, area, sidebar_collapsed);
    let panes = renderer::pane_rectangles(snapshot, pane_area);
    panes.into_iter().find_map(|pane| {
        let borders = renderer::pane_borders_for_rect(
            pane.rect,
            &renderer::pane_rectangles(snapshot, pane_area),
            config.pane_borders,
            config.pane_outer_borders,
            config.pane_gaps,
        );
        let inner = renderer::pane_inner_area(pane.rect, borders, true);
        let track = Rect::new(inner.right(), inner.y, 1, inner.height);
        let view = snapshot
            .panes
            .iter()
            .find(|view| view.pane_id == pane.pane_id)?;
        (track.contains((mouse.column, mouse.row).into())
            && max_offset_for_pane(view, track.height) > 0)
            .then_some((pane.pane_id, track))
    })
}

fn forward_mouse_to_pane(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    area: Rect,
    mouse: MouseEvent,
    capture: &mut Option<PaneMouseCapture>,
    sidebar_collapsed: bool,
) -> Result<bool, ClientError> {
    let target = pane_mouse_target(snapshot, area, mouse, capture, sidebar_collapsed);
    let Some((pane_id, rect)) = target else {
        clear_mouse_capture(capture, mouse.kind);
        return Ok(false);
    };
    let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) else {
        clear_mouse_capture(capture, mouse.kind);
        return Ok(false);
    };
    let configured_modifier = crate::config::load().right_click_passthrough_modifier;
    if !should_forward_pane_mouse_with_modifier(pane, mouse, configured_modifier) {
        clear_mouse_capture(capture, mouse.kind);
        // A popup is modal even when its terminal is not reporting mouse
        // events. Do not let a border click fall through to the tiled pane
        // underneath it.
        return Ok(snapshot.popup_pane_id.as_deref() == Some(pane_id.as_str()));
    }

    let terminal_area = pane_terminal_area(snapshot, area, &pane_id, sidebar_collapsed)
        .unwrap_or_else(|| {
            Rect::new(
                rect.x.saturating_add(1),
                rect.y.saturating_add(1),
                rect.width.saturating_sub(2),
                rect.height.saturating_sub(2),
            )
        });
    let (x, y) = super::mouse::terminal_coordinates(
        mouse.column,
        mouse.row,
        terminal_area,
        pane.cols,
        pane.rows,
    );
    let mut forwarded_mouse = mouse;
    if mouse.kind == MouseEventKind::Down(MouseButton::Right)
        && configured_modifier.is_some_and(|modifier| modifier == mouse.modifiers)
    {
        forwarded_mouse.modifiers = crossterm::event::KeyModifiers::empty();
    }
    let Some(bytes) =
        crate::terminal::encode_mouse_event(forwarded_mouse, x, y, pane.sgr_mouse, pane.utf8_mouse)
    else {
        clear_mouse_capture(capture, mouse.kind);
        return Ok(false);
    };

    if matches!(mouse.kind, MouseEventKind::Down(_))
        && snapshot.popup_pane_id.as_deref() != Some(pane_id.as_str())
    {
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
        renderer::pane_content_area_for_snapshot(snapshot, area, sidebar_collapsed),
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

fn activate_context_menu(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    menu: ContextMenu,
    action: ContextMenuAction,
    terminal_size: (u16, u16),
    mouse_state: &mut MouseState,
) -> Result<Option<RenameTarget>, ClientError> {
    let source_pane_id = menu.source_pane_id.clone();
    match menu.target {
        ContextMenuTarget::Workspace { space_id, id } => {
            let close_group = menu.close_group;
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
                    if crate::config::load().prompt_new_tab_name {
                        Ok(Some(RenameTarget::CreateTab))
                    } else {
                        create_context_tab(client, terminal_size)?;
                        Ok(None)
                    }
                }
                ContextMenuAction::Rename => Ok(Some(RenameTarget::Workspace)),
                ContextMenuAction::Close => {
                    if crate::config::load().confirm_close {
                        Ok(Some(if close_group {
                            RenameTarget::DeleteWorkspaceGroup
                        } else {
                            RenameTarget::DeleteWorkspace
                        }))
                    } else {
                        request_action(
                            client,
                            "context-close-workspace",
                            "delete_workspace",
                            json!({ "id": id, "close_group": close_group }),
                            "close workspace",
                        )?;
                        ensure_active_default_pane(client, terminal_size)?;
                        Ok(None)
                    }
                }
                ContextMenuAction::NewWorktree => Ok(Some(RenameTarget::CreateWorktree)),
                ContextMenuAction::OpenWorktree => Ok(Some(RenameTarget::OpenWorktree)),
                ContextMenuAction::RemoveWorktree => Ok(Some(RenameTarget::RemoveWorktree)),
                ContextMenuAction::ToggleWorktreeGroup => {
                    if let Some(group) = workspace_group_key(snapshot, &space_id, &id) {
                        if !mouse_state.collapsed_worktree_groups.remove(&group) {
                            mouse_state.collapsed_worktree_groups.insert(group);
                        }
                    }
                    Ok(None)
                }
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
                    if crate::config::load().prompt_new_tab_name {
                        Ok(Some(RenameTarget::CreateTab))
                    } else {
                        create_context_tab(client, terminal_size)?;
                        Ok(None)
                    }
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
                ContextMenuAction::Activate
                | ContextMenuAction::NewTab
                | ContextMenuAction::NewWorktree
                | ContextMenuAction::OpenWorktree
                | ContextMenuAction::RemoveWorktree => Ok(None),
                ContextMenuAction::ToggleWorktreeGroup => Ok(None),
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
    if snapshot.popup_pane_id.as_deref() == Some(focused) {
        return snapshot.panes.iter().any(|pane| pane.pane_id == focused);
    }
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
        Action::NewTab => Some(RenameTarget::CreateTab),
        Action::RenameActiveSpace => Some(RenameTarget::Space),
        Action::CreateSpace => Some(RenameTarget::CreateSpace),
        Action::DeleteActiveWorkspace => Some(RenameTarget::DeleteWorkspace),
        Action::DeleteActiveSpace => Some(RenameTarget::DeleteSpace),
        Action::SwitchWorkspaceByName => Some(RenameTarget::SwitchWorkspace),
        Action::PluginAction => Some(RenameTarget::PluginAction),
        _ => None,
    }
}

fn prompt_for_target(target: RenameTarget, snapshot: &SessionSnapshot) -> RenamePrompt {
    if target == RenameTarget::CreateTab {
        RenamePrompt::new_tab(next_tab_name(snapshot))
    } else {
        let input = match target {
            RenameTarget::Pane => snapshot
                .focused_pane_id
                .as_deref()
                .and_then(|id| snapshot.panes.iter().find(|pane| pane.pane_id == id))
                .and_then(|pane| pane.label.clone())
                .unwrap_or_default(),
            RenameTarget::Tab => {
                let tab_id = active_tab_id(snapshot);
                active_workspace(snapshot)
                    .and_then(|workspace| {
                        tab_id
                            .as_deref()
                            .and_then(|id| workspace.tabs.iter().find(|tab| tab.tab_id == id))
                    })
                    .map(|tab| tab.name.clone())
                    .unwrap_or_default()
            }
            RenameTarget::Workspace => active_workspace(snapshot)
                .map(|workspace| workspace.name.clone())
                .unwrap_or_default(),
            RenameTarget::Space => snapshot
                .spaces
                .iter()
                .find(|space| space.space_id == snapshot.active_space_id)
                .map(|space| space.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        RenamePrompt::with_input(target, input, false)
    }
}

fn next_tab_name(snapshot: &SessionSnapshot) -> String {
    active_workspace(snapshot)
        .map(|workspace| workspace.tabs.len() + 1)
        .unwrap_or(1)
        .to_string()
}

fn close_active_workspace(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    let workspace = active_workspace(snapshot)
        .ok_or_else(|| ClientError::Server("no active workspace".into()))?;
    let close_group =
        workspace_has_linked_children(snapshot, &snapshot.active_space_id, &workspace.workspace_id);
    request_action(
        client,
        "close-workspace",
        "delete_workspace",
        json!({ "id": workspace.workspace_id, "close_group": close_group }),
        "close workspace",
    )?;
    ensure_active_default_pane(client, terminal_size)
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
        RenameTarget::CreateWorktree | RenameTarget::OpenWorktree | RenameTarget::RemoveWorktree
    ) {
        let workspace = active_workspace(snapshot)
            .ok_or_else(|| ClientError::Server("no active workspace".into()))?;
        if target == RenameTarget::RemoveWorktree {
            if workspace.name != name {
                return Ok(());
            }
            worktree_actions::run(["remove", "--workspace", &workspace.workspace_id, "--force"])?;
        } else {
            let mut args = vec![
                if target == RenameTarget::CreateWorktree {
                    "create"
                } else {
                    "open"
                },
                "--workspace",
                workspace.workspace_id.as_str(),
            ];
            if target == RenameTarget::CreateWorktree {
                args.extend(["--branch", name.as_str(), "--focus"]);
            } else if std::path::Path::new(&name).is_absolute() {
                args.extend(["--path", name.as_str(), "--focus"]);
            } else {
                args.extend(["--branch", name.as_str(), "--focus"]);
            }
            worktree_actions::run(args)?;
        }
        ensure_active_default_pane(client, terminal_size)?;
        return Ok(());
    }
    if matches!(
        target,
        RenameTarget::DeleteWorkspace
            | RenameTarget::DeleteWorkspaceGroup
            | RenameTarget::DeleteSpace
    ) {
        let (operation, id, expected_name) = match target {
            RenameTarget::DeleteWorkspace | RenameTarget::DeleteWorkspaceGroup => {
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
                let payload = if matches!(target, RenameTarget::DeleteWorkspaceGroup) {
                    json!({ "id": id, "close_group": true })
                } else {
                    json!({ "id": id })
                };
                request_action(
                    client,
                    &format!("confirm-{operation}"),
                    operation,
                    payload,
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
        RenameTarget::CreateTab => {
            request_action(
                client,
                "create-tab",
                "create_tab",
                json!({ "name": name }),
                "create tab",
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
        RenameTarget::DeleteWorkspace
        | RenameTarget::DeleteWorkspaceGroup
        | RenameTarget::DeleteSpace
        | RenameTarget::CreateWorktree
        | RenameTarget::OpenWorktree
        | RenameTarget::RemoveWorktree => unreachable!(),
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
        RenameTarget::PluginAction => unreachable!(),
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

fn switch_to_workspace(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    space_id: &str,
    workspace_id: &str,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    let mut result = Ok(());
    if space_id != snapshot.active_space_id {
        result = request_action(
            client,
            "navigate-space",
            "switch_space",
            json!({ "id": space_id }),
            "switch space",
        );
    }
    result
        .and_then(|()| {
            request_action(
                client,
                "navigate-workspace",
                "switch_workspace",
                json!({ "id": workspace_id }),
                "switch workspace",
            )
        })
        .and_then(|()| ensure_active_default_pane(client, terminal_size))
}

fn workspace_has_linked_children(
    snapshot: &SessionSnapshot,
    space_id: &str,
    workspace_id: &str,
) -> bool {
    let Some(workspaces) = snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == space_id)
        .map(|space| &space.workspaces)
    else {
        return false;
    };
    let Some(workspace) = workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
    else {
        return false;
    };
    let Some(group) = workspace.worktree_group.as_deref() else {
        return false;
    };
    !workspace.is_linked_worktree
        && workspaces.iter().any(|candidate| {
            candidate.workspace_id != workspace_id
                && candidate.is_linked_worktree
                && candidate.worktree_group.as_deref() == Some(group)
        })
}

fn workspace_group_key(
    snapshot: &SessionSnapshot,
    space_id: &str,
    workspace_id: &str,
) -> Option<String> {
    snapshot
        .spaces
        .iter()
        .find(|space| space.space_id == space_id)
        .and_then(|space| {
            space
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == workspace_id)
        })
        .and_then(|workspace| workspace.worktree_group.clone())
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

fn swap_direction_name(action: Action) -> &'static str {
    match action {
        Action::SwapLeft => "left",
        Action::SwapRight => "right",
        Action::SwapUp => "up",
        Action::SwapDown => "down",
        _ => unreachable!("not a directional swap action"),
    }
}

fn resize_direction_name(action: Action) -> &'static str {
    match action {
        Action::ResizePaneLeft => "left",
        Action::ResizePaneDown => "down",
        Action::ResizePaneUp => "up",
        Action::ResizePaneRight => "right",
        _ => unreachable!("not a directional resize action"),
    }
}

fn directional_pane_id<'a>(
    snapshot: &'a SessionSnapshot,
    source_pane_id: &str,
    direction: &str,
) -> Option<&'a str> {
    let target = match direction {
        "left" => crate::model::layout::FocusDirection::Left,
        "right" => crate::model::layout::FocusDirection::Right,
        "up" => crate::model::layout::FocusDirection::Up,
        "down" => crate::model::layout::FocusDirection::Down,
        _ => return None,
    };
    active_workspace(snapshot)
        .and_then(|workspace| {
            workspace
                .tabs
                .iter()
                .find(|tab| tab.tab_id == workspace.active_tab_id)
        })
        .and_then(|tab| tab.layout.as_ref())
        .and_then(|layout| layout.directional_pane(source_pane_id, target))
}

fn execute_action(
    pressed: Action,
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    previous_pane_id: Option<&str>,
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
        Action::MoveTabPrevious | Action::MoveTabNext => {
            if let Some((tab_id, index, tab_count)) = active_tab_position(snapshot) {
                let insert_index = if matches!(pressed, Action::MoveTabPrevious) {
                    index.saturating_sub(1)
                } else {
                    (index + 2).min(tab_count)
                };
                request_action(
                    client,
                    "palette-move-tab",
                    "move_tab",
                    json!({ "id": tab_id, "insert_index": insert_index }),
                    "move tab",
                )?;
            }
            Ok(false)
        }
        Action::SwitchTab(index) => {
            if let Some(tab_id) = indexed_tab_id(snapshot, index) {
                request_action(
                    client,
                    "palette-switch-indexed-tab",
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
        Action::EditScrollback => {
            let pane = snapshot
                .focused_pane_id
                .as_deref()
                .and_then(|pane_id| snapshot.panes.iter().find(|pane| pane.pane_id == pane_id))
                .ok_or_else(|| ClientError::Server("no focused pane".into()))?;
            open_scrollback_in_editor(pane)?;
            Ok(false)
        }
        Action::EnterResizeMode => Ok(false),
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
        Action::LastPane => {
            if let Some(pane_id) = last_pane_target(snapshot, previous_pane_id) {
                request_action(
                    client,
                    "palette-focus-last-pane",
                    "focus_pane",
                    json!({ "pane_id": pane_id }),
                    "focus last pane",
                )?;
            }
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
        Action::ResizePaneLeft
        | Action::ResizePaneDown
        | Action::ResizePaneUp
        | Action::ResizePaneRight => {
            request_action(
                client,
                "palette-resize-pane-direction",
                "resize_pane_direction",
                json!({
                    "direction": resize_direction_name(pressed),
                    "amount": 0.05,
                }),
                "resize pane",
            )?;
            Ok(false)
        }
        Action::SwapLeft | Action::SwapRight | Action::SwapUp | Action::SwapDown => {
            let source = snapshot
                .focused_pane_id
                .as_deref()
                .ok_or_else(|| ClientError::Server("no focused pane".into()))?;
            let target = directional_pane_id(snapshot, source, swap_direction_name(pressed))
                .ok_or_else(|| ClientError::Server("no pane in that direction".into()))?;
            request_action(
                client,
                "palette-swap-direction",
                "swap_panes",
                json!({
                    "source_pane_id": source,
                    "target_pane_id": target,
                }),
                "swap panes",
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

fn adjacent_agent_pane_id(snapshot: &SessionSnapshot, forward: bool) -> Option<String> {
    let agents = snapshot
        .panes
        .iter()
        .filter(|pane| pane.agent.is_some())
        .map(|pane| pane.pane_id.clone())
        .collect::<Vec<_>>();
    if agents.is_empty() {
        return None;
    }
    let current = snapshot
        .focused_pane_id
        .as_deref()
        .and_then(|focused| agents.iter().position(|pane_id| pane_id == focused));
    let index = match (current, forward) {
        (Some(index), true) => (index + 1) % agents.len(),
        (Some(index), false) => (index + agents.len() - 1) % agents.len(),
        (None, true) => 0,
        (None, false) => agents.len() - 1,
    };
    agents.get(index).cloned()
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
    let config = crate::config::load();
    let borders = if config.pane_borders.shows_borders(false) && config.pane_outer_borders {
        ratatui::widgets::Borders::ALL
    } else {
        ratatui::widgets::Borders::NONE
    };
    renderer::pane_inner_size_with_options(
        renderer::pane_content_area(area),
        borders,
        config.pane_scrollbars,
    )
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

fn switch_navigator_target(
    client: &ControlClient,
    snapshot: &SessionSnapshot,
    target: NavigatorTarget,
    terminal_size: (u16, u16),
) -> Result<(), ClientError> {
    let (space_id, workspace_id, tab_id, pane_id) = match target {
        NavigatorTarget::NewWorkspace => {
            return create_workspace_from_current_directory(client, terminal_size);
        }
        NavigatorTarget::NewTab => {
            return execute_action(Action::NewTab, client, snapshot, None, terminal_size)
                .map(|_| ());
        }
        NavigatorTarget::Menu(_) => {
            return Err(ClientError::Server(
                "menu target requires the client UI".into(),
            ));
        }
        NavigatorTarget::Space(space_id) => (space_id, None, None, None),
        NavigatorTarget::Workspace { space_id, id } => (space_id, Some(id), None, None),
        NavigatorTarget::Tab {
            space_id,
            workspace_id,
            id,
        } => (space_id, Some(workspace_id), Some(id), None),
        NavigatorTarget::Pane {
            space_id,
            workspace_id,
            tab_id,
            id,
        } => (space_id, Some(workspace_id), Some(tab_id), Some(id)),
        NavigatorTarget::Agent {
            space_id,
            workspace_id,
            tab_id,
            id,
        } => (space_id, Some(workspace_id), Some(tab_id), Some(id)),
    };
    if !snapshot
        .spaces
        .iter()
        .any(|space| space.space_id == space_id)
    {
        return Err(ClientError::Server(
            "navigator target no longer exists".into(),
        ));
    }
    request_action(
        client,
        "navigator-space",
        "switch_space",
        json!({ "id": space_id }),
        "switch space",
    )?;
    if let Some(workspace_id) = workspace_id {
        request_action(
            client,
            "navigator-workspace",
            "switch_workspace",
            json!({ "id": workspace_id }),
            "switch workspace",
        )?;
    }
    if let Some(tab_id) = tab_id {
        request_action(
            client,
            "navigator-tab",
            "switch_tab",
            json!({ "id": tab_id }),
            "switch tab",
        )?;
    }
    if let Some(pane_id) = pane_id {
        request_action(
            client,
            "navigator-pane",
            "focus_pane",
            json!({ "pane_id": pane_id }),
            "focus pane",
        )?;
    }
    ensure_active_default_pane(client, terminal_size)
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

fn input_pane_id(snapshot: &SessionSnapshot) -> Option<&str> {
    snapshot
        .popup_pane_id
        .as_deref()
        .or(snapshot.focused_pane_id.as_deref())
}

fn startup_error_message(error: ClientError) -> String {
    match error {
        ClientError::Io(error) => error.to_string(),
        ClientError::Frame(error) => format!("{error:?}"),
        ClientError::Json(error) => error.to_string(),
        ClientError::Server(message) => message,
    }
}

fn open_scrollback_in_editor(pane: &crate::server::session::PaneView) -> Result<(), ClientError> {
    let path = std::env::temp_dir().join(format!(
        "spindle-{}-{}-scrollback.txt",
        std::process::id(),
        pane.pane_id
    ));
    std::fs::write(&path, scrollback_text(pane)).map_err(ClientError::Io)?;

    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad.exe".into()
            } else {
                "vi".into()
            }
        });
    let mut parts = editor.split_whitespace();
    let command = parts.next().unwrap_or("notepad.exe");
    let mut process = std::process::Command::new(command);
    process.args(parts).arg(&path);
    process.spawn().map(|_| ()).map_err(ClientError::Io)
}

fn scrollback_text(pane: &crate::server::session::PaneView) -> String {
    if pane.scrollback.is_empty() {
        return pane.screen.clone();
    }

    let mut parser = vt100::Parser::new(pane.rows.max(1), pane.cols.max(1), MAX_SCROLLBACK_ROWS);
    parser.process(&pane.scrollback);
    let screen = parser.screen_mut();
    screen.set_scrollback(usize::MAX);
    let history_rows = screen.scrollback();
    let mut lines = Vec::with_capacity(history_rows + usize::from(pane.rows));

    for offset in (1..=history_rows).rev() {
        screen.set_scrollback(offset);
        if let Some(line) = screen.rows(0, pane.cols.max(1)).next() {
            lines.push(line);
        }
    }
    screen.set_scrollback(0);
    lines.extend(screen.rows(0, pane.cols.max(1)));
    lines.join("\n")
}

fn key_code_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let modifiers = key.modifiers;
    if let Some(bytes) = modified_special_key_bytes(key.code, modifiers) {
        return Some(bytes);
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(character) = key.code {
            let character = character.to_ascii_lowercase();
            let byte = match character {
                '@' | ' ' | '2' => 0,
                'a'..='z' => character as u8 - b'a' + 1,
                '[' | '3' => 27,
                '\\' | '4' => 28,
                ']' | '5' => 29,
                '^' | '6' => 30,
                '_' | '7' | '/' => 31,
                _ => return None,
            };
            return Some(vec![byte]);
        }
    }
    match key.code {
        KeyCode::Char(character) => {
            let mut bytes = Vec::new();
            if modifiers.contains(KeyModifiers::ALT) {
                bytes.push(27);
            }
            let character = if modifiers.contains(KeyModifiers::SHIFT) {
                character.to_ascii_uppercase()
            } else {
                character
            };
            bytes.extend(character.to_string().into_bytes());
            Some(bytes)
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(if modifiers.contains(KeyModifiers::ALT) {
            vec![27, 127]
        } else {
            vec![127]
        }),
        KeyCode::Tab => Some(if modifiers.contains(KeyModifiers::SHIFT) {
            b"\x1b[Z".to_vec()
        } else {
            vec![b'\t']
        }),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Esc => Some(vec![27]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::F(n @ 1..=12) => Some(plain_function_key_bytes(n)),
        _ => None,
    }
}

fn plain_function_key_bytes(n: u8) -> Vec<u8> {
    match n {
        1 => b"\x1bOP".to_vec(),
        2 => b"\x1bOQ".to_vec(),
        3 => b"\x1bOR".to_vec(),
        4 => b"\x1bOS".to_vec(),
        5 => b"\x1b[15~".to_vec(),
        6 => b"\x1b[17~".to_vec(),
        7 => b"\x1b[18~".to_vec(),
        8 => b"\x1b[19~".to_vec(),
        9 => b"\x1b[20~".to_vec(),
        10 => b"\x1b[21~".to_vec(),
        11 => b"\x1b[23~".to_vec(),
        12 => b"\x1b[24~".to_vec(),
        _ => Vec::new(),
    }
}

fn modified_special_key_bytes(code: KeyCode, modifiers: KeyModifiers) -> Option<Vec<u8>> {
    if !modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL) {
        return None;
    }
    let modifier = 1
        + u8::from(modifiers.contains(KeyModifiers::SHIFT))
        + 2 * u8::from(modifiers.contains(KeyModifiers::ALT))
        + 4 * u8::from(modifiers.contains(KeyModifiers::CONTROL));
    let bytes = match code {
        KeyCode::Up => format!("\x1b[1;{modifier}A").into_bytes(),
        KeyCode::Down => format!("\x1b[1;{modifier}B").into_bytes(),
        KeyCode::Right => format!("\x1b[1;{modifier}C").into_bytes(),
        KeyCode::Left => format!("\x1b[1;{modifier}D").into_bytes(),
        KeyCode::Home => format!("\x1b[1;{modifier}H").into_bytes(),
        KeyCode::End => format!("\x1b[1;{modifier}F").into_bytes(),
        KeyCode::Insert => format!("\x1b[2;{modifier}~").into_bytes(),
        KeyCode::Delete => format!("\x1b[3;{modifier}~").into_bytes(),
        KeyCode::PageUp => format!("\x1b[5;{modifier}~").into_bytes(),
        KeyCode::PageDown => format!("\x1b[6;{modifier}~").into_bytes(),
        KeyCode::F(1) => format!("\x1b[1;{modifier}P").into_bytes(),
        KeyCode::F(2) => format!("\x1b[1;{modifier}Q").into_bytes(),
        KeyCode::F(3) => format!("\x1b[1;{modifier}R").into_bytes(),
        KeyCode::F(4) => format!("\x1b[1;{modifier}S").into_bytes(),
        KeyCode::F(n @ 5..=12) => {
            let code = [0, 0, 0, 0, 0, 15, 17, 18, 19, 20, 21, 23, 24][n as usize];
            format!("\x1b[{code};{modifier}~").into_bytes()
        }
        _ => return None,
    };
    Some(bytes)
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

fn indexed_tab_id(snapshot: &SessionSnapshot, index: usize) -> Option<String> {
    active_workspace(snapshot)
        .and_then(|workspace| workspace.tabs.get(index))
        .map(|tab| tab.tab_id.clone())
}

fn active_tab_position(snapshot: &SessionSnapshot) -> Option<(String, usize, usize)> {
    let workspace = active_workspace(snapshot)?;
    let index = workspace
        .tabs
        .iter()
        .position(|tab| tab.tab_id == workspace.active_tab_id)?;
    Some((workspace.active_tab_id.clone(), index, workspace.tabs.len()))
}

fn last_pane_target(snapshot: &SessionSnapshot, previous_pane_id: Option<&str>) -> Option<String> {
    let pane_id = previous_pane_id?;
    if snapshot.focused_pane_id.as_deref() == Some(pane_id)
        || !snapshot.panes.iter().any(|pane| pane.pane_id == pane_id)
    {
        return None;
    }
    Some(pane_id.to_owned())
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

fn uses_mobile_navigation(terminal_width: u16, threshold: u16) -> bool {
    terminal_width <= threshold
}

#[cfg(test)]
mod tests {
    use super::{
        active_tab_id, active_workspace, adjacent_agent_pane_id, adjacent_space_id,
        adjacent_tab_id, adjacent_workspace_id, adjust_scrollback_offset, apply_scrollback_views,
        current_snapshot, ensure_active_default_pane, indexed_workspace_selection, input_pane_id,
        key_code_bytes, move_workspace_selection, page_key_bytes, pane_mouse_target, pane_size,
        reconnect_requires_reattach, record_action_error, rename_target, renderer,
        require_server_success, should_forward_pane_mouse, should_forward_pane_mouse_with_modifier,
        snapshot_has_focused_pane, startup_error_action, uses_mobile_navigation,
        visible_web_url_at_point, workspace_has_linked_children, workspace_id_by_name,
        workspace_picker_key, CachedScrollbackView, ControlClient, PaneClick, PaneMouseCapture,
        SplitDirection, SplitDrag, StartupErrorAction, WorkspacePickerKey,
    };
    use crate::client::input::{Action, Keymap};
    use crate::config::Config;
    use crate::protocol::{ProtocolError, Response, PROTOCOL_VERSION};
    use crate::server::session::Session;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    fn snapshot_with_mouse_pane() -> crate::server::session::SessionSnapshot {
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.spaces[0].workspaces[0].tabs[0].layout =
            Some(crate::model::layout::LayoutNode::pane("pane-1"));
        snapshot.panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": "pane-1", "command": "powershell.exe", "args": [],
                "cwd": "C:/", "status": "Running", "scrollback_bytes": 0,
                "mouse_reporting": true, "mouse_release": true,
                "mouse_motion": true, "mouse_any_motion": true
            }))
            .unwrap(),
        );
        snapshot
    }

    fn mouse_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

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
                "scrollback_bytes": 37,
                "scrollback": b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix".to_vec()
            }))
            .unwrap(),
        );
        let mut offsets = std::collections::HashMap::from([("pane-1".into(), 4)]);
        let mut cached_views = std::collections::HashMap::<String, CachedScrollbackView>::new();

        apply_scrollback_views(&mut snapshot, &mut offsets, &mut cached_views);
        assert!(snapshot.panes[0].screen.contains("one"));
        assert!(snapshot.panes[0].screen.contains("two"));
        assert!(!snapshot.panes[0].screen.contains("five"));

        offsets.insert("pane-1".into(), 0);
        apply_scrollback_views(&mut snapshot, &mut offsets, &mut cached_views);
        assert!(snapshot.panes[0].screen.contains("five"));
        assert!(snapshot.panes[0].screen.contains("six"));
        assert!(!offsets.contains_key("pane-1"));
    }

    #[test]
    fn common_keys_encode_for_a_pty() {
        assert_eq!(
            key_code_bytes(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(vec![b'\r'])
        );
        assert_eq!(
            key_code_bytes(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            key_code_bytes(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(vec![3])
        );
        assert_eq!(
            key_code_bytes(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL)),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            key_code_bytes(KeyEvent::new(KeyCode::Home, KeyModifiers::SHIFT)),
            Some(b"\x1b[1;2H".to_vec())
        );
        assert_eq!(
            key_code_bytes(KeyEvent::new(KeyCode::F(5), KeyModifiers::ALT)),
            Some(b"\x1b[15;3~".to_vec())
        );
        assert_eq!(page_key_bytes(KeyCode::PageUp), Some(b"\x1b[5~".to_vec()));
        assert_eq!(page_key_bytes(KeyCode::PageDown), Some(b"\x1b[6~".to_vec()));
        assert_eq!(pane_size((120, 40)), (93, 39));
        assert_eq!(pane_size((0, 0)), (1, 1));
    }

    #[test]
    fn mobile_navigation_replaces_hidden_sidebar_controls() {
        assert!(uses_mobile_navigation(64, 64));
        assert!(!uses_mobile_navigation(65, 64));
    }

    #[test]
    fn saved_sidebar_preference_overrides_startup_default() {
        let config = Config {
            sidebar_start_collapsed: true,
            ..Config::default()
        };
        let preferences = super::super::preferences::ClientPreferences {
            sidebar_collapsed: Some(false),
            ..Default::default()
        };
        assert!(!super::initial_sidebar_collapsed(&preferences, &config));
        assert!(super::initial_sidebar_collapsed(
            &Default::default(),
            &config
        ));
    }

    #[test]
    fn new_tab_action_opens_a_create_tab_prompt() {
        assert_eq!(
            rename_target(Action::NewTab),
            Some(super::RenameTarget::CreateTab)
        );
    }

    #[test]
    fn rename_prompts_start_with_the_current_herdr_labels() {
        let snapshot = Session::default().snapshot().clone();
        assert_eq!(
            super::prompt_for_target(super::RenameTarget::Workspace, &snapshot).input,
            "Current project"
        );
        assert_eq!(
            super::prompt_for_target(super::RenameTarget::Tab, &snapshot).input,
            "Main"
        );
        assert_eq!(
            super::prompt_for_target(super::RenameTarget::CreateTab, &snapshot).input,
            "2"
        );
    }

    #[test]
    fn agent_navigation_wraps_like_herdr() {
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.panes = vec![
            serde_json::from_value(serde_json::json!({
                "pane_id": "agent-1", "command": "codex", "args": [], "cwd": "C:/one",
                "status": "Running", "scrollback_bytes": 0, "agent": "codex"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "pane_id": "agent-2", "command": "opencode", "args": [], "cwd": "C:/two",
                "status": "Running", "scrollback_bytes": 0, "agent": "open_code"
            }))
            .unwrap(),
        ];
        assert_eq!(
            adjacent_agent_pane_id(&snapshot, true).as_deref(),
            Some("agent-1")
        );
        assert_eq!(
            adjacent_agent_pane_id(&snapshot, false).as_deref(),
            Some("agent-2")
        );
        snapshot.focused_pane_id = Some("agent-1".into());
        assert_eq!(
            adjacent_agent_pane_id(&snapshot, true).as_deref(),
            Some("agent-2")
        );
        assert_eq!(
            adjacent_agent_pane_id(&snapshot, false).as_deref(),
            Some("agent-2")
        );
    }

    #[test]
    fn popup_gets_keyboard_input_before_background_focus() {
        let mut snapshot = Session::default().snapshot().clone();
        snapshot.focused_pane_id = Some("background".into());
        snapshot.popup_pane_id = Some("popup".into());
        assert_eq!(input_pane_id(&snapshot), Some("popup"));
        snapshot.popup_pane_id = None;
        assert_eq!(input_pane_id(&snapshot), Some("background"));
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
    fn terminal_mouse_routing_respects_each_reported_event_mode() {
        let mut snapshot = snapshot_with_mouse_pane();
        let pane = &mut snapshot.panes[0];
        for (kind, expected) in [
            (MouseEventKind::Down(MouseButton::Left), true),
            (MouseEventKind::Up(MouseButton::Left), true),
            (MouseEventKind::Drag(MouseButton::Left), true),
            (MouseEventKind::Moved, true),
            (MouseEventKind::ScrollDown, true),
        ] {
            assert_eq!(should_forward_pane_mouse(pane, kind), expected);
        }
        pane.mouse_release = false;
        pane.mouse_motion = false;
        pane.mouse_any_motion = false;
        assert!(!should_forward_pane_mouse(
            pane,
            MouseEventKind::Up(MouseButton::Left)
        ));
        assert!(!should_forward_pane_mouse(
            pane,
            MouseEventKind::Drag(MouseButton::Left)
        ));
        assert!(!should_forward_pane_mouse(pane, MouseEventKind::Moved));
    }

    #[test]
    fn right_click_is_forwarded_only_when_passthrough_is_enabled() {
        let mut snapshot = snapshot_with_mouse_pane();
        let pane = &mut snapshot.panes[0];
        let right_click = MouseEventKind::Down(MouseButton::Right);
        assert!(!should_forward_pane_mouse(pane, right_click));
        pane.right_click_passthrough = true;
        assert!(should_forward_pane_mouse(pane, right_click));
    }

    #[test]
    fn configured_right_click_modifier_allows_passthrough_and_strips_modifier() {
        let snapshot = snapshot_with_mouse_pane();
        let pane = &snapshot.panes[0];
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::CONTROL,
        };
        assert!(should_forward_pane_mouse_with_modifier(
            pane,
            mouse,
            Some(crossterm::event::KeyModifiers::CONTROL)
        ));
        assert!(!should_forward_pane_mouse_with_modifier(
            pane,
            mouse,
            Some(crossterm::event::KeyModifiers::ALT)
        ));
    }

    #[test]
    fn captured_terminal_drag_stays_with_its_pane_outside_the_pane_bounds() {
        let snapshot = snapshot_with_mouse_pane();
        let area = Rect::new(0, 0, 120, 40);
        let pane_rect = renderer::pane_rectangles(
            &snapshot,
            renderer::pane_content_area_for_snapshot(&snapshot, area, false),
        )[0]
        .rect;
        let capture = Some(PaneMouseCapture {
            pane_id: "pane-1".into(),
            rect: pane_rect,
            button: MouseButton::Left,
        });
        for kind in [
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            assert_eq!(
                pane_mouse_target(&snapshot, area, mouse_event(kind, 0, 0), &capture, false,),
                Some(("pane-1".into(), pane_rect))
            );
        }
        assert_eq!(
            pane_mouse_target(
                &snapshot,
                area,
                mouse_event(MouseEventKind::Up(MouseButton::Right), 0, 0),
                &capture,
                false,
            ),
            None
        );
    }

    #[test]
    fn terminal_mouse_hit_region_matches_pane_inner_area() {
        let snapshot = snapshot_with_mouse_pane();
        let area = Rect::new(0, 0, 120, 40);
        let pane_rect = renderer::pane_rectangles(
            &snapshot,
            renderer::pane_content_area_for_snapshot(&snapshot, area, false),
        )[0]
        .rect;
        let config = crate::config::load();
        let borders = renderer::pane_borders_for_rect(
            pane_rect,
            &renderer::pane_rectangles(
                &snapshot,
                renderer::pane_content_area_for_snapshot(&snapshot, area, false),
            ),
            config.pane_borders,
            config.pane_outer_borders,
            config.pane_gaps,
        );
        let inner = renderer::pane_inner_area(pane_rect, borders, config.pane_scrollbars);
        let capture = None;
        assert!(pane_mouse_target(
            &snapshot,
            area,
            mouse_event(MouseEventKind::Down(MouseButton::Left), inner.x, inner.y),
            &capture,
            false,
        )
        .is_some());
        assert!(pane_mouse_target(
            &snapshot,
            area,
            mouse_event(
                MouseEventKind::Down(MouseButton::Left),
                inner.right(),
                inner.y,
            ),
            &capture,
            false,
        )
        .is_none());
        if pane_rect.x < inner.x || pane_rect.y < inner.y {
            assert!(pane_mouse_target(
                &snapshot,
                area,
                mouse_event(
                    MouseEventKind::Down(MouseButton::Left),
                    pane_rect.x,
                    pane_rect.y
                ),
                &capture,
                false,
            )
            .is_none());
        }
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
        let pane_rect = renderer::pane_rectangles(&snapshot, pane_area)[0].rect;
        let config = crate::config::load();
        let inner = renderer::pane_inner_area(
            pane_rect,
            renderer::pane_borders_for_rect(
                pane_rect,
                &renderer::pane_rectangles(&snapshot, pane_area),
                config.pane_borders,
                config.pane_outer_borders,
                config.pane_gaps,
            ),
            config.pane_scrollbars,
        );
        assert_eq!(
            visible_web_url_at_point(&snapshot, pane_area, inner.x + 5, inner.y).as_deref(),
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
    fn workspace_context_close_detects_linked_worktree_group() {
        let mut session = Session::default();
        let linked_id = session.create_workspace("Linked".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut snapshot = session.snapshot().clone();
        snapshot.spaces[0].workspaces[0].worktree_group = Some("repo-group".into());
        snapshot.spaces[0].workspaces[1].is_linked_worktree = true;
        snapshot.spaces[0].workspaces[1].worktree_group = Some("repo-group".into());

        assert!(workspace_has_linked_children(
            &snapshot,
            "space-1",
            "workspace-1"
        ));
        assert!(!workspace_has_linked_children(
            &snapshot, "space-1", &linked_id
        ));
    }

    #[test]
    fn workspace_picker_moves_around_the_active_spaces_workspaces() {
        let mut session = Session::default();
        let first = active_workspace(session.snapshot())
            .unwrap()
            .workspace_id
            .clone();
        let second = session.create_workspace("Build".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let third = session.create_workspace("Review".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let snapshot = session.snapshot();
        let selected = (snapshot.active_space_id.clone(), first.clone());

        assert_eq!(
            move_workspace_selection(snapshot, Some(&selected), true),
            Some((snapshot.active_space_id.clone(), second.clone()))
        );
        assert_eq!(
            move_workspace_selection(
                snapshot,
                Some(&(snapshot.active_space_id.clone(), second.clone())),
                true,
            ),
            Some((snapshot.active_space_id.clone(), third))
        );
        assert_eq!(
            move_workspace_selection(
                snapshot,
                Some(&(snapshot.active_space_id.clone(), second.clone())),
                false,
            ),
            Some((snapshot.active_space_id.clone(), first))
        );
    }

    #[test]
    fn workspace_picker_number_selects_a_workspace_in_the_active_space() {
        let mut session = Session::default();
        let second = session.create_workspace("Build".into()).unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let snapshot = session.snapshot();

        assert_eq!(
            indexed_workspace_selection(snapshot, 1),
            Some((snapshot.active_space_id.clone(), second))
        );
        assert_eq!(indexed_workspace_selection(snapshot, 9), None);
    }

    #[test]
    fn workspace_picker_cycles_and_indexes_workspaces_across_spaces() {
        let mut session = Session::default();
        let first = active_workspace(session.snapshot())
            .unwrap()
            .workspace_id
            .clone();
        session.create_space("Other".into()).unwrap();
        let second = active_workspace(session.snapshot())
            .unwrap()
            .workspace_id
            .clone();
        let snapshot = session.snapshot();
        let first_space = snapshot
            .spaces
            .iter()
            .find(|space| {
                space
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == first)
            })
            .unwrap()
            .space_id
            .clone();
        let second_space = snapshot
            .spaces
            .iter()
            .find(|space| {
                space
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == second)
            })
            .unwrap()
            .space_id
            .clone();

        assert_ne!(first_space, second_space);
        assert_eq!(
            move_workspace_selection(snapshot, Some(&(first_space, first)), true),
            Some((second_space.clone(), second.clone()))
        );
        assert_eq!(
            indexed_workspace_selection(snapshot, 1),
            Some((second_space, second))
        );
    }

    #[test]
    fn workspace_picker_keys_match_herdr_navigation_controls() {
        for (key, expected) in [
            (KeyCode::Up, WorkspacePickerKey::Move(false)),
            (KeyCode::Down, WorkspacePickerKey::Move(true)),
            (KeyCode::Enter, WorkspacePickerKey::Confirm),
            (KeyCode::Esc, WorkspacePickerKey::Cancel),
        ] {
            assert_eq!(
                workspace_picker_key(KeyEvent::new(key, KeyModifiers::NONE), &Keymap::default()),
                expected
            );
        }
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL,),
                &Keymap::default(),
            ),
            WorkspacePickerKey::Cancel
        );
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
                &Keymap::default(),
            ),
            WorkspacePickerKey::Choose(0)
        );
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('9'), KeyModifiers::NONE),
                &Keymap::default(),
            ),
            WorkspacePickerKey::Choose(8)
        );
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE),
                &Keymap::default(),
            ),
            WorkspacePickerKey::Ignore
        );
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL),
                &Keymap::default(),
            ),
            WorkspacePickerKey::Ignore
        );
    }

    #[test]
    fn workspace_picker_uses_configured_navigate_keys_and_prefix() {
        let config = Config {
            prefix: Some("ctrl+a".into()),
            bindings: BTreeMap::from([
                ("navigate_workspace_up".into(), vec!["k".into()]),
                (
                    "navigate_workspace_down".into(),
                    vec!["j".into(), "ctrl+j".into()],
                ),
            ]),
            ..Config::default()
        };
        let keymap = Keymap::from_config(&config);

        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
                &keymap,
            ),
            WorkspacePickerKey::Move(false)
        );
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                &keymap,
            ),
            WorkspacePickerKey::Move(true)
        );
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                &keymap,
            ),
            WorkspacePickerKey::Move(true)
        );
        assert_eq!(
            workspace_picker_key(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
                &keymap,
            ),
            WorkspacePickerKey::Cancel
        );
        assert_eq!(
            workspace_picker_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &keymap),
            WorkspacePickerKey::Ignore
        );
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

    #[test]
    fn pane_scrollbar_click_mapping_matches_history_direction() {
        let track = Rect::new(0, 10, 1, 11);
        assert_eq!(
            crate::client::scrollbar::offset_from_row(100, 10, track, track.y),
            100
        );
        assert_eq!(
            crate::client::scrollbar::offset_from_row(100, 10, track, track.bottom() - 1),
            0
        );
        assert_eq!(
            crate::client::scrollbar::offset_from_row(100, 10, track, track.y + 5),
            50
        );
    }
}
