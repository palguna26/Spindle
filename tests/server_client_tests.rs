use serde_json::Value;
use spindle::client::ControlClient;
use spindle::protocol::Response;
use spindle::server;
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn test_state_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("spindle-server-test-{stamp}"))
}

fn request(address: &str, operation: &str) -> Response<Value> {
    ControlClient::connect(address)
        .unwrap()
        .request(operation, operation, Value::Object(Default::default()))
        .unwrap()
}

fn start_server(state_dir: &Path) -> (std::thread::JoinHandle<()>, String) {
    let server_state = state_dir.to_path_buf();
    let thread = std::thread::spawn(move || server::run(&server_state).unwrap());
    let endpoint = state_dir.join("server.endpoint");
    for _ in 0..80 {
        if endpoint.exists() {
            let address = std::fs::read_to_string(&endpoint).unwrap();
            if ControlClient::connect(address.trim()).is_ok() {
                return (thread, address);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("server endpoint was not created");
}

#[test]
fn split_ratio_control_updates_the_persisted_layout() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let pane = serde_json::json!({
        "command": "cmd.exe",
        "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
        "cwd": cwd,
        "cols": 80,
        "rows": 24
    });
    assert!(
        client
            .request("ratio-first-pane", "ensure_active_pane", pane.clone())
            .unwrap()
            .ok
    );
    let mut split = pane;
    split["direction"] = serde_json::json!("horizontal");
    assert!(
        client
            .request("ratio-second-pane", "split_pane", split)
            .unwrap()
            .ok
    );
    let nested_split = serde_json::json!({
        "command": "cmd.exe",
        "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
        "cwd": std::env::current_dir().unwrap().to_string_lossy(),
        "cols": 80,
        "rows": 24,
        "direction": "vertical"
    });
    assert!(
        client
            .request("ratio-third-pane", "split_pane", nested_split)
            .unwrap()
            .ok
    );
    let updated = client
        .request(
            "set-nested-split-ratio",
            "set_split_ratio",
            serde_json::json!({ "path": [true], "ratio": 0.7 }),
        )
        .unwrap();
    assert!(updated.ok);
    let snapshot = client
        .request(
            "ratio-snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    let root = &snapshot["spaces"][0]["workspaces"][0]["tabs"][0]["layout"]["Split"];
    let root_ratio = root["ratio"]
        .as_f64()
        .expect("root ratio should be numeric");
    let nested_ratio = root["second"]["Split"]["ratio"]
        .as_f64()
        .expect("nested ratio should be numeric");
    assert!((root_ratio - 0.5).abs() < 0.00001);
    assert!((nested_ratio - 0.7).abs() < 0.00001);
    client
        .request(
            "ratio-stop-server",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn pane_view_settings_survive_server_restart() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let pane = client
        .request(
            "zoom-create-pane",
            "ensure_active_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap()
        .payload
        .unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    let zoomed = client
        .request(
            "zoom-pane",
            "toggle_pane_zoom",
            serde_json::json!({ "pane_id": pane }),
        )
        .unwrap();
    assert!(zoomed.ok);
    let passthrough = client
        .request(
            "toggle-pane-right-click",
            "toggle_right_click_passthrough",
            serde_json::json!({ "pane_id": pane }),
        )
        .unwrap();
    assert!(passthrough.ok);
    client
        .request(
            "zoom-stop-server",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();

    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let snapshot = client
        .request(
            "zoom-snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(
        snapshot["spaces"][0]["workspaces"][0]["tabs"][0]["zoomed"],
        true
    );
    assert_eq!(snapshot["panes"][0]["right_click_passthrough"], true);
    client
        .request(
            "zoom-stop-server-restarted",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn server_accepts_attach_snapshot_and_stop() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    assert!(state_dir.join("server.pid").exists());
    let identity: spindle::server::ServerIdentity =
        serde_json::from_str(&std::fs::read_to_string(state_dir.join("server.json")).unwrap())
            .unwrap();
    assert_eq!(identity.pid, std::process::id());
    assert_eq!(
        identity.protocol_version,
        spindle::protocol::PROTOCOL_VERSION
    );
    assert!(!identity.interactive_endpoint.is_empty());

    let attach = request(address.trim(), "attach");
    assert!(attach.ok);
    let snapshot = request(address.trim(), "get_snapshot");
    assert!(snapshot.ok);
    let snapshot_payload = snapshot.payload.unwrap();
    assert_eq!(snapshot_payload["spaces"].as_array().unwrap().len(), 1);
    let workspace = &snapshot_payload["spaces"][0]["workspaces"][0];
    let expected_repository = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        workspace["repository_path"].as_str(),
        Some(expected_repository.as_str())
    );

    let stop = request(address.trim(), "stop_server");
    assert!(stop.ok);
    thread.join().unwrap();
    let endpoint = state_dir.join("server.endpoint");
    assert!(!endpoint.exists());
    assert!(!state_dir.join("server.interactive.endpoint").exists());
    assert!(!state_dir.join("server.pid").exists());
    assert!(!state_dir.join("server.json").exists());
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn session_metadata_survives_server_restart() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let created = client
        .request(
            "workspace",
            "create_workspace",
            serde_json::json!({ "name": "Feature" }),
        )
        .unwrap();
    assert!(created.ok);
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();

    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let snapshot = client
        .request(
            "snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap();
    let workspaces = &snapshot.payload.unwrap()["spaces"][0]["workspaces"];
    assert_eq!(workspaces.as_array().unwrap().len(), 2);
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn ensuring_active_pane_is_idempotent_and_restores_tab_focus() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    client
        .attach_with_terminal(100, 30, vec!["alternate_screen".into()])
        .unwrap();
    let pane_request = serde_json::json!({
        "command": "cmd.exe",
        "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
        "cwd": std::env::current_dir().unwrap().to_string_lossy(),
        "cols": 80,
        "rows": 24
    });

    let mut failed_start_request = pane_request.clone();
    failed_start_request["command"] = serde_json::json!("spindle-command-that-does-not-exist.exe");
    let failed_start = client.request(
        "ensure-failed-shell",
        "ensure_active_pane",
        failed_start_request,
    );
    assert!(failed_start.is_err());
    let empty_snapshot = client
        .request(
            "snapshot-after-failed-shell",
            "get_snapshot",
            serde_json::Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert!(empty_snapshot["panes"].as_array().unwrap().is_empty());

    let first = client
        .request("ensure-first", "ensure_active_pane", pane_request.clone())
        .unwrap();
    assert!(first.ok);
    let first_payload = first.payload.unwrap();
    assert_eq!(first_payload["created"], true);
    let first_snapshot = client
        .request(
            "ensure-first-snapshot",
            "get_snapshot",
            serde_json::json!({}),
        )
        .unwrap()
        .payload
        .unwrap();
    let focused = first_snapshot["focused_pane_id"].as_str().unwrap();
    assert_eq!(focused, first_payload["pane_id"].as_str().unwrap());
    assert!(first_snapshot["panes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|pane| pane["pane_id"] == focused));

    let second = client
        .request("ensure-second", "ensure_active_pane", pane_request.clone())
        .unwrap();
    assert!(second.ok);
    let second_payload = second.payload.unwrap();
    assert_eq!(second_payload["created"], false);
    assert_eq!(first_payload["pane_id"], second_payload["pane_id"]);

    let new_tab = client
        .request(
            "ensure-new-tab",
            "create_tab",
            serde_json::json!({ "name": "Activity" }),
        )
        .unwrap()
        .payload
        .unwrap();
    let tab_pane = client
        .request(
            "ensure-tab-pane",
            "ensure_active_pane",
            pane_request.clone(),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(tab_pane["created"], true);
    let mut split_request = pane_request.clone();
    split_request["direction"] = serde_json::json!("vertical");
    let other_tab_pane = client
        .request("ensure-split-activity", "split_pane", split_request)
        .unwrap()
        .payload
        .unwrap();
    client
        .request(
            "ensure-focus-second-activity-pane",
            "focus_pane",
            serde_json::json!({ "pane_id": other_tab_pane["pane_id"] }),
        )
        .unwrap();

    client
        .request(
            "ensure-switch-to-main-tab",
            "switch_tab",
            serde_json::json!({ "id": "tab-1" }),
        )
        .unwrap();
    let main_tab_pane = client
        .request(
            "ensure-main-tab-pane",
            "ensure_active_pane",
            pane_request.clone(),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(main_tab_pane["created"], false);
    assert_eq!(main_tab_pane["pane_id"], first_payload["pane_id"]);
    let focused_after_tab_switch = client
        .request(
            "ensure-main-tab-snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(
        focused_after_tab_switch["focused_pane_id"],
        first_payload["pane_id"]
    );
    client
        .request(
            "ensure-switch-back-to-activity",
            "switch_tab",
            serde_json::json!({ "id": new_tab["tab_id"] }),
        )
        .unwrap();
    let activity_focus_before_ensure = client
        .request(
            "ensure-activity-focus-snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(
        activity_focus_before_ensure["focused_pane_id"],
        other_tab_pane["pane_id"]
    );
    let restored_activity_focus = client
        .request(
            "ensure-refocus-activity",
            "ensure_active_pane",
            pane_request.clone(),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(restored_activity_focus["created"], false);
    assert_eq!(
        restored_activity_focus["pane_id"],
        other_tab_pane["pane_id"]
    );
    assert_ne!(restored_activity_focus["pane_id"], tab_pane["pane_id"]);

    client
        .request(
            "ensure-new-workspace",
            "create_workspace",
            serde_json::json!({
                "name": "Feature",
                "repository_path": std::env::current_dir().unwrap().to_string_lossy()
            }),
        )
        .unwrap();
    let workspace_pane = client
        .request(
            "ensure-workspace-pane",
            "ensure_active_pane",
            pane_request.clone(),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(workspace_pane["created"], true);

    client
        .request(
            "ensure-new-space",
            "create_space",
            serde_json::json!({ "name": "Review" }),
        )
        .unwrap();
    let space_pane = client
        .request(
            "ensure-space-pane",
            "ensure_active_pane",
            pane_request.clone(),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(space_pane["created"], true);

    let repeated_space_pane = client
        .request(
            "ensure-space-pane-again",
            "ensure_active_pane",
            pane_request,
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(repeated_space_pane["created"], false);
    assert_eq!(repeated_space_pane["pane_id"], space_pane["pane_id"]);

    let snapshot = client
        .request(
            "ensure-snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(snapshot["panes"].as_array().unwrap().len(), 5);
    assert_eq!(snapshot["focused_pane_id"], space_pane["pane_id"]);

    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn concurrent_clients_ensure_only_one_initial_shell() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let pane_request = serde_json::json!({
        "command": "cmd.exe",
        "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
        "cwd": std::env::current_dir().unwrap().to_string_lossy(),
        "cols": 80,
        "rows": 24
    });
    let workers = (0..2)
        .map(|index| {
            let client = ControlClient::connect(address.trim()).unwrap();
            client
                .attach_with_terminal(100, 30, vec!["alternate_screen".into()])
                .unwrap();
            let barrier = barrier.clone();
            let pane_request = pane_request.clone();
            std::thread::spawn(move || {
                barrier.wait();
                client
                    .request(
                        format!("concurrent-ensure-{index}"),
                        "ensure_active_pane",
                        pane_request,
                    )
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let responses = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert!(responses.iter().all(|response| response.ok));
    let payloads = responses
        .iter()
        .map(|response| response.payload.as_ref().unwrap())
        .collect::<Vec<_>>();
    assert_ne!(payloads[0]["created"], payloads[1]["created"]);
    assert_eq!(payloads[0]["pane_id"], payloads[1]["pane_id"]);

    let client = ControlClient::connect(address.trim()).unwrap();
    let snapshot = client
        .request(
            "concurrent-ensure-snapshot",
            "get_snapshot",
            serde_json::json!({}),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(snapshot["panes"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["focused_pane_id"], payloads[0]["pane_id"]);

    client
        .request("stop", "stop_server", serde_json::json!({}))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn terminal_screen_and_scrollback_survive_server_restart() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let pane = client
        .request(
            "history-pane",
            "create_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "echo", "persisted-terminal-output"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap()
        .payload
        .unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut before_restart = None;
    for _ in 0..30 {
        let snapshot = client
            .request(
                "history-snapshot",
                "get_snapshot",
                Value::Object(Default::default()),
            )
            .unwrap()
            .payload
            .unwrap();
        let current = snapshot["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|current| current["pane_id"] == pane)
            .unwrap();
        let scrollback: Vec<u8> = current["scrollback"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|byte| byte.as_u64().and_then(|byte| u8::try_from(byte).ok()))
            .collect();
        if scrollback.windows(4).any(|window| window == b"\x1b[6n") {
            client
                .request(
                    "history-cursor-response",
                    "send_input",
                    serde_json::json!({ "pane_id": pane, "bytes": b"\x1b[1;1R" }),
                )
                .unwrap();
        }
        if current["screen"]
            .as_str()
            .unwrap_or_default()
            .contains("persisted-terminal-output")
        {
            before_restart = Some(current.clone());
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let before_restart = before_restart.expect("terminal output did not reach the snapshot");
    assert!(before_restart["scrollback_bytes"].as_u64().unwrap() > 0);
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();

    let (thread, address) = start_server(&state_dir);
    let recovered = ControlClient::connect(address.trim()).unwrap();
    let snapshot = recovered
        .request(
            "recovered-history",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    let pane = snapshot["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|current| current["pane_id"] == pane)
        .unwrap();
    assert!(pane["screen"]
        .as_str()
        .unwrap_or_default()
        .contains("persisted-terminal-output"));
    assert!(pane["scrollback_bytes"].as_u64().unwrap() > 0);
    recovered
        .request(
            "stop-again",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn pty_exit_statuses_are_persisted_and_reported() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let create = |request_id: &str, code: u8| {
        client
            .request(
                request_id,
                "create_pane",
                serde_json::json!({
                    "command": "cmd.exe",
                    "args": ["/C", "exit", code.to_string()],
                    "cwd": cwd.clone(),
                    "cols": 80,
                    "rows": 24
                }),
            )
            .unwrap()
            .payload
            .unwrap()["pane_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let completed_id = create("completed-pane", 0);
    let halted_id = create("halted-pane", 7);

    let mut statuses = None;
    for _ in 0..40 {
        let snapshot = client
            .request(
                "status-snapshot",
                "get_snapshot",
                Value::Object(Default::default()),
            )
            .unwrap()
            .payload
            .unwrap();
        let panes = snapshot["panes"].as_array().unwrap();
        for current in panes {
            let scrollback: Vec<u8> = current["scrollback"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|byte| byte.as_u64().and_then(|byte| u8::try_from(byte).ok()))
                .collect();
            if scrollback.windows(4).any(|window| window == b"\x1b[6n") {
                client
                    .request(
                        "status-cursor-response",
                        "send_input",
                        serde_json::json!({
                            "pane_id": current["pane_id"],
                            "bytes": b"\x1b[1;1R"
                        }),
                    )
                    .unwrap();
            }
        }
        let completed = panes
            .iter()
            .find(|pane| pane["pane_id"] == completed_id)
            .unwrap();
        let halted = panes
            .iter()
            .find(|pane| pane["pane_id"] == halted_id)
            .unwrap();
        if completed["status"]["Completed"]["exit_code"] == 0
            && halted["status"]["Halted"]["reason"].is_string()
        {
            statuses = Some((completed.clone(), halted.clone()));
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let (completed, halted) = statuses.expect("PTY exit statuses were not reported");
    assert_eq!(completed["status"]["Completed"]["exit_code"], 0);
    assert!(halted["status"]["Halted"]["reason"]
        .as_str()
        .unwrap()
        .contains("7"));

    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn tab_metadata_and_active_tab_survive_server_restart() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let created = client
        .request(
            "create-tab",
            "create_tab",
            serde_json::json!({ "name": "Logs" }),
        )
        .unwrap();
    let tab_id = created.payload.unwrap()["tab_id"]
        .as_str()
        .unwrap()
        .to_string();
    client
        .request(
            "rename-tab",
            "rename_tab",
            serde_json::json!({ "id": tab_id, "name": "Build logs" }),
        )
        .unwrap();
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();

    let (thread, address) = start_server(&state_dir);
    let recovered = ControlClient::connect(address.trim()).unwrap();
    let snapshot = recovered
        .request(
            "snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    let workspace = &snapshot["spaces"][0]["workspaces"][0];
    assert_eq!(workspace["active_tab_id"], tab_id);
    assert_eq!(workspace["tabs"][1]["name"], "Build logs");
    recovered
        .request(
            "stop-again",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn space_and_workspace_deletion_use_control_api_safely() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();

    let workspace = client
        .request(
            "create-workspace",
            "create_workspace",
            serde_json::json!({ "name": "Temporary" }),
        )
        .unwrap();
    let workspace_id = workspace.payload.unwrap()["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        client
            .request(
                "delete-workspace",
                "delete_workspace",
                serde_json::json!({ "id": workspace_id }),
            )
            .unwrap()
            .ok
    );

    let space = client
        .request(
            "create-space",
            "create_space",
            serde_json::json!({ "name": "Temporary space" }),
        )
        .unwrap();
    let space_id = space.payload.unwrap()["space_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        client
            .request(
                "delete-space",
                "delete_space",
                serde_json::json!({ "id": space_id }),
            )
            .unwrap()
            .ok
    );

    let last_space = client.request(
        "delete-last-space",
        "delete_space",
        serde_json::json!({ "id": "space-1" }),
    );
    assert!(last_space.is_err());

    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn split_panes_survive_workspace_switching() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let pane_request = serde_json::json!({
        "command": "cmd.exe",
        "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
        "cwd": cwd,
        "cols": 80,
        "rows": 24
    });
    let first = client
        .request("first-pane", "create_pane", pane_request.clone())
        .unwrap();
    let first_id = first.payload.unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    let second = client
        .request(
            "second-pane",
            "split_pane",
            serde_json::json!({
                "direction": "horizontal",
                "command": "cmd.exe",
                "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap();
    let second_id = second.payload.unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let workspace = client
        .request(
            "new-workspace",
            "create_workspace",
            serde_json::json!({ "name": "Feature", "repository_path": cwd, "branch": "main" }),
        )
        .unwrap();
    let workspace_id = workspace.payload.unwrap()["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let switched = client
        .request(
            "back-workspace",
            "switch_workspace",
            serde_json::json!({ "id": "workspace-1" }),
        )
        .unwrap();
    assert!(switched.ok);
    let snapshot = client
        .request("workspace-snapshot", "get_snapshot", serde_json::json!({}))
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(snapshot["panes"].as_array().unwrap().len(), 2);
    assert!(snapshot["spaces"][0]["workspaces"][0]["tabs"][0]["layout"].is_object());
    assert_eq!(
        snapshot["spaces"][0]["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(snapshot["spaces"][0]["active_workspace_id"], "workspace-1");
    assert_eq!(snapshot["focused_pane_id"], second_id);
    assert!(!workspace_id.is_empty());

    for pane_id in [first_id, second_id] {
        client
            .request(
                format!("stop-{pane_id}"),
                "stop_pane",
                serde_json::json!({ "pane_id": pane_id }),
            )
            .unwrap();
    }
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn detach_preserves_multiple_live_panes() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let pane_request = serde_json::json!({
        "command": "cmd.exe",
        "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
        "cwd": cwd,
        "cols": 80,
        "rows": 24
    });
    let first = client
        .request("detach-first", "create_pane", pane_request.clone())
        .unwrap()
        .payload
        .unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    let second = client
        .request(
            "detach-second",
            "split_pane",
            serde_json::json!({
                "direction": "vertical",
                "command": "cmd.exe",
                "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap()
        .payload
        .unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    client.attach().unwrap();
    client.detach().unwrap();

    let reconnected = ControlClient::connect(address.trim()).unwrap();
    let snapshot = reconnected
        .request(
            "detach-snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    for pane_id in [&first, &second] {
        let pane = snapshot["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"] == *pane_id)
            .unwrap();
        assert_eq!(pane["status"], "Running");
    }

    for pane_id in [&first, &second] {
        reconnected
            .request(
                format!("detach-stop-{pane_id}"),
                "stop_pane",
                serde_json::json!({ "pane_id": pane_id }),
            )
            .unwrap();
    }
    reconnected
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn resize_and_close_layout_changes_survive_restart() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let create = |client: &ControlClient, request_id: &str| {
        client
            .request(
                request_id,
                "create_pane",
                serde_json::json!({
                    "command": "cmd.exe",
                    "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                    "cwd": cwd.clone(),
                    "cols": 80,
                    "rows": 24
                }),
            )
            .unwrap()
            .payload
            .unwrap()["pane_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let first_id = create(&client, "layout-first");
    let second_id = client
        .request(
            "layout-second",
            "split_pane",
            serde_json::json!({
                "direction": "vertical",
                "command": "cmd.exe",
                "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                "cwd": cwd.clone(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap()
        .payload
        .unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    client
        .request(
            "layout-resize",
            "resize_pane",
            serde_json::json!({ "pane_id": second_id, "delta": 0.2 }),
        )
        .unwrap();
    let resized = client
        .request("layout-snapshot", "get_snapshot", serde_json::json!({}))
        .unwrap()
        .payload
        .unwrap();
    let ratio = resized["spaces"][0]["workspaces"][0]["tabs"][0]["layout"]["Split"]["ratio"]
        .as_f64()
        .unwrap();
    assert!(
        (ratio - 0.3).abs() < 0.0001,
        "unexpected split ratio: {ratio}"
    );

    for pane_id in [&first_id, &second_id] {
        client
            .request(
                format!("stop-{pane_id}"),
                "stop_pane",
                serde_json::json!({ "pane_id": pane_id }),
            )
            .unwrap();
    }
    client
        .request(
            "layout-close",
            "close_pane",
            serde_json::json!({ "pane_id": second_id }),
        )
        .unwrap();
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();

    let (thread, address) = start_server(&state_dir);
    let recovered = ControlClient::connect(address.trim()).unwrap();
    let snapshot = recovered
        .request("recovered-layout", "get_snapshot", serde_json::json!({}))
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(snapshot["panes"].as_array().unwrap().len(), 1);
    assert_eq!(
        snapshot["spaces"][0]["workspaces"][0]["tabs"][0]["layout"]["Pane"]["pane_id"],
        first_id
    );
    recovered
        .request(
            "stop-again",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn client_loss_allows_geometry_ownership_takeover() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let first = ControlClient::connect(address.trim()).unwrap();
    let second = ControlClient::connect(address.trim()).unwrap();
    let pane = first
        .request(
            "surviving-pane",
            "create_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap()
        .payload
        .unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_attach = first.attach().unwrap();
    assert_eq!(first_attach.payload.unwrap()["active"], true);
    let second_attach = second.attach().unwrap();
    assert_eq!(second_attach.payload.unwrap()["active"], false);
    drop(first);
    std::thread::sleep(Duration::from_millis(1_100));
    let takeover = second.attach().unwrap();
    assert_eq!(takeover.payload.unwrap()["active"], true);
    let snapshot = second
        .request(
            "surviving-snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(snapshot["panes"][0]["pane_id"], pane);
    let status = &snapshot["panes"][0]["status"];
    assert_eq!(status, "Running");

    second
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn server_restart_marks_panes_interrupted_and_ensure_restarts_in_place() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let pane = client
        .request(
            "live-pane",
            "create_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap();
    let pane_id = pane.payload.unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();

    let (thread, address) = start_server(&state_dir);
    let recovered = ControlClient::connect(address.trim()).unwrap();
    let snapshot = recovered
        .request(
            "snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    let recovered_pane = snapshot["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pane| pane["pane_id"] == pane_id)
        .unwrap();
    assert!(recovered_pane["status"]["Interrupted"].is_object());
    let ensured = recovered
        .request(
            "ensure-after-restart",
            "ensure_active_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "ping", "127.0.0.1", "-n", "30"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap();
    let ensured_pane = ensured.payload.unwrap();
    assert_eq!(ensured_pane["pane_id"], pane_id);
    assert_eq!(ensured_pane["restarted"], true);
    let snapshot = recovered
        .request(
            "snapshot-after-ensure",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(snapshot["panes"].as_array().unwrap().len(), 1);
    let recovered_pane = snapshot["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pane| pane["pane_id"] == pane_id)
        .unwrap();
    assert_eq!(recovered_pane["status"], "Running");
    recovered
        .request(
            "stop-again",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn client_can_subscribe_from_a_sequence() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();

    let batch = client.subscribe_events(0).unwrap();
    assert!(batch.events.is_empty());
    assert_eq!(batch.latest_sequence, 0);

    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn client_can_open_a_long_lived_event_stream() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let interactive =
        std::fs::read_to_string(state_dir.join("server.interactive.endpoint")).unwrap();
    assert!(!interactive.trim().is_empty());
    assert_ne!(interactive.trim(), address.trim());
    let mut stream = client.open_event_stream(0).unwrap();
    let batch = stream.next_batch().unwrap();
    assert!(batch.events.is_empty());
    assert_eq!(batch.latest_sequence, 0);
    assert!(
        client
            .request(
                "ping-while-streaming",
                "ping",
                Value::Object(Default::default())
            )
            .unwrap()
            .ok
    );
    client
        .request(
            "stream-pane",
            "create_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "echo", "stream-output"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap();
    let mut saw_output = false;
    for _ in 0..30 {
        let batch = stream.next_batch().unwrap();
        if batch.events.iter().any(|event| {
            event.event == "pane_output"
                && event.payload["bytes"]
                    .as_array()
                    .is_some_and(|bytes| !bytes.is_empty())
        }) {
            saw_output = true;
            break;
        }
    }
    assert!(saw_output, "interactive stream did not receive PTY output");
    drop(stream);

    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn pty_output_reaches_event_subscribers() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let created = client
        .request(
            "create-pane",
            "create_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "echo %SPINDLE_EVENT%"],
                "cwd": cwd,
                "env": { "SPINDLE_EVENT": "from-env" },
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap();
    let pane_id = created.payload.unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut sequence = 0;
    let mut saw_output = false;
    for _ in 0..80 {
        let batch = client.subscribe_events(sequence).unwrap();
        sequence = batch.latest_sequence;
        for event in &batch.events {
            if event.event != "pane_output" || event.payload["pane_id"] != pane_id {
                continue;
            }
            let bytes: Vec<u8> = event.payload["bytes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|byte| byte.as_u64().and_then(|byte| u8::try_from(byte).ok()))
                .collect();
            if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                client
                    .request(
                        "cursor-response",
                        "send_input",
                        serde_json::json!({
                            "pane_id": pane_id,
                            "bytes": b"\x1b[1;1R",
                        }),
                    )
                    .unwrap();
            }
            saw_output |= String::from_utf8_lossy(&bytes).contains("from-env");
        }
        if saw_output {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(saw_output, "pane output was not published");

    let observer = ControlClient::connect(address.trim()).unwrap();
    let observer_batch = observer.subscribe_events(0).unwrap();
    assert!(observer_batch
        .events
        .iter()
        .any(|event| { event.event == "pane_output" && event.payload["pane_id"] == pane_id }));

    let snapshot = client
        .request(
            "snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap();
    assert!(snapshot.payload.unwrap()["panes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|pane| pane["pane_id"] == pane_id));

    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn powershell_prompt_starts_without_a_host_injected_cursor_reply() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let pane_id = client
        .request(
            "powershell-pane",
            "create_pane",
            serde_json::json!({
                "command": "powershell.exe",
                "args": ["-NoLogo", "-NoProfile"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap()
        .payload
        .unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut prompt_visible = false;
    let mut last_screen = String::new();
    for _ in 0..200 {
        let snapshot = client
            .request(
                "powershell-snapshot",
                "get_snapshot",
                Value::Object(Default::default()),
            )
            .unwrap()
            .payload
            .unwrap();
        let pane = snapshot["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"] == pane_id)
            .unwrap();
        last_screen = pane["screen"].as_str().unwrap_or_default().to_owned();
        if last_screen.contains("PS ") {
            prompt_visible = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        prompt_visible,
        "PowerShell prompt did not appear without a host-injected cursor reply; screen={last_screen:?}"
    );

    client
        .request(
            "stop-powershell",
            "stop_pane",
            serde_json::json!({ "pane_id": pane_id }),
        )
        .unwrap();
    client
        .request(
            "stop-server",
            "stop_server",
            Value::Object(Default::default()),
        )
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn interactive_pty_accepts_input_and_resize() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let pane = client
        .request(
            "interactive-pane",
            "create_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/Q", "/K"],
                "cwd": std::env::current_dir().unwrap().to_string_lossy(),
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap();
    let pane_id = pane.payload.unwrap()["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    client
        .interactive_request(
            "interactive-resize",
            "resize_pty",
            serde_json::json!({ "pane_id": pane_id, "cols": 100, "rows": 30 }),
        )
        .unwrap();
    client
        .interactive_request(
            "interactive-input",
            "send_input",
            serde_json::json!({ "pane_id": pane_id, "bytes": b"echo spindle-input\r" }),
        )
        .unwrap();

    let mut sequence = 0;
    let mut saw_input = false;
    let mut output = Vec::new();
    for _ in 0..80 {
        let batch = client.subscribe_events(sequence).unwrap();
        sequence = batch.latest_sequence;
        for event in batch.events {
            if event.event == "pane_output" && event.payload["pane_id"] == pane_id {
                let bytes: Vec<u8> = event.payload["bytes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .filter_map(|byte| u8::try_from(byte).ok())
                    .collect();
                output.extend(&bytes);
                if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                    client
                        .interactive_request(
                            "interactive-cursor",
                            "send_input",
                            serde_json::json!({
                                "pane_id": pane_id,
                                "bytes": b"\x1b[1;1R"
                            }),
                        )
                        .unwrap();
                }
            }
        }
        saw_input = String::from_utf8_lossy(&output).contains("spindle-input");
        if saw_input {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        saw_input,
        "interactive input did not reach the PTY; output={:?}",
        String::from_utf8_lossy(&output)
    );
    client
        .request(
            "interactive-stop",
            "stop_pane",
            serde_json::json!({ "pane_id": pane_id }),
        )
        .unwrap();
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn live_event_stream_receives_new_pty_output() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let mut stream = client.open_event_stream(0).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    client
        .request(
            "create-stream-pane",
            "create_pane",
            serde_json::json!({
                "command": "cmd.exe",
                "args": ["/C", "echo live-stream"],
                "cwd": cwd,
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap();

    let mut saw_output = false;
    for _ in 0..20 {
        let batch = stream.next_batch().unwrap();
        saw_output |= batch.events.iter().any(|event| {
            event.event == "pane_output"
                && event.payload["bytes"]
                    .as_array()
                    .is_some_and(|bytes| !bytes.is_empty())
        });
        if saw_output {
            break;
        }
    }
    assert!(saw_output, "live stream did not receive pane output");
    drop(stream);
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}

#[test]
fn split_layout_survives_server_restart() {
    let state_dir = test_state_dir();
    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let pane = serde_json::json!({
        "command": "cmd.exe",
        "args": ["/C", "echo first"],
        "cwd": cwd,
        "cols": 80,
        "rows": 24
    });
    let first = client
        .request("first-pane", "create_pane", pane.clone())
        .unwrap()
        .payload
        .unwrap();
    let second = client
        .request(
            "second-pane",
            "split_pane",
            serde_json::json!({
                "direction": "vertical",
                "command": "cmd.exe",
                "args": ["/C", "echo second"],
                "cwd": pane["cwd"],
                "cols": 80,
                "rows": 24
            }),
        )
        .unwrap()
        .payload
        .unwrap();
    let before = client
        .request(
            "snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(before["panes"].as_array().unwrap().len(), 2);
    assert!(before["spaces"][0]["workspaces"][0]["tabs"][0]["layout"].is_object());
    assert_eq!(before["focused_pane_id"], second["pane_id"]);
    assert_eq!(
        before["spaces"][0]["workspaces"][0]["tabs"][0]["focused_pane_id"],
        second["pane_id"]
    );
    assert_ne!(first["pane_id"], second["pane_id"]);
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();

    let (thread, address) = start_server(&state_dir);
    let client = ControlClient::connect(address.trim()).unwrap();
    let after = client
        .request(
            "snapshot",
            "get_snapshot",
            Value::Object(Default::default()),
        )
        .unwrap()
        .payload
        .unwrap();
    assert_eq!(after["panes"].as_array().unwrap().len(), 2);
    assert!(after["spaces"][0]["workspaces"][0]["tabs"][0]["layout"].is_object());
    assert_eq!(after["focused_pane_id"], second["pane_id"]);
    assert_eq!(
        after["spaces"][0]["workspaces"][0]["tabs"][0]["focused_pane_id"],
        second["pane_id"]
    );
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}
