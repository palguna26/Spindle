use std::io;
use std::path::PathBuf;
use std::process::Command;

pub(crate) fn run(args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("install") => install(&args[1..]),
        Some("uninstall") => uninstall(&args[1..]),
        Some("link") => link(&args[1..]),
        Some("unlink") => unlink(&args[1..]),
        Some("list") => list(&args[1..]),
        Some("action") => action(&args[1..]),
        Some("config-dir") => config_dir(&args[1..]),
        Some("pane") => pane(&args[1..]),
        Some("log") | Some("logs") => log(&args[1..]),
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

fn install(args: &[String]) -> io::Result<()> {
    let Some(source) = args.first() else {
        return usage("usage: spindle plugin install <owner>/<repo>[/subdir] [--ref REF] [--yes]");
    };
    let mut requested_ref = None;
    let mut yes = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--ref" => {
                let Some(value) = args.get(index + 1) else {
                    return usage("missing value for --ref");
                };
                requested_ref = Some(value.clone());
                index += 2;
            }
            "--yes" | "-y" => {
                yes = true;
                index += 1;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option: {other}"),
                ))
            }
        }
    }
    if !yes {
        return usage("remote plugin install requires --yes");
    }
    let parts = source.split('/').collect::<Vec<_>>();
    if parts.len() < 2
        || parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return usage("plugin source must be owner/repo[/subdir]");
    }
    let owner = parts[0];
    let repo = parts[1];
    let subdir = parts[2..].join("/");
    let checkout = crate::plugin::managed_path(source)?;
    let mut registrations = crate::plugin::read_registry()?;
    let replacing = checkout.exists();
    let backup = if replacing {
        let allowed = registrations.iter().any(|registration| {
            registration.managed && registration.source.as_deref() == Some(source)
        });
        if !allowed {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "plugin checkout exists but is not a managed install: {}",
                    checkout.display()
                ),
            ));
        }
        let backup = checkout.with_extension(format!("backup-{}", std::process::id()));
        if backup.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "plugin install backup already exists",
            ));
        }
        std::fs::rename(&checkout, &backup)?;
        Some(backup)
    } else {
        None
    };
    let parent = checkout
        .parent()
        .ok_or_else(|| io::Error::other("invalid plugin checkout path"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}-{}",
        crate::plugin::safe_component(source),
        std::process::id()
    ));
    let url = format!("https://github.com/{owner}/{repo}.git");
    let mut command = Command::new("git");
    command.args(["clone", "--depth", "1"]);
    if let Some(reference) = requested_ref.as_deref() {
        command.args(["--branch", reference]);
    }
    let temporary_string = temporary.to_string_lossy().into_owned();
    let status = command.args([&url, &temporary_string]).status()?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&temporary);
        if let Some(backup) = &backup {
            let _ = std::fs::rename(backup, &checkout);
        }
        return Err(io::Error::other("git clone failed"));
    }
    let manifest_root = if subdir.is_empty() {
        temporary.clone()
    } else {
        temporary.join(subdir.replace('/', std::path::MAIN_SEPARATOR_STR))
    };
    let manifest = match crate::plugin::load(&manifest_root) {
        Ok(manifest) => manifest,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&temporary);
            if let Some(backup) = &backup {
                let _ = std::fs::rename(backup, &checkout);
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid plugin manifest: {error}"),
            ));
        }
    };
    if checkout.exists() {
        let _ = std::fs::remove_dir_all(&temporary);
        if let Some(backup) = &backup {
            let _ = std::fs::rename(backup, &checkout);
        }
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "plugin checkout appeared during install",
        ));
    }
    if let Err(error) = std::fs::rename(&temporary, &checkout) {
        if let Some(backup) = &backup {
            let _ = std::fs::rename(backup, &checkout);
        }
        return Err(error);
    }
    let path = if subdir.is_empty() {
        checkout.clone()
    } else {
        checkout.join(subdir.replace('/', std::path::MAIN_SEPARATOR_STR))
    };
    registrations.retain(|registration| registration.id != manifest.id);
    registrations.push(crate::plugin::Registration {
        id: manifest.id.clone(),
        path,
        enabled: true,
        managed: true,
        source: Some(source.clone()),
    });
    if let Err(error) = crate::plugin::write_registry(&registrations) {
        let _ = std::fs::remove_dir_all(&checkout);
        if let Some(backup) = &backup {
            let _ = std::fs::rename(backup, &checkout);
        }
        return Err(error);
    }
    if let Some(backup) = backup {
        std::fs::remove_dir_all(backup)?;
    }
    println!("installed plugin {}", manifest.id);
    Ok(())
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
        managed: false,
        source: None,
    });
    crate::plugin::write_registry(&registrations)?;
    println!("linked plugin {}", manifest.id);
    Ok(())
}

fn uninstall(args: &[String]) -> io::Result<()> {
    let Some(target) = args.first() else {
        return usage("usage: spindle plugin uninstall <plugin_id|owner/repo[/subdir]>");
    };
    if args.len() != 1 {
        return usage("usage: spindle plugin uninstall <plugin_id|owner/repo[/subdir]>");
    }
    let mut registrations = crate::plugin::read_registry()?;
    let Some(index) = registrations.iter().position(|registration| {
        registration.managed
            && (registration.id == *target || registration.source.as_deref() == Some(target))
    }) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("managed plugin '{target}' is not installed"),
        ));
    };
    let registration = registrations[index].clone();
    let source = registration
        .source
        .as_deref()
        .ok_or_else(|| io::Error::other("managed plugin has no source"))?;
    let checkout = crate::plugin::managed_path(source)?;
    let managed_root = crate::plugin::root()?.join("github").canonicalize()?;
    let checkout_absolute = checkout.canonicalize()?;
    if checkout_absolute.parent() != Some(managed_root.as_path()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to remove a path outside managed plugin storage",
        ));
    }
    std::fs::remove_dir_all(&checkout_absolute)?;
    registrations.remove(index);
    crate::plugin::write_registry(&registrations)?;
    println!("uninstalled plugin {}", registration.id);
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
    let mut plugin_id = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--plugin" => {
                let Some(id) = args.get(index + 1) else {
                    return usage("missing value for --plugin");
                };
                plugin_id = Some(id.clone());
                index += 2;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option: {other}"),
                ))
            }
        }
    }
    let plugins = crate::plugin::installed()?
        .into_iter()
        .filter(|(_, manifest)| plugin_id.as_ref().is_none_or(|id| id == &manifest.id))
        .map(|(registration, manifest)| {
            serde_json::json!({
                "id": manifest.id,
                "name": manifest.name,
                "version": manifest.version,
                "enabled": registration.enabled && manifest.enabled,
                "path": registration.path,
                "managed": registration.managed,
                "source": registration.source,
            })
        })
        .collect::<Vec<_>>();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&plugins).map_err(io::Error::other)?
        );
        return Ok(());
    }
    for plugin in plugins {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            plugin["id"].as_str().unwrap_or(""),
            plugin["name"].as_str().unwrap_or(""),
            plugin["version"].as_str().unwrap_or(""),
            if plugin["enabled"].as_bool().unwrap_or(false) {
                "enabled"
            } else {
                "disabled"
            },
            plugin["path"].as_str().unwrap_or("")
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

fn pane(args: &[String]) -> io::Result<()> {
    match args.first().map(String::as_str) {
        Some("open") => pane_open(&args[1..]),
        Some("focus") => pane_lifecycle(&args[1..], "focus_pane"),
        Some("close") => pane_lifecycle(&args[1..], "close_pane"),
        _ => usage("usage: spindle plugin pane <open|focus|close> ..."),
    }
}

fn pane_lifecycle(args: &[String], operation: &str) -> io::Result<()> {
    let Some(pane_id) = args.first() else {
        return usage(&format!(
            "usage: spindle plugin pane {} <pane_id>",
            operation.strip_suffix("_pane").unwrap_or(operation)
        ));
    };
    if args.len() != 1 {
        return usage(&format!(
            "usage: spindle plugin pane {} <pane_id>",
            operation.strip_suffix("_pane").unwrap_or(operation)
        ));
    }
    let project = super::Project::from_current_dir()?;
    let response = super::send_command_with_payload(
        &project,
        operation,
        serde_json::json!({ "pane_id": pane_id }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| format!("server rejected plugin pane {operation}")),
        ));
    }
    println!(
        "{} plugin pane {}",
        operation.strip_suffix("_pane").unwrap_or(operation),
        pane_id
    );
    Ok(())
}

fn pane_open(args: &[String]) -> io::Result<()> {
    let plugin_id = required_option(args, "--plugin")?;
    let entrypoint = required_option(args, "--entrypoint")?;
    let requested_placement = optional_option(args, "--placement")?;
    let requested_width = optional_option(args, "--width")?;
    let requested_height = optional_option(args, "--height")?;
    let cwd = optional_option(args, "--cwd")?;
    let caller_env = repeated_option(args, "--env")?;
    let no_focus = args.iter().any(|arg| arg == "--no-focus");
    if args.iter().any(|arg| arg == "--focus") && no_focus {
        return usage("--focus and --no-focus cannot be combined");
    }
    let (registration, manifest) = crate::plugin::installed()?
        .into_iter()
        .find(|(registration, manifest)| {
            registration.id == plugin_id
                && registration.enabled
                && manifest.enabled
                && crate::plugin::supports_windows(manifest.platforms.as_deref())
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("enabled plugin '{plugin_id}' was not found"),
            )
        })?;
    let pane = manifest
        .panes
        .iter()
        .find(|pane| pane.id == entrypoint)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("plugin pane '{entrypoint}' was not found"),
            )
        })?;
    if !crate::plugin::supports_windows(pane.platforms.as_deref()) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "plugin pane is not supported on Windows",
        ));
    }
    let placement = requested_placement.as_deref().unwrap_or(&pane.placement);
    if !matches!(placement, "split" | "tab" | "zoomed" | "popup") {
        return usage("plugin pane placement must be split, tab, zoomed, or popup");
    }
    let popup_width = requested_width
        .as_deref()
        .map(str::parse::<u16>)
        .transpose()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "--width must be a number"))?
        .unwrap_or(40);
    let popup_height = requested_height
        .as_deref()
        .map(str::parse::<u16>)
        .transpose()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "--height must be a number"))?
        .unwrap_or(12);
    let Some(command) = pane.command.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "plugin pane command is empty",
        ));
    };
    let project = super::Project::from_current_dir()?;
    project.ensure_state_dir()?;
    if super::ping_server(&project).is_err() {
        super::start_server(&project)?;
    }
    let previous_focus = if no_focus {
        let snapshot = super::send_command(&project, "get_snapshot")?;
        snapshot
            .payload
            .as_ref()
            .and_then(|payload| payload.get("focused_pane_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    } else {
        None
    };
    if placement == "tab" {
        super::send_command_with_payload(
            &project,
            "create_tab",
            serde_json::json!({ "name": pane.title }),
        )?;
    }
    let (config_dir, state_dir) = crate::plugin::ensure_user_dirs(&manifest.id)?;
    let context = serde_json::json!({ "source": "pane", "plugin_id": manifest.id, "entrypoint_id": pane.id, "placement": placement }).to_string();
    let mut env = serde_json::Map::new();
    for value in caller_env {
        let Some((key, value)) = value.split_once('=') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "plugin pane --env values must use KEY=VALUE",
            ));
        };
        if key.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "plugin pane --env keys cannot be empty",
            ));
        }
        env.insert(key.to_owned(), value.to_owned().into());
    }
    for (key, value) in [
        ("SPINDLE_PLUGIN_ID", manifest.id.clone()),
        (
            "SPINDLE_PLUGIN_ROOT",
            registration.path.display().to_string(),
        ),
        (
            "SPINDLE_PLUGIN_CONFIG_DIR",
            config_dir.display().to_string(),
        ),
        ("SPINDLE_PLUGIN_STATE_DIR", state_dir.display().to_string()),
        ("SPINDLE_PLUGIN_ENTRYPOINT_ID", pane.id.clone()),
        ("SPINDLE_PLUGIN_CONTEXT_JSON", context.clone()),
        ("HERDR_PLUGIN_ID", manifest.id.clone()),
        ("HERDR_PLUGIN_ROOT", registration.path.display().to_string()),
        ("HERDR_PLUGIN_CONFIG_DIR", config_dir.display().to_string()),
        ("HERDR_PLUGIN_STATE_DIR", state_dir.display().to_string()),
        ("HERDR_PLUGIN_ENTRYPOINT_ID", pane.id.clone()),
        ("HERDR_PLUGIN_CONTEXT_JSON", context),
    ] {
        env.insert(key.into(), value.into());
    }
    let cwd = cwd.unwrap_or_else(|| registration.path.display().to_string());
    let response = super::send_command_with_payload(
        &project,
        "create_pane",
        serde_json::json!({ "command": command, "args": pane.command.iter().skip(1).collect::<Vec<_>>(), "cwd": cwd, "label": pane.title, "env": env, "cols": if placement == "popup" { popup_width } else { 80 }, "rows": if placement == "popup" { popup_height } else { 24 }, "popup": placement == "popup" }),
    )?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected plugin pane".into()),
        ));
    }
    let pane_id = response
        .payload
        .as_ref()
        .and_then(|payload| payload.get("pane_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if placement != "tab" && placement != "popup" {
        if let Some(previous_focus) = previous_focus {
            super::send_command_with_payload(
                &project,
                "focus_pane",
                serde_json::json!({ "pane_id": previous_focus }),
            )?;
        }
    }
    if placement == "zoomed" {
        super::send_command_with_payload(
            &project,
            "toggle_pane_zoom",
            serde_json::json!({ "pane_id": pane_id }),
        )?;
    }
    println!("opened plugin pane {plugin_id}.{entrypoint}: {pane_id}");
    Ok(())
}

fn required_option(args: &[String], option: &str) -> io::Result<String> {
    optional_option(args, option)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing required {option} ID"),
        )
    })
}

fn optional_option(args: &[String], option: &str) -> io::Result<Option<String>> {
    let matches = args
        .iter()
        .enumerate()
        .filter(|(_, value)| value.as_str() == option)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Ok(None);
    }
    if matches.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("option {option} may only be used once"),
        ));
    }
    let index = matches[0];
    args.get(index + 1)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("missing value for {option}"),
            )
        })
        .map(Some)
}

fn repeated_option(args: &[String], option: &str) -> io::Result<Vec<String>> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == option {
            let Some(value) = args.get(index + 1) else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("missing value for {option}"),
                ));
            };
            values.push(value.clone());
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok(values)
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
            || !crate::plugin::supports_windows(manifest.platforms.as_deref())
            || plugin_id.as_ref().is_some_and(|id| id != &manifest.id)
        {
            continue;
        }
        for action in manifest.actions {
            if !crate::plugin::supports_windows(action.platforms.as_deref()) {
                continue;
            }
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
            || !crate::plugin::supports_windows(manifest.platforms.as_deref())
            || selected_plugin.is_some_and(|id| id != manifest.id)
        {
            continue;
        }
        for action in manifest.actions {
            if action.id == action_id
                && crate::plugin::supports_windows(action.platforms.as_deref())
            {
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
    let _ = crate::plugin::record_launch(&manifest_id, "action", &action.id, child.id());
    println!("started {}.{} (pid {})", manifest_id, action.id, child.id());
    Ok(())
}

fn log(args: &[String]) -> io::Result<()> {
    if args.first().map(String::as_str) != Some("list") {
        return usage("usage: spindle plugin log list [--plugin ID] [--limit N]");
    }
    let mut plugin_id = None;
    let mut limit = 50usize;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--plugin" => {
                plugin_id = args.get(index + 1).cloned();
                if plugin_id.is_none() {
                    return usage("missing value for --plugin");
                }
                index += 2;
            }
            "--limit" => {
                let Some(value) = args.get(index + 1) else {
                    return usage("missing value for --limit");
                };
                limit = value.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid --limit value")
                })?;
                index += 2;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option: {other}"),
                ))
            }
        }
    }
    let mut entries = crate::plugin::launch_log()?;
    entries.retain(|entry| plugin_id.as_ref().is_none_or(|id| id == &entry.plugin_id));
    let start = entries.len().saturating_sub(limit);
    for entry in &entries[start..] {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            entry.timestamp, entry.plugin_id, entry.kind, entry.command_id, entry.pid
        );
    }
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
    println!(
        "Usage: spindle plugin <install|uninstall|link|unlink|list|enable|disable|config-dir|action|pane>"
    );
    println!("  install owner/repo[/subdir] [--ref REF] --yes  install from GitHub");
    println!("  uninstall <id|owner/repo[/subdir]>            remove a managed plugin");
    println!("  link <path> [--disabled]  register a local Herdr manifest");
    println!("  list [--plugin ID] [--json] list linked plugins");
    println!("  unlink <plugin_id>        unregister a plugin, leaving files alone");
    println!("  enable|disable <id>       change a plugin's global enabled state");
    println!("  config-dir <id>           print and create the plugin config directory");
    println!("  pane open --plugin ID --entrypoint ID [--env KEY=VALUE]  open a manifest pane");
    println!("  pane focus|close <pane_id>              manage a plugin pane");
    println!("  log list [--plugin ID] [--limit N]       show plugin launches");
    println!("  action list [--plugin ID] list manifest actions");
    println!("  action invoke <id>        start a manifest action without a shell");
}
