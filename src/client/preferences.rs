use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct ClientPreferences {
    #[serde(default)]
    pub(super) sidebar_collapsed: bool,
}

pub(super) fn load(path: &Path) -> ClientPreferences {
    std::fs::read(path)
        .ok()
        .and_then(|content| serde_json::from_slice(&content).ok())
        .unwrap_or_default()
}

pub(super) fn store(path: &Path, preferences: ClientPreferences) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "client preference path has no parent directory",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let content = serde_json::to_vec_pretty(&preferences).map_err(std::io::Error::other)?;
    let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let mut temp_name = path
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "client preference path has no file name",
            )
        })?
        .to_os_string();
    temp_name.push(format!(".tmp-{}-{sequence}", std::process::id()));
    let temp_path = parent.join(temp_name);
    std::fs::write(&temp_path, content)?;
    if let Err(error) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(test: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "spindle-client-preferences-{test}-{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn missing_or_invalid_preferences_use_defaults() {
        let path = test_path("defaults");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path), ClientPreferences::default());
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(load(&path), ClientPreferences::default());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn sidebar_preference_survives_replacement() {
        let path = test_path("replacement");
        let _ = std::fs::remove_file(&path);
        store(
            &path,
            ClientPreferences {
                sidebar_collapsed: true,
            },
        )
        .unwrap();
        assert!(load(&path).sidebar_collapsed);
        store(&path, ClientPreferences::default()).unwrap();
        assert!(!load(&path).sidebar_collapsed);
        std::fs::remove_file(path).unwrap();
    }
}
