use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::client::ControlClient;
use crate::protocol::Response;
use serde_json::Value;

const APP_DIR: &str = "Spindle";

pub(super) fn parse_env_assignment(value: &str) -> io::Result<(String, String)> {
    let (key, value) = value
        .split_once('=')
        .ok_or_else(|| io::Error::other(format!("environment must use KEY=VALUE: {value}")))?;
    if key.is_empty() {
        return Err(io::Error::other("environment key cannot be empty"));
    }
    if key.contains('\0') || value.contains('\0') {
        return Err(io::Error::other("environment must not contain NUL bytes"));
    }
    Ok((key.to_owned(), value.to_owned()))
}

pub(crate) fn default_shell() -> (String, Vec<String>) {
    let configured = crate::config::load().default_shell;
    let command = configured
        .or_else(|| env::var("SHELL").ok())
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "powershell.exe".into()
            } else {
                "sh".into()
            }
        });
    let is_powershell = command.rsplit(['/', '\\']).next().is_some_and(|name| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
        )
    });
    let args = match (is_powershell, crate::config::load().shell_mode) {
        (true, crate::config::ShellMode::Login) => vec!["-NoLogo".into()],
        (true, _) => vec!["-NoLogo".into(), "-NoProfile".into()],
        (false, crate::config::ShellMode::Login) if !cfg!(windows) => vec!["-l".into()],
        _ => Vec::new(),
    };
    (command, args)
}

pub(crate) fn new_terminal_cwd(follow_cwd: Option<String>) -> String {
    match crate::config::load().new_cwd {
        crate::config::NewCwd::Follow => follow_cwd
            .or_else(|| {
                env::current_dir()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| ".".into()),
        crate::config::NewCwd::Home => env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .map(|path| path.to_string_lossy().into_owned())
            .or_else(|| {
                env::current_dir()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| ".".into()),
        crate::config::NewCwd::Current => env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".into()),
        crate::config::NewCwd::Path(path) => {
            if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
                env::var_os("USERPROFILE")
                    .or_else(|| env::var_os("HOME"))
                    .map(PathBuf::from)
                    .map(|home| home.join(rest).to_string_lossy().into_owned())
                    .unwrap_or(path)
            } else {
                path
            }
        }
    }
}

mod agent;
mod api;
mod completion;
mod integration;
mod notification;
mod pane;
mod plugin;
mod status;
mod tab;
mod workspace;
mod worktree;

pub fn run() -> io::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let session_name = if args.first().map(String::as_str) == Some("--session") {
        if args.len() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle --session <name> [command]",
            ));
        }
        let name = args.remove(1);
        args.remove(0);
        Some(name)
    } else {
        selected_session_from_environment()
    };
    let command = args.first().cloned().unwrap_or_else(|| "attach".into());
    let command_args = args.get(1..).unwrap_or(&[]);
    if command == "run-server" {
        let state_dir = command_args.first().ok_or_else(|| {
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
            return run_config_command(command_args);
        }
        "completion" => {
            return completion::run(command_args);
        }
        "api" => {
            return api::run(
                &Project::from_current_dir_named(session_name.as_deref())?,
                command_args,
            );
        }
        "agent" => {
            return agent::run_agent_command(
                &Project::from_current_dir_named(session_name.as_deref())?,
                command_args,
            );
        }
        "plugin" => {
            return plugin::run(command_args);
        }
        "notification" => {
            return notification::run(command_args);
        }
        "integration" => {
            return integration::run(command_args);
        }
        "session" => return run_session_command(command_args),
        _ => {}
    }
    let project = Project::from_current_dir_named(session_name.as_deref())?;

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
        "status" => status::run(&project, command_args)?,
        "stop" => stop_server(&project)?,
        "workspace" => workspace::run_workspace_command(&project, command_args)?,
        "worktree" => worktree::run_worktree_command(&project, command_args)?,
        "tab" => tab::run_tab_command(&project, command_args)?,
        "pane" => pane::run_pane_command(&project, command_args)?,
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
    start_server_with_output(project, true)
}

fn start_server_with_output(project: &Project, show_status: bool) -> io::Result<()> {
    project.ensure_state_dir()?;
    if ping_server(project).is_ok() {
        if show_status {
            println!("server already running for {}", project.describe());
        }
        return Ok(());
    }
    let executable = env::current_exe()?;
    let mut server = Command::new(executable);
    server
        .args(["run-server", &project.state_dir.to_string_lossy()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::platform::launch_server_daemon(&mut server)?;
    // Detached Windows processes can take longer to initialize than the
    // endpoint file creation. Keep startup bounded, but allow WMI process
    // creation, the listener, and restored session to become ready under
    // normal Windows system load.
    for _ in 0..600 {
        if ping_server(project).is_ok() {
            if show_status {
                println!("server started for {}", project.describe());
            }
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
        start_server_with_output(project, false)?;
    }
    let address = control_address(project)?;
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
    let address = control_address(project)?;
    let client = ControlClient::connect(address.trim())
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    client
        .request(format!("cli-{}", std::process::id()), operation, payload)
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

fn control_address(project: &Project) -> io::Result<String> {
    if let Some(address) = socket_override(
        std::env::var("SPINDLE_SOCKET_PATH").ok().as_deref(),
        std::env::var("HERDR_SOCKET_PATH").ok().as_deref(),
    ) {
        return Ok(address);
    }
    fs::read_to_string(project.endpoint_path())
}

fn socket_override(spindle: Option<&str>, herdr: Option<&str>) -> Option<String> {
    spindle
        .filter(|address| !address.trim().is_empty())
        .or_else(|| herdr.filter(|address| !address.trim().is_empty()))
        .map(str::to_owned)
}

fn print_help() {
    println!("Spindle - persistent parallel coding-agent sessions");
    println!();
    println!(
        "Usage: spindle [--session <name>] [start|attach|stop|list|status|doctor|workspace|worktree|tab|pane|agent|notification|integration|session|help]"
    );
    println!();
    println!("Commands:");
    println!("  start    start a server for the current project");
    println!("  attach   attach to the current project's server (default)");
    println!("  --session <name>  use a named persistent session");
    println!("  stop     stop the current project's server");
    println!("  list     show the current project identity and state path");
    println!("  status   show Herdr-style client and server status");
    println!("  doctor   check local Spindle state");
    println!("  integration status  show agent integration status");
    println!("  config path     show the user config path");
    println!("  config default  print a starter config");
    println!("  config check    validate the user config");
    println!("  config reset-keys  back up config.toml and remove custom keybindings");
    println!(
        "  completion <shell>  generate shell completions (bash, elvish, fish, powershell, zsh)"
    );
    println!("  api snapshot/schema  inspect the session or control API schema");
    println!("  plugin link/list/unlink/enable/disable/action/pane  manage local plugins");
    println!("  notification show  show a desktop notification");
    println!("  workspace list/create/close/report-metadata  manage workspaces in the current project session");
    println!("  workspace get <id>  show a workspace by ID");
    println!("  workspace focus <id>  focus a workspace by ID");
    println!("  workspace move <id> <insert-index>  reorder a workspace");
    println!("  workspace rename <id> <label>  rename a workspace");
    println!("  worktree list/create/open/remove  manage Git worktree workspaces");
    println!("  tab list        list tabs in the active workspace");
    println!("  tab create      create a tab in the active workspace");
    println!("  tab get/focus/move/rename/close  manage tabs by ID");
    println!("  pane list/current/get/focus/neighbor/edges/layout/process-info/input/rename/stop/restart/zoom/close/send-text/send-keys/run/read/swap/move/report-agent/report-agent-session/report-metadata/release-agent/wait-output/split/resize  manage panes");
    println!("  agent list/get/focus/start/wait/read/send-keys/prompt/rename/explain <target>  inspect and control agents");
    println!("  session list/attach/stop/delete <name>  manage named sessions");
    println!("Options:");
    println!("  --help, -h       show this help");
    println!("  --version, -V    print the version");
}

fn run_session_command(args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "list" => {
            let project = Project::from_current_dir()?;
            println!("default\t{}", project.state_dir.display());
            let sessions_dir = project.state_dir.join("sessions");
            if let Ok(entries) = fs::read_dir(sessions_dir) {
                for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
                    println!(
                        "{}\t{}",
                        entry.file_name().to_string_lossy(),
                        entry.path().display()
                    );
                }
            }
            Ok(())
        }
        [command, name] if command == "attach" => {
            validate_session_name(name)?;
            attach_server(&Project::from_current_dir_named(Some(name))?)
        }
        [command, name] if command == "stop" => {
            validate_session_name(name)?;
            let project = Project::from_current_dir_named(Some(name))?;
            stop_server(&project)
        }
        [command, name] if command == "delete" => {
            validate_session_name(name)?;
            let project = Project::from_current_dir_named(Some(name))?;
            if project.endpoint_path().exists() && ping_server(&project).is_ok() {
                stop_server(&project)?;
            }
            if project.state_dir.exists() {
                fs::remove_dir_all(&project.state_dir)?;
                println!("deleted session {name}");
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("session '{name}' does not exist"),
                ));
            }
            Ok(())
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            println!("Usage: spindle session <list|attach|stop|delete> [name]");
            Ok(())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: spindle session <list|attach|stop|delete> [name]",
        )),
    }
}

fn validate_session_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid session name: {name}"),
        ));
    }
    Ok(())
}

fn selected_session_from_environment() -> Option<String> {
    selected_session_from_values(
        std::env::var("SPINDLE_SESSION").ok().as_deref(),
        std::env::var("HERDR_SESSION").ok().as_deref(),
    )
}

fn selected_session_from_values(spindle: Option<&str>, herdr: Option<&str>) -> Option<String> {
    spindle
        .filter(|name| !name.trim().is_empty())
        .or_else(|| herdr.filter(|name| !name.trim().is_empty()))
        .map(str::to_owned)
}

fn stop_server(project: &Project) -> io::Result<()> {
    let response = send_command(project, "stop_server")?;
    if !response.ok {
        let error = response
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "server rejected the stop request".into());
        return Err(io::Error::other(error));
    }
    wait_for_server_stop(project)?;
    println!("server stopped");
    Ok(())
}

fn run_config_command(args: &[String]) -> io::Result<()> {
    match args {
        [command] if command == "path" => println!("{}", crate::config::path().display()),
        [command] if command == "default" => print!("{}", crate::config::default_document()),
        [command] if command == "check" => check_config()?,
        [command] if command == "reset-keys" => reset_config_keys()?,
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            println!("Usage: spindle config <path|default|check|reset-keys>");
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle config <path|default|check|reset-keys>",
            ));
        }
    }
    Ok(())
}

fn check_config() -> io::Result<()> {
    let path = crate::config::path();
    let diagnostics = crate::config::check(&path)?;
    if diagnostics.is_empty() {
        println!("config: ok");
    } else {
        println!("config: issues found");
        for diagnostic in diagnostics {
            println!("{diagnostic}");
        }
        return Err(io::Error::other("config check failed"));
    }
    Ok(())
}

fn reset_config_keys() -> io::Result<()> {
    let path = crate::config::path();
    if !path.exists() {
        println!(
            "No config file found at {}. Built-in keybindings already apply.",
            path.display()
        );
        return Ok(());
    }
    let content = fs::read_to_string(&path)?;
    if content.parse::<toml::Value>().is_err() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("config file at {} is invalid TOML", path.display()),
        ));
    }
    let (updated, removed) = crate::config::remove_keybinding_config_sections(&content);
    if !removed {
        println!(
            "No [keys] config found in {}. Built-in keybindings already apply.",
            path.display()
        );
        return Ok(());
    }
    if updated.parse::<toml::Value>().is_err() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "removing keybinding config would make the config invalid TOML",
        ));
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = path.with_file_name(format!(
        "{}.bak-keybind-v2-{timestamp}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.toml")
    ));
    fs::copy(&path, &backup)?;
    fs::write(&path, updated)?;
    println!("Created backup: {}", backup.display());
    println!(
        "Removed [keys] and [[keys.command]] from {}.",
        path.display()
    );
    println!("Built-in keybindings will apply after restart or config reload.");
    Ok(())
}

struct Project {
    directory: PathBuf,
    state_dir: PathBuf,
    id: String,
}

impl Project {
    fn from_current_dir() -> io::Result<Self> {
        Self::from_current_dir_named(None)
    }

    fn from_current_dir_named(session_name: Option<&str>) -> io::Result<Self> {
        let directory = env::current_dir()?.canonicalize()?;
        let id = project_id(&directory);
        let mut state_dir = state_root()?.join("projects").join(&id);
        if let Some(name) = session_name {
            validate_session_name(name)?;
            state_dir = state_dir.join("sessions").join(name);
        }
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
    use super::{
        endpoint_status_label, project_id, selected_session_from_values, socket_override,
        validate_session_name,
    };
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

    #[test]
    fn session_names_are_safe_path_components() {
        assert!(validate_session_name("review-1").is_ok());
        assert!(validate_session_name("feature\\work").is_err());
        assert!(validate_session_name("..").is_err());
    }

    #[test]
    fn session_environment_prefers_spindle_and_falls_back_to_herdr() {
        assert_eq!(
            selected_session_from_values(None, Some("legacy")).as_deref(),
            Some("legacy")
        );
        assert_eq!(
            selected_session_from_values(Some("review"), Some("legacy")).as_deref(),
            Some("review")
        );
        assert_eq!(
            selected_session_from_values(Some(" "), Some("legacy")).as_deref(),
            Some("legacy")
        );
    }

    #[test]
    fn socket_environment_prefers_spindle_and_ignores_blank_values() {
        assert_eq!(
            socket_override(Some("pipe-spindle"), Some("pipe-herdr")).as_deref(),
            Some("pipe-spindle")
        );
        assert_eq!(
            socket_override(Some(" "), Some("pipe-herdr")).as_deref(),
            Some("pipe-herdr")
        );
        assert_eq!(socket_override(None, Some(" ")), None);
    }
}
