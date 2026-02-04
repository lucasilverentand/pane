use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::pane::{terminal::render_screen, PaneGroup};

pub fn render_group(group: &PaneGroup, is_active: bool, frame: &mut Frame, area: Rect) {
    let pane = group.active_pane();
    let title = format!("  {} ", pane.title);

    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width <= 2 || inner.height == 0 {
        return;
    }

    // 1-cell padding on left and right
    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height);

    let has_tab_bar = group.tab_count() > 1;

    if has_tab_bar {
        // Split padded area: 1 row for tab bar, rest for content
        let [tab_area, content_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(padded);

        render_tab_bar(group, is_active, frame, tab_area);

        let lines: Vec<Line<'static>> = render_screen(pane.screen(), content_area);
        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, content_area);
    } else {
        let lines: Vec<Line<'static>> = render_screen(pane.screen(), padded);
        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, padded);
    }
}

fn render_tab_bar(group: &PaneGroup, _is_active: bool, frame: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active_tab = i == group.active_tab;
        let label = format!("[{}]", tab.title);

        let style = if is_active_tab {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let prefix = if is_active_tab { "*" } else { " " };
        spans.push(Span::styled(format!("{}{} ", prefix, label), style));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
