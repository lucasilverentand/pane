use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(50, 30, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" new tab ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let dim_style = Style::default().fg(Color::DarkGray);

    let lines = vec![
        Line::raw(""),
        Line::from(vec![
            ratatui::text::Span::styled("   [a]  ", key_style),
            ratatui::text::Span::styled("AI Agent     ", desc_style),
            ratatui::text::Span::styled("(claude)", dim_style),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("   [n]  ", key_style),
            ratatui::text::Span::styled("Neovim       ", desc_style),
            ratatui::text::Span::styled("(editor)", dim_style),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("   [s]  ", key_style),
            ratatui::text::Span::styled("Shell        ", desc_style),
            ratatui::text::Span::styled("(terminal)", dim_style),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("   [d]  ", key_style),
            ratatui::text::Span::styled("Dev Server   ", desc_style),
            ratatui::text::Span::styled("(command)", dim_style),
        ]),
        Line::raw(""),
        Line::styled("   [esc] cancel", dim_style),
    ];

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
