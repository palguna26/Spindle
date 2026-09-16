//! Shared file/config primitives for agent integrations.
//!
//! This follows Herdr's `src/integration/file_ops.rs` boundary. Installers
//! own agent-specific config shape; this module owns safe file operations.

use std::io;
use std::path::Path;

use serde_json::{json, Value};

pub(super) fn read_json_object(path: &Path, label: &str) -> io::Result<Value> {
    if !path.is_file() {
        return Ok(json!({}));
    }
    let value =
        serde_json::from_str::<Value>(&std::fs::read_to_string(path)?).map_err(|error| {
            io::Error::other(format!("failed to parse {}: {error}", path.display()))
        })?;
    if !value.is_object() {
        return Err(io::Error::other(format!(
            "{label} at {} must be a JSON object",
            path.display()
        )));
    }
    Ok(value)
}

pub(super) fn remove_file_if_exists(path: &Path) -> io::Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn remove_dir_if_exists(path: &Path) -> io::Result<bool> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
