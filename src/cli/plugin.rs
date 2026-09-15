use std::io;
use std::path::PathBuf;

pub(crate) fn run(args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("link") => link(&args[1..]),
        Some("unlink") => unlink(&args[1..]),
        Some("list") => list(&args[1..]),
        Some("enable") => set_enabled(&args[1..], true),
        Some("disable") => set_enabled(&args[1..], false),
        Some("help") | Some("--help") | Some("-h") | None => {
            help();
            Ok(())
        }
        Some(other) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown plugin command '{other}'"),
        )),
    }
}

fn link(args: &[String]) -> io::Result<()> {
    let Some(path_arg) = args.first() else {
        return usage("usage: spindle plugin link <path> [--disabled]");
    };
    if args.len() > 2 || (args.len() == 2 && args[1] != "--disabled") {
        return usage("usage: spindle plugin link <path> [--disabled]");
    }
    let input_path = PathBuf::from(path_arg).canonicalize()?;
    let manifest = crate::plugin::load(&input_path)?;
    let path = if input_path.is_file() {
        input_path.parent().unwrap_or(&input_path).to_path_buf()
    } else {
        input_path
    };
    let mut registrations = crate::plugin::read_registry()?;
    registrations.retain(|registration| registration.id != manifest.id);
    registrations.push(crate::plugin::Registration {
        id: manifest.id.clone(),
        path,
        enabled: args.get(1).is_none(),
    });
    crate::plugin::write_registry(&registrations)?;
    println!("linked plugin {}", manifest.id);
    Ok(())
}

fn unlink(args: &[String]) -> io::Result<()> {
    let Some(id) = args.first() else {
        return usage("usage: spindle plugin unlink <plugin_id>");
    };
    if args.len() != 1 {
        return usage("usage: spindle plugin unlink <plugin_id>");
    }
    let mut registrations = crate::plugin::read_registry()?;
    let before = registrations.len();
    registrations.retain(|registration| registration.id != *id);
    if before == registrations.len() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("plugin '{id}' is not linked"),
        ));
    }
    crate::plugin::write_registry(&registrations)?;
    println!("unlinked plugin {id}");
    Ok(())
}

fn list(args: &[String]) -> io::Result<()> {
    if !args.is_empty() {
        return usage("usage: spindle plugin list");
    }
    for (registration, manifest) in crate::plugin::installed()? {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            manifest.id,
            manifest.name,
            manifest.version,
            if registration.enabled {
                "enabled"
            } else {
                "disabled"
            },
            registration.path.display()
        );
    }
    Ok(())
}

fn set_enabled(args: &[String], enabled: bool) -> io::Result<()> {
    let Some(id) = args.first() else {
        return usage("usage: spindle plugin enable|disable <plugin_id>");
    };
    if args.len() != 1 {
        return usage("usage: spindle plugin enable|disable <plugin_id>");
    }
    let mut registrations = crate::plugin::read_registry()?;
    let Some(registration) = registrations
        .iter_mut()
        .find(|registration| registration.id == *id)
    else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("plugin '{id}' is not linked"),
        ));
    };
    registration.enabled = enabled;
    crate::plugin::write_registry(&registrations)
}

fn usage(message: &str) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::InvalidInput, message))
}

fn help() {
    println!("Usage: spindle plugin <link|unlink|list|enable|disable>");
    println!("  link <path> [--disabled]  register a local Herdr manifest");
    println!("  list                      list linked plugins");
    println!("  unlink <plugin_id>        unregister a plugin, leaving files alone");
    println!("  enable|disable <id>       change a plugin's global enabled state");
}
