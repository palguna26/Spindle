use crate::client::renderer::ClickTarget;
use ratatui::layout::Rect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextMenuTarget {
    Workspace { space_id: String, id: String },
    Tab(String),
    Pane(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextMenuAction {
    Activate,
    NewTab,
    Rename,
    Close,
    Focus,
    SplitRight,
    SplitDown,
    Zoom,
    ToggleRightClickPassthrough,
    Stop,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextMenu {
    pub(crate) target: ContextMenuTarget,
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) selected: usize,
}

impl ContextMenu {
    pub(crate) fn from_target(target: ClickTarget, x: u16, y: u16) -> Option<Self> {
        let target = match target {
            ClickTarget::Workspace {
                space_id,
                workspace_id,
            } => ContextMenuTarget::Workspace {
                space_id,
                id: workspace_id,
            },
            ClickTarget::Tab(id) => ContextMenuTarget::Tab(id),
            ClickTarget::Pane(id) => ContextMenuTarget::Pane(id),
            ClickTarget::SidebarToggle | ClickTarget::Space(_) | ClickTarget::SplitBorder(_) => {
                return None
            }
        };
        Some(Self {
            target,
            x,
            y,
            selected: 0,
        })
    }

    pub(crate) fn items(&self) -> &'static [(&'static str, ContextMenuAction)] {
        use ContextMenuAction as A;
        match &self.target {
            ContextMenuTarget::Workspace { .. } => &[
                ("Open workspace", A::Activate),
                ("New tab", A::NewTab),
                ("Rename workspace", A::Rename),
                ("Close workspace", A::Close),
            ],
            ContextMenuTarget::Tab(_) => &[
                ("Open tab", A::Activate),
                ("New tab", A::NewTab),
                ("Rename tab", A::Rename),
                ("Close tab", A::Close),
            ],
            ContextMenuTarget::Pane(_) => &[
                ("Focus pane", A::Focus),
                ("Split right", A::SplitRight),
                ("Split down", A::SplitDown),
                ("Zoom", A::Zoom),
                (
                    "Toggle right-click passthrough",
                    A::ToggleRightClickPassthrough,
                ),
                ("Rename pane", A::Rename),
                ("Stop pane", A::Stop),
                ("Restart pane", A::Restart),
                ("Close pane", A::Close),
            ],
        }
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        self.selected = (self.selected as isize + delta)
            .clamp(0, self.items().len().saturating_sub(1) as isize)
            as usize;
    }

    pub(crate) fn rect(&self, area: Rect) -> Rect {
        let width = self
            .items()
            .iter()
            .map(|(label, _)| label.len())
            .max()
            .unwrap_or(0)
            .saturating_add(4)
            .max(14)
            .min(usize::from(area.width)) as u16;
        let height = (self.items().len() as u16)
            .saturating_add(2)
            .min(area.height);
        let x = self
            .x
            .min(area.x.saturating_add(area.width.saturating_sub(width)));
        let y = self
            .y
            .min(area.y.saturating_add(area.height.saturating_sub(height)));
        Rect::new(x, y, width, height)
    }

    pub(crate) fn visible_range(&self, area: Rect) -> std::ops::Range<usize> {
        let visible = usize::from(self.rect(area).height.saturating_sub(2));
        let start = self.selected.saturating_add(1).saturating_sub(visible);
        start..start.saturating_add(visible).min(self.items().len())
    }

    pub(crate) fn action_at(&self, area: Rect, x: u16, y: u16) -> Option<ContextMenuAction> {
        let rect = self.rect(area);
        if x < rect.x + 1 || x >= rect.right().saturating_sub(1) || y <= rect.y {
            return None;
        }
        let row = usize::from(y - rect.y - 1);
        if y >= rect.bottom().saturating_sub(1) {
            return None;
        }
        let index = self.visible_range(area).start.saturating_add(row);
        (index < self.items().len()).then(|| self.items()[index].1)
    }
}

#[cfg(test)]
mod tests {
    use super::{ContextMenu, ContextMenuAction, ContextMenuTarget};
    use crate::client::renderer::ClickTarget;
    use ratatui::layout::Rect;

    #[test]
    fn target_builds_the_matching_actions() {
        let pane = ContextMenu::from_target(ClickTarget::Pane("pane-1".into()), 4, 3).unwrap();
        assert_eq!(
            pane.items()[1],
            ("Split right", ContextMenuAction::SplitRight)
        );
        assert_eq!(pane.items().last().unwrap().1, ContextMenuAction::Close);

        let workspace = ContextMenu::from_target(
            ClickTarget::Workspace {
                space_id: "space-1".into(),
                workspace_id: "workspace-1".into(),
            },
            4,
            3,
        )
        .unwrap();
        assert_eq!(
            workspace.target,
            ContextMenuTarget::Workspace {
                space_id: "space-1".into(),
                id: "workspace-1".into(),
            }
        );
    }

    #[test]
    fn menu_selection_and_screen_edge_placement_are_bounded() {
        let mut menu =
            ContextMenu::from_target(ClickTarget::Pane("pane-1".into()), 78, 22).unwrap();
        menu.move_selection(99);
        assert_eq!(menu.selected, menu.items().len() - 1);
        let rect = menu.rect(Rect::new(0, 0, 80, 24));
        assert!(rect.right() <= 80);
        assert!(rect.bottom() <= 24);
        let area = Rect::new(0, 0, 80, 24);
        assert_eq!(
            menu.action_at(area, rect.x + 2, rect.y + 1),
            Some(ContextMenuAction::Focus)
        );
        assert_eq!(menu.action_at(area, rect.x, rect.y + 1), None);
    }

    #[test]
    fn small_menu_keeps_the_selected_item_visible_and_clickable() {
        let area = Rect::new(0, 0, 24, 5);
        let mut menu = ContextMenu::from_target(ClickTarget::Pane("pane-1".into()), 20, 4).unwrap();
        menu.move_selection(6);
        assert_eq!(menu.visible_range(area), 4..7);
        let rect = menu.rect(area);
        assert_eq!(
            menu.action_at(area, rect.x + 2, rect.y + 3),
            Some(ContextMenuAction::Stop)
        );
    }
}
