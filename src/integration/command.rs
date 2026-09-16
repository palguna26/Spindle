//! Commands used by installed agent hooks.
//!
//! Kept separate from integration paths and config editing, matching Herdr's
//! `src/integration/command.rs` organization.

use std::path::Path;

pub(super) fn direct_hook_command(path: &Path) -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        path.display().to_string().replace('"', "\\\"")
    )
}
