use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use pane_protocol::config::{Action, KeyMap, Theme};

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
    pub fn new(keymap: &KeyMap) -> Self {
        let all_commands = build_command_list(keymap);
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
pub fn build_command_list(keymap: &KeyMap) -> Vec<CommandEntry> {
    let actions = all_actions();
    let reverse = keymap.reverse_map();

    actions
        .into_iter()
        .map(|(action, display_name, description)| {
            let keybind = reverse.get(&action).map(|keys| {
                keys.iter()
                    .map(|k| key_event_to_string(k))
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            CommandEntry {
                action,
                name: display_name,
                keybind,
                description,
            }
        })
        .collect()
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
        Action::SelectMode => "Select Mode",
        Action::EnterInteract => "Enter Interact Mode",
        Action::EnterNormal => "Enter Normal Mode",
        Action::MaximizeFocused => "Maximize Focused",
        Action::ToggleZoom => "Toggle Zoom",
        Action::ToggleFloat => "Toggle Float",
        Action::NewFloat => "New Float",
    }
}

/// All actions available for the command palette (excludes parameterized ones).
/// Returns (Action, display_name, description).
fn all_actions() -> Vec<(Action, String, String)> {
    vec![
        (
            Action::Quit,
            "Quit".into(),
            "Exit pane and close the session".into(),
        ),
        (
            Action::NewWorkspace,
            "New Workspace".into(),
            "Create a new workspace".into(),
        ),
        (
            Action::CloseWorkspace,
            "Close Workspace".into(),
            "Close the current workspace".into(),
        ),
        (
            Action::NewTab,
            "New Tab".into(),
            "Open a new tab in the current window".into(),
        ),
        (
            Action::DevServerInput,
            "New Dev Server Tab".into(),
            "Open a dev server tab".into(),
        ),
        (
            Action::NextTab,
            "Next Tab".into(),
            "Switch to the next tab".into(),
        ),
        (
            Action::PrevTab,
            "Previous Tab".into(),
            "Switch to the previous tab".into(),
        ),
        (
            Action::CloseTab,
            "Close Tab".into(),
            "Close the current tab".into(),
        ),
        (
            Action::SplitHorizontal,
            "Split Right".into(),
            "Split the focused window horizontally".into(),
        ),
        (
            Action::SplitVertical,
            "Split Down".into(),
            "Split the focused window vertically".into(),
        ),
        (
            Action::RestartPane,
            "Restart Pane".into(),
            "Restart the exited pane process".into(),
        ),
        (
            Action::FocusLeft,
            "Focus Left".into(),
            "Move focus to the left window".into(),
        ),
        (
            Action::FocusDown,
            "Focus Down".into(),
            "Move focus to the window below".into(),
        ),
        (
            Action::FocusUp,
            "Focus Up".into(),
            "Move focus to the window above".into(),
        ),
        (
            Action::FocusRight,
            "Focus Right".into(),
            "Move focus to the right window".into(),
        ),
        (
            Action::MoveTabLeft,
            "Move Tab Left".into(),
            "Move the current tab to the left window".into(),
        ),
        (
            Action::MoveTabDown,
            "Move Tab Down".into(),
            "Move the current tab to the window below".into(),
        ),
        (
            Action::MoveTabUp,
            "Move Tab Up".into(),
            "Move the current tab to the window above".into(),
        ),
        (
            Action::MoveTabRight,
            "Move Tab Right".into(),
            "Move the current tab to the right window".into(),
        ),
        (
            Action::ResizeShrinkH,
            "Shrink Horizontally".into(),
            "Decrease the focused window width".into(),
        ),
        (
            Action::ResizeGrowH,
            "Grow Horizontally".into(),
            "Increase the focused window width".into(),
        ),
        (
            Action::ResizeGrowV,
            "Grow Vertically".into(),
            "Increase the focused window height".into(),
        ),
        (
            Action::ResizeShrinkV,
            "Shrink Vertically".into(),
            "Decrease the focused window height".into(),
        ),
        (
            Action::Equalize,
            "Equalize Panes".into(),
            "Reset all split ratios to equal".into(),
        ),
        (
            Action::MaximizeFocused,
            "Maximize Focused".into(),
            "Toggle maximize the focused window".into(),
        ),
        (
            Action::ToggleZoom,
            "Toggle Zoom".into(),
            "Toggle full-screen zoom on the focused window".into(),
        ),
        (
            Action::ToggleFloat,
            "Toggle Float".into(),
            "Toggle floating mode for the focused window".into(),
        ),
        (
            Action::NewFloat,
            "New Float".into(),
            "Create a new floating window".into(),
        ),
        (
            Action::SessionPicker,
            "Session Picker".into(),
            "Open the session picker".into(),
        ),
        (Action::Help, "Help".into(), "Show keybinding help".into()),
        (
            Action::ScrollMode,
            "Scroll Mode".into(),
            "Enter scroll mode for the focused pane".into(),
        ),
        (
            Action::CopyMode,
            "Copy Mode".into(),
            "Enter copy mode with vim-style selection".into(),
        ),
        (
            Action::PasteClipboard,
            "Paste from Clipboard".into(),
            "Paste system clipboard into the focused pane".into(),
        ),
        (
            Action::ToggleSyncPanes,
            "Toggle Sync Panes".into(),
            "Broadcast input to all panes in workspace".into(),
        ),
        (
            Action::RenameWindow,
            "Rename Window".into(),
            "Rename the current window".into(),
        ),
        (
            Action::RenamePane,
            "Rename Pane".into(),
            "Rename the current pane".into(),
        ),
        (
            Action::Detach,
            "Detach".into(),
            "Detach from the session".into(),
        ),
        (
            Action::SelectMode,
            "Select Mode".into(),
            "Toggle select mode for window navigation".into(),
        ),
        (
            Action::EnterInteract,
            "Enter Interact Mode".into(),
            "Switch to interact mode (forward keys to PTY)".into(),
        ),
        (
            Action::EnterNormal,
            "Enter Normal Mode".into(),
            "Switch to normal mode (vim-style navigation)".into(),
        ),
    ]
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
    use pane_protocol::config::KeyMap;

    #[test]
    fn test_build_command_list_not_empty() {
        let keymap = KeyMap::from_defaults();
        let commands = build_command_list(&keymap);
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_build_command_list_has_quit() {
        let keymap = KeyMap::from_defaults();
        let commands = build_command_list(&keymap);
        let quit = commands.iter().find(|e| e.action == Action::Quit);
        assert!(quit.is_some());
        let entry = quit.unwrap();
        assert_eq!(entry.name, "Quit");
        assert!(entry.keybind.is_some());
    }

    #[test]
    fn test_filter_commands_empty_query() {
        let keymap = KeyMap::from_defaults();
        let commands = build_command_list(&keymap);
        let filtered = filter_commands(&commands, "");
        assert_eq!(filtered.len(), commands.len());
    }

    #[test]
    fn test_filter_commands_by_name() {
        let keymap = KeyMap::from_defaults();
        let commands = build_command_list(&keymap);
        let filtered = filter_commands(&commands, "quit");
        assert!(filtered.iter().any(|e| e.action == Action::Quit));
    }

    #[test]
    fn test_filter_commands_case_insensitive() {
        let keymap = KeyMap::from_defaults();
        let commands = build_command_list(&keymap);
        let filtered = filter_commands(&commands, "QUIT");
        assert!(filtered.iter().any(|e| e.action == Action::Quit));
    }

    #[test]
    fn test_filter_commands_no_match() {
        let keymap = KeyMap::from_defaults();
        let commands = build_command_list(&keymap);
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
        let keymap = KeyMap::from_defaults();
        let mut state = CommandPaletteState::new(&keymap);
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
        let keymap = KeyMap::from_defaults();
        let mut state = CommandPaletteState::new(&keymap);
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
