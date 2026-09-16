use super::agent_detection::AgentAuthority;
use super::agent_detection::{observe_agent_process, AgentStartupGrace, PendingIdleConfirmation};
use super::PaneEvent;
use crate::detect::{self, AgentKind, AgentProcessScan, AgentState};
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
    pub agent_done: bool,
    agent_missing_scans: u8,
    agent_authority: Option<AgentAuthority>,
    pub scrollback: VecDeque<u8>,
    pub terminal: TerminalEmulator,
    session: PtySession,
    pending_idle: PendingIdleConfirmation,
    startup_grace: AgentStartupGrace,
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
                agent_done: false,
                agent_missing_scans: 0,
                agent_authority: None,
                scrollback: VecDeque::with_capacity(self.scrollback_limit),
                terminal: TerminalEmulator::new(rows, cols, self.scrollback_limit),
                session,
                pending_idle: PendingIdleConfirmation::default(),
                startup_grace: AgentStartupGrace::default(),
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
        pane.pending_idle.clear();
        pane.status = PaneStatus::Halted {
            reason: "stopped by user".into(),
        };
        if pane.agent.is_some() {
            pane.agent_state = Some(AgentState::Idle);
            pane.agent_done = true;
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
            let previous_agent = pane.agent;
            let previous_agent_state = pane.agent_state;
            let mut agent_changed = false;
            let mut agent_exited = false;
            if scan_agents && pane.status.is_running() && pane.agent_authority.is_none() {
                let scan = pane
                    .session
                    .process_id()
                    .map(detect::detect_in_process_tree)
                    .unwrap_or(AgentProcessScan::Unavailable);
                let (changed, exited) =
                    observe_agent_process(&mut pane.agent, scan, &mut pane.agent_missing_scans);
                agent_changed = changed;
                agent_exited = exited;
                if matches!(scan, AgentProcessScan::Found(_)) {
                    pane.agent_done = false;
                }
                if agent_changed {
                    pane.terminal.clear_agent_osc_evidence();
                    pane.pending_idle.clear();
                }
                if agent_exited {
                    pane.pending_idle.clear();
                    pane.terminal.clear_agent_osc_evidence();
                    pane.startup_grace.start(None, Instant::now());
                    pane.agent_state = Some(AgentState::Idle);
                    pane.agent_done = true;
                }
                if agent_changed {
                    pane.startup_grace.start(pane.agent, Instant::now());
                    pane.agent_state = Some(AgentState::Unknown);
                }
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
                if pane.agent_authority.is_none() {
                    if let Some(agent) = pane.agent.filter(|_| pane.agent_missing_scans == 0) {
                        if pane.startup_grace.is_active(Instant::now()) {
                            pane.pending_idle.clear();
                        } else if !detect::should_skip_state_update(agent, &terminal.contents) {
                            let next_state = detect::detect_state_with_osc(
                                agent,
                                &terminal.contents,
                                &terminal.osc_title,
                                &terminal.osc_progress,
                            );
                            let visible_idle = detect::has_visible_idle_signal(
                                agent,
                                &terminal.contents,
                                &terminal.osc_title,
                                &terminal.osc_progress,
                            );
                            if !pane.pending_idle.should_hold(
                                pane.agent_state,
                                next_state,
                                visible_idle,
                                agent_changed,
                                false,
                                Instant::now(),
                            ) {
                                pane.agent_state = Some(next_state);
                            }
                        }
                    }
                } else {
                    pane.pending_idle.clear();
                    if pane.agent.is_none() {
                        pane.agent_state = None;
                        pane.agent_done = false;
                    }
                }
            }

            if pane.status.is_running() {
                if let Ok(Some(exit_code)) = pane.session.try_wait() {
                    pane.pending_idle.clear();
                    if pane.agent.is_some() {
                        pane.agent_state = Some(AgentState::Idle);
                        pane.agent_done = true;
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

            if agent_changed {
                events.push(PaneEvent::AgentDetected {
                    pane_id: pane.id.clone(),
                    agent: pane.agent,
                    released: false,
                    final_status: None,
                });
            } else if agent_exited {
                events.push(PaneEvent::AgentDetected {
                    pane_id: pane.id.clone(),
                    agent: pane.agent,
                    released: true,
                    final_status: pane.agent_state,
                });
            } else if previous_agent != pane.agent {
                events.push(PaneEvent::AgentDetected {
                    pane_id: pane.id.clone(),
                    agent: pane.agent,
                    released: pane.agent.is_none(),
                    final_status: pane.agent_state,
                });
            }
            if previous_agent_state != pane.agent_state {
                if let Some(agent_state) = pane.agent_state {
                    events.push(PaneEvent::AgentStatusChanged {
                        pane_id: pane.id.clone(),
                        agent_state,
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

    pub fn process_id(&self, id: &str) -> Result<Option<u32>, PaneManagerError> {
        self.panes
            .get(id)
            .map(|pane| pane.session.process_id())
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))
    }

    pub fn report_agent(
        &mut self,
        id: &str,
        agent: AgentKind,
        state: AgentState,
        source: String,
        seq: Option<u64>,
    ) -> Result<bool, PaneManagerError> {
        let pane = self
            .panes
            .get_mut(id)
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))?;
        if pane
            .agent_authority
            .as_ref()
            .is_some_and(|authority| !authority.accepts(&source, seq))
        {
            return Ok(false);
        }
        let changed = pane.agent != Some(agent) || pane.agent_state != Some(state);
        pane.agent = Some(agent);
        pane.agent_state = Some(state);
        pane.agent_done = false;
        pane.agent_missing_scans = 0;
        pane.pending_idle.clear();
        pane.startup_grace.start(None, Instant::now());
        pane.agent_authority = Some(AgentAuthority { source, seq });
        Ok(changed)
    }

    pub fn release_agent(
        &mut self,
        id: &str,
        source: &str,
        agent: AgentKind,
        seq: Option<u64>,
    ) -> Result<bool, PaneManagerError> {
        let pane = self
            .panes
            .get_mut(id)
            .ok_or_else(|| PaneManagerError::MissingPane(id.into()))?;
        if pane.agent != Some(agent)
            || pane
                .agent_authority
                .as_ref()
                .is_some_and(|authority| authority.source != source)
            || pane
                .agent_authority
                .as_ref()
                .is_some_and(|authority| !authority.accepts(source, seq))
        {
            return Ok(false);
        }
        pane.agent = None;
        pane.agent_state = None;
        pane.agent_done = false;
        pane.agent_missing_scans = 0;
        pane.agent_authority = None;
        pane.pending_idle.clear();
        pane.terminal.clear_agent_osc_evidence();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::super::agent_detection::{
        AgentStartupGrace, PendingIdleConfirmation, AGENT_EXIT_CONFIRMATIONS,
        AGENT_STARTUP_GRACE_WINDOW, IDLE_CONFIRM_CAP, IDLE_CONFIRM_INTERVAL,
    };
    use super::{observe_agent_process, PaneConfig, PaneManager};
    use crate::detect::{AgentKind, AgentProcessScan, AgentState};
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

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

    #[test]
    fn agent_identity_survives_a_missed_scan_and_finishes_after_confirmation() {
        let mut missing_scans = 0;
        let mut agent = Some(AgentKind::Claude);
        assert_eq!(
            observe_agent_process(&mut agent, AgentProcessScan::Absent, &mut missing_scans,),
            (false, false)
        );
        assert_eq!(agent, Some(AgentKind::Claude));
        assert_eq!(missing_scans, 1);
        assert_eq!(
            observe_agent_process(
                &mut agent,
                AgentProcessScan::Found(AgentKind::Claude),
                &mut missing_scans,
            ),
            (false, false)
        );
        assert_eq!(missing_scans, 0);
        assert_eq!(
            observe_agent_process(&mut agent, AgentProcessScan::Absent, &mut missing_scans,),
            (false, false)
        );
        assert_eq!(
            observe_agent_process(&mut agent, AgentProcessScan::Absent, &mut missing_scans,),
            (false, true)
        );
        assert_eq!(agent, Some(AgentKind::Claude));
        assert_eq!(missing_scans, AGENT_EXIT_CONFIRMATIONS);
        assert_eq!(
            observe_agent_process(
                &mut agent,
                AgentProcessScan::Found(AgentKind::Codex),
                &mut missing_scans,
            ),
            (true, false)
        );
        assert_eq!(agent, Some(AgentKind::Codex));
        assert_eq!(missing_scans, 0);
    }

    #[test]
    fn unavailable_process_scans_do_not_change_agent_identity_or_status() {
        let mut missing_scans = 1;
        let mut agent = Some(AgentKind::Claude);
        assert_eq!(
            observe_agent_process(
                &mut agent,
                AgentProcessScan::Unavailable,
                &mut missing_scans,
            ),
            (false, false)
        );
        assert_eq!(agent, Some(AgentKind::Claude));
        assert_eq!(missing_scans, 0);
        assert_eq!(
            observe_agent_process(&mut agent, AgentProcessScan::Absent, &mut missing_scans,),
            (false, false)
        );
    }

    #[test]
    fn agent_startup_grace_is_active_until_three_seconds_pass() {
        let started = Instant::now();
        let mut grace = AgentStartupGrace::default();
        grace.start(Some(AgentKind::Claude), started);

        assert!(grace.is_active(started + Duration::from_secs(2)));
        assert!(!grace.is_active(started + AGENT_STARTUP_GRACE_WINDOW));
        assert!(!grace.is_active(started + AGENT_STARTUP_GRACE_WINDOW));
    }

    #[test]
    fn removing_agent_does_not_start_a_detection_grace_period() {
        let started = Instant::now();
        let mut grace = AgentStartupGrace::default();
        grace.start(Some(AgentKind::Claude), started);
        grace.start(None, started + Duration::from_secs(1));
        assert!(!grace.is_active(started + Duration::from_secs(1)));
    }

    #[test]
    fn plain_idle_waits_for_three_confirmation_checks() {
        let mut pending = PendingIdleConfirmation::default();
        let started = Instant::now();
        let hold = |pending: &mut PendingIdleConfirmation, now| {
            pending.should_hold(
                Some(AgentState::Working),
                AgentState::Idle,
                false,
                false,
                false,
                now,
            )
        };

        assert!(hold(&mut pending, started));
        assert!(hold(&mut pending, started + Duration::from_millis(50)));
        assert!(hold(&mut pending, started + IDLE_CONFIRM_INTERVAL));
        assert!(hold(&mut pending, started + IDLE_CONFIRM_INTERVAL * 2));
        assert!(!hold(&mut pending, started + IDLE_CONFIRM_INTERVAL * 3));
    }

    #[test]
    fn visible_idle_agent_change_and_process_exit_bypass_confirmation() {
        for (visible_idle, agent_changed, process_exited) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let mut pending = PendingIdleConfirmation::default();
            let now = Instant::now();
            assert!(pending.should_hold(
                Some(AgentState::Working),
                AgentState::Idle,
                false,
                false,
                false,
                now,
            ));
            assert!(!pending.should_hold(
                Some(AgentState::Working),
                AgentState::Idle,
                visible_idle,
                agent_changed,
                process_exited,
                now + IDLE_CONFIRM_INTERVAL,
            ));
        }
    }

    #[test]
    fn pending_idle_releases_at_safety_cap() {
        let mut pending = PendingIdleConfirmation::default();
        let started = Instant::now();
        assert!(pending.should_hold(
            Some(AgentState::Working),
            AgentState::Idle,
            false,
            false,
            false,
            started,
        ));
        assert!(!pending.should_hold(
            Some(AgentState::Working),
            AgentState::Idle,
            false,
            false,
            false,
            started + IDLE_CONFIRM_CAP,
        ));
    }
}
