use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request<T> {
    pub version: u16,
    pub request_id: String,
    pub op: String,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response<T> {
    pub version: u16,
    pub request_id: String,
    pub ok: bool,
    pub payload: Option<T>,
    pub error: Option<ProtocolError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event<T> {
    pub version: u16,
    pub sequence: u64,
    pub event: String,
    pub payload: T,
}

pub fn negotiate(version: u16) -> Result<(), ProtocolError> {
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ProtocolError {
            code: "unsupported_version".into(),
            message: format!(
                "protocol version {version} is unsupported; expected {PROTOCOL_VERSION}"
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{negotiate, Request, PROTOCOL_VERSION};

    #[test]
    fn request_round_trips_as_json() {
        let request = Request {
            version: PROTOCOL_VERSION,
            request_id: "42".into(),
            op: "ping".into(),
            payload: (),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: Request<()> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn unsupported_versions_return_clear_errors() {
        let error = negotiate(99).unwrap_err();
        assert_eq!(error.code, "unsupported_version");
        assert!(error.message.contains("expected 1"));
    }
}
