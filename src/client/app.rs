use super::input::{action, is_prefix, Action};
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
    let mut last_size = None;
    loop {
        let snapshot = current_snapshot(client)?;
        let terminal_size = size().map_err(ClientError::Io)?;
        if last_size != Some(terminal_size) {
            resize_panes(client, &snapshot, terminal_size)?;
            last_size = Some(terminal_size);
        }
        terminal
            .draw(|frame| renderer::render(frame, &snapshot))
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
        if is_prefix(key) {
            prefix_active = true;
            continue;
        }
        let pressed = action(prefix_active, key);
        match pressed {
            Action::Detach => break,
            Action::NewTab => {
                client.request("new-tab", "create_tab", json!({ "name": "Activity" }))?;
            }
            Action::StopFocusedPane => {
                if let Some(pane_id) = snapshot.focused_pane_id {
                    let _ = client.request("stop-pane", "stop_pane", json!({ "pane_id": pane_id }));
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
                        "cols": 80,
                        "rows": 24
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
        }
        prefix_active = false;
    }
    Ok(())
}

fn current_snapshot(client: &ControlClient) -> Result<SessionSnapshot, ClientError> {
    let response = client.request_with_retry(
        "snapshot",
        "get_snapshot",
        json!({}),
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
    let cols = terminal_size.0.max(1);
    let rows = terminal_size.1.saturating_sub(1).max(1);
    for pane in &snapshot.panes {
        let _ = client.request_with_retry(
            format!("resize-{}", pane.pane_id),
            "resize_pty",
            json!({ "pane_id": pane.pane_id, "cols": cols, "rows": rows }),
            2,
            Duration::from_millis(20),
        );
    }
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

#[cfg(test)]
mod tests {
    use super::key_code_bytes;
    use crossterm::event::KeyCode;

    #[test]
    fn common_keys_encode_for_a_pty() {
        assert_eq!(key_code_bytes(KeyCode::Enter), Some(vec![b'\r']));
        assert_eq!(key_code_bytes(KeyCode::Left), Some(b"\x1b[D".to_vec()));
    }
}
