use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Build a plugin command with the same Windows resolution rules as Herdr.
/// In particular, batch files must be launched through `ComSpec`.
pub(crate) fn command_for_argv_in_dir(program: &str, args: &[String], cwd: &Path) -> Command {
    let program = program_for_cwd(program, cwd);
    let mut command = command_for_program(program.as_os_str());
    command.args(args).current_dir(cwd);
    command
}

fn program_for_cwd(program: &str, cwd: &Path) -> PathBuf {
    let path = Path::new(program);
    let has_separator = program.contains('/') || (cfg!(windows) && program.contains('\\'));
    if path.is_relative() && (has_separator || (cfg!(windows) && cwd.join(path).is_file())) {
        let relative = path.strip_prefix(Path::new(".")).unwrap_or(path);
        #[cfg(windows)]
        if is_windows_batch_file_name(path.as_os_str()) {
            return cwd.join(relative);
        }
        #[cfg(windows)]
        let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
        cwd.join(relative)
    } else {
        path.to_path_buf()
    }
}

#[cfg(not(windows))]
fn command_for_program(program: &OsStr) -> Command {
    Command::new(program)
}

#[cfg(windows)]
fn command_for_program(program: &OsStr) -> Command {
    let resolved = resolve_windows_program(program);
    let command_program = resolved.as_ref().map_or_else(
        || program.to_os_string(),
        |path| path.as_os_str().to_os_string(),
    );
    if is_windows_batch_file_name(program)
        || resolved
            .as_ref()
            .is_some_and(|path| is_windows_batch_path(path))
    {
        let shell =
            std::env::var_os("ComSpec").unwrap_or_else(|| r"C:\Windows\System32\cmd.exe".into());
        let mut command = Command::new(shell);
        command.arg("/d").arg("/c").arg(command_program);
        command
    } else {
        Command::new(command_program)
    }
}

#[cfg(windows)]
fn resolve_windows_program(program: &OsStr) -> Option<PathBuf> {
    if has_path_separator(program) {
        return None;
    }
    let path = Path::new(program);
    let path_var = std::env::var_os("PATH")?;
    if path.extension().is_some() {
        return std::env::split_paths(&path_var)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file());
    }
    windows_path_extensions().into_iter().find_map(|extension| {
        std::env::split_paths(&path_var)
            .map(|dir| {
                let mut file_name = program.to_os_string();
                file_name.push(&extension);
                dir.join(file_name)
            })
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(windows)]
fn windows_path_extensions() -> Vec<String> {
    std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(|part| {
                    if part.starts_with('.') {
                        part.to_string()
                    } else {
                        format!(".{part}")
                    }
                })
                .collect()
        })
        .filter(|extensions: &Vec<String>| !extensions.is_empty())
        .unwrap_or_else(|| vec![".COM".into(), ".EXE".into(), ".BAT".into(), ".CMD".into()])
}

#[cfg(windows)]
fn has_path_separator(program: &OsStr) -> bool {
    program.to_string_lossy().contains(['/', '\\'])
}

#[cfg(any(windows, test))]
fn is_windows_batch_file_name(program: &OsStr) -> bool {
    Path::new(program)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(is_windows_batch_extension)
}

#[cfg(windows)]
fn is_windows_batch_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(is_windows_batch_extension)
}

#[cfg(any(windows, test))]
fn is_windows_batch_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_programs_against_plugin_directory() {
        let cwd = Path::new("plugin-root");
        assert_eq!(program_for_cwd("./bin/tool", cwd), cwd.join("bin/tool"));
        assert_eq!(program_for_cwd("tool", cwd), PathBuf::from("tool"));
    }

    #[test]
    fn recognizes_windows_batch_extensions_case_insensitively() {
        assert!(is_windows_batch_file_name(OsStr::new("npm.cmd")));
        assert!(is_windows_batch_file_name(OsStr::new("script.BAT")));
        assert!(!is_windows_batch_file_name(OsStr::new("node.exe")));
    }

    #[cfg(windows)]
    #[test]
    fn runs_a_batch_plugin_command_from_its_working_directory() {
        let root =
            std::env::temp_dir().join(format!("spindle-plugin-command-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("capture.cmd");
        std::fs::write(&path, "@echo off\r\necho plugin-%1\r\n").unwrap();
        let output = command_for_argv_in_dir("./capture.cmd", &["ready".into()], &root)
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "plugin-ready"
        );
    }
}
