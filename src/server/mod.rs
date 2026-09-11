pub mod control;
pub mod session;
#[cfg(windows)]
pub(crate) mod transport;

use std::fs;
use std::io;
#[cfg(not(windows))]
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServerError {
    AlreadyRunning,
    NotRunning,
    InvalidTransition { from: ServerState, to: ServerState },
}

#[derive(Debug)]
pub struct ServerLifecycle {
    state: ServerState,
}

impl Default for ServerLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerLifecycle {
    pub fn new() -> Self {
        Self {
            state: ServerState::Stopped,
        }
    }

    pub fn state(&self) -> ServerState {
        self.state
    }

    pub fn start(&mut self) -> Result<(), ServerError> {
        if self.state != ServerState::Stopped {
            return Err(ServerError::AlreadyRunning);
        }
        self.state = ServerState::Starting;
        self.state = ServerState::Running;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), ServerError> {
        if self.state == ServerState::Stopped {
            return Err(ServerError::NotRunning);
        }
        self.state = ServerState::Stopping;
        self.state = ServerState::Stopped;
        Ok(())
    }

    pub fn restore(state: ServerState) -> Result<Self, ServerError> {
        match state {
            ServerState::Running => Ok(Self {
                state: ServerState::Running,
            }),
            other => Err(ServerError::InvalidTransition {
                from: other,
                to: ServerState::Running,
            }),
        }
    }
}

pub fn run(state_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(state_dir)?;
    #[cfg(not(windows))]
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    #[cfg(not(windows))]
    let address = listener.local_addr()?.to_string();
    #[cfg(windows)]
    let address = transport::endpoint(state_dir);
    let session = Arc::new(Mutex::new(
        session::Session::load_or_default(state_dir.join("session.json"))
            .map_err(|error| io::Error::other(format!("session snapshot is invalid: {error:?}")))?,
    ));
    let endpoint = state_dir.join("server.endpoint");
    fs::write(&endpoint, &address)?;

    let stopping = Arc::new(AtomicBool::new(false));
    #[cfg(not(windows))]
    let accept_next = || listener.accept().map(|(stream, _)| stream);
    #[cfg(windows)]
    let accept_next = || transport::accept(&address);

    while !stopping.load(Ordering::Acquire) {
        let stream = accept_next()?;
        let session = Arc::clone(&session);
        let stopping = Arc::clone(&stopping);
        let wake_address = address.clone();
        thread::spawn(move || {
            if let Ok(true) = control::handle_connection(stream, session) {
                stopping.store(true, Ordering::Release);
                wake_server(&wake_address);
            }
        });
    }

    let _ = fs::remove_file(endpoint);
    Ok(())
}

#[cfg(not(windows))]
fn wake_server(address: &str) {
    let _ = TcpStream::connect(address);
}

#[cfg(windows)]
fn wake_server(address: &str) {
    let _ = transport::connect(address);
}

#[cfg(test)]
mod tests {
    use super::{ServerError, ServerLifecycle, ServerState};

    #[test]
    fn start_and_stop_are_explicit() {
        let mut server = ServerLifecycle::new();
        assert_eq!(server.state(), ServerState::Stopped);
        server.start().unwrap();
        assert_eq!(server.state(), ServerState::Running);
        assert_eq!(server.start(), Err(ServerError::AlreadyRunning));
        server.stop().unwrap();
        assert_eq!(server.state(), ServerState::Stopped);
        assert_eq!(server.stop(), Err(ServerError::NotRunning));
    }

    #[test]
    fn only_running_servers_can_be_restored() {
        assert!(ServerLifecycle::restore(ServerState::Running).is_ok());
        assert!(matches!(
            ServerLifecycle::restore(ServerState::Stopped),
            Err(ServerError::InvalidTransition {
                from: ServerState::Stopped,
                to: ServerState::Running,
            })
        ));
    }
}
