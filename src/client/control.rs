use crate::protocol::frame::{read_frame, write_frame, FrameError};
use crate::protocol::{Request, Response, PROTOCOL_VERSION};
use serde::Serialize;
use std::io::{self, BufReader};
use std::net::TcpStream;
use std::time::Duration;

pub struct ControlClient {
    address: String,
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
    pub fn connect(address: impl Into<String>) -> Result<Self, ClientError> {
        let client = Self {
            address: address.into(),
        };
        client.request_once("connect".into(), "ping".into(), ())?;
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

    pub fn request_with_retry<T: Serialize>(
        &self,
        request_id: impl Into<String> + Clone,
        operation: impl Into<String> + Clone,
        payload: T,
        attempts: usize,
        delay: Duration,
    ) -> Result<Response<serde_json::Value>, ClientError>
    where
        T: Clone,
    {
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
        let mut stream = TcpStream::connect(&self.address)?;
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

#[cfg(test)]
mod tests {
    use super::ControlClient;
    use std::time::Duration;

    #[test]
    fn unavailable_endpoint_can_be_retried() {
        let client = ControlClient {
            address: "127.0.0.1:1".into(),
        };
        assert!(client
            .request_with_retry("1", "ping", (), 2, Duration::from_millis(1))
            .is_err());
    }
}
