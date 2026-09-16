use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameTarget {
    Pane,
    Tab,
    Workspace,
    CreateWorkspace,
    CreateTab,
    Space,
    CreateSpace,
    DeleteWorkspace,
    DeleteWorkspaceGroup,
    DeleteSpace,
    CreateWorktree,
    OpenWorktree,
    RemoveWorktree,
    SwitchWorkspace,
    PluginAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePrompt {
    pub target: RenameTarget,
    pub input: String,
    replace_on_type: bool,
}

impl RenamePrompt {
    pub fn new(target: RenameTarget) -> Self {
        Self::with_input(target, String::new(), false)
    }

    pub fn new_tab(default_name: String) -> Self {
        Self::with_input(RenameTarget::CreateTab, default_name, true)
    }

    pub fn with_input(target: RenameTarget, input: String, replace_on_type: bool) -> Self {
        Self {
            target,
            input,
            replace_on_type,
        }
    }

    pub fn apply_key(&mut self, key: KeyCode) -> PromptResult {
        match key {
            KeyCode::Char(character) => {
                if self.replace_on_type {
                    self.input.clear();
                    self.replace_on_type = false;
                }
                self.input.push(character);
            }
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

    #[test]
    fn new_tab_prompt_replaces_herdr_default_name_on_first_key() {
        let mut prompt = RenamePrompt::new_tab("2".into());
        prompt.apply_key(KeyCode::Char('L'));
        prompt.apply_key(KeyCode::Char('o'));
        prompt.apply_key(KeyCode::Char('g'));
        assert_eq!(prompt.input, "Log");
        assert_eq!(
            prompt.apply_key(KeyCode::Enter),
            PromptResult::Submit("Log".into())
        );
    }
}
