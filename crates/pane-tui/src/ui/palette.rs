use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use std::collections::HashMap;

use pane_protocol::config::{Action, KeyMap, LeaderConfig, LeaderNode, Theme};
use pane_protocol::registry;

use super::dialog;

// ---------------------------------------------------------------------------
// PaletteEntry — a single item in the palette
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PaletteEntry {
    pub action: Action,
    pub name: String,
    pub keybind: Option<String>,
    pub description: String,
    pub category: &'static str,
}

// ---------------------------------------------------------------------------
// UnifiedPaletteState
// ---------------------------------------------------------------------------

pub struct UnifiedPaletteState {
    pub input: String,
    pub selected: usize,
    all_entries: Vec<PaletteEntry>,
    pub filtered: Vec<usize>,
    /// Pre-selected action from a leader key match (highlighted in palette)
    pub highlighted_action: Option<Action>,
}

impl UnifiedPaletteState {
    /// Create a full-search palette with all actions.
    pub fn new_full_search(keymap: &KeyMap, leader: &LeaderConfig) -> Self {
        let all_entries = build_palette_entries(keymap, leader);
        let filtered: Vec<usize> = (0..all_entries.len()).collect();
        Self {
            input: String::new(),
            selected: 0,
            all_entries,
            filtered,
            highlighted_action: None,
        }
    }

    pub fn update_filter(&mut self) {
        if self.input.is_empty() && self.highlighted_action.is_none() {
            self.filtered = (0..self.all_entries.len()).collect();
        } else {
            let query_lower = self.input.to_lowercase();
            self.filtered = self
                .all_entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    if query_lower.is_empty() {
                        return true;
                    }
                    e.name.to_lowercase().contains(&query_lower)
                        || e.description.to_lowercase().contains(&query_lower)
                })
                .map(|(i, _)| i)
                .collect();

            // If we have a highlighted action, sort it to the front
            if let Some(ref ha) = self.highlighted_action {
                self.filtered.sort_by_key(|&i| {
                    if self.all_entries[i].action == *ha {
                        0
                    } else {
                        1
                    }
                });
            }
        }
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }

    pub fn move_up(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn selected_action(&self) -> Option<Action> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.all_entries.get(i))
            .map(|e| e.action.clone())
    }
}

// ---------------------------------------------------------------------------
// Build entries
// ---------------------------------------------------------------------------

fn build_palette_entries(keymap: &KeyMap, leader: &LeaderConfig) -> Vec<PaletteEntry> {
    let reverse = keymap.reverse_map();
    let leader_binds = leader_bindings(leader);

    let categories = registry::actions_by_category();
    let mut entries = Vec::new();

    for (category, actions) in &categories {
        for meta in actions {
            if !meta.palette_visible {
                continue;
            }
            let mut hints = Vec::new();

            if let Some(keys) = reverse.get(&meta.action) {
                for k in keys {
                    hints.push(key_event_to_string(k));
                }
            }

            if let Some(leader_hint) = leader_binds.get(&meta.action) {
                hints.push(leader_hint.clone());
            }

            let keybind = if hints.is_empty() {
                None
            } else {
                Some(hints.join(", "))
            };

            entries.push(PaletteEntry {
                action: meta.action.clone(),
                name: meta.display_name.to_string(),
                keybind,
                description: meta.description.to_string(),
                category: category.label(),
            });
        }
    }

    entries
}

/// Walk the leader tree and return the shortest key sequence for each action.
fn leader_bindings(leader: &LeaderConfig) -> HashMap<Action, String> {
    let leader_str = key_event_to_string(&leader.key);
    let mut all: Vec<(Action, String)> = Vec::new();

    fn walk(node: &LeaderNode, path: &str, all: &mut Vec<(Action, String)>) {
        match node {
            LeaderNode::Leaf { action, .. } => {
                all.push((action.clone(), path.to_string()));
            }
            LeaderNode::Group { children, .. } => {
                for (key, child) in children {
                    let child_path = format!("{} {}", path, key_event_to_string(key));
                    walk(child, &child_path, all);
                }
            }
            LeaderNode::PassThrough => {}
        }
    }

    if let LeaderNode::Group { children, .. } = &leader.root {
        for (key, child) in children {
            let path = format!("{} {}", leader_str, key_event_to_string(key));
            walk(child, &path, &mut all);
        }
    }

    all.sort_by_key(|(_, path)| path.len());
    let mut result: HashMap<Action, String> = HashMap::new();
    for (action, path) in all {
        result.entry(action).or_insert(path);
    }
    result
}

// ---------------------------------------------------------------------------
// key_event_to_string — convert KeyEvent to human-readable string
// ---------------------------------------------------------------------------

pub fn key_event_to_string(key: &KeyEvent) -> String {
    let mut parts = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }

    let key_str = match key.code {
        KeyCode::Char(c) => {
            if c == ' ' {
                "Space".to_string()
            } else {
                c.to_string()
            }
        }
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "Shift+Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        _ => "?".to_string(),
    };

    parts.push(&key_str);
    let result = parts.join("+");
    result.replace("Shift+Shift+", "Shift+")
}


// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render(state: &UnifiedPaletteState, theme: &Theme, frame: &mut Frame, area: Rect) {
    render_full_search(state, theme, frame, area);
}

fn render_full_search(state: &UnifiedPaletteState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Percent { width: 60, height: 50 },
        dialog::PopupAnchor::Center,
        area,
    );
    let inner = dialog::render_popup(frame, popup_area, "command", theme);

    if inner.height < 3 {
        return;
    }

    let [input_area, sep_area, list_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    // Render filter input
    dialog::render_filter_input_placeholder(
        frame, input_area, &state.input, None, theme,
    );

    // Separator
    dialog::render_separator(frame, sep_area, theme);

    // Render filtered entries grouped by category
    let visible_count = list_area.height as usize;
    let scroll_offset = if state.selected >= visible_count {
        state.selected - visible_count + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();
    let mut current_category: Option<&str> = None;
    let show_categories = state.input.is_empty();

    for (visual_idx, &entry_idx) in state.filtered.iter().enumerate() {
        let entry = &state.all_entries[entry_idx];

        // Category header (only in unfiltered view)
        if show_categories
            && current_category != Some(entry.category)
        {
            current_category = Some(entry.category);
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            lines.push(Line::styled(
                format!("  {}", entry.category),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let is_selected = visual_idx == state.selected;
        let is_highlighted = state
            .highlighted_action
            .as_ref()
            .is_some_and(|a| *a == entry.action);

        let name_style = if is_selected || is_highlighted {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };

        let desc_text = if !entry.description.is_empty() {
            format!(" \u{2014} {}", entry.description)
        } else {
            String::new()
        };

        let hint_text = entry.keybind.as_deref().unwrap_or("");

        // Left side: prefix + name + description
        let prefix = "  ";
        let left_len = prefix.len() + entry.name.len() + desc_text.len();
        // Right side: keybind + trailing space
        let right_len = if hint_text.is_empty() { 0 } else { hint_text.len() + 1 };
        let available = list_area.width as usize;
        let gap = available.saturating_sub(left_len + right_len);

        let mut spans = vec![
            Span::styled(prefix, name_style),
            Span::styled(entry.name.clone(), name_style),
            Span::styled(desc_text, Style::default().fg(theme.dim)),
        ];

        if !hint_text.is_empty() {
            spans.push(Span::raw(" ".repeat(gap)));
            spans.push(Span::styled(hint_text.to_string(), Style::default().fg(theme.dim)));
            spans.push(Span::raw(" "));
        }

        lines.push(Line::from(spans));
    }

    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll_offset)
        .take(visible_count)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, list_area);
}



// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use pane_protocol::config::{KeyMap, LeaderConfig};

    fn defaults() -> (KeyMap, LeaderConfig) {
        (KeyMap::from_defaults(), LeaderConfig::default())
    }

    #[test]
    fn test_build_palette_entries_not_empty() {
        let (km, lc) = defaults();
        let entries = build_palette_entries(&km, &lc);
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_palette_has_quit_with_leader_bind() {
        let (km, lc) = defaults();
        let entries = build_palette_entries(&km, &lc);
        let quit = entries.iter().find(|e| e.action == Action::Quit).unwrap();
        assert_eq!(quit.name, "Quit");
        let kb = quit.keybind.as_ref().expect("should have leader keybind");
        assert!(kb.contains("Space"), "expected leader hint, got: {}", kb);
    }

    #[test]
    fn test_leader_bindings_includes_nested() {
        let (_, lc) = defaults();
        let binds = leader_bindings(&lc);
        let split = binds.get(&Action::SplitHorizontal).unwrap();
        assert_eq!(split, "Space d");
    }

    #[test]
    fn test_palette_state_full_search_navigation() {
        let (km, lc) = defaults();
        let mut state = UnifiedPaletteState::new_full_search(&km, &lc);
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_up();
        assert_eq!(state.selected, 0);
        state.move_up();
        assert_eq!(state.selected, state.filtered.len() - 1);
    }

    #[test]
    fn test_palette_state_filter() {
        let (km, lc) = defaults();
        let mut state = UnifiedPaletteState::new_full_search(&km, &lc);
        let total = state.filtered.len();
        state.input = "tab".to_string();
        state.update_filter();
        assert!(state.filtered.len() < total);
    }

    #[test]
    fn test_highlighted_action_sorts_to_front() {
        let (km, lc) = defaults();
        let mut state = UnifiedPaletteState::new_full_search(&km, &lc);
        state.highlighted_action = Some(Action::Quit);
        state.update_filter();
        if let Some(&first_idx) = state.filtered.first() {
            assert_eq!(state.all_entries[first_idx].action, Action::Quit);
        }
    }

    #[test]
    fn test_key_event_to_string_ctrl_q() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_string(&key), "Ctrl+q");
    }

    #[test]
    fn test_key_event_to_string_alt_h() {
        let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT);
        assert_eq!(key_event_to_string(&key), "Alt+h");
    }

    #[test]
    fn test_key_event_to_string_plain_enter() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_string(&key), "Enter");
    }

    #[test]
    fn test_key_event_to_string_f1() {
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert_eq!(key_event_to_string(&key), "F1");
    }

    #[test]
    fn test_filter_empty_query_returns_all() {
        let (km, lc) = defaults();
        let state = UnifiedPaletteState::new_full_search(&km, &lc);
        assert_eq!(state.filtered.len(), state.all_entries.len());
    }

    #[test]
    fn test_filter_no_match() {
        let (km, lc) = defaults();
        let mut state = UnifiedPaletteState::new_full_search(&km, &lc);
        state.input = "zzzzzzzzz".to_string();
        state.update_filter();
        assert!(state.filtered.is_empty());
    }
}
