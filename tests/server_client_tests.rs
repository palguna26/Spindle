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
    let mut stream = client.open_event_stream(0).unwrap();
    let batch = stream.next_batch().unwrap();
    assert!(batch.events.is_empty());
    assert_eq!(batch.latest_sequence, 0);
    drop(stream);

    client
        .request("stop", "stop_server", Value::Object(Default::default()))
        .unwrap();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(state_dir);
}
