use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::session::store::SessionSummary;

pub fn render(
    sessions: &[SessionSummary],
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let popup_area = centered_rect(50, 60, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" sessions ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if sessions.is_empty() {
        let msg = Paragraph::new(vec![
            Line::raw(""),
            Line::raw("  No saved sessions."),
            Line::raw(""),
            Line::styled(
                "  [n] new session    [esc] quit",
                Style::default().fg(Color::DarkGray),
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
                .fg(Color::Cyan)
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
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  ─────────────────────────────────────",
        Style::default().fg(Color::DarkGray),
    ));
    lines.push(Line::styled(
        "  [enter] open    [n] new    [d] delete    [esc] quit",
        Style::default().fg(Color::DarkGray),
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
