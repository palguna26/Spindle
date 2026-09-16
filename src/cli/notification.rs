use std::io;

const USAGE: &str = "usage: spindle notification show <title> [--body TEXT] [--position top-left|top-right|bottom-left|bottom-right] [--sound none|done|request]";

pub(super) fn run(args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("show") => show(&args[1..]),
        Some("help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE))
        }
    }
}

fn show(args: &[String]) -> io::Result<()> {
    let Some(title) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE));
    };
    let mut body = None;
    let mut sound = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("missing value for {flag}"),
            )
        })?;
        match flag {
            "--body" => body = Some(value.as_str()),
            "--position"
                if matches!(
                    value.as_str(),
                    "top-left" | "top-right" | "bottom-left" | "bottom-right"
                ) => {}
            "--position" => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid position: {value}"),
                ));
            }
            "--sound" => sound = Some(value.as_str()),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option: {flag}"),
                ))
            }
        }
        index += 2;
    }
    let sound = match sound.unwrap_or("none") {
        "none" => None,
        "done" => Some(crate::platform::NotificationSound::Finished),
        "request" => Some(crate::platform::NotificationSound::Attention),
        value => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid sound: {value} (expected none, done, or request)"),
            ))
        }
    };
    crate::platform::show_desktop_notification(title, body)?;
    if let Some(sound) = sound {
        crate::platform::play_notification_sound(sound)?;
    }
    Ok(())
}

fn print_help() {
    eprintln!("spindle notification commands:");
    eprintln!("  {USAGE}");
}

#[cfg(test)]
mod tests {
    #[test]
    fn help_does_not_require_a_server() {
        super::run(&["help".into()]).unwrap();
    }
}
