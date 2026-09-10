#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneStatus {
    Running,
    Completed { exit_code: i32 },
    Halted { reason: String },
    Interrupted { reason: String },
}

impl PaneStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_successful(&self) -> bool {
        matches!(self, Self::Completed { exit_code: 0 })
    }

    pub fn indicator(&self) -> char {
        match self {
            Self::Running => '◉',
            Self::Completed { .. } => '●',
            Self::Halted { .. } | Self::Interrupted { .. } => '●',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PaneStatus;

    #[test]
    fn lifecycle_helpers_match_status() {
        assert!(PaneStatus::Running.is_running());
        assert!(PaneStatus::Completed { exit_code: 0 }.is_successful());
        assert!(!PaneStatus::Completed { exit_code: 1 }.is_successful());
    }
}
