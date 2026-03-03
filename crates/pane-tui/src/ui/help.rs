use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use pane_protocol::config::{KeyMap, Theme};
use pane_protocol::registry;
use crate::ui::command_palette::key_event_to_string;

/// State for the help overlay (scroll and search).
pub struct HelpState {
    pub scroll_offset: usize,
    pub search_input: Option<String>,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            search_input: None,
        }
    }
}

/// Build all help lines dynamically from the KeyMap and action registry.
fn build_help_lines<'a>(
    keymap: &KeyMap,
    search: Option<&str>,
    width: usize,
    heading: Style,
    dim: Style,
    desc_style: Style,
) -> Vec<Line<'a>> {
    let reverse = keymap.reverse_map();
    let categories = registry::actions_by_category();
    let mut lines = Vec::new();

    let search_lower = search.map(|s| s.to_lowercase());
    let indent = 4usize;
    let right_margin = 2usize;

    for (category, actions) in &categories {
        let mut section_lines: Vec<Line> = Vec::new();

        for meta in actions {
            let mut keys_strs = Vec::new();
            if let Some(keys) = reverse.get(&meta.action) {
                for key in keys {
                    keys_strs.push(key_event_to_string(key));
                }
            }

            if keys_strs.is_empty() {
                continue;
            }

            let keys_display = keys_strs.join(", ");
            let entry_text = format!("{}: {}", meta.display_name, keys_display);

            // Filter by search if active
            if let Some(ref query) = search_lower {
                if !entry_text.to_lowercase().contains(query) {
                    continue;
                }
            }

            // Description left, keybinds right-aligned and dimmed
            let left = format!("{:indent$}{}", "", meta.display_name, indent = indent);
            let used = left.len() + keys_display.len() + right_margin;
            let gap = if width > used { width - used } else { 2 };
            let padding = " ".repeat(gap);

            section_lines.push(Line::from(vec![
                Span::styled(left, desc_style),
                Span::raw(padding),
                Span::styled(keys_display, dim),
            ]));
        }

        if !section_lines.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(format!("  {}", category.label()), heading));
            lines.push(Line::raw(""));
            lines.append(&mut section_lines);
        }
    }

    lines
}

pub fn render(
    keymap: &KeyMap,
    help_state: &HelpState,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let popup_area = centered_rect(60, 70, area);

    frame.render_widget(Clear, popup_area);

    let title = if let Some(ref search) = help_state.search_input {
        format!(" keybindings  /{}_ ", search)
    } else {
        " keybindings ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let heading = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let dim = Style::default().fg(theme.dim);

    let search_query = help_state.search_input.as_deref();
    let width = inner.width as usize;
    let mut lines = build_help_lines(keymap, search_query, width, heading, dim, desc_style);

    lines.push(Line::raw(""));
    let hint = if help_state.search_input.is_some() {
        "  Esc clear search  j/k scroll"
    } else {
        "  Esc close  / search  j/k scroll"
    };
    lines.push(Line::styled(hint, dim));

    // Apply scroll offset
    let visible_height = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(visible_height);
    let scroll = help_state.scroll_offset.min(max_scroll);

    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
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

#[cfg(test)]
mod tests {
    use super::*;
    use pane_protocol::config::KeyMap;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_help_lines_not_empty() {
        let keymap = KeyMap::from_defaults();
        let s = Style::default();
        let lines = build_help_lines(&keymap, None, 80, s, s, s);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_help_lines_search_filters() {
        let keymap = KeyMap::from_defaults();
        let s = Style::default();
        let all_lines = build_help_lines(&keymap, None, 80, s, s, s);
        let filtered_lines = build_help_lines(&keymap, Some("quit"), 80, s, s, s);
        assert!(filtered_lines.len() < all_lines.len());
    }

    #[test]
    fn test_help_state_default() {
        let state = HelpState::default();
        assert_eq!(state.scroll_offset, 0);
        assert!(state.search_input.is_none());
    }

    #[test]
    fn test_key_event_to_string_variants() {
        use crate::ui::command_palette::key_event_to_string;
        let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_string(&ctrl_q), "Ctrl+q");

        let alt_shift_h = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::ALT);
        assert_eq!(key_event_to_string(&alt_shift_h), "Alt+H");

        let pageup = KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT);
        assert_eq!(key_event_to_string(&pageup), "Shift+PageUp");
    }
}
