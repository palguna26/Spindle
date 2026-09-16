use std::io;

pub(super) fn run(args: &[String]) -> io::Result<()> {
    match args {
        [] => crate::integration::run_status(false),
        [command] if command == "status" => crate::integration::run_status(false),
        [command, flag] if command == "status" && flag == "--json" => {
            crate::integration::run_status(true)
        }
        [command, target] if command == "install" && target == "codex" => {
            print_messages(crate::integration::install_codex()?)
        }
        [command, target] if command == "uninstall" && target == "codex" => {
            print_messages(crate::integration::uninstall_codex()?)
        }
        [command, target] if command == "install" && target == "opencode" => {
            print_messages(crate::integration::install_opencode()?)
        }
        [command, target] if command == "uninstall" && target == "opencode" => {
            print_messages(crate::integration::uninstall_opencode()?)
        }
        [command, target] if command == "install" && target == "claude" => {
            print_messages(crate::integration::install_claude()?)
        }
        [command, target] if command == "uninstall" && target == "claude" => {
            print_messages(crate::integration::uninstall_claude()?)
        }
        [command, target] if command == "install" && target == "pi" => {
            print_messages(crate::integration::install_pi()?)
        }
        [command, target] if command == "uninstall" && target == "pi" => {
            print_messages(crate::integration::uninstall_pi()?)
        }
        [command, target] if command == "install" && target == "omp" => {
            print_messages(crate::integration::install_omp()?)
        }
        [command, target] if command == "uninstall" && target == "omp" => {
            print_messages(crate::integration::uninstall_omp()?)
        }
        [command, target] if command == "install" && target == "copilot" => {
            print_messages(crate::integration::install_copilot()?)
        }
        [command, target] if command == "uninstall" && target == "copilot" => {
            print_messages(crate::integration::uninstall_copilot()?)
        }
        [command, target] if command == "install" && target == "cursor" => {
            print_messages(crate::integration::install_cursor()?)
        }
        [command, target] if command == "uninstall" && target == "cursor" => {
            print_messages(crate::integration::uninstall_cursor()?)
        }
        [command, target] if command == "install" && target == "devin" => {
            print_messages(crate::integration::install_devin()?)
        }
        [command, target] if command == "uninstall" && target == "devin" => {
            print_messages(crate::integration::uninstall_devin()?)
        }
        [command, target] if command == "install" && target == "droid" => {
            print_messages(crate::integration::install_droid()?)
        }
        [command, target] if command == "uninstall" && target == "droid" => {
            print_messages(crate::integration::uninstall_droid()?)
        }
        [command, target] if command == "install" && target == "kimi" => {
            print_messages(crate::integration::install_kimi()?)
        }
        [command, target] if command == "uninstall" && target == "kimi" => {
            print_messages(crate::integration::uninstall_kimi()?)
        }
        [command, target] if command == "install" && target == "qodercli" => {
            print_messages(crate::integration::install_qodercli()?)
        }
        [command, target] if command == "uninstall" && target == "qodercli" => {
            print_messages(crate::integration::uninstall_qodercli()?)
        }
        [command, target] if command == "install" && target == "qwen" => {
            print_messages(crate::integration::install_qwen()?)
        }
        [command, target] if command == "uninstall" && target == "qwen" => {
            print_messages(crate::integration::uninstall_qwen()?)
        }
        [command, target] if command == "install" && target == "grok" => {
            print_messages(crate::integration::install_grok()?)
        }
        [command, target] if command == "uninstall" && target == "grok" => {
            print_messages(crate::integration::uninstall_grok()?)
        }
        [command, target] if command == "install" && target == "kilo" => {
            print_messages(crate::integration::install_kilo()?)
        }
        [command, target] if command == "uninstall" && target == "kilo" => {
            print_messages(crate::integration::uninstall_kilo()?)
        }
        [command, target] if command == "install" && target == "hermes" => {
            print_messages(crate::integration::install_hermes()?)
        }
        [command, target] if command == "uninstall" && target == "hermes" => {
            print_messages(crate::integration::uninstall_hermes()?)
        }
        [command, target] if command == "install" && target == "antigravity-cli" => {
            print_messages(crate::integration::install_antigravity_cli()?)
        }
        [command, target] if command == "uninstall" && target == "antigravity-cli" => {
            print_messages(crate::integration::uninstall_antigravity_cli()?)
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle integration <status [--json]|install codex|uninstall codex>",
            ))
        }
    }
}

fn print_help() {
    eprintln!("usage: spindle integration status [--json]");
    eprintln!("       spindle integration install codex");
    eprintln!("       spindle integration uninstall codex");
    eprintln!("       spindle integration install opencode");
    eprintln!("       spindle integration uninstall opencode");
    eprintln!("       spindle integration install claude");
    eprintln!("       spindle integration uninstall claude");
    eprintln!("       spindle integration install pi");
    eprintln!("       spindle integration uninstall pi");
    eprintln!("       spindle integration install omp");
    eprintln!("       spindle integration uninstall omp");
    eprintln!("       spindle integration install copilot");
    eprintln!("       spindle integration uninstall copilot");
    eprintln!("       spindle integration install cursor");
    eprintln!("       spindle integration uninstall cursor");
    eprintln!("       spindle integration install devin");
    eprintln!("       spindle integration uninstall devin");
    eprintln!("       spindle integration install droid");
    eprintln!("       spindle integration uninstall droid");
    eprintln!("       spindle integration install kimi");
    eprintln!("       spindle integration uninstall kimi");
    eprintln!("       spindle integration install qodercli");
    eprintln!("       spindle integration uninstall qodercli");
    eprintln!("       spindle integration install qwen");
    eprintln!("       spindle integration uninstall qwen");
    eprintln!("       spindle integration install grok");
    eprintln!("       spindle integration uninstall grok");
    eprintln!("       spindle integration install kilo");
    eprintln!("       spindle integration uninstall kilo");
    eprintln!("       spindle integration install hermes");
    eprintln!("       spindle integration uninstall hermes");
    eprintln!("       spindle integration install antigravity-cli");
    eprintln!("       spindle integration uninstall antigravity-cli");
}

fn print_messages(messages: Vec<String>) -> io::Result<()> {
    for message in messages {
        println!("{message}");
    }
    Ok(())
}
