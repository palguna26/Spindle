use super::PaneEvent;
use crate::detect::{self, AgentKind, AgentState};
use crate::model::status::PaneStatus;
use crate::pty::{PtyConfig, PtySession, PtySessionError};
use crate::terminal::{TerminalEmulator, TerminalSnapshot};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};

const DEFAULT_SCROLLBACK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct PaneConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

pub struct Pane {
    pub id: String,
    pub config: PaneConfig,
    pub status: PaneStatus,
    pub agent: Option<AgentKind>,
    pub agent_state: Option<AgentState>,
    pub scrollback: VecDeque<u8>,
    pub terminal: TerminalEmulator,
    session: PtySession,
}

#[derive(Debug)]
pub enum PaneManagerError {
    Pty(PtySessionError),
    MissingPane(String),
}

impl From<PtySessionError> for PaneManagerError {
    fn from(error: PtySessionError) -> Self {
        Self::Pty(error)
    }
}

pub struct PaneManager {
    panes: HashMap<String, Pane>,
    scrollback_limit: usize,
    last_agent_scan: Instant,
}

impl Default for PaneManager {
    fn default() -> Self {
        Self::new(DEFAULT_SCROLLBACK_BYTES)
    }
}

impl PaneManager {
    pub fn new(scrollback_limit: usize) -> Self {
        Self {
            panes: HashMap::new(),
            scrollback_limit,
            last_agent_scan: Instant::now() - Duration::from_secs(2),
        }
    }

    pub fn spawn(
        &mut self,
        id: impl Into<String>,
        config: PaneConfig,
    ) -> Result<(), PaneManagerError> {
        let id = id.into();
        let rows = config.rows;
        let cols = config.cols;
        let session = PtySession::spawn(&PtyConfig {
            command: config.command.clone(),
            args: config.args.clone(),
            cwd: config.cwd.clone(),
            env: config.env.clone(),
            cols: config.cols,
            rows: config.rows,
        })?;
        self.panes.insert(
            id.clone(),
            Pane {
                id,
                config,
                status: PaneStatus::Running,
                agent: None,
                agent_state: None,
                scrollback: VecDeque::with_capacity(self.scrollback_limit),
                terminal: TerminalEmulator::new(rows, cols, self.scrollback_limit),
                session,
            },
        );
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Pane> {
        self.panes.get(id)
    }

    pub fn remove(&mut self, id: &str) -> Result<Pane, PaneManagerError> {
        self.panes
            .remove(id)
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))
    }

    pub fn send_input(&mut self, id: &str, input: &[u8]) -> Result<(), PaneManagerError> {
        self.panes
            .get_mut(id)
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))?
            .session
            .send_input(input)
            .map_err(Into::into)
    }

    pub fn resize(&mut self, id: &str, cols: u16, rows: u16) -> Result<(), PaneManagerError> {
        let pane = self
            .panes
            .get_mut(id)
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))?;
        pane.session
            .resize(cols, rows)
            .map_err(PaneManagerError::from)?;
        pane.terminal.resize(rows, cols);
        Ok(())
    }

    pub fn stop(&mut self, id: &str) -> Result<Vec<PaneEvent>, PaneManagerError> {
        let pane = self
            .panes
            .get_mut(id)
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))?;
        pane.session.stop()?;
        pane.status = PaneStatus::Halted {
            reason: "stopped by user".into(),
        };
        if pane.agent.is_some() {
            pane.agent_state = Some(AgentState::Idle);
        }
        Ok(vec![PaneEvent::Status {
            pane_id: id.into(),
            status: pane.status.clone(),
        }])
    }

    pub fn poll(&mut self) -> Vec<PaneEvent> {
        let mut events = Vec::new();
        let scan_agents = self.last_agent_scan.elapsed() >= Duration::from_secs(1);
        if scan_agents {
            self.last_agent_scan = Instant::now();
        }
        for pane in self.panes.values_mut() {
            if scan_agents && pane.status.is_running() {
                pane.agent = pane
                    .session
                    .process_id()
                    .and_then(detect::detect_in_process_tree);
            }
            while let Ok(Some(bytes)) = pane.session.try_read_output() {
                for byte in &bytes {
                    pane.scrollback.push_back(*byte);
                }
                while pane.scrollback.len() > self.scrollback_limit {
                    pane.scrollback.pop_front();
                }
                for response in pane.terminal.process(&bytes) {
                    let _ = pane.session.send_input(&response);
                }
                events.push(PaneEvent::Output {
                    pane_id: pane.id.clone(),
                    bytes,
                });
            }

            if pane.status.is_running() {
                let terminal = pane.terminal.snapshot();
                pane.agent_state = pane
                    .agent
                    .map(|agent| detect::detect_state(agent, &terminal.contents, &terminal.title));
            }

            if pane.status.is_running() {
                if let Ok(Some(exit_code)) = pane.session.try_wait() {
                    if pane.agent.is_some() {
                        pane.agent_state = Some(AgentState::Idle);
                    }
                    pane.status = if exit_code == 0 {
                        PaneStatus::Completed { exit_code: 0 }
                    } else {
                        PaneStatus::Halted {
                            reason: format!("process exited with code {exit_code}"),
                        }
                    };
                    events.push(PaneEvent::Status {
                        pane_id: pane.id.clone(),
                        status: pane.status.clone(),
                    });
                }
            }
        }
        events
    }

    pub fn terminal_snapshot(&self, id: &str) -> Result<TerminalSnapshot, PaneManagerError> {
        self.panes
            .get(id)
            .map(|pane| pane.terminal.snapshot())
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::{PaneConfig, PaneManager};
    use std::collections::BTreeMap;

    fn config(command: &str) -> PaneConfig {
        PaneConfig {
            command: command.into(),
            args: Vec::new(),
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            env: BTreeMap::new(),
            cols: 80,
            rows: 24,
        }
    }

    #[test]
    fn missing_pane_is_reported() {
        let mut manager = PaneManager::new(32);
        assert!(manager.send_input("missing", b"hello").is_err());
    }

    #[test]
    fn invalid_command_does_not_enter_the_manager() {
        let mut manager = PaneManager::new(32);
        assert!(manager
            .spawn("pane-1", config("spindle-command-that-does-not-exist.exe"))
            .is_err());
        assert!(manager.get("pane-1").is_none());
    }
}
