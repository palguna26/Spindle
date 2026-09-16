use std::io;

pub(super) fn run(args: &[String]) -> io::Result<()> {
    match args {
        [] => crate::integration::run_status(false),
        [command] if command == "status" => crate::integration::run_status(false),
        [command, flag] if command == "status" && flag == "--json" => {
            crate::integration::run_status(true)
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: spindle integration status [--json]",
            ))
        }
    }
}

fn print_help() {
    eprintln!("usage: spindle integration status [--json]");
}
