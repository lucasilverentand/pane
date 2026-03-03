use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use std::collections::HashMap;

use pane_protocol::config::{Action, KeyMap, LeaderConfig, LeaderNode, Theme};
use pane_protocol::registry;

/// A single entry in the command palette.
#[derive(Clone, Debug)]
pub struct CommandEntry {
    pub action: Action,
    pub name: String,
    pub keybind: Option<String>,
    pub description: String,
}

/// State for the command palette overlay.
pub struct CommandPaletteState {
    pub input: String,
    pub selected: usize,
    pub filtered: Vec<CommandEntry>,
    all_commands: Vec<CommandEntry>,
}

impl CommandPaletteState {
    pub fn new(keymap: &KeyMap, leader: &LeaderConfig) -> Self {
        let all_commands = build_command_list(keymap, leader);
        let filtered = all_commands.clone();
        Self {
            input: String::new(),
            selected: 0,
            filtered,
            all_commands,
        }
    }

    pub fn update_filter(&mut self) {
        self.filtered = filter_commands(&self.all_commands, &self.input);
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
        self.filtered.get(self.selected).map(|e| e.action.clone())
    }
}

/// Build a list of all available commands with display names, keybinding hints, and descriptions.
pub fn build_command_list(keymap: &KeyMap, leader: &LeaderConfig) -> Vec<CommandEntry> {
    let reverse = keymap.reverse_map();
    let leader_binds = leader_bindings(leader);

    registry::palette_actions()
        .map(|meta| {
            let mut hints = Vec::new();

            // Regular keybinds
            if let Some(keys) = reverse.get(&meta.action) {
                for k in keys {
                    hints.push(key_event_to_string(k));
                }
            }

            // Leader keybind (e.g. "Space q")
            if let Some(leader_hint) = leader_binds.get(&meta.action) {
                hints.push(leader_hint.clone());
            }

            let keybind = if hints.is_empty() {
                None
            } else {
                Some(hints.join(", "))
            };

            CommandEntry {
                action: meta.action.clone(),
                name: meta.display_name.to_string(),
                keybind,
                description: meta.description.to_string(),
            }
        })
        .collect()
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

    // Keep shortest path per action
    all.sort_by_key(|(_, path)| path.len());
    let mut result: HashMap<Action, String> = HashMap::new();
    for (action, path) in all {
        result.entry(action).or_insert(path);
    }
    result
}

/// Filter commands by substring match on display name or description (case-insensitive).
pub fn filter_commands(commands: &[CommandEntry], query: &str) -> Vec<CommandEntry> {
    if query.is_empty() {
        return commands.to_vec();
    }
    let query_lower = query.to_lowercase();
    commands
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&query_lower)
                || e.description.to_lowercase().contains(&query_lower)
        })
        .cloned()
        .collect()
}

/// Convert a KeyEvent to a human-readable string like "Ctrl+Q" or "Alt+H".
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
    // For BackTab, the "Shift" is already in the key_str, so filter duplicate
    let result = parts.join("+");
    // Remove duplicate "Shift+" for BackTab
    result.replace("Shift+Shift+", "Shift+")
}

/// Render the command palette overlay.
pub fn render(state: &CommandPaletteState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 50, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" command palette ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 3 {
        return;
    }

    let [input_area, _, list_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    // Render input line
    let input_line = Line::from(vec![
        Span::styled("  > ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{}_", state.input),
            Style::default().fg(Color::White),
        ),
    ]);
    frame.render_widget(Paragraph::new(input_line), input_area);

    // Render filtered command list
    let visible_count = list_area.height as usize;
    let scroll_offset = if state.selected >= visible_count {
        state.selected - visible_count + 1
    } else {
        0
    };

    let lines: Vec<Line> = state
        .filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_count)
        .map(|(i, entry)| {
            let is_selected = i == state.selected;
            let name_style = if is_selected {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let hint_text = entry
                .keybind
                .as_ref()
                .map(|h| format!("  {}", h))
                .unwrap_or_default();
            let desc_text = if is_selected && !entry.description.is_empty() {
                format!("  {}", entry.description)
            } else {
                String::new()
            };
            let indicator = if is_selected { "  > " } else { "    " };
            Line::from(vec![
                Span::styled(indicator, name_style),
                Span::styled(entry.name.clone(), name_style),
                Span::styled(hint_text, Style::default().fg(theme.dim)),
                Span::styled(desc_text, Style::default().fg(theme.dim)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, list_area);
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
    use pane_protocol::config::{KeyMap, LeaderConfig};

    fn defaults() -> (KeyMap, LeaderConfig) {
        (KeyMap::from_defaults(), LeaderConfig::default())
    }

    #[test]
    fn test_build_command_list_not_empty() {
        let (km, lc) = defaults();
        let commands = build_command_list(&km, &lc);
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_build_command_list_has_quit_with_leader_bind() {
        let (km, lc) = defaults();
        let commands = build_command_list(&km, &lc);
        let quit = commands.iter().find(|e| e.action == Action::Quit).unwrap();
        assert_eq!(quit.name, "Quit");
        // Quit is leader-only (space+q)
        let kb = quit.keybind.as_ref().expect("should have leader keybind");
        assert!(kb.contains("Space"), "expected leader hint, got: {}", kb);
    }

    #[test]
    fn test_leader_bindings_includes_nested() {
        let (_, lc) = defaults();
        let binds = leader_bindings(&lc);
        // SplitHorizontal has a root shortcut (space d) and a nested (space w d)
        let split = binds.get(&Action::SplitHorizontal).unwrap();
        // Shortest path should win — "Space d" is shorter than "Space w d"
        assert_eq!(split, "Space d");
    }

    #[test]
    fn test_filter_commands_empty_query() {
        let (km, lc) = defaults();
        let commands = build_command_list(&km, &lc);
        let filtered = filter_commands(&commands, "");
        assert_eq!(filtered.len(), commands.len());
    }

    #[test]
    fn test_filter_commands_by_name() {
        let (km, lc) = defaults();
        let commands = build_command_list(&km, &lc);
        let filtered = filter_commands(&commands, "quit");
        assert!(filtered.iter().any(|e| e.action == Action::Quit));
    }

    #[test]
    fn test_filter_commands_case_insensitive() {
        let (km, lc) = defaults();
        let commands = build_command_list(&km, &lc);
        let filtered = filter_commands(&commands, "QUIT");
        assert!(filtered.iter().any(|e| e.action == Action::Quit));
    }

    #[test]
    fn test_filter_commands_no_match() {
        let (km, lc) = defaults();
        let commands = build_command_list(&km, &lc);
        let filtered = filter_commands(&commands, "zzzzzzzzz");
        assert!(filtered.is_empty());
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
    fn test_display_name_for_coverage() {
        use pane_protocol::registry::display_name_for;
        assert_eq!(display_name_for(&Action::Quit), "Quit");
        assert_eq!(display_name_for(&Action::NewTab), "New Tab");
        assert_eq!(display_name_for(&Action::SplitHorizontal), "Split Right");
    }

    #[test]
    fn test_command_palette_state_navigation() {
        let (km, lc) = defaults();
        let mut state = CommandPaletteState::new(&km, &lc);
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_up();
        assert_eq!(state.selected, 0);
        // Wrap around
        state.move_up();
        assert_eq!(state.selected, state.filtered.len() - 1);
    }

    #[test]
    fn test_command_palette_state_filter() {
        let (km, lc) = defaults();
        let mut state = CommandPaletteState::new(&km, &lc);
        let total = state.filtered.len();
        state.input = "tab".to_string();
        state.update_filter();
        assert!(state.filtered.len() < total);
        assert!(state
            .filtered
            .iter()
            .all(|e| e.name.to_lowercase().contains("tab")));
    }
}
