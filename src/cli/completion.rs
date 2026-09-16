use std::io;

const SHELLS: [&str; 5] = ["bash", "elvish", "fish", "powershell", "zsh"];

pub(super) fn run(args: &[String]) -> io::Result<()> {
    match args {
        [shell] if SHELLS.contains(&shell.as_str()) => {
            print!("{}", script(shell));
            Ok(())
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle completion <bash|elvish|fish|powershell|zsh>",
            ))
        }
    }
}

fn script(shell: &str) -> &'static str {
    match shell {
        "bash" => BASH,
        "elvish" => ELVISH,
        "fish" => FISH,
        "powershell" => POWERSHELL,
        "zsh" => ZSH,
        _ => unreachable!("shell was validated by run"),
    }
}

fn print_help() {
    eprintln!("usage: spindle completion <{}>", SHELLS.join("|"));
}

const BASH: &str = r#"_spindle() {
  local cur prev
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  if (( COMP_CWORD == 1 )); then
    COMPREPLY=( $(compgen -W "start attach stop list status doctor config workspace worktree tab pane agent completion api help" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "completion" ]]; then
    COMPREPLY=( $(compgen -W "bash elvish fish powershell zsh" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "workspace" ]]; then
    if [[ "$COMP_WORDS[2]" == "create" ]]; then
      COMPREPLY=( $(compgen -W "--cwd --label --env --focus --no-focus" -- "$cur") )
    elif [[ "$COMP_WORDS[2]" == "report-metadata" ]]; then
      COMPREPLY=( $(compgen -W "--source --token --clear-token --ttl-ms --seq" -- "$cur") )
    elif [[ "$COMP_WORDS[2]" == "close" ]]; then
      COMPREPLY=( $(compgen -W "--group" -- "$cur") )
    else
      COMPREPLY=( $(compgen -W "list create get focus report-metadata move rename close" -- "$cur") )
    fi
  elif [[ "$COMP_WORDS[1]" == "tab" ]]; then
    if [[ "$COMP_WORDS[2]" == "list" ]]; then
      COMPREPLY=( $(compgen -W "--workspace" -- "$cur") )
    elif [[ "$COMP_WORDS[2]" == "create" ]]; then
      COMPREPLY=( $(compgen -W "--label --workspace --cwd --env --focus --no-focus" -- "$cur") )
    else
      COMPREPLY=( $(compgen -W "list create get focus move rename close" -- "$cur") )
    fi
  elif [[ "$COMP_WORDS[1]" == "worktree" ]]; then
    if [[ "$COMP_WORDS[2]" == "list" ]]; then
      COMPREPLY=( $(compgen -W "--workspace --cwd" -- "$cur") )
    elif [[ "$COMP_WORDS[2]" == "create" ]]; then
      COMPREPLY=( $(compgen -W "--workspace --cwd --branch --base --path --label --focus --no-focus" -- "$cur") )
    elif [[ "$COMP_WORDS[2]" == "open" ]]; then
      COMPREPLY=( $(compgen -W "--workspace --cwd --path --branch --label --focus --no-focus" -- "$cur") )
    else
      COMPREPLY=( $(compgen -W "list create open remove help" -- "$cur") )
    fi
  elif [[ "$COMP_WORDS[1]" == "api" ]]; then
    COMPREPLY=( $(compgen -W "schema help" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "agent" ]]; then
    COMPREPLY=( $(compgen -W "list get focus start wait read send-keys prompt rename help" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "pane" ]]; then
    COMPREPLY=( $(compgen -W "list current get focus neighbor edges layout process-info input rename stop restart zoom close send-text send-keys run read swap move report-agent report-agent-session report-metadata release-agent wait-output split resize" -- "$cur") )
  fi
}
complete -F _spindle spindle
"#;

const FISH: &str = r#"complete -c spindle -f -n '__fish_use_subcommand' -a 'start attach stop list status doctor config workspace worktree tab pane agent completion api help'
complete -c spindle -f -n '__fish_seen_subcommand_from completion' -a 'bash elvish fish powershell zsh'
complete -c spindle -f -n '__fish_seen_subcommand_from workspace' -a 'list create get focus report-metadata move rename close'
complete -c spindle -f -n '__fish_seen_subcommand_from worktree' -a 'list create open remove help'
complete -c spindle -f -n '__fish_seen_subcommand_from worktree; and __fish_seen_subcommand_from list' -l workspace -r -l cwd -r
complete -c spindle -f -n '__fish_seen_subcommand_from worktree; and __fish_seen_subcommand_from create' -l workspace -r -l cwd -r -l branch -r -l base -r -l path -r -l label -r -l focus -l no-focus
complete -c spindle -f -n '__fish_seen_subcommand_from worktree; and __fish_seen_subcommand_from open' -l workspace -r -l cwd -r -l path -r -l branch -r -l label -r -l focus -l no-focus
complete -c spindle -f -n '__fish_seen_subcommand_from worktree; and __fish_seen_subcommand_from remove' -l workspace -r -l force
complete -c spindle -f -n '__fish_seen_subcommand_from tab' -a 'list create get focus move rename close'
complete -c spindle -f -n '__fish_seen_subcommand_from workspace; and __fish_seen_subcommand_from create' -l cwd -r
complete -c spindle -f -n '__fish_seen_subcommand_from workspace; and __fish_seen_subcommand_from create' -l label -r
complete -c spindle -f -n '__fish_seen_subcommand_from workspace; and __fish_seen_subcommand_from report-metadata' -l source -r -l token -r -l clear-token -r -l ttl-ms -r -l seq -r
complete -c spindle -f -n '__fish_seen_subcommand_from workspace; and __fish_seen_subcommand_from create' -l env -r
complete -c spindle -f -n '__fish_seen_subcommand_from workspace; and __fish_seen_subcommand_from create' -l focus -l no-focus
complete -c spindle -f -n '__fish_seen_subcommand_from workspace; and __fish_seen_subcommand_from close' -l group
complete -c spindle -f -n '__fish_seen_subcommand_from tab; and __fish_seen_subcommand_from create' -l workspace -r
complete -c spindle -f -n '__fish_seen_subcommand_from tab; and __fish_seen_subcommand_from create' -l label -r
complete -c spindle -f -n '__fish_seen_subcommand_from tab; and __fish_seen_subcommand_from create' -l cwd -r
complete -c spindle -f -n '__fish_seen_subcommand_from tab; and __fish_seen_subcommand_from create' -l env -r
complete -c spindle -f -n '__fish_seen_subcommand_from tab; and __fish_seen_subcommand_from create' -l focus -l no-focus
complete -c spindle -f -n '__fish_seen_subcommand_from tab; and __fish_seen_subcommand_from list' -l workspace -r
complete -c spindle -f -n '__fish_seen_subcommand_from pane' -a 'list current get focus neighbor edges layout process-info input rename stop restart zoom close send-text send-keys run read swap move report-agent report-agent-session report-metadata release-agent wait-output split resize'
complete -c spindle -f -n '__fish_seen_subcommand_from api' -a 'schema help'
complete -c spindle -f -n '__fish_seen_subcommand_from agent' -a 'list get focus start wait read send-keys prompt rename help'
"#;

const ZSH: &str = r#"#compdef spindle
_spindle() {
  _arguments '1:command:(start attach stop list status doctor config workspace worktree tab pane agent completion api help)' '*::argument:->args'
  case $words[2] in
    completion) _arguments '1:shell:(bash elvish fish powershell zsh)' ;;
    workspace) _arguments '1:command:(list create get focus report-metadata move rename close)' '2:options:(--cwd --label --env --focus --no-focus --group --source --token --clear-token --ttl-ms --seq)' ;;
    worktree) _arguments '1:command:(list create open remove help)' '2:options:(--workspace --cwd --branch --base --path --label --focus --no-focus --force)' ;;
    tab) _arguments '1:command:(list create get focus move rename close)' '2:options:(--label --workspace --cwd --env --focus --no-focus)' ;;
    pane) _arguments '1:command:(list current get focus neighbor edges layout process-info input rename stop restart zoom close send-text send-keys run read swap move report-agent report-agent-session report-metadata release-agent wait-output split resize)' ;;
    api) _arguments '1:command:(schema help)' ;;
    agent) _arguments '1:command:(list get focus start wait read send-keys prompt rename help)' ;;
  esac
}
_spindle "$@"
"#;

const POWERSHELL: &str = r#"Register-ArgumentCompleter -Native -CommandName spindle -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)
  $words = $commandAst.ToString().Split(' ', [System.StringSplitOptions]::RemoveEmptyEntries)
  $choices = if ($words.Count -le 1) { 'start attach stop list status doctor config workspace worktree tab pane agent completion api help' }
    elseif ($words[1] -eq 'completion') { 'bash elvish fish powershell zsh' }
    elseif ($words[1] -eq 'workspace' -and $words[2] -eq 'create') { '--cwd --label --env --focus --no-focus' }
    elseif ($words[1] -eq 'workspace' -and $words[2] -eq 'report-metadata') { '--source --token --clear-token --ttl-ms --seq' }
    elseif ($words[1] -eq 'workspace' -and $words[2] -eq 'close') { '--group' }
    elseif ($words[1] -eq 'workspace') { 'list create get focus report-metadata move rename close --source --token --clear-token --ttl-ms --seq' }
    elseif ($words[1] -eq 'worktree') { 'list create open remove help --workspace --cwd --branch --base --path --label --focus --no-focus --force' }
    elseif ($words[1] -eq 'tab' -and $words[2] -eq 'list') { '--workspace' }
    elseif ($words[1] -eq 'tab' -and $words[2] -eq 'create') { '--label --workspace --cwd --env --focus --no-focus' }
    elseif ($words[1] -eq 'tab') { 'list create get focus move rename close' }
    elseif ($words[1] -eq 'pane') { 'list current get focus neighbor edges layout process-info input rename stop restart zoom close send-text send-keys run read swap move report-agent report-agent-session report-metadata release-agent wait-output split resize' }
    elseif ($words[1] -eq 'agent') { 'list get focus start wait read send-keys prompt rename help' }
  $choices | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
}
"#;

const ELVISH: &str = r#"# Add to ~/.elvish/rc.elv:
edit:completion:argadd spindle (start attach stop list status doctor config workspace worktree tab pane agent completion api help)
edit:completion:argadd 'spindle workspace' (list create get focus report-metadata move rename close)
edit:completion:argadd 'spindle workspace close' (--group)
edit:completion:argadd 'spindle worktree' (list create open remove help)
edit:completion:argadd 'spindle worktree list' (--workspace --cwd)
edit:completion:argadd 'spindle worktree create' (--workspace --cwd --branch --base --path --label --focus --no-focus)
edit:completion:argadd 'spindle worktree open' (--workspace --cwd --path --branch --label --focus --no-focus)
edit:completion:argadd 'spindle worktree remove' (--workspace --force)
edit:completion:argadd 'spindle workspace create' (--cwd --label --env --focus --no-focus)
edit:completion:argadd 'spindle workspace report-metadata' (--source --token --clear-token --ttl-ms --seq)
edit:completion:argadd 'spindle tab list' (--workspace)
edit:completion:argadd 'spindle tab create' (--label --workspace --cwd --env --focus --no-focus)
edit:completion:argadd 'spindle tab' (list create get focus move rename close)
edit:completion:argadd 'spindle pane' (list current get focus neighbor edges layout process-info input rename stop restart zoom close send-text send-keys run read swap move report-agent report-agent-session report-metadata release-agent wait-output split resize)
edit:completion:argadd 'spindle agent' (list get focus start wait read send-keys prompt rename help)
"#;

#[cfg(test)]
mod tests {
    use super::script;

    #[test]
    fn completion_scripts_expose_spindle_commands() {
        for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
            let output = script(shell);
            assert!(output.contains("workspace"));
            assert!(output.contains("status"));
            assert!(output.contains("pane"));
            assert!(output.contains("wait-output"));
            assert!(output.contains("tab"));
            assert!(output.contains("neighbor"));
            assert!(output.contains("edges"));
            assert!(output.contains("layout"));
            assert!(output.contains("process-info"));
            assert!(output.contains("input"));
            assert!(output.contains("no-focus"));
        }
    }
}
