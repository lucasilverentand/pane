use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use pane_protocol::config::{Action, KeyMap, Theme};
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

/// A section of keybindings for display.
struct Section {
    title: &'static str,
    entries: Vec<(&'static str, Vec<Action>)>,
}

fn keybinding_sections() -> Vec<Section> {
    vec![
        Section {
            title: "Workspaces",
            entries: vec![
                ("New workspace", vec![Action::NewWorkspace]),
                ("Switch workspace", vec![Action::SwitchWorkspace(1)]),
                ("Close workspace", vec![Action::CloseWorkspace]),
            ],
        },
        Section {
            title: "Pane Groups",
            entries: vec![
                ("Focus group N", vec![Action::FocusGroupN(1)]),
                ("Focus left", vec![Action::FocusLeft]),
                ("Focus down", vec![Action::FocusDown]),
                ("Focus up", vec![Action::FocusUp]),
                ("Focus right", vec![Action::FocusRight]),
                ("Split right", vec![Action::SplitHorizontal]),
                ("Split down", vec![Action::SplitVertical]),
            ],
        },
        Section {
            title: "Tabs",
            entries: vec![
                ("New tab", vec![Action::NewTab]),
                ("Dev server tab", vec![Action::DevServerInput]),
                ("Next tab", vec![Action::NextTab]),
                ("Previous tab", vec![Action::PrevTab]),
                ("Close tab", vec![Action::CloseTab]),
                ("Move tab left", vec![Action::MoveTabLeft]),
                ("Move tab down", vec![Action::MoveTabDown]),
                ("Move tab up", vec![Action::MoveTabUp]),
                ("Move tab right", vec![Action::MoveTabRight]),
            ],
        },
        Section {
            title: "Resizing",
            entries: vec![
                ("Shrink horizontally", vec![Action::ResizeShrinkH]),
                ("Grow horizontally", vec![Action::ResizeGrowH]),
                ("Grow vertically", vec![Action::ResizeGrowV]),
                ("Shrink vertically", vec![Action::ResizeShrinkV]),
                ("Equalize", vec![Action::Equalize]),
            ],
        },
        Section {
            title: "Management",
            entries: vec![
                ("Session picker", vec![Action::SessionPicker]),
                ("Command palette", vec![Action::CommandPalette]),
                ("Scroll mode", vec![Action::ScrollMode]),
                ("Copy mode", vec![Action::CopyMode]),
                ("Paste clipboard", vec![Action::PasteClipboard]),
                ("Restart pane", vec![Action::RestartPane]),
                ("Quit", vec![Action::Quit]),
                ("Help", vec![Action::Help]),
            ],
        },
    ]
}

/// Build all help lines dynamically from the KeyMap.
fn build_help_lines<'a>(
    keymap: &KeyMap,
    search: Option<&str>,
    heading: Style,
    key_style: Style,
    desc_style: Style,
) -> Vec<Line<'a>> {
    let reverse = keymap.reverse_map();
    let sections = keybinding_sections();
    let mut lines = Vec::new();

    let search_lower = search.map(|s| s.to_lowercase());

    for section in &sections {
        let mut section_lines: Vec<Line> = Vec::new();

        for (desc, actions) in &section.entries {
            // Collect all keybindings for this entry's actions
            let mut keys_strs = Vec::new();
            for action in actions {
                if let Some(keys) = reverse.get(action) {
                    for key in keys {
                        keys_strs.push(key_event_to_string(key));
                    }
                }
            }

            if keys_strs.is_empty() {
                continue;
            }

            let keys_display = keys_strs.join(", ");
            let entry_text = format!("{}: {}", desc, keys_display);

            // Filter by search if active
            if let Some(ref query) = search_lower {
                if !entry_text.to_lowercase().contains(query) {
                    continue;
                }
            }

            // Pad key display to align descriptions
            let padded_keys = format!("    {:<20}", keys_display);
            section_lines.push(Line::from(vec![
                Span::styled(padded_keys, key_style),
                Span::styled(desc.to_string(), desc_style),
            ]));
        }

        if !section_lines.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(format!("  {}", section.title), heading));
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
    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default().fg(Color::White);
    let dim = Style::default().fg(theme.dim);

    let search_query = help_state.search_input.as_deref();
    let mut lines = build_help_lines(keymap, search_query, heading, key_style, desc_style);

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
        let heading = Style::default();
        let key_style = Style::default();
        let desc_style = Style::default();
        let lines = build_help_lines(&keymap, None, heading, key_style, desc_style);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_help_lines_search_filters() {
        let keymap = KeyMap::from_defaults();
        let heading = Style::default();
        let key_style = Style::default();
        let desc_style = Style::default();
        let all_lines = build_help_lines(&keymap, None, heading, key_style, desc_style);
        let filtered_lines =
            build_help_lines(&keymap, Some("quit"), heading, key_style, desc_style);
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
