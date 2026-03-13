use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use std::collections::HashMap;

use pane_protocol::config::{Action, KeyMap, LeaderConfig, LeaderNode, Theme};
use pane_protocol::registry;

use super::dialog;

// ---------------------------------------------------------------------------
// PaletteView — the two display modes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteView {
    /// Full-screen centered overlay with search and all actions listed
    FullSearch,
    /// Small popup (bottom-right) showing a leader group's available keys
    CompactHints,
}

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
    pub view: PaletteView,
    pub input: String,
    pub selected: usize,
    all_entries: Vec<PaletteEntry>,
    pub filtered: Vec<usize>,
    /// Current leader group children for compact hints view
    pub leader_group: Option<Vec<(KeyEvent, String, bool)>>, // (key, label, is_group)
    /// Display path for which-key header (e.g. "⎵ w" for leader → window)
    pub leader_path: Option<String>,
    /// Pre-selected action from a leader key match (highlighted in palette)
    pub highlighted_action: Option<Action>,
}

impl UnifiedPaletteState {
    /// Create a full-search palette with all actions.
    pub fn new_full_search(keymap: &KeyMap, leader: &LeaderConfig) -> Self {
        let all_entries = build_palette_entries(keymap, leader);
        let filtered: Vec<usize> = (0..all_entries.len()).collect();
        Self {
            view: PaletteView::FullSearch,
            input: String::new(),
            selected: 0,
            all_entries,
            filtered,
            leader_group: None,
            leader_path: None,
            highlighted_action: None,
        }
    }

    /// Create a compact-hints palette showing a leader group's children.
    pub fn new_compact_hints(children: &HashMap<KeyEvent, LeaderNode>, path: String) -> Self {
        let entries: Vec<(KeyEvent, String, bool)> = {
            let mut v: Vec<_> = children
                .iter()
                .filter(|(_, node)| !matches!(node, LeaderNode::PassThrough))
                .map(|(key, node)| {
                    let (label, is_group) = match node {
                        LeaderNode::Leaf { label, .. } => (label.clone(), false),
                        LeaderNode::Group { label, .. } => (format!("+{}", label), true),
                        LeaderNode::PassThrough => unreachable!(),
                    };
                    (*key, label, is_group)
                })
                .collect();
            v.sort_by(|a, b| format_key_short(&a.0).cmp(&format_key_short(&b.0)));
            v
        };
        Self {
            view: PaletteView::CompactHints,
            input: String::new(),
            selected: 0,
            all_entries: Vec::new(),
            filtered: Vec::new(),
            leader_group: Some(entries),
            leader_path: Some(path),
            highlighted_action: None,
        }
    }

    /// Transition from compact hints to full search, carrying over typed chars as query.
    #[allow(dead_code)]
    pub fn transition_to_full_search(&mut self, keymap: &KeyMap, leader: &LeaderConfig) {
        self.all_entries = build_palette_entries(keymap, leader);
        self.view = PaletteView::FullSearch;
        self.leader_group = None;
        self.update_filter();
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

/// Short key format for compact hints (e.g. "C-q", "S-Tab")
fn format_key_short(key: &KeyEvent) -> String {
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

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render(state: &UnifiedPaletteState, theme: &Theme, frame: &mut Frame, area: Rect) {
    match state.view {
        PaletteView::FullSearch => render_full_search(state, theme, frame, area),
        PaletteView::CompactHints => render_compact_hints(state, theme, frame, area),
    }
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

    // Render filter input (palette uses slightly wider prefix)
    let input_line = Line::from(vec![
        Span::styled("  > ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{}_", state.input),
            Style::default().fg(Color::White),
        ),
    ]);
    frame.render_widget(Paragraph::new(input_line), input_area);

    // Separator
    dialog::render_separator(frame, sep_area);

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
            Style::default().fg(Color::White)
        };

        let indicator = if is_selected { "  > " } else { "    " };

        let desc_text = if !entry.description.is_empty() {
            format!(" \u{2014} {}", entry.description)
        } else {
            String::new()
        };

        let hint_text = entry.keybind.as_deref().unwrap_or("");

        // Left side: indicator + name + description
        let left_len = indicator.len() + entry.name.len() + desc_text.len();
        // Right side: keybind + trailing space
        let right_len = if hint_text.is_empty() { 0 } else { hint_text.len() + 1 };
        let available = list_area.width as usize;
        let gap = available.saturating_sub(left_len + right_len);

        let mut spans = vec![
            Span::styled(indicator, name_style),
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

fn render_compact_hints(
    state: &UnifiedPaletteState,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let entries = match &state.leader_group {
        Some(e) => e,
        None => return,
    };

    if entries.is_empty() {
        return;
    }

    // Separate groups and leaves for display
    let groups: Vec<_> = entries.iter().filter(|(_, _, is_group)| *is_group).collect();
    let leaves: Vec<_> = entries.iter().filter(|(_, _, is_group)| !*is_group).collect();

    let max_key_len = entries
        .iter()
        .map(|(k, _, _)| format_key_short(k).len())
        .max()
        .unwrap_or(1);
    let max_label_len = entries
        .iter()
        .map(|(_, l, _)| l.len())
        .max()
        .unwrap_or(1);

    // Multi-column layout: fit entries in columns
    let item_width = (max_key_len + 2 + max_label_len + 2) as u16;
    let max_popup_width = (area.width * 60 / 100).max(30);
    let cols = ((max_popup_width - 2) / item_width).clamp(1, 4) as usize;
    let total_items = entries.len();
    let rows = total_items.div_ceil(cols);

    let popup_width = ((item_width * cols as u16) + 4).min(max_popup_width);
    let popup_height = (rows as u16 + 4).min(area.height.saturating_sub(4));

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed { width: popup_width, height: popup_height },
        dialog::PopupAnchor::BottomCenter,
        area,
    );

    let title = match &state.leader_path {
        Some(path) if !path.is_empty() => path.clone(),
        _ => "which key".to_string(),
    };

    let inner = dialog::render_popup(frame, popup_area, &title, theme);

    if inner.height == 0 || inner.width < 4 {
        return;
    }

    let max_visible_rows = inner.height as usize;
    let col_width = inner.width / cols as u16;

    // Render entries in column-major order (groups first, then leaves)
    let all_items: Vec<&(KeyEvent, String, bool)> = groups.into_iter().chain(leaves).collect();
    for (idx, (key, label, is_group)) in all_items.iter().enumerate() {
        let row = idx % rows;
        let col = idx / rows;
        if row >= max_visible_rows || col >= cols {
            continue;
        }

        let cell_x = inner.x + (col as u16 * col_width);
        let cell_y = inner.y + row as u16;
        let cell_width = col_width.min(inner.x + inner.width - cell_x);

        let key_str = format_key_short(key);
        let key_style = Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD);
        let label_style = if *is_group {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(Color::White)
        };
        let pad = " ".repeat(max_key_len.saturating_sub(key_str.len()));
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(key_str, key_style),
            Span::raw(pad),
            Span::raw(" "),
            Span::styled(label.clone(), label_style),
        ]);
        let cell_area = Rect::new(cell_x, cell_y, cell_width, 1);
        frame.render_widget(Paragraph::new(line), cell_area);
    }
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
    fn test_compact_hints_from_leader_root() {
        let lc = LeaderConfig::default();
        if let LeaderNode::Group { children, .. } = &lc.root {
            let state = UnifiedPaletteState::new_compact_hints(children, "⎵".into());
            assert_eq!(state.view, PaletteView::CompactHints);
            assert!(state.leader_group.as_ref().unwrap().len() > 0);
        }
    }

    #[test]
    fn test_transition_to_full_search() {
        let (km, lc) = defaults();
        let mut state = if let LeaderNode::Group { children, .. } = &lc.root {
            UnifiedPaletteState::new_compact_hints(children, "⎵".into())
        } else {
            panic!("expected group root");
        };
        state.input = "split".to_string();
        state.transition_to_full_search(&km, &lc);
        assert_eq!(state.view, PaletteView::FullSearch);
        assert!(!state.filtered.is_empty());
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
