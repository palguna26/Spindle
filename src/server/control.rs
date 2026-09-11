use crate::protocol::frame::{read_frame, write_frame, FrameError};
use crate::protocol::{negotiate, ProtocolError, Request, Response, PROTOCOL_VERSION};
use crate::server::session::{CreatePaneRequest, Session};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufReader};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

#[derive(Debug, Deserialize)]
struct PaneRequest {
    pane_id: String,
}

#[derive(Debug, Deserialize)]
struct InputRequest {
    pane_id: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct ResizeRequest {
    pane_id: String,
    cols: u16,
    rows: u16,
}

pub fn handle_connection(stream: TcpStream, session: Arc<Mutex<Session>>) -> io::Result<bool> {
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;
    let response = match read_frame(&mut reader) {
        Ok(frame) => response_for(&frame, &session),
        Err(error) => error_response("invalid_frame", frame_error_message(error)),
    };
    let encoded = serde_json::to_vec(&response).map_err(io::Error::other)?;
    write_frame(&mut writer, &encoded).map_err(frame_io_error)?;
    Ok(response_requests_stop(&response))
}

fn response_for(frame: &[u8], session: &Arc<Mutex<Session>>) -> Response<Value> {
    let request: Request<Value> = match serde_json::from_slice(frame) {
        Ok(request) => request,
        Err(error) => return error_response("invalid_json", error.to_string()),
    };
    if let Err(error) = negotiate(request.version) {
        return Response {
            version: PROTOCOL_VERSION,
            request_id: request.request_id,
            ok: false,
            payload: None,
            error: Some(error),
        };
    }

    let result = match request.op.as_str() {
        "ping" => Ok(json!({ "status": "ok" })),
        "attach" => Ok(json!({ "attached": true })),
        "get_snapshot" => {
            let mut session = session.lock().expect("session lock poisoned");
            session.poll();
            session.refresh_snapshot();
            serde_json::to_value(session.snapshot()).map_err(|error| error.to_string())
        }
        "create_pane" => {
            let payload: CreatePaneRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            session
                .lock()
                .expect("session lock poisoned")
                .create_pane(payload)
        }
        "send_input" => {
            let payload: InputRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            session
                .lock()
                .expect("session lock poisoned")
                .send_input(&payload.pane_id, &payload.bytes)
                .map(|_| json!({ "sent": payload.bytes.len() }))
        }
        "resize_pty" => {
            let payload: ResizeRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            session
                .lock()
                .expect("session lock poisoned")
                .resize(&payload.pane_id, payload.cols, payload.rows)
                .map(|_| json!({ "resized": true }))
        }
        "stop_pane" => {
            let payload: PaneRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            session
                .lock()
                .expect("session lock poisoned")
                .stop_pane(&payload.pane_id)
                .map(|_| json!({ "stopped": true }))
        }
        "stop_server" => Ok(json!({ "stopping": true })),
        _ => Err(format!("unknown operation '{}'", request.op)),
    };

    match result {
        Ok(payload) => Response {
            version: PROTOCOL_VERSION,
            request_id: request.request_id,
            ok: true,
            payload: Some(payload),
            error: None,
        },
        Err(message) => request_error(request.request_id, "operation_failed", message),
    }
}

fn request_error(request_id: String, code: &str, message: String) -> Response<Value> {
    Response {
        version: PROTOCOL_VERSION,
        request_id,
        ok: false,
        payload: None,
        error: Some(ProtocolError {
            code: code.into(),
            message,
        }),
    }
}

fn error_response(code: &str, message: String) -> Response<Value> {
    request_error(String::new(), code, message)
}

fn response_requests_stop(response: &Response<Value>) -> bool {
    response.ok
        && response
            .payload
            .as_ref()
            .and_then(Value::as_object)
            .and_then(|payload| payload.get("stopping"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn frame_error_message(error: FrameError) -> String {
    match error {
        FrameError::Io(error) => error.to_string(),
        FrameError::TooLarge => "frame exceeds the 1 MiB limit".into(),
        FrameError::Empty => "frame is empty".into(),
    }
}

fn frame_io_error(error: FrameError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, frame_error_message(error))
}

#[cfg(test)]
mod tests {
    use super::response_for;
    use crate::protocol::PROTOCOL_VERSION;
    use crate::server::session::Session;
    use std::sync::{Arc, Mutex};

    fn session() -> Arc<Mutex<Session>> {
        Arc::new(Mutex::new(Session::default()))
    }

    #[test]
    fn ping_returns_a_success_response() {
        let response = response_for(
            br#"{"version":1,"request_id":"1","op":"ping","payload":{}}"#,
            &session(),
        );
        assert!(response.ok);
        assert_eq!(response.request_id, "1");
    }

    #[test]
    fn create_pane_rejects_missing_executable() {
        let response = response_for(
            br#"{"version":1,"request_id":"2","op":"create_pane","payload":{"command":"spindle-command-that-does-not-exist.exe","cwd":"C:/","cols":80,"rows":24}}"#,
            &session(),
        );
        assert!(!response.ok);
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let response = response_for(
            br#"{"version":99,"request_id":"3","op":"ping","payload":{}}"#,
            &session(),
        );
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "unsupported_version");
    }

    #[test]
    fn malformed_json_is_rejected() {
        let response = response_for(br#"not-json"#, &session());
        assert!(!response.ok);
        assert_eq!(response.version, PROTOCOL_VERSION);
        assert_eq!(response.error.unwrap().code, "invalid_json");
    }
}
