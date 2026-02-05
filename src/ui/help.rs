use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::config::Theme;

pub fn render(theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 70, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" keybindings ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let heading = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default().fg(Color::White);
    let dim = Style::default().fg(theme.dim);

    let lines = vec![
        Line::raw(""),
        Line::styled("  Workspaces", heading),
        Line::raw(""),
        line_entry("    Ctrl+t         ", "New workspace", key_style, desc_style),
        line_entry("    Ctrl+1..9      ", "Switch workspace", key_style, desc_style),
        line_entry("    Ctrl+Shift+w   ", "Close workspace", key_style, desc_style),
        Line::raw(""),
        Line::styled("  Pane Groups", heading),
        Line::raw(""),
        line_entry("    Alt+1..9       ", "Focus group N", key_style, desc_style),
        line_entry("    Alt+h/j/k/l    ", "Focus group left/down/up/right", key_style, desc_style),
        line_entry("    Ctrl+d         ", "Split right (new group)", key_style, desc_style),
        line_entry("    Ctrl+Shift+d   ", "Split down (new group)", key_style, desc_style),
        Line::raw(""),
        Line::styled("  Tabs", heading),
        Line::raw(""),
        line_entry("    Ctrl+n         ", "New tab", key_style, desc_style),
        line_entry("    Ctrl+Shift+n   ", "New dev server tab", key_style, desc_style),
        line_entry("    Ctrl+Tab       ", "Next tab (or Alt+])", key_style, desc_style),
        line_entry("    Ctrl+Shift+Tab ", "Prev tab (or Alt+[)", key_style, desc_style),
        line_entry("    Ctrl+w         ", "Close tab / group", key_style, desc_style),
        line_entry("    Alt+Shift+hjkl ", "Move tab left/down/up/right", key_style, desc_style),
        Line::raw(""),
        Line::styled("  Resizing", heading),
        Line::raw(""),
        line_entry("    Ctrl+Alt+h/l   ", "Shrink/grow horizontally", key_style, desc_style),
        line_entry("    Ctrl+Alt+j/k   ", "Grow/shrink vertically", key_style, desc_style),
        line_entry("    Ctrl+Alt+=     ", "Equalize all groups", key_style, desc_style),
        Line::raw(""),
        Line::styled("  Management", heading),
        Line::raw(""),
        line_entry("    Ctrl+s         ", "Session picker", key_style, desc_style),
        line_entry("    Ctrl+q         ", "Quit (auto-save)", key_style, desc_style),
        line_entry("    Ctrl+h         ", "This help", key_style, desc_style),
        Line::raw(""),
        Line::styled("  Press Esc to close", dim),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn line_entry<'a>(
    key: &'a str,
    desc: &'a str,
    key_style: Style,
    desc_style: Style,
) -> Line<'a> {
    Line::from(vec![
        ratatui::text::Span::styled(key, key_style),
        ratatui::text::Span::styled(desc, desc_style),
    ])
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
