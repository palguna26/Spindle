use crate::{layout::LayoutNode, status::PaneStatus};
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
    fs::rename(temporary, path)?;
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

#[cfg(test)]
mod tests {
    use super::{load, recover, save, PaneSnapshot, Snapshot, SNAPSHOT_VERSION};
    use crate::{layout::LayoutNode, status::PaneStatus};
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
}
