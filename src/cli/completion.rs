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
    COMPREPLY=( $(compgen -W "start attach stop list doctor config workspace tab pane completion help" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "completion" ]]; then
    COMPREPLY=( $(compgen -W "bash elvish fish powershell zsh" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "workspace" ]]; then
    COMPREPLY=( $(compgen -W "list get focus rename" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "tab" ]]; then
    COMPREPLY=( $(compgen -W "list create get focus rename close" -- "$cur") )
  elif [[ "$COMP_WORDS[1]" == "pane" ]]; then
    COMPREPLY=( $(compgen -W "list current get focus rename stop restart zoom close send-text send-keys run read swap split resize" -- "$cur") )
  fi
}
complete -F _spindle spindle
"#;

const FISH: &str = r#"complete -c spindle -f -n '__fish_use_subcommand' -a 'start attach stop list doctor config workspace tab pane completion help'
complete -c spindle -f -n '__fish_seen_subcommand_from completion' -a 'bash elvish fish powershell zsh'
complete -c spindle -f -n '__fish_seen_subcommand_from workspace' -a 'list get focus rename'
complete -c spindle -f -n '__fish_seen_subcommand_from tab' -a 'list create get focus rename close'
complete -c spindle -f -n '__fish_seen_subcommand_from pane' -a 'list current get focus rename stop restart zoom close send-text send-keys run read swap split resize'
"#;

const ZSH: &str = r#"#compdef spindle
_spindle() {
  _arguments '1:command:(start attach stop list doctor config workspace tab pane completion help)' '*::argument:->args'
  case $words[2] in
    completion) _arguments '1:shell:(bash elvish fish powershell zsh)' ;;
    workspace) _arguments '1:command:(list get focus rename)' ;;
    tab) _arguments '1:command:(list create get focus rename close)' ;;
    pane) _arguments '1:command:(list current get focus rename stop restart zoom close send-text send-keys run read swap split resize)' ;;
  esac
}
_spindle "$@"
"#;

const POWERSHELL: &str = r#"Register-ArgumentCompleter -Native -CommandName spindle -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)
  $words = $commandAst.ToString().Split(' ', [System.StringSplitOptions]::RemoveEmptyEntries)
  $choices = if ($words.Count -le 1) { 'start attach stop list doctor config workspace tab pane completion help' }
    elseif ($words[1] -eq 'completion') { 'bash elvish fish powershell zsh' }
    elseif ($words[1] -eq 'workspace') { 'list get focus rename' }
    elseif ($words[1] -eq 'tab') { 'list create get focus rename close' }
    elseif ($words[1] -eq 'pane') { 'list current get focus rename stop restart zoom close send-text send-keys run read swap split resize' }
  $choices | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
}
"#;

const ELVISH: &str = r#"# Add to ~/.elvish/rc.elv:
edit:completion:argadd spindle (start attach stop list doctor config workspace tab pane completion help)
"#;

#[cfg(test)]
mod tests {
    use super::script;

    #[test]
    fn completion_scripts_expose_spindle_commands() {
        for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
            let output = script(shell);
            assert!(output.contains("workspace"));
            assert!(output.contains("pane"));
        }
    }
}
