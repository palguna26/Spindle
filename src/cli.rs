use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::client::ControlClient;
use crate::protocol::Response;
use serde_json::Value;

const APP_DIR: &str = "Spindle";

pub fn run() -> io::Result<()> {
    let command = env::args().nth(1).unwrap_or_else(|| "attach".into());
    if command == "run-server" {
        let state_dir = env::args().nth(2).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "run-server needs a state directory",
            )
        })?;
        return crate::server::run(Path::new(&state_dir));
    }
    let project = Project::from_current_dir()?;

    match command.as_str() {
        "start" => start_server(&project)?,
        "attach" => attach_server(&project)?,
        "list" => {
            println!("project: {}", project.describe());
            println!("state: {}", project.state_dir.display());
        }
        "doctor" => {
            println!("project: {}", project.describe());
            println!("state directory: {}", project.state_dir.display());
            println!("state directory exists: {}", project.state_dir.exists());
            println!("endpoint exists: {}", project.endpoint_path().exists());
            println!("server running: {}", ping_server(&project).is_ok());
            let pid_path = project.state_dir.join("server.pid");
            match fs::read_to_string(pid_path) {
                Ok(pid) => println!("server pid: {}", pid.trim()),
                Err(_) => println!("server pid: unavailable"),
            }
            let identity_path = project.state_dir.join("server.json");
            match fs::read_to_string(identity_path) {
                Ok(identity) => println!("server identity: {}", identity.replace('\n', " ")),
                Err(_) => println!("server identity: unavailable"),
            }
        }
        "stop" => {
            send_command(&project, "stop_server")?;
            println!("server stopped");
        }
        "help" | "--help" | "-h" => print_help(),
        other => {
            print_help();
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown command '{other}'"),
            ));
        }
    }

    Ok(())
}

fn start_server(project: &Project) -> io::Result<()> {
    project.ensure_state_dir()?;
    if ping_server(project).is_ok() {
        println!("server already running for {}", project.describe());
        return Ok(());
    }
    let executable = env::current_exe()?;
    Command::new(executable)
        .args(["run-server", &project.state_dir.to_string_lossy()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    for _ in 0..20 {
        if ping_server(project).is_ok() {
            println!("server started for {}", project.describe());
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "server did not become ready",
    ))
}

fn attach_server(project: &Project) -> io::Result<()> {
    project.ensure_state_dir()?;
    if ping_server(project).is_err() {
        start_server(project)?;
    }
    let address = fs::read_to_string(project.endpoint_path())?;
    crate::client::app::run(address.trim(), &project.state_dir)
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

fn ping_server(project: &Project) -> io::Result<Response<Value>> {
    send_command(project, "ping")
}

fn send_command(project: &Project, operation: &str) -> io::Result<Response<Value>> {
    let address = fs::read_to_string(project.endpoint_path())?;
    let client = ControlClient::connect(address.trim())
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    client
        .request(
            format!("cli-{}", std::process::id()),
            operation,
            Value::Object(Default::default()),
        )
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

fn print_help() {
    println!("Spindle - persistent parallel coding-agent sessions");
    println!();
    println!("Usage: spindle [start|attach|stop|list|doctor]");
    println!();
    println!("Commands:");
    println!("  start    start a server for the current project");
    println!("  attach   attach to the current project's server (default)");
    println!("  stop     stop the current project's server");
    println!("  list     show the current project identity and state path");
    println!("  doctor   check local Spindle state");
}

struct Project {
    directory: PathBuf,
    state_dir: PathBuf,
    id: String,
}

impl Project {
    fn from_current_dir() -> io::Result<Self> {
        let directory = env::current_dir()?.canonicalize()?;
        let id = project_id(&directory);
        let state_dir = state_root()?.join("projects").join(&id);
        Ok(Self {
            directory,
            state_dir,
            id,
        })
    }

    fn ensure_state_dir(&self) -> io::Result<()> {
        fs::create_dir_all(&self.state_dir)
    }

    fn describe(&self) -> String {
        format!("{} ({})", self.directory.display(), self.id)
    }
}

fn state_root() -> io::Result<PathBuf> {
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(local_app_data).join(APP_DIR));
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(data_home).join("spindle"));
    }
    env::current_dir().map(|path| path.join(".spindle"))
}

fn project_id(path: &Path) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

impl Project {
    fn endpoint_path(&self) -> PathBuf {
        self.state_dir.join("server.endpoint")
    }
}

#[cfg(test)]
mod tests {
    use super::project_id;
    use std::path::Path;

    #[test]
    fn project_id_is_deterministic() {
        assert_eq!(
            project_id(Path::new("C:/repo")),
            project_id(Path::new("C:/repo"))
        );
    }

    #[test]
    fn project_id_changes_with_path() {
        assert_ne!(
            project_id(Path::new("C:/repo")),
            project_id(Path::new("C:/other"))
        );
    }
}
