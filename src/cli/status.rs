use std::io;

use serde::Serialize;

use super::Project;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Full,
    Server,
    Client,
}

pub(super) fn run(project: &Project, args: &[String]) -> io::Result<()> {
    let (scope, json) = parse_args(args).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: spindle status [server|client] [--json]",
        )
    })?;
    let endpoint_exists = project.endpoint_path().exists();
    let server_running = super::ping_server(project).is_ok();
    let client = ClientStatus {
        version: format!("spindle {}", env!("CARGO_PKG_VERSION")),
        binary: std::env::current_exe()?.display().to_string(),
        protocol: u32::from(crate::protocol::PROTOCOL_VERSION),
    };
    let server = ServerStatus {
        status: super::endpoint_status_label(endpoint_exists, server_running),
        endpoint: project.endpoint_path().display().to_string(),
        project: project.describe(),
    };
    if json {
        return print_json(scope, client, server);
    }
    match scope {
        Scope::Full => {
            println!("client:");
            print_client(&client);
            println!();
            println!("server:");
            print_server(&server, server_running);
        }
        Scope::Client => print_client(&client),
        Scope::Server => print_server(&server, server_running),
    }
    Ok(())
}

fn parse_args(args: &[String]) -> Option<(Scope, bool)> {
    match args {
        [] => Some((Scope::Full, false)),
        [arg] if arg == "--json" => Some((Scope::Full, true)),
        [scope] if scope == "server" => Some((Scope::Server, false)),
        [scope] if scope == "client" => Some((Scope::Client, false)),
        [scope, json] if (scope == "server" || scope == "client") && json == "--json" => Some((
            if scope == "server" {
                Scope::Server
            } else {
                Scope::Client
            },
            true,
        )),
        _ => None,
    }
}

fn print_client(client: &ClientStatus) {
    println!("  version: {}", client.version);
    println!("  binary: {}", client.binary);
    println!("  protocol: {}", client.protocol);
}

fn print_server(server: &ServerStatus, running: bool) {
    println!(
        "  status: {}",
        if running { "running" } else { "not running" }
    );
    println!("  endpoint: {}", server.endpoint);
    println!("  project: {}", server.project);
}

fn print_json(scope: Scope, client: ClientStatus, server: ServerStatus) -> io::Result<()> {
    let value = match scope {
        Scope::Full => serde_json::to_value(FullStatus { client, server }),
        Scope::Client => serde_json::to_value(client),
        Scope::Server => serde_json::to_value(server),
    }
    .map_err(io::Error::other)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(io::Error::other)?
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct ClientStatus {
    version: String,
    binary: String,
    protocol: u32,
}

#[derive(Debug, Serialize)]
struct ServerStatus {
    status: &'static str,
    endpoint: String,
    project: String,
}

#[derive(Debug, Serialize)]
struct FullStatus {
    client: ClientStatus,
    server: ServerStatus,
}

#[cfg(test)]
mod tests {
    use super::{parse_args, Scope};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn status_scopes_match_herdr_forms() {
        assert_eq!(parse_args(&[]), Some((Scope::Full, false)));
        assert_eq!(
            parse_args(&args(&["server", "--json"])),
            Some((Scope::Server, true))
        );
        assert_eq!(parse_args(&args(&["client"])), Some((Scope::Client, false)));
        assert_eq!(parse_args(&args(&["unknown"])), None);
    }
}
