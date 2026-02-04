use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Mode};

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let (left, right) = match &app.mode {
        Mode::Normal => {
            let ws = app.active_workspace();
            let pane_title = ws
                .groups
                .get(&ws.active_group)
                .map(|g| g.active_pane().title.as_str())
                .unwrap_or("?");

            let left = if app.workspaces.len() > 1 {
                format!(
                    "  [{}] {} · ACTIVE",
                    app.active_workspace + 1,
                    pane_title
                )
            } else {
                format!("  {} · ACTIVE", pane_title)
            };
            let right = "ctrl+h help ".to_string();
            (left, right)
        }
        Mode::SessionPicker => (
            String::new(),
            "↑↓ navigate  enter open  n new  d delete  esc quit ".to_string(),
        ),
        Mode::NewPane => (
            String::new(),
            "a agent  n nvim  s shell  d devserver  esc cancel ".to_string(),
        ),
        Mode::Help => (String::new(), "esc close ".to_string()),
        Mode::DevServerInput => (
            String::new(),
            "type command, enter to confirm, esc to cancel ".to_string(),
        ),
    };

    let left_len = left.len();
    let right_len = right.len();
    let padding = (area.width as usize).saturating_sub(left_len + right_len);

    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(padding)),
        Span::styled(right, Style::default().fg(Color::DarkGray)),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
