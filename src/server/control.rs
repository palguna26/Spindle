use crate::protocol::frame::{read_frame, write_frame, FrameError};
use crate::protocol::{negotiate, ProtocolError, Request, Response, PROTOCOL_VERSION};
use crate::server::session::{CreatePaneRequest, Session};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufReader, Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Debug, Deserialize)]
struct PaneRequest {
    pane_id: String,
}

#[derive(Debug, Deserialize)]
struct IdRequest {
    id: String,
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
    #[serde(default)]
    client_id: String,
}

#[derive(Debug, Deserialize)]
struct AttachRequest {
    #[serde(default = "default_client_id")]
    client_id: String,
    #[serde(default)]
    cols: u16,
    #[serde(default)]
    rows: u16,
    #[serde(default)]
    capabilities: Vec<String>,
}

fn default_client_id() -> String {
    "anonymous".into()
}

#[derive(Debug, Deserialize)]
struct NameRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct IdNameRequest {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct SplitRequest {
    direction: String,
    #[serde(flatten)]
    pane: CreatePaneRequest,
}

#[derive(Debug, Deserialize)]
struct LayoutResizeRequest {
    pane_id: String,
    delta: f32,
}

#[derive(Debug, Deserialize)]
struct EventsRequest {
    #[serde(default)]
    after_sequence: u64,
}

#[derive(Debug, Deserialize)]
struct ClientRequest {
    #[serde(default)]
    client_id: String,
}

pub fn handle_connection<S>(stream: S, session: Arc<Mutex<Session>>) -> io::Result<bool>
where
    S: Read + Write,
{
    let mut reader = BufReader::new(stream);
    let frame = match read_frame(&mut reader) {
        Ok(frame) => frame,
        Err(error) => {
            let response = error_response("invalid_frame", frame_error_message(error));
            let mut writer = reader.into_inner();
            let encoded = serde_json::to_vec(&response).map_err(io::Error::other)?;
            write_frame(&mut writer, &encoded).map_err(frame_io_error)?;
            return Ok(false);
        }
    };
    let streaming = serde_json::from_slice::<Request<Value>>(&frame)
        .map(|request| request.op == "stream_events")
        .unwrap_or(false);
    if streaming {
        return handle_event_stream(reader.into_inner(), &frame, session);
    }
    let response = response_for(&frame, &session);
    let mut writer = reader.into_inner();
    let encoded = serde_json::to_vec(&response).map_err(io::Error::other)?;
    write_frame(&mut writer, &encoded).map_err(frame_io_error)?;
    Ok(response_requests_stop(&response))
}

fn handle_event_stream<S>(
    mut writer: S,
    frame: &[u8],
    session: Arc<Mutex<Session>>,
) -> io::Result<bool>
where
    S: Read + Write,
{
    let response = response_for(frame, &session);
    let encoded = serde_json::to_vec(&response).map_err(io::Error::other)?;
    write_frame(&mut writer, &encoded).map_err(frame_io_error)?;
    if !response.ok {
        return Ok(false);
    }
    let request: Request<Value> = serde_json::from_slice(frame).map_err(io::Error::other)?;
    let payload: EventsRequest =
        serde_json::from_value(request.payload).map_err(io::Error::other)?;
    let mut sequence = payload.after_sequence;
    loop {
        let batch = {
            let mut session = session.lock().expect("session lock poisoned");
            let events = session.events_since(sequence);
            let resync_required = session.event_gap(sequence);
            sequence = session.snapshot().event_sequence;
            json!({
                "events": events,
                "latest_sequence": sequence,
                "resync_required": resync_required,
            })
        };
        let encoded = serde_json::to_vec(&batch).map_err(io::Error::other)?;
        write_frame(&mut writer, &encoded).map_err(frame_io_error)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
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
        "stream_events" => Ok(json!({ "streaming": true })),
        "attach" => {
            let payload: AttachRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            let mut response = session.attach(payload.client_id);
            if let Some(object) = response.as_object_mut() {
                object.insert("cols".into(), json!(payload.cols));
                object.insert("rows".into(), json!(payload.rows));
                object.insert("capabilities".into(), json!(payload.capabilities));
            }
            Ok(response)
        }
        "detach" => {
            let payload: AttachRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            Ok(session.detach(&payload.client_id))
        }
        "create_space" => {
            let payload: NameRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.create_space(payload.name))
        }
        "switch_space" => {
            let payload: IdRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.switch_space(&payload.id))
        }
        "rename_space" => {
            let payload: IdNameRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.rename_space(&payload.id, payload.name)
            })
        }
        "delete_space" => {
            let payload: IdRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.delete_space(&payload.id))
        }
        "get_snapshot" => {
            let mut session = session.lock().expect("session lock poisoned");
            if let Ok(payload) = serde_json::from_value::<ClientRequest>(request.payload) {
                if !payload.client_id.is_empty() {
                    session.touch_client(&payload.client_id);
                }
            }
            session.poll();
            session.refresh_snapshot();
            if let Err(error) = session.save() {
                return request_error(
                    request.request_id,
                    "persistence_failed",
                    format!("{error:?}"),
                );
            }
            serde_json::to_value(session.snapshot()).map_err(|error| error.to_string())
        }
        "subscribe_events" => {
            let payload: EventsRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            let events = session.events_since(payload.after_sequence);
            let resync_required = session.event_gap(payload.after_sequence);
            Ok(json!({
                "events": events,
                "latest_sequence": session.snapshot().event_sequence,
                "resync_required": resync_required,
            }))
        }
        "create_pane" => {
            let payload: CreatePaneRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            let result = session.create_pane(payload);
            if result.is_ok() {
                if let Err(error) = session.save() {
                    return request_error(
                        request.request_id,
                        "persistence_failed",
                        format!("{error:?}"),
                    );
                }
            }
            result
        }
        "split_pane" => {
            let payload: SplitRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let direction = match payload.direction.as_str() {
                "horizontal" => crate::model::layout::Direction::Horizontal,
                "vertical" => crate::model::layout::Direction::Vertical,
                _ => {
                    return request_error(
                        request.request_id,
                        "invalid_payload",
                        "direction must be horizontal or vertical".into(),
                    )
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.split_pane(payload.pane, direction)
            })
        }
        "send_input" => {
            let payload: InputRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            session
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
            let mut session = session.lock().expect("session lock poisoned");
            if !session.can_resize(&payload.client_id) {
                return request_error(
                    request.request_id,
                    "geometry_owned",
                    "another client owns PTY geometry".into(),
                );
            }
            session
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
            let mut session = session.lock().expect("session lock poisoned");
            let result = session
                .stop_pane(&payload.pane_id)
                .map(|_| json!({ "stopped": true }));
            if result.is_ok() {
                if let Err(error) = session.save() {
                    return request_error(
                        request.request_id,
                        "persistence_failed",
                        format!("{error:?}"),
                    );
                }
            }
            result
        }
        "restart_pane" => {
            let payload: PaneRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.restart_pane(&payload.pane_id)
            })
        }
        "create_workspace" => {
            let payload: NameRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.create_workspace(payload.name)
            })
        }
        "switch_workspace" => {
            let payload: IdRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.switch_workspace(&payload.id)
            })
        }
        "rename_workspace" => {
            let payload: IdNameRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.rename_workspace(&payload.id, payload.name)
            })
        }
        "delete_workspace" => {
            let payload: IdRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.delete_workspace(&payload.id)
            })
        }
        "create_tab" => {
            let payload: NameRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.create_tab(payload.name))
        }
        "switch_tab" => {
            let payload: IdRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.switch_tab(&payload.id))
        }
        "rename_tab" => {
            let payload: IdNameRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.rename_tab(&payload.id, payload.name)
            })
        }
        "close_tab" => {
            let payload: IdRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.close_tab(&payload.id))
        }
        "focus_pane" => {
            let payload: PaneRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.focus_pane(&payload.pane_id))
        }
        "rename_pane" => {
            let payload: IdNameRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.rename_pane(&payload.id, payload.name)
            })
        }
        "focus_next" => {
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.focus_next())
        }
        "resize_pane" => {
            let payload: LayoutResizeRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| {
                session.resize_pane(&payload.pane_id, payload.delta)
            })
        }
        "close_pane" => {
            let payload: PaneRequest = match serde_json::from_value(request.payload) {
                Ok(payload) => payload,
                Err(error) => {
                    return request_error(request.request_id, "invalid_payload", error.to_string())
                }
            };
            let mut session = session.lock().expect("session lock poisoned");
            save_after(&mut session, |session| session.close_pane(&payload.pane_id))
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

fn save_after(
    session: &mut Session,
    operation: impl FnOnce(&mut Session) -> Result<Value, String>,
) -> Result<Value, String> {
    let result = operation(session);
    if result.is_ok() {
        session.save().map_err(|error| format!("{error:?}"))?;
    }
    result
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
    fn attach_returns_handshake_details() {
        let response = response_for(
            br#"{"version":1,"request_id":"a","op":"attach","payload":{"client_id":"client-a","cols":120,"rows":40,"capabilities":["mouse"]}}"#,
            &session(),
        );
        assert!(response.ok);
        let payload = response.payload.unwrap();
        assert_eq!(payload["cols"], 120);
        assert_eq!(payload["rows"], 40);
        assert_eq!(payload["capabilities"][0], "mouse");
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

    #[test]
    fn passive_client_cannot_resize_pty() {
        let shared = session();
        let attached = response_for(
            br#"{"version":1,"request_id":"a","op":"attach","payload":{"client_id":"active"}}"#,
            &shared,
        );
        assert!(attached.ok);

        let response = response_for(
            br#"{"version":1,"request_id":"r","op":"resize_pty","payload":{"pane_id":"missing","cols":80,"rows":24,"client_id":"passive"}}"#,
            &shared,
        );
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "geometry_owned");
    }

    #[test]
    fn owner_can_detach_and_release_geometry() {
        let shared = session();
        response_for(
            br#"{"version":1,"request_id":"a","op":"attach","payload":{"client_id":"active"}}"#,
            &shared,
        );
        let detached = response_for(
            br#"{"version":1,"request_id":"d","op":"detach","payload":{"client_id":"active"}}"#,
            &shared,
        );
        assert!(detached.ok);
        assert_eq!(detached.payload.unwrap()["released_geometry"], true);
    }
}
