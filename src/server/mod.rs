pub mod control;
pub mod lifecycle;
mod plugins;
pub mod session;
#[cfg(windows)]
pub(crate) mod transport;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerIdentity {
    pub pid: u32,
    pub protocol_version: u16,
    pub control_endpoint: String,
    pub interactive_endpoint: String,
    pub started_at_unix_seconds: u64,
}

pub fn run(state_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(state_dir)?;
    let logger = crate::logging::Logger::new(state_dir.join("server.log"))?;
    #[cfg(not(windows))]
    let control_listener = TcpListener::bind(("127.0.0.1", 0))?;
    #[cfg(not(windows))]
    let address = control_listener.local_addr()?.to_string();
    #[cfg(not(windows))]
    let interactive_listener = TcpListener::bind(("127.0.0.1", 0))?;
    #[cfg(not(windows))]
    let interactive_address = interactive_listener.local_addr()?.to_string();
    #[cfg(windows)]
    let address = transport::endpoint(state_dir);
    #[cfg(windows)]
    let interactive_address = transport::interactive_endpoint(state_dir);
    let mut loaded_session = session::Session::load_or_default(state_dir.join("session.json"))
        .map_err(|error| io::Error::other(format!("session snapshot is invalid: {error:?}")))?;
    let repository_path = std::env::current_dir()?.to_string_lossy().into_owned();
    loaded_session.set_default_workspace_context(repository_path, git_branch());
    loaded_session.save().map_err(|error| {
        io::Error::other(format!("session snapshot could not be saved: {error:?}"))
    })?;
    let session = Arc::new(Mutex::new(loaded_session));
    let endpoint = state_dir.join("server.endpoint");
    let interactive_endpoint = state_dir.join("server.interactive.endpoint");
    let identity_json = state_dir.join("server.json");
    let identity = state_dir.join("server.pid");
    fs::write(&identity, std::process::id().to_string())?;
    fs::write(&endpoint, &address)?;
    fs::write(&interactive_endpoint, &interactive_address)?;
    let server_identity = ServerIdentity {
        pid: std::process::id(),
        protocol_version: crate::protocol::PROTOCOL_VERSION,
        control_endpoint: address.clone(),
        interactive_endpoint: interactive_address.clone(),
        started_at_unix_seconds: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    fs::write(
        &identity_json,
        serde_json::to_vec_pretty(&server_identity).map_err(io::Error::other)?,
    )?;
    let mut start_fields = BTreeMap::new();
    start_fields.insert("pid".into(), std::process::id().to_string());
    start_fields.insert("control_endpoint".into(), address.clone());
    start_fields.insert("interactive_endpoint".into(), interactive_address.clone());
    logger.info("server_started", start_fields)?;

    let stopping = Arc::new(AtomicBool::new(false));
    let interactive_stopping = Arc::clone(&stopping);
    let interactive_session = Arc::clone(&session);
    let interactive_address_for_wake = interactive_address.clone();
    thread::spawn(move || {
        #[cfg(not(windows))]
        let accept_next = || interactive_listener.accept().map(|(stream, _)| stream);
        #[cfg(windows)]
        let accept_next = || transport::accept(&interactive_address_for_wake);
        while !interactive_stopping.load(Ordering::Acquire) {
            let Ok(stream) = accept_next() else { break };
            let session = Arc::clone(&interactive_session);
            let stopping = Arc::clone(&interactive_stopping);
            thread::spawn(move || {
                if let Ok(true) = control::handle_connection(stream, session) {
                    stopping.store(true, Ordering::Release);
                }
            });
        }
    });
    #[cfg(not(windows))]
    let accept_next = || control_listener.accept().map(|(stream, _)| stream);
    #[cfg(windows)]
    let accept_next = || transport::accept(&address);
    let startup_hooks_started = Arc::new(AtomicBool::new(false));
    let event_session = Arc::clone(&session);
    let event_stopping = Arc::clone(&stopping);
    let event_endpoint = address.clone();
    thread::spawn(move || {
        let mut sequence = 0;
        while !event_stopping.load(Ordering::Acquire) {
            let (events, snapshot, latest) = {
                let mut session = event_session.lock().expect("session lock poisoned");
                (
                    session.events_since(sequence),
                    session.snapshot().clone(),
                    session.snapshot().event_sequence,
                )
            };
            for event in events {
                plugins::run_event_hook(&event, &snapshot, &event_endpoint);
            }
            sequence = latest;
            thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    while !stopping.load(Ordering::Acquire) {
        let stream = accept_next()?;
        let session = Arc::clone(&session);
        let stopping = Arc::clone(&stopping);
        let wake_address = address.clone();
        let wake_interactive_address = interactive_address.clone();
        let handler_interactive_address = interactive_address.clone();
        let startup_hooks_started = Arc::clone(&startup_hooks_started);
        let startup_endpoint = address.clone();
        thread::spawn(move || {
            let stopped = control::handle_connection_with_interactive(
                stream,
                Arc::clone(&session),
                &handler_interactive_address,
            )
            .unwrap_or(false);
            if startup_hooks_started
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let snapshot = session
                    .lock()
                    .expect("session lock poisoned")
                    .snapshot()
                    .clone();
                plugins::run_startup_hooks(&snapshot, &startup_endpoint);
            }
            if stopped {
                stopping.store(true, Ordering::Release);
                wake_server(&wake_address);
                wake_server(&wake_interactive_address);
            }
        });
    }

    let _ = fs::remove_file(endpoint);
    let _ = fs::remove_file(interactive_endpoint);
    let _ = fs::remove_file(identity_json);
    let _ = fs::remove_file(identity);
    let mut stop_fields = BTreeMap::new();
    stop_fields.insert("pid".into(), std::process::id().to_string());
    let _ = logger.info("server_stopped", stop_fields);
    Ok(())
}

fn git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

#[cfg(not(windows))]
fn wake_server(address: &str) {
    let _ = TcpStream::connect(address);
}

#[cfg(windows)]
fn wake_server(address: &str) {
    // The listener creates its next named-pipe instance only after handling
    // the current request. Retry the wakeup if shutdown races that gap.
    for _ in 0..20 {
        if transport::connect(address).is_ok() {
            return;
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
}
