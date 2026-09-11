use serde_json::Value;
use spindle::client::ControlClient;
use spindle::protocol::frame::{read_frame, write_frame};
use spindle::protocol::{Request, Response, PROTOCOL_VERSION};
use spindle::server;
use std::io::BufReader;
use std::net::TcpStream;
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
    let mut stream = TcpStream::connect(address).unwrap();
    let message = Request {
        version: PROTOCOL_VERSION,
        request_id: operation.into(),
        op: operation.into(),
        payload: Value::Object(Default::default()),
    };
    write_frame(&mut stream, &serde_json::to_vec(&message).unwrap()).unwrap();
    let mut reader = BufReader::new(stream);
    let frame = read_frame(&mut reader).unwrap();
    serde_json::from_slice(&frame).unwrap()
}

fn start_server(state_dir: &PathBuf) -> (std::thread::JoinHandle<()>, String) {
    let server_state = state_dir.clone();
    let thread = std::thread::spawn(move || server::run(&server_state).unwrap());
    let endpoint = state_dir.join("server.endpoint");
    for _ in 0..80 {
        if endpoint.exists() {
            return (thread, std::fs::read_to_string(endpoint).unwrap());
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
