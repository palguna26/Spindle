use crate::model::status::PaneStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneEvent {
    Output { pane_id: String, bytes: Vec<u8> },
    Status { pane_id: String, status: PaneStatus },
}
