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
}

fn print_messages(messages: Vec<String>) -> io::Result<()> {
    for message in messages {
        println!("{message}");
    }
    Ok(())
}
