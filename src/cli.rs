use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::client::ControlClient;
use crate::protocol::Response;
use serde_json::Value;

const APP_DIR: &str = "Spindle";

mod api;
mod completion;
mod pane;
mod tab;
mod workspace;

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
    match command.as_str() {
        "help" | "--help" | "-h" => {
            print_help();
            return Ok(());
        }
        "--version" | "-V" => {
            println!("spindle {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        "config" => {
            return run_config_command(&env::args().skip(2).collect::<Vec<_>>());
        }
        "completion" => {
            return completion::run(&env::args().skip(2).collect::<Vec<_>>());
        }
        "api" => {
            return api::run(&env::args().skip(2).collect::<Vec<_>>());
        }
        _ => {}
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
            println!("client version: spindle {}", env!("CARGO_PKG_VERSION"));
            println!("client binary: {}", env::current_exe()?.display());
            println!("client protocol: {}", crate::protocol::PROTOCOL_VERSION);
            println!("state directory: {}", project.state_dir.display());
            println!("state directory exists: {}", project.state_dir.exists());
            let endpoint_exists = project.endpoint_path().exists();
            let server_running = ping_server(&project).is_ok();
            let endpoint_status = endpoint_status_label(endpoint_exists, server_running);
            println!("endpoint exists: {endpoint_exists}");
            println!("server running: {server_running}");
            println!("endpoint status: {endpoint_status}");
            if endpoint_status == "stale" {
                println!("recovery: run spindle start or spindle attach to replace stale metadata");
            }
            let pid_path = project.state_dir.join("server.pid");
            match fs::read_to_string(pid_path) {
                Ok(pid) => println!("recorded server pid: {}", pid.trim()),
                Err(_) => println!("recorded server pid: unavailable"),
            }
            let identity_path = project.state_dir.join("server.json");
            match fs::read_to_string(identity_path) {
                Ok(identity) => println!("last server identity: {}", identity.replace('\n', " ")),
                Err(_) => println!("last server identity: unavailable"),
            }
        }
        "stop" => {
            let response = send_command(&project, "stop_server")?;
            if !response.ok {
                let error = response
                    .error
                    .map(|error| error.message)
                    .unwrap_or_else(|| "server rejected the stop request".into());
                return Err(io::Error::other(error));
            }
            wait_for_server_stop(&project)?;
            println!("server stopped");
        }
        "workspace" => {
            workspace::run_workspace_command(&project, &env::args().skip(2).collect::<Vec<_>>())?
        }
        "tab" => tab::run_tab_command(&project, &env::args().skip(2).collect::<Vec<_>>())?,
        "pane" => pane::run_pane_command(&project, &env::args().skip(2).collect::<Vec<_>>())?,
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

fn wait_for_server_stop(project: &Project) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while project.endpoint_path().exists() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "server did not remove its endpoint after the stop request",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
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

fn endpoint_status_label(endpoint_exists: bool, server_running: bool) -> &'static str {
    if server_running {
        "reachable"
    } else if endpoint_exists {
        "stale"
    } else {
        "missing"
    }
}

fn send_command(project: &Project, operation: &str) -> io::Result<Response<Value>> {
    send_command_with_payload(project, operation, Value::Object(Default::default()))
}

fn send_command_with_payload(
    project: &Project,
    operation: &str,
    payload: Value,
) -> io::Result<Response<Value>> {
    let address = fs::read_to_string(project.endpoint_path())?;
    let client = ControlClient::connect(address.trim())
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    client
        .request(format!("cli-{}", std::process::id()), operation, payload)
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

fn print_help() {
    println!("Spindle - persistent parallel coding-agent sessions");
    println!();
    println!("Usage: spindle [start|attach|stop|list|doctor|workspace|tab|pane|help]");
    println!();
    println!("Commands:");
    println!("  start    start a server for the current project");
    println!("  attach   attach to the current project's server (default)");
    println!("  stop     stop the current project's server");
    println!("  list     show the current project identity and state path");
    println!("  doctor   check local Spindle state");
    println!("  config path     show the user config path");
    println!("  config default  print a starter config");
    println!(
        "  completion <shell>  generate shell completions (bash, elvish, fish, powershell, zsh)"
    );
    println!("  api schema [--json|--output PATH]  inspect the control API schema");
    println!("  workspace list  list workspaces in the current project session");
    println!("  workspace get <id>  show a workspace by ID");
    println!("  workspace focus <id>  focus a workspace by ID");
    println!("  workspace rename <id> <label>  rename a workspace");
    println!("  tab list        list tabs in the active workspace");
    println!("  tab create      create a tab in the active workspace");
    println!("  tab get/focus/rename/close  manage tabs by ID");
    println!("  pane list/current/get/focus/rename/stop/restart/zoom/close  manage panes");
    println!("Options:");
    println!("  --help, -h       show this help");
    println!("  --version, -V    print the version");
}

fn run_config_command(args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "path" => println!("{}", crate::config::path().display()),
        [command] if command == "default" => print!("{}", crate::config::default_document()),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            println!("Usage: spindle config <path|default>");
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle config <path|default>",
            ));
        }
    }
    Ok(())
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
    use super::{endpoint_status_label, project_id};
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

    #[test]
    fn doctor_distinguishes_reachable_stale_and_missing_endpoints() {
        assert_eq!(endpoint_status_label(true, true), "reachable");
        assert_eq!(endpoint_status_label(true, false), "stale");
        assert_eq!(endpoint_status_label(false, false), "missing");
        assert_eq!(endpoint_status_label(false, true), "reachable");
    }
}
