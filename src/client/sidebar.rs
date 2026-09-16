use ratatui::layout::Rect;

/// The expanded sidebar is split like Herdr: workspaces above, agent details
/// below, with a draggable one-cell divider between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SidebarSections {
    pub(super) workspaces: Rect,
    pub(super) agents: Rect,
    pub(super) divider: Rect,
}

pub(super) fn content_area(area: Rect) -> Rect {
    Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height)
}

pub(super) fn sections(area: Rect, split_ratio: f32) -> SidebarSections {
    let content = content_area(area);
    if content.is_empty() {
        return SidebarSections {
            workspaces: Rect::default(),
            agents: Rect::default(),
            divider: Rect::default(),
        };
    }

    let (workspace_height, agent_height) = section_heights(content.height, split_ratio);
    let workspaces = Rect::new(content.x, content.y, content.width, workspace_height);
    let agents = Rect::new(
        content.x,
        content.y + workspace_height,
        content.width,
        agent_height,
    );
    let divider = if content.height >= 6 {
        Rect::new(content.x, agents.y, content.width, 1)
    } else {
        Rect::default()
    };
    SidebarSections {
        workspaces,
        agents,
        divider,
    }
}

/// Herdr reserves a header row and footer row in the workspace section.
pub(super) fn workspace_body(sections: SidebarSections) -> Rect {
    Rect::new(
        sections.workspaces.x,
        sections.workspaces.y.saturating_add(1),
        sections.workspaces.width,
        sections.workspaces.height.saturating_sub(2),
    )
}

/// Herdr keeps the divider row, agent heading, and a blank spacer above rows.
pub(super) fn agent_header(sections: SidebarSections) -> Rect {
    Rect::new(
        sections.agents.x,
        sections.agents.y.saturating_add(1),
        sections.agents.width,
        u16::from(sections.agents.height >= 2),
    )
}

pub(super) fn agent_body(sections: SidebarSections) -> Rect {
    Rect::new(
        sections.agents.x,
        sections.agents.y.saturating_add(3),
        sections.agents.width,
        sections.agents.height.saturating_sub(3),
    )
}

fn section_heights(total_height: u16, split_ratio: f32) -> (u16, u16) {
    if total_height == 0 {
        return (0, 0);
    }
    if total_height < 6 {
        let workspace_height = total_height.div_ceil(2);
        return (
            workspace_height,
            total_height.saturating_sub(workspace_height),
        );
    }
    let workspace_height = ((f32::from(total_height) * split_ratio.clamp(0.1, 0.9)).round() as u16)
        .clamp(3, total_height.saturating_sub(3));
    (
        workspace_height,
        total_height.saturating_sub(workspace_height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_herdr_default_split_and_divider() {
        let sections = sections(Rect::new(2, 3, 27, 20), 0.5);
        assert_eq!(sections.workspaces, Rect::new(2, 3, 26, 10));
        assert_eq!(sections.agents, Rect::new(2, 13, 26, 10));
        assert_eq!(sections.divider, Rect::new(2, 13, 26, 1));
    }

    #[test]
    fn preserves_minimum_three_rows_per_section() {
        let sections = sections(Rect::new(0, 0, 20, 6), 0.1);
        assert_eq!(sections.workspaces.height, 3);
        assert_eq!(sections.agents.height, 3);
    }

    #[test]
    fn small_sidebar_splits_without_a_divider() {
        let sections = sections(Rect::new(0, 0, 20, 5), 0.5);
        assert_eq!(sections.workspaces.height, 3);
        assert_eq!(sections.agents.height, 2);
        assert!(sections.divider.is_empty());
    }

    #[test]
    fn reserves_herdr_sidebar_chrome_rows() {
        let sections = sections(Rect::new(0, 0, 20, 20), 0.5);
        assert_eq!(workspace_body(sections).y, sections.workspaces.y + 1);
        assert_eq!(
            workspace_body(sections).height,
            sections.workspaces.height - 2
        );
        assert_eq!(agent_header(sections).y, sections.agents.y + 1);
        assert_eq!(agent_body(sections).y, sections.agents.y + 3);
        assert_eq!(agent_body(sections).height, sections.agents.height - 3);
    }
}
