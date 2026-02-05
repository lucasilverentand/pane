use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Mode};
use crate::config::Theme;

pub fn render(app: &App, theme: &Theme, frame: &mut Frame, area: Rect) {
    let (left, right) = match &app.mode {
        Mode::Normal => {
            let left = build_left(app);
            let mut right_parts: Vec<String> = Vec::new();
            // System stats
            if app.config.status_bar.show_cpu {
                right_parts.push(app.system_stats.format_cpu());
            }
            if app.config.status_bar.show_memory {
                right_parts.push(app.system_stats.format_memory());
            }
            if app.config.status_bar.show_load {
                right_parts.push(app.system_stats.format_load());
            }
            if app.config.status_bar.show_disk {
                right_parts.push(app.system_stats.format_disk());
            }
            let stats = if right_parts.is_empty() {
                String::new()
            } else {
                format!("{}  ", right_parts.join(" │ "))
            };
            let right = format!("{}ctrl+h help ", stats);
            (left, right)
        }
        Mode::Scroll => {
            let left = build_left(app);
            let right = "j/k ↑↓  u/d page  g/G top/end  esc quit ".to_string();
            (left, right)
        }
        Mode::SessionPicker => (
            String::new(),
            "↑↓ navigate  enter open  n new  d delete  q quit ".to_string(),
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
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(padding)),
        Span::styled(right, Style::default().fg(theme.dim)),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

fn build_left(app: &App) -> String {
    if app.workspaces.is_empty() {
        return String::new();
    }

    let ws = app.active_workspace();
    let pane_title = ws
        .groups
        .get(&ws.active_group)
        .map(|g| g.active_pane().title.as_str())
        .unwrap_or("");

    format!(" {} ", pane_title)
}
