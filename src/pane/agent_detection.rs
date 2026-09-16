use crate::detect::{AgentKind, AgentProcessScan, AgentState};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentSessionInfo {
    pub source: String,
    pub agent: String,
    pub kind: AgentSessionRefKind,
    pub value: String,
}

#[derive(Debug)]
pub(crate) struct AgentReport {
    pub(crate) agent: AgentKind,
    pub(crate) state: AgentState,
    pub(crate) source: String,
    pub(crate) seq: Option<u64>,
    pub(crate) session_id: Option<String>,
    pub(crate) session_path: Option<String>,
}

#[derive(Debug)]
pub(crate) struct AgentSessionReport {
    pub(crate) agent: AgentKind,
    pub(crate) source: String,
    pub(crate) seq: Option<u64>,
    pub(crate) session_id: Option<String>,
    pub(crate) session_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionRefKind {
    Id,
    Path,
}

impl AgentSessionInfo {
    pub(super) fn from_report(
        source: &str,
        agent: AgentKind,
        session_id: Option<String>,
        session_path: Option<String>,
    ) -> Option<Self> {
        let (kind, value) = session_path
            .filter(|value| !value.is_empty())
            .map(|value| (AgentSessionRefKind::Path, value))
            .or_else(|| {
                session_id
                    .filter(|value| !value.is_empty())
                    .map(|value| (AgentSessionRefKind::Id, value))
            })?;
        Some(Self {
            source: source.into(),
            agent: agent.label().into(),
            kind,
            value,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentAuthority {
    pub(super) source: String,
    pub(super) seq: Option<u64>,
}

impl AgentAuthority {
    pub(super) fn accepts(&self, source: &str, seq: Option<u64>) -> bool {
        self.source != source
            || match (self.seq, seq) {
                (Some(current), Some(next)) => next > current,
                (Some(_), None) => false,
                _ => true,
            }
    }
}

pub(super) const IDLE_CONFIRM_INTERVAL: Duration = Duration::from_millis(100);
pub(super) const IDLE_CONFIRM_CAP: Duration = Duration::from_millis(700);
pub(super) const IDLE_CONFIRMATIONS: u8 = 3;
pub(super) const AGENT_EXIT_CONFIRMATIONS: u8 = 2;
pub(super) const AGENT_STARTUP_GRACE_WINDOW: Duration = Duration::from_secs(3);

#[derive(Debug, Default)]
pub(super) struct AgentStartupGrace {
    until: Option<Instant>,
}

impl AgentStartupGrace {
    pub(super) fn start(&mut self, agent: Option<AgentKind>, now: Instant) {
        self.until = agent.map(|_| now + AGENT_STARTUP_GRACE_WINDOW);
    }

    pub(super) fn is_active(&mut self, now: Instant) -> bool {
        let Some(until) = self.until else {
            return false;
        };
        if now < until {
            return true;
        }
        self.until = None;
        false
    }
}

#[derive(Debug, Default)]
pub(super) struct PendingIdleConfirmation {
    started_at: Option<Instant>,
    last_check: Option<Instant>,
    confirmations: u8,
}

impl PendingIdleConfirmation {
    pub(super) fn clear(&mut self) {
        self.started_at = None;
        self.last_check = None;
        self.confirmations = 0;
    }

    pub(super) fn should_hold(
        &mut self,
        previous: Option<AgentState>,
        next: AgentState,
        visible_idle: bool,
        agent_changed: bool,
        process_exited: bool,
        now: Instant,
    ) -> bool {
        if previous != Some(AgentState::Working)
            || next != AgentState::Idle
            || visible_idle
            || agent_changed
            || process_exited
        {
            self.clear();
            return false;
        }

        let Some(started_at) = self.started_at else {
            self.started_at = Some(now);
            self.last_check = Some(now);
            return true;
        };

        if now.duration_since(started_at) >= IDLE_CONFIRM_CAP {
            self.clear();
            return false;
        }
        if self
            .last_check
            .is_some_and(|last_check| now.duration_since(last_check) < IDLE_CONFIRM_INTERVAL)
        {
            return true;
        }

        self.last_check = Some(now);
        self.confirmations = self.confirmations.saturating_add(1);
        if self.confirmations >= IDLE_CONFIRMATIONS {
            self.clear();
            false
        } else {
            true
        }
    }
}

pub(super) fn observe_agent_process(
    current: &mut Option<AgentKind>,
    scan: AgentProcessScan,
    missing_scans: &mut u8,
) -> (bool, bool) {
    if let AgentProcessScan::Found(detected) = scan {
        let changed = *current != Some(detected);
        *current = Some(detected);
        *missing_scans = 0;
        return (changed, false);
    }
    if scan == AgentProcessScan::Unavailable {
        *missing_scans = 0;
        return (false, false);
    }
    if current.is_none() {
        *missing_scans = 0;
        return (false, false);
    }
    *missing_scans = missing_scans.saturating_add(1);
    (false, *missing_scans == AGENT_EXIT_CONFIRMATIONS)
}

#[cfg(test)]
mod tests {
    use super::AgentAuthority;

    #[test]
    fn authority_accepts_newer_sequences_and_rejects_replays() {
        let authority = AgentAuthority {
            source: "herdr:codex".into(),
            seq: Some(4),
        };
        assert!(!authority.accepts("herdr:codex", Some(4)));
        assert!(!authority.accepts("herdr:codex", Some(3)));
        assert!(authority.accepts("herdr:codex", Some(5)));
        assert!(authority.accepts("custom:codex", Some(1)));
    }

    #[test]
    fn authority_without_sequence_accepts_unsequenced_updates() {
        let authority = AgentAuthority {
            source: "custom:agent".into(),
            seq: None,
        };
        assert!(authority.accepts("custom:agent", None));
        assert!(authority.accepts("custom:agent", Some(1)));
    }
}
