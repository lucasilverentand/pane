use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::config::Theme;
use crate::session::store::SessionSummary;

pub fn render(
    sessions: &[SessionSummary],
    selected: usize,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let popup_area = centered_rect(50, 60, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("───── sessions ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if sessions.is_empty() {
        let msg = Paragraph::new(vec![
            Line::raw(""),
            Line::raw("  No saved sessions."),
            Line::raw(""),
            Line::styled(
                "  [n] new session    [q] quit",
                Style::default().fg(theme.dim),
            ),
        ]);
        frame.render_widget(msg, inner);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::raw(""));

    for (i, session) in sessions.iter().enumerate() {
        let is_selected = i == selected;
        let prefix = if is_selected { "  ▸ " } else { "    " };
        let style = if is_selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let pane_count = format!("{} panes", session.pane_count);
        let time_ago = format_time_ago(session.updated_at);

        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(&session.name, style),
            Span::styled(
                format!("    {}   {}", pane_count, time_ago),
                Style::default().fg(theme.dim),
            ),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  ─────────────────────────────────────",
        Style::default().fg(theme.dim),
    ));
    lines.push(Line::styled(
        "  [enter] open    [n] new    [d] delete    [q] quit",
        Style::default().fg(theme.dim),
    ));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

fn format_time_ago(time: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now - time;

    if diff.num_minutes() < 1 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else {
        format!("{}d ago", diff.num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_format_time_ago_just_now() {
        let time = Utc::now();
        assert_eq!(format_time_ago(time), "just now");
    }

    #[test]
    fn test_format_time_ago_seconds_ago() {
        let time = Utc::now() - Duration::seconds(30);
        assert_eq!(format_time_ago(time), "just now");
    }

    #[test]
    fn test_format_time_ago_minutes() {
        let time = Utc::now() - Duration::minutes(5);
        assert_eq!(format_time_ago(time), "5m ago");
    }

    #[test]
    fn test_format_time_ago_59_minutes() {
        let time = Utc::now() - Duration::minutes(59);
        assert_eq!(format_time_ago(time), "59m ago");
    }

    #[test]
    fn test_format_time_ago_hours() {
        let time = Utc::now() - Duration::hours(3);
        assert_eq!(format_time_ago(time), "3h ago");
    }

    #[test]
    fn test_format_time_ago_23_hours() {
        let time = Utc::now() - Duration::hours(23);
        assert_eq!(format_time_ago(time), "23h ago");
    }

    #[test]
    fn test_format_time_ago_days() {
        let time = Utc::now() - Duration::days(7);
        assert_eq!(format_time_ago(time), "7d ago");
    }

    #[test]
    fn test_format_time_ago_1_day() {
        let time = Utc::now() - Duration::hours(25);
        assert_eq!(format_time_ago(time), "1d ago");
    }
}
