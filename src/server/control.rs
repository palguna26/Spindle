use crate::protocol::frame::{read_frame, write_frame, FrameError};
use crate::protocol::{negotiate, ProtocolError, Request, Response, PROTOCOL_VERSION};
use serde_json::{json, Value};
use std::io::{self, BufReader};
use std::net::TcpStream;

pub fn handle_connection(stream: TcpStream) -> io::Result<bool> {
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;
    let response = match read_frame(&mut reader) {
        Ok(frame) => response_for(&frame),
        Err(error) => error_response("invalid_frame", frame_error_message(error)),
    };
    let encoded = serde_json::to_vec(&response).map_err(io::Error::other)?;
    write_frame(&mut writer, &encoded).map_err(frame_io_error)?;
    Ok(response_requests_stop(&response))
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

fn response_for(frame: &[u8]) -> Response<Value> {
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
    let payload = match request.op.as_str() {
        "ping" => json!({ "status": "ok" }),
        "attach" => json!({ "attached": true }),
        "get_snapshot" => json!({ "version": 1, "spaces": [] }),
        "stop_server" => json!({ "stopping": true }),
        _ => {
            return error_response(
                "unknown_operation",
                format!("unknown operation '{}'", request.op),
            );
        }
    };
    Response {
        version: PROTOCOL_VERSION,
        request_id: request.request_id,
        ok: true,
        payload: Some(payload),
        error: None,
    }
}

fn error_response(code: &str, message: String) -> Response<Value> {
    Response {
        version: PROTOCOL_VERSION,
        request_id: String::new(),
        ok: false,
        payload: None,
        error: Some(ProtocolError {
            code: code.into(),
            message,
        }),
    }
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

    #[test]
    fn ping_returns_a_success_response() {
        let response = response_for(br#"{"version":1,"request_id":"1","op":"ping","payload":{}}"#);
        assert!(response.ok);
        assert_eq!(response.request_id, "1");
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let response = response_for(br#"{"version":99,"request_id":"2","op":"ping","payload":{}}"#);
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "unsupported_version");
    }

    #[test]
    fn malformed_json_is_rejected() {
        let response = response_for(br#"not-json"#);
        assert!(!response.ok);
        assert_eq!(response.version, PROTOCOL_VERSION);
        assert_eq!(response.error.unwrap().code, "invalid_json");
    }
}
