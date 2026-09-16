use super::input::Keymap;
use crate::server::session::SessionSnapshot;
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspacePickerKey {
    Cancel,
    Move(bool),
    Confirm,
    Choose(usize),
    Ignore,
}

pub(super) fn workspace_picker_key(key: KeyEvent, keymap: &Keymap) -> WorkspacePickerKey {
    if key.code == KeyCode::Esc || keymap.is_prefix(key) {
        WorkspacePickerKey::Cancel
    } else if let Some(forward) = keymap.navigate_workspace_direction(key) {
        WorkspacePickerKey::Move(forward)
    } else if key.modifiers.is_empty() && key.code == KeyCode::Enter {
        WorkspacePickerKey::Confirm
    } else if key.modifiers.is_empty() {
        match key.code {
            KeyCode::Char(digit @ '1'..='9') => {
                WorkspacePickerKey::Choose((digit as usize) - ('1' as usize))
            }
            _ => WorkspacePickerKey::Ignore,
        }
    } else {
        WorkspacePickerKey::Ignore
    }
}

pub(super) fn move_workspace_selection(
    snapshot: &SessionSnapshot,
    selected: Option<&(String, String)>,
    forward: bool,
) -> Option<(String, String)> {
    let workspaces = snapshot
        .spaces
        .iter()
        .flat_map(|space| {
            space
                .workspaces
                .iter()
                .map(|workspace| (space.space_id.clone(), workspace.workspace_id.clone()))
        })
        .collect::<Vec<_>>();
    if workspaces.is_empty() {
        return None;
    }
    let current = selected
        .and_then(|selected| workspaces.iter().position(|item| item == selected))
        .or_else(|| {
            workspaces.iter().position(|(space_id, workspace_id)| {
                space_id == &snapshot.active_space_id
                    && snapshot
                        .spaces
                        .iter()
                        .find(|space| &space.space_id == space_id)
                        .and_then(|space| space.active_workspace_id.as_ref())
                        == Some(workspace_id)
            })
        });
    let index = current.unwrap_or(if forward { workspaces.len() - 1 } else { 0 });
    let next = if forward {
        (index + 1) % workspaces.len()
    } else {
        (index + workspaces.len() - 1) % workspaces.len()
    };
    Some(workspaces[next].clone())
}

pub(super) fn indexed_workspace_selection(
    snapshot: &SessionSnapshot,
    index: usize,
) -> Option<(String, String)> {
    snapshot
        .spaces
        .iter()
        .flat_map(|space| {
            space
                .workspaces
                .iter()
                .map(|workspace| (space.space_id.clone(), workspace.workspace_id.clone()))
        })
        .nth(index)
}
