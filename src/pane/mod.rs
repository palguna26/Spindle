mod agent_detection;
mod events;
mod manager;

pub(crate) use agent_detection::{AgentReport, AgentSessionReport};
pub use agent_detection::{AgentSessionInfo, AgentSessionRefKind};
pub use events::PaneEvent;
pub use manager::{Pane, PaneConfig, PaneManager, PaneManagerError};
