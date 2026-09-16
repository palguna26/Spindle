use crate::server::session::SessionSnapshot;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub(super) fn render_header(frame: &mut Frame<'_>, area: Rect, snapshot: &SessionSnapshot) {
    if area.is_empty() {
        return;
    }
    let switch = super::layout::mobile_switch_rect(area);
    let title_width = switch.x.saturating_sub(area.x);
    let title = super::active_title(snapshot);
    let title = title
        .chars()
        .take(usize::from(title_width.saturating_sub(1)))
        .collect::<String>();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().fg(Color::Cyan)),
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        Rect::new(area.x, area.y, title_width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " switch",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))),
        switch,
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(" Ctrl-b g for the full navigator"),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}
