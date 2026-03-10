use ratatui::{layout::Rect, Frame};

use pane_protocol::config::{TabPickerEntryConfig, Theme};

use super::dialog;

/// What the tab picker is being opened for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabPickerMode {
    NewTab,
    SplitHorizontal,
    SplitVertical,
}

/// State for the fuzzy tab picker overlay.
pub struct TabPickerState {
    pub input: String,
    pub selected: usize,
    pub entries: Vec<TabPickerEntry>,
    pub mode: TabPickerMode,
    filtered: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabPickerSection {
    Shells,
    Custom,
    Tools,
    Recent,
}

impl TabPickerSection {
    fn label(&self) -> &'static str {
        match self {
            Self::Shells => "Shells",
            Self::Custom => "Custom",
            Self::Tools => "Tools",
            Self::Recent => "Recent",
        }
    }
}

#[derive(Clone)]
pub struct TabPickerEntry {
    pub name: String,
    pub command: Option<String>,
    pub description: String,
    pub section: TabPickerSection,
}

impl TabPickerState {
    pub fn new(custom_entries: &[TabPickerEntryConfig]) -> Self {
        Self::with_mode(custom_entries, TabPickerMode::NewTab)
    }

    pub fn with_mode(custom_entries: &[TabPickerEntryConfig], mode: TabPickerMode) -> Self {
        let entries = build_entries(custom_entries);
        let filtered: Vec<usize> = (0..entries.len()).collect();
        Self {
            input: String::new(),
            selected: 0,
            entries,
            mode,
            filtered,
        }
    }

    pub fn filtered_entries(&self) -> Vec<(usize, &TabPickerEntry)> {
        self.filtered
            .iter()
            .map(|&i| (i, &self.entries[i]))
            .collect()
    }

    pub fn update_filter(&mut self) {
        let query = self.input.to_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            self.filtered = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    e.name.to_lowercase().contains(&query)
                        || e.description.to_lowercase().contains(&query)
                        || e.command
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                })
                .map(|(i, _)| i)
                .collect();
        }
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    /// Returns the command to spawn for the selected entry, or None if nothing is selected.
    pub fn selected_command(&self) -> Option<String> {
        self.filtered.get(self.selected).map(|&i| {
            let entry = &self.entries[i];
            let base = match self.mode {
                TabPickerMode::NewTab => "new-window",
                TabPickerMode::SplitHorizontal => "split-window -h",
                TabPickerMode::SplitVertical => "split-window -v",
            };
            match &entry.command {
                Some(cmd) => format!("{} -c {}", base, cmd),
                None => base.to_string(),
            }
        })
    }
}

fn build_entries(custom_entries: &[TabPickerEntryConfig]) -> Vec<TabPickerEntry> {
    let mut entries = Vec::new();

    // -- Shells --
    entries.push(TabPickerEntry {
        name: "Shell".into(),
        command: None,
        description: "Default shell ($SHELL)".into(),
        section: TabPickerSection::Shells,
    });

    for (name, path, desc) in [
        ("Bash", "/bin/bash", "Bourne Again Shell"),
        ("Zsh", "/bin/zsh", "Z Shell"),
        ("Fish", "/usr/local/bin/fish", "Friendly Interactive Shell"),
        (
            "Fish (Homebrew)",
            "/opt/homebrew/bin/fish",
            "Friendly Interactive Shell",
        ),
    ] {
        if std::path::Path::new(path).exists() {
            entries.push(TabPickerEntry {
                name: name.into(),
                command: Some(path.into()),
                description: desc.into(),
                section: TabPickerSection::Shells,
            });
        }
    }

    // -- Custom entries from config --
    for ce in custom_entries {
        entries.push(TabPickerEntry {
            name: ce.name.clone(),
            command: Some(ce.command.clone()),
            description: ce.description.clone().unwrap_or_default(),
            section: TabPickerSection::Custom,
        });
    }

    // -- Tools --
    for (name, cmd, desc) in [
        ("Htop", "htop", "Interactive process viewer"),
        ("Btop", "btop", "Resource monitor"),
        ("Python", "python3", "Python REPL"),
        ("Node", "node", "Node.js REPL"),
    ] {
        if std::process::Command::new("sh")
            .args(["-c", &format!("command -v {} >/dev/null 2>&1", cmd)])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            entries.push(TabPickerEntry {
                name: name.into(),
                command: Some(cmd.into()),
                description: desc.into(),
                section: TabPickerSection::Tools,
            });
        }
    }

    // -- Recent commands from shell history --
    if let Some(recent) = read_recent_commands(5) {
        for cmd in recent {
            entries.push(TabPickerEntry {
                name: cmd.clone(),
                command: Some(cmd),
                description: "From history".into(),
                section: TabPickerSection::Recent,
            });
        }
    }

    entries
}

/// Read the last N unique commands from shell history.
fn read_recent_commands(count: usize) -> Option<Vec<String>> {
    let home = std::env::var("HOME")
        .ok()
        .map(std::path::PathBuf::from)?;

    // Try zsh first, then bash
    let history_path = {
        let zsh = home.join(".zsh_history");
        if zsh.exists() {
            zsh
        } else {
            let bash = home.join(".bash_history");
            if bash.exists() {
                bash
            } else {
                return None;
            }
        }
    };

    let content = std::fs::read_to_string(&history_path).ok()?;
    let mut seen = std::collections::HashSet::new();
    let mut recent = Vec::new();

    for line in content.lines().rev() {
        // zsh history format: ": timestamp:0;command"
        let cmd = if let Some(idx) = line.find(";") {
            line[idx + 1..].trim()
        } else {
            line.trim()
        };

        if cmd.is_empty() || cmd.len() < 3 {
            continue;
        }
        // Skip common shell builtins
        if matches!(
            cmd.split_whitespace().next(),
            Some("cd" | "ls" | "echo" | "export" | "source" | "exit" | "clear")
        ) {
            continue;
        }
        if seen.insert(cmd.to_string()) {
            recent.push(cmd.to_string());
            if recent.len() >= count {
                break;
            }
        }
    }

    if recent.is_empty() {
        None
    } else {
        Some(recent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_custom_entries() -> Vec<TabPickerEntryConfig> {
        vec![
            TabPickerEntryConfig {
                name: "My Tool".into(),
                command: "mytool".into(),
                description: Some("A custom tool".into()),
            },
            TabPickerEntryConfig {
                name: "Editor".into(),
                command: "vim".into(),
                description: None,
            },
        ]
    }

    #[test]
    fn test_new_creates_state_with_entries() {
        let state = TabPickerState::new(&[]);
        assert_eq!(state.mode, TabPickerMode::NewTab);
        assert!(state.input.is_empty());
        assert_eq!(state.selected, 0);
        // Should always have at least "Shell" entry
        assert!(
            state.entries.iter().any(|e| e.name == "Shell"),
            "should contain the default Shell entry"
        );
    }

    #[test]
    fn test_with_mode_split_horizontal() {
        let state = TabPickerState::with_mode(&[], TabPickerMode::SplitHorizontal);
        assert_eq!(state.mode, TabPickerMode::SplitHorizontal);
    }

    #[test]
    fn test_with_mode_split_vertical() {
        let state = TabPickerState::with_mode(&[], TabPickerMode::SplitVertical);
        assert_eq!(state.mode, TabPickerMode::SplitVertical);
    }

    #[test]
    fn test_custom_entries_included() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&custom);
        assert!(
            state.entries.iter().any(|e| e.name == "My Tool"),
            "custom entries should be included"
        );
        assert!(
            state.entries.iter().any(|e| e.name == "Editor"),
            "custom entries should be included"
        );
    }

    #[test]
    fn test_custom_entries_in_custom_section() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&custom);
        let my_tool = state.entries.iter().find(|e| e.name == "My Tool").unwrap();
        assert_eq!(my_tool.section, TabPickerSection::Custom);
        assert_eq!(my_tool.description, "A custom tool");
        assert_eq!(my_tool.command, Some("mytool".into()));
    }

    #[test]
    fn test_custom_entry_empty_description() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&custom);
        let editor = state.entries.iter().find(|e| e.name == "Editor").unwrap();
        assert_eq!(editor.description, "");
    }

    #[test]
    fn test_filtered_entries_all_initially() {
        let state = TabPickerState::new(&[]);
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), state.entries.len());
    }

    #[test]
    fn test_move_down_increments() {
        let state_entries = make_custom_entries();
        let mut state = TabPickerState::new(&state_entries);
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn test_move_down_clamps_at_end() {
        let mut state = TabPickerState::new(&[]);
        let max = state.filtered_entries().len() - 1;
        // Move past the end
        for _ in 0..max + 5 {
            state.move_down();
        }
        assert_eq!(state.selected, max);
    }

    #[test]
    fn test_move_up_decrements() {
        let mut state = TabPickerState::new(&[]);
        state.selected = 2;
        state.move_up();
        assert_eq!(state.selected, 1);
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_move_up_clamps_at_zero() {
        let mut state = TabPickerState::new(&[]);
        state.selected = 0;
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_filter_narrows_results() {
        let custom = vec![TabPickerEntryConfig {
            name: "UniqueXYZ".into(),
            command: "xyz_cmd".into(),
            description: Some("special tool".into()),
        }];
        let mut state = TabPickerState::new(&custom);
        state.input = "UniqueXYZ".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.name, "UniqueXYZ");
    }

    #[test]
    fn test_filter_case_insensitive() {
        let custom = vec![TabPickerEntryConfig {
            name: "MySpecialTool".into(),
            command: "mst".into(),
            description: None,
        }];
        let mut state = TabPickerState::new(&custom);
        state.input = "myspecialtool".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(
            filtered.iter().any(|(_, e)| e.name == "MySpecialTool"),
            "filter should be case-insensitive"
        );
    }

    #[test]
    fn test_filter_matches_description() {
        let custom = vec![TabPickerEntryConfig {
            name: "Foo".into(),
            command: "foo".into(),
            description: Some("Unique Description Here".into()),
        }];
        let mut state = TabPickerState::new(&custom);
        state.input = "unique description".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(
            filtered.iter().any(|(_, e)| e.name == "Foo"),
            "should match on description"
        );
    }

    #[test]
    fn test_filter_matches_command() {
        let custom = vec![TabPickerEntryConfig {
            name: "Foo".into(),
            command: "my_unique_command_xyz".into(),
            description: None,
        }];
        let mut state = TabPickerState::new(&custom);
        state.input = "unique_command".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(
            filtered.iter().any(|(_, e)| e.name == "Foo"),
            "should match on command"
        );
    }

    #[test]
    fn test_filter_no_match() {
        let mut state = TabPickerState::new(&[]);
        state.input = "zzzzz_no_match_ever".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_clears_restores_all() {
        let mut state = TabPickerState::new(&[]);
        let total = state.entries.len();
        state.input = "zzzzz".to_string();
        state.update_filter();
        assert_eq!(state.filtered_entries().len(), 0);
        state.input.clear();
        state.update_filter();
        assert_eq!(state.filtered_entries().len(), total);
    }

    #[test]
    fn test_filter_resets_selection_when_out_of_bounds() {
        let custom = vec![
            TabPickerEntryConfig { name: "A".into(), command: "a".into(), description: None },
            TabPickerEntryConfig { name: "B".into(), command: "b".into(), description: None },
        ];
        let mut state = TabPickerState::new(&custom);
        // Move to a high index
        for _ in 0..state.entries.len() {
            state.move_down();
        }
        let prev_selected = state.selected;
        // Now filter to only 1 result
        state.input = "zzzzz_no_match".to_string();
        state.update_filter();
        // Selected should be clamped
        assert_eq!(state.selected, 0);
        assert!(prev_selected > 0, "test setup: should have moved down");
    }

    #[test]
    fn test_selected_command_shell() {
        let state = TabPickerState::new(&[]);
        // Shell entry has no command -> returns just base
        let cmd = state.selected_command().unwrap();
        assert_eq!(cmd, "new-window");
    }

    #[test]
    fn test_selected_command_split_horizontal() {
        let custom = vec![TabPickerEntryConfig {
            name: "Vim".into(),
            command: "vim".into(),
            description: None,
        }];
        let mut state = TabPickerState::with_mode(&custom, TabPickerMode::SplitHorizontal);
        // Select the custom entry
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Vim" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert_eq!(cmd, "split-window -h -c vim");
    }

    #[test]
    fn test_selected_command_split_vertical() {
        let custom = vec![TabPickerEntryConfig {
            name: "Vim".into(),
            command: "vim".into(),
            description: None,
        }];
        let mut state = TabPickerState::with_mode(&custom, TabPickerMode::SplitVertical);
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Vim" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert_eq!(cmd, "split-window -v -c vim");
    }

    #[test]
    fn test_section_labels() {
        assert_eq!(TabPickerSection::Shells.label(), "Shells");
        assert_eq!(TabPickerSection::Custom.label(), "Custom");
        assert_eq!(TabPickerSection::Tools.label(), "Tools");
        assert_eq!(TabPickerSection::Recent.label(), "Recent");
    }
}

pub fn render(state: &TabPickerState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup_area = dialog::popup_rect(
        dialog::PopupSize::FixedClamped { width: 55, height: 20, pad: 4 },
        dialog::PopupAnchor::Center,
        area,
    );

    let title = match state.mode {
        TabPickerMode::NewTab => "New Tab",
        TabPickerMode::SplitHorizontal => "Split Right",
        TabPickerMode::SplitVertical => "Split Down",
    };

    let inner = dialog::render_popup(frame, popup_area, title, theme);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    // Filter input
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    dialog::render_filter_input_placeholder(frame, input_area, &state.input, Some("command"), theme);

    // Separator
    let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    dialog::render_separator(frame, sep_area);

    // Build list items with section info
    let filtered = state.filtered_entries();
    let show_sections = state.input.is_empty();
    let has_input = !state.input.trim().is_empty();
    let no_matches = filtered.is_empty();

    // Custom command label shown when input doesn't match or as first option
    let run_label = format!("Run '{}'", state.input.trim());
    let run_desc = "Custom command".to_string();

    let mut items: Vec<dialog::ListItem> = Vec::new();

    // Show "Run '<input>'" at the top when there are no matches
    if has_input && no_matches {
        items.push(dialog::ListItem {
            label: &run_label,
            description: &run_desc,
            section: None,
            hint: None,
        });
    }

    for (_, entry) in &filtered {
        items.push(dialog::ListItem {
            label: &entry.name,
            description: &entry.description,
            section: if show_sections {
                Some(entry.section.label())
            } else {
                None
            },
            hint: None,
        });
    }

    let list_area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );
    dialog::render_select_list(frame, list_area, &items, state.selected, show_sections, theme);
}
