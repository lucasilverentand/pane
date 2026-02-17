use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::LeaderState;
use crate::config::{LeaderNode, Theme};

pub fn render(ls: &LeaderState, theme: &Theme, frame: &mut Frame, area: Rect) {
    if !ls.popup_visible {
        return;
    }

    let children = match &ls.current_node {
        LeaderNode::Group { children, .. } => children,
        _ => return,
    };

    // Build sorted entries: (display_key, label, is_group)
    let mut entries: Vec<(String, String, bool)> = children
        .iter()
        .map(|(key, node)| {
            let display = format_key_short(key);
            match node {
                LeaderNode::Leaf { label, .. } => (display, label.clone(), false),
                LeaderNode::Group { label, .. } => (display, format!("+{}", label), true),
                LeaderNode::PassThrough => (display, "passthrough".into(), false),
            }
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // Calculate popup size
    let max_key_len = entries.iter().map(|(k, _, _)| k.len()).max().unwrap_or(1);
    let max_label_len = entries.iter().map(|(_, l, _)| l.len()).max().unwrap_or(1);
    let content_width = (max_key_len + 2 + max_label_len).min(38) as u16;
    let popup_width = content_width + 4; // borders + padding
    let popup_height = (entries.len() as u16 + 2).min(area.height.saturating_sub(2)); // borders

    // Position: bottom-right, above status bar (1 row)
    let x = area.width.saturating_sub(popup_width);
    let y = area.height.saturating_sub(popup_height + 1); // +1 for status bar
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.dim));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let max_visible = inner.height as usize;
    let lines: Vec<Line> = entries
        .iter()
        .take(max_visible)
        .map(|(key, label, is_group)| {
            let key_style = Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD);
            let label_style = if *is_group {
                Style::default().fg(theme.accent)
            } else {
                Style::default().fg(theme.fg)
            };
            let pad = " ".repeat(max_key_len.saturating_sub(key.len()));
            Line::from(vec![
                Span::raw(" "),
                Span::styled(key.clone(), key_style),
                Span::raw(pad),
                Span::raw("  "),
                Span::styled(label.clone(), label_style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn format_key_short(key: &crossterm::event::KeyEvent) -> String {
    let mut parts = Vec::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("C");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("A");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("S");
    }
    let code = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "CR".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::BackTab => "S-Tab".into(),
        KeyCode::Backspace => "BS".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::F(n) => format!("F{}", n),
        _ => "?".into(),
    };
    if parts.is_empty() {
        code
    } else {
        format!("{}-{}", parts.join("-"), code)
    }
}
