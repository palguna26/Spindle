use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameTarget {
    Pane,
    Tab,
    Workspace,
    CreateWorkspace,
    Space,
    CreateSpace,
    DeleteWorkspace,
    DeleteSpace,
    SwitchWorkspace,
    PluginAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePrompt {
    pub target: RenameTarget,
    pub input: String,
}

impl RenamePrompt {
    pub fn new(target: RenameTarget) -> Self {
        Self {
            target,
            input: String::new(),
        }
    }

    pub fn apply_key(&mut self, key: KeyCode) -> PromptResult {
        match key {
            KeyCode::Char(character) => self.input.push(character),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Esc => return PromptResult::Cancel,
            KeyCode::Enter if !self.input.trim().is_empty() => {
                return PromptResult::Submit(self.input.trim().to_string());
            }
            _ => {}
        }
        PromptResult::Continue
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptResult {
    Continue,
    Cancel,
    Submit(String),
}

#[cfg(test)]
mod tests {
    use super::{PromptResult, RenamePrompt, RenameTarget};
    use crossterm::event::KeyCode;

    #[test]
    fn prompt_edits_and_submits_trimmed_text() {
        let mut prompt = RenamePrompt::new(RenameTarget::Tab);
        prompt.apply_key(KeyCode::Char(' '));
        prompt.apply_key(KeyCode::Char('L'));
        prompt.apply_key(KeyCode::Char('o'));
        prompt.apply_key(KeyCode::Char('g'));
        prompt.apply_key(KeyCode::Char('s'));
        assert_eq!(
            prompt.apply_key(KeyCode::Enter),
            PromptResult::Submit("Logs".into())
        );
    }

    #[test]
    fn prompt_can_cancel() {
        let mut prompt = RenamePrompt::new(RenameTarget::Pane);
        assert_eq!(prompt.apply_key(KeyCode::Esc), PromptResult::Cancel);
    }
}
