use crate::detect::{AgentKind, AgentState};
use crate::model::status::PaneStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneEvent {
    Output {
        pane_id: String,
        bytes: Vec<u8>,
    },
    Status {
        pane_id: String,
        status: PaneStatus,
    },
    AgentDetected {
        pane_id: String,
        agent: Option<AgentKind>,
        released: bool,
        final_status: Option<AgentState>,
    },
    AgentStatusChanged {
        pane_id: String,
        agent_state: AgentState,
    },
}
