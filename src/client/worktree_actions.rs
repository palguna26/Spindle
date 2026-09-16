use super::ClientError;

pub(super) fn run<I, S>(args: I) -> Result<(), ClientError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let executable = std::env::current_exe().map_err(ClientError::Io)?;
    let output = std::process::Command::new(executable)
        .args(command_args(args))
        .output()
        .map_err(ClientError::Io)?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(ClientError::Server(if message.is_empty() {
        "worktree command failed".into()
    } else {
        message
    }))
}

fn command_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    std::iter::once("worktree".to_owned())
        .chain(args.into_iter().map(|arg| arg.as_ref().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::command_args;

    #[test]
    fn prefixes_worktree_subcommands_for_the_spindle_cli() {
        assert_eq!(
            command_args(["create", "--branch", "feature"]),
            vec!["worktree", "create", "--branch", "feature"]
        );
    }
}
