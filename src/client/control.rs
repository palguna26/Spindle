use crate::protocol::frame::{read_frame, write_frame, FrameError};
use crate::protocol::{Event, Request, Response, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::io::{self, BufReader};
#[cfg(not(windows))]
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(not(windows))]
type ClientStream = TcpStream;
#[cfg(windows)]
type ClientStream = std::fs::File;

pub struct ControlClient {
    address: String,
    interactive_address: String,
    client_id: String,
}

#[derive(Debug, Deserialize)]
pub struct EventBatch {
    pub events: Vec<Event<serde_json::Value>>,
    pub latest_sequence: u64,
    #[serde(default)]
    pub resync_required: bool,
}

pub struct EventStream {
    address: String,
    after_sequence: u64,
    reader: BufReader<ClientStream>,
}

impl EventStream {
    pub fn next_batch(&mut self) -> Result<EventBatch, ClientError> {
        let frame = match read_frame(&mut self.reader) {
            Ok(frame) => frame,
            Err(_) => {
                self.reader = open_stream_connection(&self.address, self.after_sequence)?;
                read_frame(&mut self.reader)?
            }
        };
        let batch: EventBatch = serde_json::from_slice(&frame)?;
        self.after_sequence = batch.latest_sequence;
        Ok(batch)
    }
}

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Frame(FrameError),
    Json(serde_json::Error),
    Server(String),
}

impl From<io::Error> for ClientError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<FrameError> for ClientError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl ControlClient {
    pub(crate) fn endpoint(&self) -> &str {
        &self.address
    }

    pub fn connect(address: impl Into<String>) -> Result<Self, ClientError> {
        let mut client = Self {
            address: address.into(),
            interactive_address: String::new(),
            client_id: format!(
                "client-{}-{}",
                std::process::id(),
                NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed)
            ),
        };
        let ping = client.request_once("connect".into(), "ping".into(), ())?;
        client.interactive_address = ping
            .payload
            .as_ref()
            .and_then(|payload| payload.get("interactive_endpoint"))
            .and_then(serde_json::Value::as_str)
            .filter(|address| !address.is_empty())
            .unwrap_or(&client.address)
            .to_string();
        Ok(client)
    }

    pub fn request<T: Serialize>(
        &self,
        request_id: impl Into<String>,
        operation: impl Into<String>,
        payload: T,
    ) -> Result<Response<serde_json::Value>, ClientError> {
        self.request_once(request_id.into(), operation.into(), payload)
    }

    pub fn interactive_request<T: Serialize>(
        &self,
        request_id: impl Into<String>,
        operation: impl Into<String>,
        payload: T,
    ) -> Result<Response<serde_json::Value>, ClientError> {
        self.request_once_at(
            &self.interactive_address,
            request_id.into(),
            operation.into(),
            payload,
        )
    }

    pub fn subscribe_events(&self, after_sequence: u64) -> Result<EventBatch, ClientError> {
        let response = self.request(
            format!("events-{after_sequence}"),
            "subscribe_events",
            serde_json::json!({ "after_sequence": after_sequence }),
        )?;
        serde_json::from_value(response.payload.unwrap_or_default()).map_err(ClientError::Json)
    }

    pub fn open_event_stream(&self, after_sequence: u64) -> Result<EventStream, ClientError> {
        let reader = open_stream_connection(&self.interactive_address, after_sequence)?;
        Ok(EventStream {
            address: self.interactive_address.clone(),
            after_sequence,
            reader,
        })
    }

    pub fn attach(&self) -> Result<Response<serde_json::Value>, ClientError> {
        self.attach_with_terminal(0, 0, Vec::new())
    }

    pub fn attach_with_terminal(
        &self,
        cols: u16,
        rows: u16,
        capabilities: Vec<String>,
    ) -> Result<Response<serde_json::Value>, ClientError> {
        self.request(
            "attach",
            "attach",
            serde_json::json!({
                "client_id": self.client_id,
                "cols": cols,
                "rows": rows,
                "capabilities": capabilities,
            }),
        )
    }

    pub fn detach(&self) -> Result<Response<serde_json::Value>, ClientError> {
        self.request(
            "detach",
            "detach",
            serde_json::json!({ "client_id": self.client_id }),
        )
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn request_with_retry<T: Serialize + Clone>(
        &self,
        request_id: impl Into<String> + Clone,
        operation: impl Into<String> + Clone,
        payload: T,
        attempts: usize,
        delay: Duration,
    ) -> Result<Response<serde_json::Value>, ClientError> {
        let attempts = attempts.max(1);
        let mut last_error = None;
        for attempt in 0..attempts {
            match self.request_once(
                request_id.clone().into(),
                operation.clone().into(),
                payload.clone(),
            ) {
                Ok(response) => return Ok(response),
                Err(error) => {
                    last_error = Some(error);
                    if attempt + 1 < attempts {
                        std::thread::sleep(delay);
                    }
                }
            }
        }
        Err(last_error.expect("at least one request attempt"))
    }

    fn request_once<T: Serialize>(
        &self,
        request_id: String,
        operation: String,
        payload: T,
    ) -> Result<Response<serde_json::Value>, ClientError> {
        self.request_once_at(&self.address, request_id, operation, payload)
    }

    fn request_once_at<T: Serialize>(
        &self,
        address: &str,
        request_id: String,
        operation: String,
        payload: T,
    ) -> Result<Response<serde_json::Value>, ClientError> {
        let mut stream = connect_stream_retry(address)?;
        let request = Request {
            version: PROTOCOL_VERSION,
            request_id,
            op: operation,
            payload: serde_json::to_value(payload)?,
        };
        write_frame(&mut stream, &serde_json::to_vec(&request)?)?;
        let mut reader = BufReader::new(stream);
        let response: Response<serde_json::Value> =
            serde_json::from_slice(&read_frame(&mut reader)?)?;
        if let Some(error) = &response.error {
            return Err(ClientError::Server(format!(
                "{}: {}",
                error.code, error.message
            )));
        }
        Ok(response)
    }
}

fn open_stream_connection(
    address: &str,
    after_sequence: u64,
) -> Result<BufReader<ClientStream>, ClientError> {
    let mut stream = connect_stream_retry(address)?;
    let request = Request {
        version: PROTOCOL_VERSION,
        request_id: format!("stream-{after_sequence}"),
        op: "stream_events".into(),
        payload: serde_json::json!({
            "after_sequence": after_sequence,
        }),
    };
    write_frame(&mut stream, &serde_json::to_vec(&request)?)?;
    let mut reader = BufReader::new(stream);
    let response: Response<serde_json::Value> = serde_json::from_slice(&read_frame(&mut reader)?)?;
    if let Some(error) = response.error {
        return Err(ClientError::Server(format!(
            "{}: {}",
            error.code, error.message
        )));
    }
    Ok(reader)
}

#[cfg(not(windows))]
fn connect_stream(address: &str) -> io::Result<TcpStream> {
    TcpStream::connect(address)
}

#[cfg(windows)]
fn connect_stream(address: &str) -> io::Result<std::fs::File> {
    crate::server::transport::connect(address)
}

#[cfg(not(windows))]
fn connect_stream_retry(address: &str) -> io::Result<TcpStream> {
    retry_connect(address, connect_stream)
}

#[cfg(windows)]
fn connect_stream_retry(address: &str) -> io::Result<std::fs::File> {
    retry_connect(address, connect_stream)
}

fn retry_connect<S, F>(address: &str, connect: F) -> io::Result<S>
where
    S: std::io::Read + std::io::Write,
    F: Fn(&str) -> io::Result<S>,
{
    let mut last_error = None;
    for attempt in 0..20 {
        match connect(address) {
            Ok(stream) => return Ok(stream),
            Err(error) => {
                last_error = Some(error);
                if attempt < 19 {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }
    Err(last_error.expect("at least one connection attempt"))
}

#[cfg(test)]
mod tests {
    use super::ControlClient;
    use std::time::Duration;

    #[test]
    fn unavailable_endpoint_can_be_retried() {
        let client = ControlClient {
            address: "127.0.0.1:1".into(),
            interactive_address: "127.0.0.1:1".into(),
            client_id: "test-client".into(),
        };
        assert!(client
            .request_with_retry("1", "ping", (), 2, Duration::from_millis(1))
            .is_err());
    }
}
