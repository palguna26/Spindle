use crate::client::navigator::Target;
use crate::client::navigator::{Navigator, Row};
use crate::server::session::SessionSnapshot;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn render_navigator(frame: &mut Frame<'_>, snapshot: &SessionSnapshot, navigator: &Navigator) {
    let area = navigator_area(frame.area());
    let rows = navigator.rows(snapshot);
    let selected = navigator.selected(snapshot);
    let body_height = usize::from(area.height.saturating_sub(3));
    let selected_index = selected
        .as_ref()
        .and_then(|target| rows.iter().position(|row| &row.target == target))
        .unwrap_or(0);
    let start = selected_index.saturating_add(1).saturating_sub(body_height);
    let mut lines = vec![Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Cyan)),
        Span::raw(if navigator.search_focused() {
            format!("{}▏", navigator.query())
        } else if let Some(filter) = navigator.filter_label() {
            format!("filter: {filter}")
        } else if navigator.query().is_empty() {
            "Search spaces, workspaces, tabs, panes".into()
        } else {
            navigator.query().to_owned()
        }),
    ])];
    lines.extend(
        rows.iter()
            .enumerate()
            .skip(start)
            .take(body_height)
            .map(|(index, row)| row_line(row, index == selected_index)),
    );
    lines.push(Line::from(Span::styled(
        " j/k move · Space expand · / search · a/b/w/i/d filter · Enter open · Esc close ",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Session navigator"),
        ),
        area,
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Search,
    Row { target: Target, expand: bool },
    Outside,
    Inside,
}

pub fn hit_test_navigator(
    snapshot: &SessionSnapshot,
    navigator: &Navigator,
    terminal: Rect,
    x: u16,
    y: u16,
) -> Hit {
    let area = navigator_area(terminal);
    if x < area.x || x >= area.right() || y < area.y || y >= area.bottom() {
        return Hit::Outside;
    }
    let content_x = area.x.saturating_add(1);
    if y == area.y.saturating_add(1) {
        return Hit::Search;
    }
    let visible = usize::from(area.height.saturating_sub(3));
    let rows = navigator.rows(snapshot);
    let selected = navigator.selected(snapshot);
    let selected_index = selected
        .as_ref()
        .and_then(|target| rows.iter().position(|row| &row.target == target))
        .unwrap_or(0);
    let start = selected_index.saturating_add(1).saturating_sub(visible);
    let row_index = usize::from(y.saturating_sub(area.y.saturating_add(2)));
    if row_index >= visible {
        return Hit::Inside;
    }
    rows.get(start + row_index)
        .map(|row| Hit::Row {
            target: row.target.clone(),
            expand: matches!(&row.target, Target::Space(_) | Target::Workspace { .. })
                && x <= content_x.saturating_add(3),
        })
        .unwrap_or(Hit::Inside)
}

fn row_line(row: &Row, selected: bool) -> Line<'static> {
    let marker = if selected {
        ">"
    } else if row.current {
        "*"
    } else {
        " "
    };
    let branch = match &row.target {
        crate::client::navigator::Target::Space(_)
        | crate::client::navigator::Target::Workspace { .. } => {
            if row.expanded {
                "▾"
            } else {
                "▸"
            }
        }
        _ => "·",
    };
    let line = format!(
        "{marker} {}{branch} {}  {}",
        "  ".repeat(row.depth),
        row.label,
        row.detail
    );
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if row.current {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };
    Line::from(Span::styled(line, style))
}

fn navigator_area(area: Rect) -> Rect {
    let width = area.width.clamp(1, 90).min(area.width);
    let height = area.height.clamp(1, 24).min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::{hit_test_navigator, Hit};
    use crate::client::navigator::{Navigator, Target};
    use crate::server::session::Session;
    use ratatui::layout::Rect;

    #[test]
    fn navigator_mouse_rows_select_and_expand_like_herdr() {
        let snapshot = Session::default().snapshot().clone();
        let navigator = Navigator::new(&snapshot);
        let terminal = Rect::new(0, 0, 120, 40);
        assert!(matches!(
            hit_test_navigator(&snapshot, &navigator, terminal, 16, 10),
            Hit::Row {
                target: Target::Space(_),
                expand: true
            }
        ));
        assert_eq!(
            hit_test_navigator(&snapshot, &navigator, terminal, 0, 0),
            Hit::Outside
        );
    }
}
