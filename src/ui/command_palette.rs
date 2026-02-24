use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::config::{Action, KeyMap, Theme};

/// A single entry in the command palette.
#[derive(Clone, Debug)]
pub struct CommandEntry {
    pub action: Action,
    pub name: String,
    pub keybind: Option<String>,
    pub description: String,
    pub category: &'static str,
}

/// State for the command palette overlay.
pub struct CommandPaletteState {
    pub input: String,
    pub selected: usize,
    pub filtered: Vec<CommandEntry>,
    all_commands: Vec<CommandEntry>,
}

impl CommandPaletteState {
    pub fn new(global_keys: &KeyMap, normal_keys: &KeyMap) -> Self {
        let all_commands = build_command_list(global_keys, normal_keys);
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
pub fn build_command_list(global_keys: &KeyMap, normal_keys: &KeyMap) -> Vec<CommandEntry> {
    let actions = all_actions();
    let global_reverse = global_keys.reverse_map();
    let normal_reverse = normal_keys.reverse_map();

    actions
        .into_iter()
        .map(|(action, display_name, description, category)| {
            // Merge keybinds from both keymaps
            let mut keys: Vec<String> = Vec::new();
            if let Some(k) = normal_reverse.get(&action) {
                keys.extend(k.iter().map(|k| key_event_to_string(k)));
            }
            if let Some(k) = global_reverse.get(&action) {
                keys.extend(k.iter().map(|k| key_event_to_string(k)));
            }
            let keybind = if keys.is_empty() {
                None
            } else {
                Some(keys.join(", "))
            };
            CommandEntry {
                action,
                name: display_name,
                keybind,
                description,
                category,
            }
        })
        .collect()
}

/// Filter commands by substring match on display name, description, or category (case-insensitive).
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
                || e.category.to_lowercase().contains(&query_lower)
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

/// Get a human-readable display name for an action.
#[allow(dead_code)]
pub fn action_display_name(action: &Action) -> &str {
    match action {
        Action::Quit => "Quit",
        Action::NewWorkspace => "New Workspace",
        Action::CloseWorkspace => "Close Workspace",
        Action::SwitchWorkspace(_) => "Switch Workspace",
        Action::NewTab => "New Tab",
        Action::DevServerInput => "New Dev Server Tab",
        Action::NextTab => "Next Tab",
        Action::PrevTab => "Previous Tab",
        Action::CloseTab => "Close Tab",
        Action::SplitHorizontal => "Split Right",
        Action::SplitVertical => "Split Down",
        Action::RestartPane => "Restart Pane",
        Action::FocusLeft => "Focus Left",
        Action::FocusDown => "Focus Down",
        Action::FocusUp => "Focus Up",
        Action::FocusRight => "Focus Right",
        Action::FocusGroupN(_) => "Focus Group N",
        Action::MoveTabLeft => "Move Tab Left",
        Action::MoveTabDown => "Move Tab Down",
        Action::MoveTabUp => "Move Tab Up",
        Action::MoveTabRight => "Move Tab Right",
        Action::ResizeShrinkH => "Shrink Horizontally",
        Action::ResizeGrowH => "Grow Horizontally",
        Action::ResizeGrowV => "Grow Vertically",
        Action::ResizeShrinkV => "Shrink Vertically",
        Action::ResizeMode => "Resize Mode",
        Action::ClientPicker => "Manage Clients",
        Action::Equalize => "Equalize Panes",
        Action::SessionPicker => "Session Picker",
        Action::Help => "Help",
        Action::ScrollMode => "Scroll Mode",
        Action::CopyMode => "Copy Mode",
        Action::PasteClipboard => "Paste from Clipboard",
        Action::SelectLayout(_) => "Select Layout",
        Action::ToggleSyncPanes => "Toggle Sync Panes",
        Action::CommandPalette => "Command Palette",
        Action::RenameWindow => "Rename Window",
        Action::RenamePane => "Rename Pane",
        Action::Detach => "Detach",
        Action::EnterInteract => "Enter Interact Mode",
        Action::EnterNormal => "Enter Normal Mode",
        Action::MaximizeFocused => "Maximize Focused",
        Action::ToggleZoom => "Toggle Zoom",
        Action::ToggleFold => "Toggle Fold",
        Action::ToggleFloat => "Toggle Float",
        Action::NewFloat => "New Float",
        Action::PrevWorkspace => "Previous Workspace",
        Action::NextWorkspace => "Next Workspace",
    }
}

/// All actions available for the command palette (excludes parameterized ones).
/// Returns (Action, display_name, description, category).
fn all_actions() -> Vec<(Action, String, String, &'static str)> {
    vec![
        // Navigation
        (
            Action::FocusLeft,
            "Focus Left".into(),
            "Move focus to the left window".into(),
            "Navigation",
        ),
        (
            Action::FocusDown,
            "Focus Down".into(),
            "Move focus to the window below".into(),
            "Navigation",
        ),
        (
            Action::FocusUp,
            "Focus Up".into(),
            "Move focus to the window above".into(),
            "Navigation",
        ),
        (
            Action::FocusRight,
            "Focus Right".into(),
            "Move focus to the right window".into(),
            "Navigation",
        ),
        (
            Action::EnterInteract,
            "Enter Interact Mode".into(),
            "Switch to interact mode (forward keys to PTY)".into(),
            "Navigation",
        ),
        (
            Action::EnterNormal,
            "Enter Normal Mode".into(),
            "Switch to normal mode (vim-style navigation)".into(),
            "Navigation",
        ),
        // Layout
        (
            Action::SplitHorizontal,
            "Split Right".into(),
            "Split the focused window horizontally".into(),
            "Layout",
        ),
        (
            Action::SplitVertical,
            "Split Down".into(),
            "Split the focused window vertically".into(),
            "Layout",
        ),
        (
            Action::ResizeShrinkH,
            "Shrink Horizontally".into(),
            "Decrease the focused window width".into(),
            "Layout",
        ),
        (
            Action::ResizeGrowH,
            "Grow Horizontally".into(),
            "Increase the focused window width".into(),
            "Layout",
        ),
        (
            Action::ResizeGrowV,
            "Grow Vertically".into(),
            "Increase the focused window height".into(),
            "Layout",
        ),
        (
            Action::ResizeShrinkV,
            "Shrink Vertically".into(),
            "Decrease the focused window height".into(),
            "Layout",
        ),
        (
            Action::ResizeMode,
            "Resize Mode".into(),
            "Enter resize mode (hjkl to resize, esc to exit)".into(),
            "Layout",
        ),
        (
            Action::Equalize,
            "Equalize Panes".into(),
            "Reset all split ratios to equal".into(),
            "Layout",
        ),
        (
            Action::MaximizeFocused,
            "Maximize Focused".into(),
            "Toggle maximize the focused window".into(),
            "Layout",
        ),
        (
            Action::ToggleZoom,
            "Toggle Zoom".into(),
            "Toggle full-screen zoom on the focused window".into(),
            "Layout",
        ),
        (
            Action::ToggleFold,
            "Toggle Fold".into(),
            "Fold/unfold panes by moving focus across groups".into(),
            "Layout",
        ),
        (
            Action::ToggleFloat,
            "Toggle Float".into(),
            "Toggle floating mode for the focused window".into(),
            "Layout",
        ),
        (
            Action::NewFloat,
            "New Float".into(),
            "Create a new floating window".into(),
            "Layout",
        ),
        // Tabs
        (
            Action::NewTab,
            "New Tab".into(),
            "Open a new tab in the current window".into(),
            "Tabs",
        ),
        (
            Action::DevServerInput,
            "New Dev Server Tab".into(),
            "Open a dev server tab".into(),
            "Tabs",
        ),
        (
            Action::NextTab,
            "Next Tab".into(),
            "Switch to the next tab".into(),
            "Tabs",
        ),
        (
            Action::PrevTab,
            "Previous Tab".into(),
            "Switch to the previous tab".into(),
            "Tabs",
        ),
        (
            Action::CloseTab,
            "Close Tab".into(),
            "Close the current tab".into(),
            "Tabs",
        ),
        (
            Action::MoveTabLeft,
            "Move Tab Left".into(),
            "Move the current tab to the left window".into(),
            "Tabs",
        ),
        (
            Action::MoveTabDown,
            "Move Tab Down".into(),
            "Move the current tab to the window below".into(),
            "Tabs",
        ),
        (
            Action::MoveTabUp,
            "Move Tab Up".into(),
            "Move the current tab to the window above".into(),
            "Tabs",
        ),
        (
            Action::MoveTabRight,
            "Move Tab Right".into(),
            "Move the current tab to the right window".into(),
            "Tabs",
        ),
        // Workspaces
        (
            Action::NewWorkspace,
            "New Workspace".into(),
            "Create a new workspace".into(),
            "Workspaces",
        ),
        (
            Action::CloseWorkspace,
            "Close Workspace".into(),
            "Close the current workspace".into(),
            "Workspaces",
        ),
        (
            Action::PrevWorkspace,
            "Previous Workspace".into(),
            "Switch to the previous workspace".into(),
            "Workspaces",
        ),
        (
            Action::NextWorkspace,
            "Next Workspace".into(),
            "Switch to the next workspace".into(),
            "Workspaces",
        ),
        // Tools
        (
            Action::ScrollMode,
            "Scroll Mode".into(),
            "Enter scroll mode for the focused pane".into(),
            "Tools",
        ),
        (
            Action::CopyMode,
            "Copy Mode".into(),
            "Enter copy mode with vim-style selection".into(),
            "Tools",
        ),
        (
            Action::PasteClipboard,
            "Paste from Clipboard".into(),
            "Paste system clipboard into the focused pane".into(),
            "Tools",
        ),
        (
            Action::RestartPane,
            "Restart Pane".into(),
            "Restart the exited pane process".into(),
            "Tools",
        ),
        (
            Action::RenameWindow,
            "Rename Window".into(),
            "Rename the current window".into(),
            "Tools",
        ),
        (
            Action::RenamePane,
            "Rename Pane".into(),
            "Rename the current pane".into(),
            "Tools",
        ),
        (
            Action::ToggleSyncPanes,
            "Toggle Sync Panes".into(),
            "Broadcast input to all panes in workspace".into(),
            "Tools",
        ),
        // Clients
        (
            Action::ClientPicker,
            "Manage Clients".into(),
            "View connected clients and kick sessions".into(),
            "Session",
        ),
        // Session
        (
            Action::Detach,
            "Detach".into(),
            "Detach from the daemon".into(),
            "Session",
        ),
        (
            Action::Quit,
            "Quit".into(),
            "Exit pane".into(),
            "Session",
        ),
    ]
}

/// Category display order.
const CATEGORY_ORDER: &[&str] = &[
    "Navigation",
    "Layout",
    "Tabs",
    "Workspaces",
    "Tools",
    "Session",
];

/// Render the command palette overlay.
pub fn render(state: &CommandPaletteState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" commands ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 3 {
        return;
    }

    let [input_area, _, list_area, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
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

    // Build display lines
    let is_filtering = !state.input.is_empty();
    let inner_width = list_area.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    // Track which visual line index maps to which entry index (for selection highlight)
    let mut line_to_entry: Vec<Option<usize>> = Vec::new();

    if is_filtering {
        // Flat filtered list — no category headers
        for (i, entry) in state.filtered.iter().enumerate() {
            let line = render_entry(entry, i == state.selected, inner_width, theme);
            lines.push(line);
            line_to_entry.push(Some(i));
        }
    } else {
        // Grouped by category
        for &cat in CATEGORY_ORDER {
            let entries: Vec<(usize, &CommandEntry)> = state
                .filtered
                .iter()
                .enumerate()
                .filter(|(_, e)| e.category == cat)
                .collect();
            if entries.is_empty() {
                continue;
            }

            // Category header
            lines.push(Line::styled(
                format!("  {}", cat),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            line_to_entry.push(None);

            for (i, entry) in entries {
                let line = render_entry(entry, i == state.selected, inner_width, theme);
                lines.push(line);
                line_to_entry.push(Some(i));
            }

            // Blank line after section
            lines.push(Line::raw(""));
            line_to_entry.push(None);
        }
    }

    // Find the scroll offset to keep selected visible
    let visible_count = list_area.height as usize;
    let selected_line = line_to_entry
        .iter()
        .position(|e| *e == Some(state.selected))
        .unwrap_or(0);
    let scroll_offset = if selected_line >= visible_count {
        selected_line - visible_count + 1
    } else {
        0
    };

    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll_offset)
        .take(visible_count)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, list_area);

    // Hint footer
    let hint = Line::from(vec![Span::styled(
        "  esc close  enter run  type to filter",
        Style::default().fg(theme.dim),
    )]);
    frame.render_widget(Paragraph::new(hint), hint_area);
}

fn render_entry<'a>(
    entry: &CommandEntry,
    is_selected: bool,
    width: usize,
    theme: &Theme,
) -> Line<'a> {
    let indicator = if is_selected { "  > " } else { "    " };
    let name_style = if is_selected {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let keybind_text = entry
        .keybind
        .as_ref()
        .map(|k| k.clone())
        .unwrap_or_default();

    // Calculate right-aligned keybind position
    let name_part_len = indicator.len() + entry.name.len();
    let keybind_len = keybind_text.len();
    let gap = if keybind_len > 0 {
        width.saturating_sub(name_part_len + keybind_len + 2)
    } else {
        0
    };

    let mut spans = vec![
        Span::styled(indicator.to_string(), name_style),
        Span::styled(entry.name.clone(), name_style),
    ];

    if !keybind_text.is_empty() {
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(
            keybind_text,
            Style::default().fg(Color::Yellow),
        ));
    }

    // Show description inline on selected row
    if is_selected && !entry.description.is_empty() {
        // Clear keybind right-align, add description below-ish by appending
        // Actually, for a cleaner look, only show desc if there's room
        // We'll show it after the name with dim style
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            entry.description.clone(),
            Style::default().fg(theme.dim),
        ));
    }

    Line::from(spans)
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
    use crate::config::KeyMap;

    #[test]
    fn test_build_command_list_not_empty() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_build_command_list_has_quit() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
        let quit = commands.iter().find(|e| e.action == Action::Quit);
        assert!(quit.is_some());
        let entry = quit.unwrap();
        assert_eq!(entry.name, "Quit");
        assert_eq!(entry.category, "Session");
    }

    #[test]
    fn test_build_command_list_has_categories() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
        let categories: Vec<&str> = commands.iter().map(|e| e.category).collect();
        assert!(categories.contains(&"Navigation"));
        assert!(categories.contains(&"Layout"));
        assert!(categories.contains(&"Tabs"));
        assert!(categories.contains(&"Workspaces"));
        assert!(categories.contains(&"Tools"));
        assert!(categories.contains(&"Session"));
    }

    #[test]
    fn test_filter_commands_empty_query() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
        let filtered = filter_commands(&commands, "");
        assert_eq!(filtered.len(), commands.len());
    }

    #[test]
    fn test_filter_commands_by_name() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
        let filtered = filter_commands(&commands, "quit");
        assert!(filtered.iter().any(|e| e.action == Action::Quit));
    }

    #[test]
    fn test_filter_commands_by_category() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
        let filtered = filter_commands(&commands, "navigation");
        assert!(filtered.iter().any(|e| e.action == Action::FocusLeft));
    }

    #[test]
    fn test_filter_commands_case_insensitive() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
        let filtered = filter_commands(&commands, "QUIT");
        assert!(filtered.iter().any(|e| e.action == Action::Quit));
    }

    #[test]
    fn test_filter_commands_no_match() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let commands = build_command_list(&global_keys, &normal_keys);
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
    fn test_action_display_name_coverage() {
        assert_eq!(action_display_name(&Action::Quit), "Quit");
        assert_eq!(action_display_name(&Action::NewTab), "New Tab");
        assert_eq!(action_display_name(&Action::SplitHorizontal), "Split Right");
    }

    #[test]
    fn test_command_palette_state_navigation() {
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let mut state = CommandPaletteState::new(&global_keys, &normal_keys);
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
        let global_keys = KeyMap::from_defaults();
        let normal_keys = KeyMap::normal_defaults();
        let mut state = CommandPaletteState::new(&global_keys, &normal_keys);
        let total = state.filtered.len();
        state.input = "tab".to_string();
        state.update_filter();
        assert!(state.filtered.len() < total);
        assert!(state
            .filtered
            .iter()
            .all(|e| e.name.to_lowercase().contains("tab")
                || e.description.to_lowercase().contains("tab")
                || e.category.to_lowercase().contains("tab")));
    }
}
