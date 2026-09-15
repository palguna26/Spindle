use super::Project;
use crate::server::session::SessionSnapshot;
use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize)]
struct WorktreeSource {
    repo_root: String,
    source_checkout_path: String,
    source_workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct WorktreeInfo {
    path: String,
    branch: Option<String>,
    is_bare: bool,
    is_detached: bool,
    is_prunable: bool,
    is_linked_worktree: bool,
    open_workspace_id: Option<String>,
    label: String,
}

#[derive(Debug, Serialize)]
struct WorktreeList {
    source: WorktreeSource,
    worktrees: Vec<WorktreeInfo>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedWorktree {
    path: String,
    branch: Option<String>,
    is_bare: bool,
    is_detached: bool,
    is_prunable: bool,
}

pub(super) fn run_worktree_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        [command, options @ ..] if command == "list" => worktree_list(project, options),
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle worktree list [--workspace ID | --cwd PATH]",
            ))
        }
    }
}

fn worktree_list(project: &Project, args: &[String]) -> io::Result<()> {
    let mut workspace_id = None;
    let mut cwd = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "missing value for --workspace",
                    ));
                };
                workspace_id = Some(value.clone());
                index += 2;
            }
            "--cwd" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "missing value for --cwd",
                    ));
                };
                cwd = Some(PathBuf::from(value));
                index += 2;
            }
            "--json" => index += 1,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option: {other}"),
                ));
            }
        }
    }
    if workspace_id.is_some() && cwd.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: spindle worktree list [--workspace ID | --cwd PATH]",
        ));
    }

    let snapshot = if let Some(workspace_id) = workspace_id {
        if super::ping_server(project).is_err() {
            super::start_server(project)?;
        }
        let payload = super::send_command(project, "get_snapshot")?
            .payload
            .ok_or_else(|| io::Error::other("server returned no session snapshot"))?;
        let snapshot: SessionSnapshot =
            serde_json::from_value(payload).map_err(io::Error::other)?;
        let workspace = snapshot
            .spaces
            .iter()
            .flat_map(|space| space.workspaces.iter())
            .find(|workspace| workspace.workspace_id == workspace_id)
            .ok_or_else(|| {
                io::Error::other(format!("workspace '{workspace_id}' does not exist"))
            })?;
        let repository_path = workspace
            .repository_path
            .clone()
            .ok_or_else(|| io::Error::other("workspace has no repository path"))?;
        Some((snapshot, PathBuf::from(repository_path)))
    } else if super::ping_server(project).is_ok() {
        let payload = super::send_command(project, "get_snapshot")?.payload;
        payload
            .and_then(|payload| serde_json::from_value::<SessionSnapshot>(payload).ok())
            .map(|snapshot| (snapshot, PathBuf::new()))
    } else {
        None
    };
    let root = cwd
        .or_else(|| {
            snapshot
                .as_ref()
                .and_then(|(_, path)| (!path.as_os_str().is_empty()).then_some(path.clone()))
        })
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| io::Error::other("could not determine repository path"))?;
    let repo_root = git_output(&root, ["rev-parse", "--show-toplevel"])?;
    let source_checkout_path = git_output(&root, ["rev-parse", "--show-toplevel"])?;
    let records = parse_porcelain(&git_output(&root, ["worktree", "list", "--porcelain"])?)?;
    let source_workspace_id = snapshot.as_ref().and_then(|(snapshot, _)| {
        snapshot
            .spaces
            .iter()
            .flat_map(|space| space.workspaces.iter())
            .find(|workspace| {
                workspace.repository_path.as_deref().is_some_and(|path| {
                    same_path(Path::new(path), Path::new(&source_checkout_path))
                })
            })
            .map(|workspace| workspace.workspace_id.clone())
    });
    let worktrees = records
        .into_iter()
        .map(|record| {
            let open_workspace_id = snapshot.as_ref().and_then(|(snapshot, _)| {
                snapshot
                    .spaces
                    .iter()
                    .flat_map(|space| space.workspaces.iter())
                    .find(|workspace| {
                        workspace
                            .repository_path
                            .as_deref()
                            .is_some_and(|path| same_path(Path::new(path), Path::new(&record.path)))
                    })
                    .map(|workspace| workspace.workspace_id.clone())
            });
            WorktreeInfo {
                label: Path::new(&record.path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&record.path)
                    .to_owned(),
                is_linked_worktree: !same_path(
                    Path::new(&record.path),
                    Path::new(&source_checkout_path),
                ),
                path: record.path,
                branch: record.branch,
                is_bare: record.is_bare,
                is_detached: record.is_detached,
                is_prunable: record.is_prunable,
                open_workspace_id,
            }
        })
        .collect();
    let result = WorktreeList {
        source: WorktreeSource {
            repo_root,
            source_checkout_path,
            source_workspace_id,
        },
        worktrees,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(io::Error::other)?
    );
    Ok(())
}

fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> io::Result<String> {
    let output = Command::new("git").arg("-C").arg(cwd).args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn parse_porcelain(output: &str) -> io::Result<Vec<ParsedWorktree>> {
    let mut records = Vec::new();
    let mut current = ParsedWorktree::default();
    let mut has_record = false;
    for line in output.lines().chain([""]) {
        if line.is_empty() {
            if has_record {
                if current.path.is_empty() {
                    return Err(io::Error::other("git returned a worktree without a path"));
                }
                records.push(current);
                current = ParsedWorktree::default();
                has_record = false;
            }
            continue;
        }
        has_record = true;
        if let Some(path) = line.strip_prefix("worktree ") {
            current.path = path.to_owned();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current.branch = branch.strip_prefix("refs/heads/").map(str::to_owned);
        } else if line == "bare" {
            current.is_bare = true;
        } else if line == "detached" {
            current.is_detached = true;
        } else if line.starts_with("prunable") {
            current.is_prunable = true;
        }
    }
    Ok(records)
}

fn same_path(left: &Path, right: &Path) -> bool {
    std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf())
        == std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf())
}

fn print_help() {
    eprintln!("spindle worktree commands:");
    eprintln!("  spindle worktree list [--workspace ID | --cwd PATH]");
}

#[cfg(test)]
mod tests {
    use super::{parse_porcelain, ParsedWorktree};

    #[test]
    fn parses_herdr_worktree_porcelain() {
        let records = parse_porcelain(
            "worktree C:/repo\nHEAD abc\nbranch refs/heads/main\n\nworktree C:/feature\nHEAD def\ndetached\n\n",
        )
        .unwrap();
        assert_eq!(
            records,
            vec![
                ParsedWorktree {
                    path: "C:/repo".into(),
                    branch: Some("main".into()),
                    ..Default::default()
                },
                ParsedWorktree {
                    path: "C:/feature".into(),
                    is_detached: true,
                    ..Default::default()
                }
            ]
        );
    }
}
