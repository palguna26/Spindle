use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "Spindle";

pub fn run() -> io::Result<()> {
    let command = env::args().nth(1).unwrap_or_else(|| "attach".into());
    let project = Project::from_current_dir()?;

    match command.as_str() {
        "start" | "attach" => {
            project.ensure_state_dir()?;
            println!("{} {}", command, project.describe());
            println!("server lifecycle is not implemented yet");
        }
        "list" => {
            println!("project: {}", project.describe());
            println!("state: {}", project.state_dir.display());
        }
        "doctor" => {
            println!("project: {}", project.describe());
            println!("state directory: {}", project.state_dir.display());
            println!("state directory exists: {}", project.state_dir.exists());
        }
        "stop" => println!("server lifecycle is not implemented yet"),
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
