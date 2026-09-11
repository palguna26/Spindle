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
    assert_eq!(
        snapshot.payload.unwrap()["spaces"]
            .as_array()
            .unwrap()
            .len(),
        1
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
    client
        .request("first-pane", "create_pane", pane.clone())
        .unwrap();
    client
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
    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}
