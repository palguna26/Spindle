use crate::{model::layout::LayoutNode, model::status::PaneStatus};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

pub const SNAPSHOT_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaneSnapshot {
    pub pane_id: String,
    pub command: String,
    pub cwd: String,
    pub label: Option<String>,
    pub status: PaneStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub version: u16,
    pub layout: LayoutNode,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Debug)]
pub enum SnapshotError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedVersion(u16),
}

impl From<io::Error> for SnapshotError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for SnapshotError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn save(path: &Path, snapshot: &Snapshot) -> Result<(), SnapshotError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let data = serde_json::to_vec_pretty(snapshot)?;
    let mut file = File::create(&temporary)?;
    file.write_all(&data)?;
    file.sync_all()?;
    replace_file(&temporary, path)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Snapshot, SnapshotError> {
    let snapshot: Snapshot = serde_json::from_reader(File::open(path)?)?;
    if snapshot.version > SNAPSHOT_VERSION {
        return Err(SnapshotError::UnsupportedVersion(snapshot.version));
    }
    Ok(snapshot)
}

pub fn recover(mut snapshot: Snapshot) -> Snapshot {
    for pane in &mut snapshot.panes {
        if pane.status.is_running() {
            pane.status = PaneStatus::Interrupted {
                reason: "process was live when the server stopped".into(),
            };
        }
    }
    snapshot
}

pub fn save_versioned<T: Serialize>(path: &Path, value: &T) -> Result<(), SnapshotError> {
    let data = serde_json::to_vec_pretty(&Versioned {
        version: SNAPSHOT_VERSION,
        data: value,
    })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = File::create(&temporary)?;
    file.write_all(&data)?;
    file.sync_all()?;
    replace_file(&temporary, path)?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn load_versioned<T: DeserializeOwned>(path: &Path) -> Result<T, SnapshotError> {
    let versioned: Versioned<T> = serde_json::from_reader(File::open(path)?)?;
    if versioned.version > SNAPSHOT_VERSION {
        return Err(SnapshotError::UnsupportedVersion(versioned.version));
    }
    Ok(versioned.data)
}

#[derive(Debug, Serialize, Deserialize)]
struct Versioned<T> {
    version: u16,
    data: T,
}

#[cfg(test)]
mod tests {
    use super::{
        load, load_versioned, recover, save, PaneSnapshot, Snapshot, SnapshotError,
        SNAPSHOT_VERSION,
    };
    use crate::{model::layout::LayoutNode, model::status::PaneStatus};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("spindle-test-{stamp}.json"))
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            version: SNAPSHOT_VERSION,
            layout: LayoutNode::pane("pane-1"),
            panes: vec![PaneSnapshot {
                pane_id: "pane-1".into(),
                command: "powershell".into(),
                cwd: "C:/repo".into(),
                label: None,
                status: PaneStatus::Running,
            }],
        }
    }

    #[test]
    fn snapshot_round_trips() {
        let path = temp_path();
        save(&path, &snapshot()).unwrap();
        assert_eq!(load(&path).unwrap(), snapshot());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn recovery_marks_live_panes_interrupted() {
        let recovered = recover(snapshot());
        assert!(matches!(
            recovered.panes[0].status,
            PaneStatus::Interrupted { .. }
        ));
    }

    #[test]
    fn versioned_snapshot_can_be_replaced() {
        let path = temp_path();
        super::save_versioned(&path, &snapshot()).unwrap();
        super::save_versioned(&path, &snapshot()).unwrap();
        assert_eq!(
            super::load_versioned::<Snapshot>(&path).unwrap(),
            snapshot()
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn newer_version_is_rejected_without_removing_file() {
        let path = temp_path();
        let data = serde_json::json!({
            "version": 99,
            "data": serde_json::to_value(snapshot()).unwrap(),
        });
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(matches!(
            load_versioned::<Snapshot>(&path),
            Err(SnapshotError::UnsupportedVersion(99))
        ));
        assert!(path.exists());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_versioned_snapshot_returns_error() {
        let path = temp_path();
        std::fs::write(&path, b"not-json").unwrap();
        assert!(load_versioned::<Snapshot>(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not-json");
        std::fs::remove_file(path).unwrap();
    }
}
