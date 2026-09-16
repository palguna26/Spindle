use std::path::Path;
use std::process::Command;

pub(crate) fn branch(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "branch", "--show-current"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!branch.is_empty()).then_some(branch)
}

pub(crate) fn is_linked_worktree(path: &str) -> bool {
    let Ok(output) = Command::new("git")
        .args(["-C", path, "rev-parse", "--git-dir"])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .replace('\\', "/")
            .trim()
            .contains("/.git/worktrees/")
}

pub(crate) fn worktree_group_key(path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", path, "rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let common_dir = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if common_dir.is_empty() {
        return None;
    }
    let common_path = Path::new(&common_dir);
    let common_path = if common_path.is_absolute() {
        common_path.to_owned()
    } else {
        Path::new(path).join(common_path)
    };
    Some(
        common_path
            .canonicalize()
            .unwrap_or(common_path)
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase(),
    )
}

#[cfg(test)]
mod tests {
    use super::{branch, is_linked_worktree, worktree_group_key};
    use std::path::Path;

    #[test]
    fn invalid_repository_paths_are_safe_misses() {
        let path = Path::new("C:\\spindle\\path-that-does-not-exist");
        assert_eq!(branch(path), None);
        assert!(!is_linked_worktree(path.to_str().unwrap()));
        assert_eq!(worktree_group_key(path.to_str().unwrap()), None);
    }
}
