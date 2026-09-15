use std::io;
use std::path::PathBuf;

pub(crate) fn run(args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("link") => link(&args[1..]),
        Some("unlink") => unlink(&args[1..]),
        Some("list") => list(&args[1..]),
        Some("action") => action(&args[1..]),
        Some("config-dir") => config_dir(&args[1..]),
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

fn config_dir(args: &[String]) -> io::Result<()> {
    let Some(id) = args.first() else {
        return usage("usage: spindle plugin config-dir <plugin_id>");
    };
    if args.len() != 1 {
        return usage("usage: spindle plugin config-dir <plugin_id>");
    }
    let (config, _) = crate::plugin::ensure_user_dirs(id)?;
    println!("{}", config.display());
    Ok(())
}

fn action(args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("list") => action_list(&args[1..]),
        Some("invoke") => action_invoke(&args[1..]),
        _ => usage("usage: spindle plugin action <list|invoke>"),
    }
}

fn action_list(args: &[String]) -> io::Result<()> {
    let plugin_id = one_option(args, "--plugin")?;
    for (registration, manifest) in crate::plugin::installed()? {
        if !registration.enabled
            || !manifest.enabled
            || plugin_id.as_ref().is_some_and(|id| id != &manifest.id)
        {
            continue;
        }
        for action in manifest.actions {
            let title = if action.title.is_empty() {
                &action.id
            } else {
                &action.title
            };
            println!("{}\t{}\t{}", manifest.id, action.id, title);
        }
    }
    Ok(())
}

fn action_invoke(args: &[String]) -> io::Result<()> {
    let Some(action_arg) = args.first() else {
        return usage("usage: spindle plugin action invoke <action_id> [--plugin ID]");
    };
    let plugin_id = one_option(&args[1..], "--plugin")?;
    let (qualified_plugin, action_id) = action_arg
        .rsplit_once('.')
        .map_or((None, action_arg.as_str()), |(plugin, action)| {
            (Some(plugin), action)
        });
    let selected_plugin = plugin_id.as_deref().or(qualified_plugin);
    let mut matches = Vec::new();
    for (registration, manifest) in crate::plugin::installed()? {
        if !registration.enabled
            || !manifest.enabled
            || selected_plugin.is_some_and(|id| id != manifest.id)
        {
            continue;
        }
        for action in manifest.actions {
            if action.id == action_id {
                matches.push((registration.clone(), manifest.id.clone(), action));
            }
        }
    }
    if matches.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            if matches.is_empty() {
                format!("plugin action '{action_arg}' was not found")
            } else {
                format!("plugin action '{action_arg}' is ambiguous; use --plugin ID or plugin.id.action")
            },
        ));
    }
    let (registration, manifest_id, action) = matches.remove(0);
    let Some(command) = action.command.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "plugin action command is empty",
        ));
    };
    let (config_dir, state_dir) = crate::plugin::ensure_user_dirs(&manifest_id)?;
    let context = serde_json::json!({
        "source": "cli",
        "plugin_id": &manifest_id,
        "action_id": &action.id,
        "cwd": std::env::current_dir()?.display().to_string(),
    })
    .to_string();
    let child = std::process::Command::new(command)
        .args(action.command.iter().skip(1))
        .current_dir(&registration.path)
        .env("SPINDLE_PLUGIN_ID", &manifest_id)
        .env("SPINDLE_PLUGIN_ROOT", &registration.path)
        .env("SPINDLE_PLUGIN_CONFIG_DIR", &config_dir)
        .env("SPINDLE_PLUGIN_STATE_DIR", &state_dir)
        .env("SPINDLE_PLUGIN_CONTEXT_JSON", &context)
        .env("SPINDLE_PLUGIN_ACTION_ID", &action.id)
        .env("HERDR_PLUGIN_ID", &manifest_id)
        .env("HERDR_PLUGIN_ROOT", &registration.path)
        .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("HERDR_PLUGIN_CONTEXT_JSON", &context)
        .env("HERDR_PLUGIN_ACTION_ID", &action.id)
        .spawn()?;
    println!("started {}.{} (pid {})", manifest_id, action.id, child.id());
    Ok(())
}

fn one_option(args: &[String], option: &str) -> io::Result<Option<String>> {
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() == 2 && args[0] == option {
        return Ok(Some(args[1].clone()));
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("unsupported plugin option; expected {option} ID"),
    ))
}

fn usage(message: &str) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::InvalidInput, message))
}

fn help() {
    println!("Usage: spindle plugin <link|unlink|list|enable|disable|config-dir|action>");
    println!("  link <path> [--disabled]  register a local Herdr manifest");
    println!("  list                      list linked plugins");
    println!("  unlink <plugin_id>        unregister a plugin, leaving files alone");
    println!("  enable|disable <id>       change a plugin's global enabled state");
    println!("  config-dir <id>           print and create the plugin config directory");
    println!("  action list [--plugin ID] list manifest actions");
    println!("  action invoke <id>        start a manifest action without a shell");
}
