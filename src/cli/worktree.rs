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

#[derive(Debug, Default)]
struct WorktreeOptions {
    workspace_id: Option<String>,
    cwd: Option<PathBuf>,
    branch: Option<String>,
    base: Option<String>,
    path: Option<PathBuf>,
    label: Option<String>,
    focus: bool,
    force: bool,
}

pub(super) fn run_worktree_command(project: &Project, args: &[String]) -> io::Result<()> {
    match args {
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        [command, options @ ..] if command == "list" => worktree_list(project, options),
        [command, options @ ..] if command == "create" => worktree_create(project, options),
        [command, options @ ..] if command == "open" => worktree_open(project, options),
        [command, options @ ..] if command == "remove" => worktree_remove(project, options),
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle worktree <list|create|open|remove> ...",
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

fn worktree_create(project: &Project, args: &[String]) -> io::Result<()> {
    let options = parse_worktree_options(args, false)?;
    let (root, snapshot) = worktree_root(project, options.workspace_id.as_deref(), options.cwd)?;
    let branch = options.branch.unwrap_or_else(|| {
        format!(
            "worktree/{:x}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )
    });
    let base = options.base.unwrap_or_else(|| {
        git_output(&root, ["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "HEAD".into())
    });
    let path = options.path.unwrap_or_else(|| {
        root.parent().unwrap_or(&root).join(format!(
            "{}-{}",
            repo_name(&root),
            branch_to_slug(&branch)
        ))
    });
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("worktree path already exists: {}", path.display()),
        ));
    }
    let path_string = path.to_string_lossy().into_owned();
    git_run_vec(
        &root,
        &["worktree", "add", "-b", &branch, &path_string, &base],
    )?;
    let result = open_workspace(
        project,
        snapshot.as_ref(),
        &path,
        Some(&branch),
        options.label.as_deref(),
        options.focus,
    );
    if result.is_err() {
        let _ = git_run_vec(&root, &["worktree", "remove", "--force", &path_string]);
    }
    result
}

fn worktree_open(project: &Project, args: &[String]) -> io::Result<()> {
    let options = parse_worktree_options(args, true)?;
    let (root, snapshot) = worktree_root(project, options.workspace_id.as_deref(), options.cwd)?;
    let records = parse_porcelain(&git_output(&root, ["worktree", "list", "--porcelain"])?)?;
    let record = records
        .into_iter()
        .find(|record| {
            options
                .path
                .as_ref()
                .is_some_and(|path| same_path(Path::new(&record.path), path))
                || options.branch.as_deref() == record.branch.as_deref()
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "worktree was not found"))?;
    open_workspace(
        project,
        snapshot.as_ref(),
        Path::new(&record.path),
        record.branch.as_deref(),
        options.label.as_deref(),
        options.focus,
    )
}

fn worktree_remove(project: &Project, args: &[String]) -> io::Result<()> {
    let options = parse_worktree_options(args, false)?;
    let workspace_id = options.workspace_id.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: spindle worktree remove --workspace ID [--force]",
        )
    })?;
    let (root, snapshot) = worktree_root(project, Some(&workspace_id), None)?;
    let workspace = snapshot
        .as_ref()
        .and_then(|snapshot| find_workspace(snapshot, &workspace_id))
        .ok_or_else(|| io::Error::other("workspace was not found"))?;
    let path = workspace
        .repository_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("workspace has no repository path"))?;
    if same_path(&path, &root) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot remove the source checkout as a worktree",
        ));
    }
    let mut remove_args = vec!["worktree", "remove"];
    if options.force {
        remove_args.push("--force");
    }
    let path_string = path.to_string_lossy().into_owned();
    remove_args.push(&path_string);
    git_run_vec(&root, &remove_args)?;
    let response = super::send_command_with_payload(
        project,
        "delete_workspace",
        serde_json::json!({ "id": workspace_id }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "workspace removal failed".into()),
        ));
    }
    println!("removed worktree: {}", path.display());
    Ok(())
}

fn parse_worktree_options(args: &[String], require_target: bool) -> io::Result<WorktreeOptions> {
    let mut options = WorktreeOptions::default();
    let mut index = 0;
    while index < args.len() {
        let option = args[index].as_str();
        let value = |index: &mut usize| {
            args.get(*index + 1).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("missing value for {option}"),
                )
            })
        };
        match option {
            "--workspace" => options.workspace_id = Some(value(&mut index)?.clone()),
            "--cwd" => options.cwd = Some(PathBuf::from(value(&mut index)?)),
            "--branch" => options.branch = Some(value(&mut index)?.clone()),
            "--base" => options.base = Some(value(&mut index)?.clone()),
            "--path" => options.path = Some(PathBuf::from(value(&mut index)?)),
            "--label" => options.label = Some(value(&mut index)?.clone()),
            "--focus" => options.focus = true,
            "--no-focus" => options.focus = false,
            "--force" => options.force = true,
            "--trust-repository" | "--json" => {}
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option: {other}"),
                ));
            }
        }
        index += if matches!(
            option,
            "--workspace" | "--cwd" | "--branch" | "--base" | "--path" | "--label"
        ) {
            2
        } else {
            1
        };
    }
    if options.workspace_id.is_some() && options.cwd.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "use either --workspace or --cwd",
        ));
    }
    if require_target && options.path.is_some() == options.branch.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "worktree open requires exactly one of --path or --branch",
        ));
    }
    Ok(options)
}

fn worktree_root(
    project: &Project,
    workspace_id: Option<&str>,
    cwd: Option<PathBuf>,
) -> io::Result<(PathBuf, Option<SessionSnapshot>)> {
    if workspace_id.is_some() && super::ping_server(project).is_err() {
        super::start_server(project)?;
    }
    let snapshot = if super::ping_server(project).is_ok() {
        super::send_command(project, "get_snapshot")?
            .payload
            .and_then(|payload| serde_json::from_value::<SessionSnapshot>(payload).ok())
    } else {
        None
    };
    let root = if let Some(workspace_id) = workspace_id {
        find_workspace(
            snapshot
                .as_ref()
                .ok_or_else(|| io::Error::other("server returned no session snapshot"))?,
            workspace_id,
        )
        .and_then(|workspace| workspace.repository_path.clone())
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::other(format!("workspace '{workspace_id}' has no repository path"))
        })?
    } else {
        cwd.or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| io::Error::other("could not determine repository path"))?
    };
    Ok((root, snapshot))
}

fn find_workspace<'a>(
    snapshot: &'a SessionSnapshot,
    workspace_id: &str,
) -> Option<&'a crate::server::session::WorkspaceView> {
    snapshot
        .spaces
        .iter()
        .flat_map(|space| space.workspaces.iter())
        .find(|workspace| workspace.workspace_id == workspace_id)
}

fn open_workspace(
    project: &Project,
    snapshot: Option<&SessionSnapshot>,
    path: &Path,
    branch: Option<&str>,
    label: Option<&str>,
    focus: bool,
) -> io::Result<()> {
    if super::ping_server(project).is_err() {
        super::start_server(project)?;
    }
    let previous_workspace_id = snapshot.and_then(|snapshot| {
        snapshot
            .spaces
            .iter()
            .find(|space| space.space_id == snapshot.active_space_id)
            .and_then(|space| space.active_workspace_id.clone())
    });
    let workspace_name = label
        .map(str::to_owned)
        .or_else(|| branch.map(|branch| branch.trim_start_matches("worktree/").to_owned()))
        .or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Worktree".into());
    let response = super::send_command_with_payload(
        project,
        "create_workspace",
        serde_json::json!({
            "name": workspace_name,
            "repository_path": path,
            "branch": branch,
        }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "workspace creation failed".into()),
        ));
    }
    let pane = super::send_command_with_payload(
        project,
        "ensure_active_pane",
        serde_json::json!({
            "command": "powershell.exe",
            "args": ["-NoLogo", "-NoProfile"],
            "cwd": path,
            "cols": 80,
            "rows": 24,
        }),
    )?;
    if !pane.ok {
        return Err(io::Error::other(
            "workspace was created but its shell could not start",
        ));
    }
    if !focus {
        if let Some(previous_workspace_id) = previous_workspace_id {
            let restore = super::send_command_with_payload(
                project,
                "focus_workspace",
                serde_json::json!({ "id": previous_workspace_id }),
            )?;
            if !restore.ok {
                return Err(io::Error::other(
                    "worktree opened but previous workspace could not be restored",
                ));
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "path": path,
            "branch": branch,
            "workspace": response.payload,
            "pane": pane.payload,
        }))
        .map_err(io::Error::other)?
    );
    Ok(())
}

fn repo_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo")
        .to_owned()
}

fn branch_to_slug(branch: &str) -> String {
    let slug: String = branch
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug.trim_matches('-').to_owned()
}

fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> io::Result<String> {
    git_run_vec(cwd, &args).map(|output| output.trim().to_owned())
}

fn git_run_vec(cwd: &Path, args: &[&str]) -> io::Result<String> {
    let output = Command::new("git").arg("-C").arg(cwd).args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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
    eprintln!("  spindle worktree create [--workspace ID | --cwd PATH] [--branch NAME] [--base REF] [--path PATH] [--label TEXT] [--focus|--no-focus]");
    eprintln!("  spindle worktree open [--workspace ID | --cwd PATH] (--path PATH | --branch NAME) [--label TEXT] [--focus|--no-focus]");
    eprintln!("  spindle worktree remove --workspace ID [--force]");
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

    #[test]
    fn worktree_open_requires_one_target() {
        let args = vec!["--label".into(), "feature".into()];
        let error = super::parse_worktree_options(&args, true).unwrap_err();
        assert_eq!(
            error.to_string(),
            "worktree open requires exactly one of --path or --branch"
        );
    }

    #[test]
    fn worktree_options_reject_workspace_and_cwd_together() {
        let args = vec![
            "--workspace".into(),
            "workspace-1".into(),
            "--cwd".into(),
            "C:/repo".into(),
        ];
        let error = super::parse_worktree_options(&args, false).unwrap_err();
        assert_eq!(error.to_string(), "use either --workspace or --cwd");
    }
}
