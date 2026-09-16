use std::io;

const API_SCHEMA_JSON: &str = include_str!("../../docs/api/spindle-api.schema.json");

pub(super) fn run(project: &super::Project, args: &[String]) -> io::Result<()> {
    match args {
        [] => print_summary(),
        [command] if command == "schema" => print_summary(),
        [command] if command == "snapshot" => print_snapshot(project),
        [command, flag] if command == "schema" && flag == "--json" => {
            print!("{API_SCHEMA_JSON}");
            Ok(())
        }
        [command, flag, path] if command == "schema" && flag == "--output" => {
            std::fs::write(path, API_SCHEMA_JSON)?;
            println!("wrote API schema to {path}");
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
                "usage: spindle api schema [--json | --output PATH]",
            ))
        }
    }
}

fn print_summary() -> io::Result<()> {
    let schema: serde_json::Value =
        serde_json::from_str(API_SCHEMA_JSON).map_err(io::Error::other)?;
    let protocol = schema["protocol"]
        .as_u64()
        .ok_or_else(|| io::Error::other("API schema is missing protocol"))?;
    let version = schema["schema_version"]
        .as_u64()
        .ok_or_else(|| io::Error::other("API schema is missing schema_version"))?;
    let count = schema["operations"]
        .as_object()
        .map_or(0, serde_json::Map::len);
    println!("Spindle API schema\nprotocol: {protocol}\nschema_version: {version}\noperations: {count}\n");
    println!("Use `spindle api schema --json` to print the full schema.");
    println!("Use `spindle api schema --output PATH` to write it to a file.");
    Ok(())
}

fn print_snapshot(project: &super::Project) -> io::Result<()> {
    let response = super::send_command(project, "get_snapshot")?;
    if !response.ok {
        return Err(io::Error::other(
            response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "server rejected the snapshot request".into()),
        ));
    }
    let payload = response
        .payload
        .ok_or_else(|| io::Error::other("server returned no session snapshot"))?;
    let json = serde_json::to_string_pretty(&payload).map_err(io::Error::other)?;
    println!("{json}");
    Ok(())
}

fn print_help() {
    eprintln!("spindle api commands:");
    eprintln!("  spindle api snapshot");
    eprintln!("  spindle api schema [--json | --output PATH]");
}

#[cfg(test)]
mod tests {
    use super::API_SCHEMA_JSON;

    #[test]
    fn bundled_schema_matches_the_protocol() {
        let schema: serde_json::Value = serde_json::from_str(API_SCHEMA_JSON).unwrap();
        assert_eq!(schema["protocol"], 1);
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("get_snapshot"));
        assert_eq!(
            schema["operations"]["create_pane"]["payload"]["properties"]["popup"]["type"],
            "boolean"
        );
        assert_eq!(
            schema["operations"]["create_pane"]["payload"]["properties"]["overlay"]["type"],
            "boolean"
        );
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("close_popup"));
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("close_overlay"));
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("report_agent_session"));
        assert_eq!(
            schema["operations"]["report_agent"]["payload"]["required"],
            serde_json::json!(["pane_id", "source", "agent", "state"])
        );
        assert_eq!(
            schema["operations"]["report_agent_session"]["payload"]["anyOf"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("report_metadata"));
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("clear_agent_authority"));
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("move_tab"));
        assert!(schema["operations"]
            .as_object()
            .unwrap()
            .contains_key("move_workspace"));
    }
}
