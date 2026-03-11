use ratatui::{layout::Rect, Frame};

use pane_protocol::config::{FavoriteConfig, TabPickerEntryConfig, Theme};

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
    Favorites,
    Shells,
    Custom,
    Tools,
    Recent,
}

impl TabPickerSection {
    fn label(&self) -> &'static str {
        match self {
            Self::Favorites => "Favorites",
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
    /// Shell to wrap the command in (e.g. "/bin/zsh"). When set, the command
    /// is executed as `shell -c "command"` rather than directly.
    pub shell: Option<String>,
}

impl TabPickerState {
    pub fn new(
        custom_entries: &[TabPickerEntryConfig],
        favorites: &[FavoriteConfig],
    ) -> Self {
        Self::with_mode(custom_entries, favorites, TabPickerMode::NewTab)
    }

    pub fn with_mode(
        custom_entries: &[TabPickerEntryConfig],
        favorites: &[FavoriteConfig],
        mode: TabPickerMode,
    ) -> Self {
        let entries = build_entries(custom_entries, favorites);
        let filtered: Vec<usize> = (0..entries.len()).collect();
        // Default selection to the first Recent entry if one exists.
        let selected = entries
            .iter()
            .position(|e| e.section == TabPickerSection::Recent)
            .unwrap_or(0);
        Self {
            input: String::new(),
            selected,
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
            build_command_string(self.mode, entry.command.as_deref(), entry.shell.as_deref())
        })
    }
}

/// Build the tmux-style command string with optional shell wrapping.
fn build_command_string(mode: TabPickerMode, command: Option<&str>, shell: Option<&str>) -> String {
    let base = match mode {
        TabPickerMode::NewTab => "new-window",
        TabPickerMode::SplitHorizontal => "split-window -h",
        TabPickerMode::SplitVertical => "split-window -v",
    };
    let mut parts = base.to_string();
    if let Some(cmd) = command {
        parts.push_str(&format!(" -c \"{}\"", cmd));
    }
    if let Some(sh) = shell {
        parts.push_str(&format!(" -s \"{}\"", sh));
    }
    parts
}

/// Detect the user's default shell from $SHELL.
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

fn build_entries(
    custom_entries: &[TabPickerEntryConfig],
    favorites: &[FavoriteConfig],
) -> Vec<TabPickerEntry> {
    let mut entries = Vec::new();
    let user_shell = default_shell();

    // -- Favorites --
    for fav in favorites {
        entries.push(TabPickerEntry {
            name: fav.name.clone(),
            command: Some(fav.command.clone()),
            description: fav.description.clone().unwrap_or_default(),
            section: TabPickerSection::Favorites,
            shell: Some(fav.shell.clone().unwrap_or_else(|| user_shell.clone())),
        });
    }

    // -- Shells --
    entries.push(TabPickerEntry {
        name: "Shell".into(),
        command: None,
        description: format!(
            "Default ({})",
            std::path::Path::new(&user_shell)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("shell")
        ),
        section: TabPickerSection::Shells,
        shell: None,
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
                // For shell entries, the command IS the shell — no wrapping needed
                command: Some(path.into()),
                description: desc.into(),
                section: TabPickerSection::Shells,
                shell: None,
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
            shell: Some(ce.shell.clone().unwrap_or_else(|| user_shell.clone())),
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
                shell: Some(user_shell.clone()),
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
                shell: Some(user_shell.clone()),
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

/// Mouse hit-test result for the tab picker.
pub enum TabPickerClick {
    /// Clicked on a list item at the given visible index.
    Item(usize),
}

/// Compute the popup area for the tab picker (must match render logic).
fn compute_popup_area(area: Rect) -> Rect {
    dialog::popup_rect(
        dialog::PopupSize::FixedClamped { width: 60, height: 22, pad: 4 },
        dialog::PopupAnchor::Center,
        area,
    )
}

/// Hit-test a mouse click against the tab picker.
///
/// `area` must be the same area passed to `render()`.
///
/// Returns `Some(TabPickerClick::Item(idx))` if the click landed on a list
/// item, where `idx` is the filtered item index (suitable for assigning to
/// `state.selected`). Returns `None` if the click is outside the list area.
pub fn hit_test(state: &TabPickerState, area: Rect, x: u16, y: u16) -> Option<TabPickerClick> {
    let popup_area = compute_popup_area(area);
    let inner = dialog::inner_rect(popup_area);

    if inner.height < 3 || inner.width < 10 {
        return None;
    }

    let list_area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );

    if x < list_area.x
        || x >= list_area.x + list_area.width
        || y < list_area.y
        || y >= list_area.y + list_area.height
    {
        return None;
    }

    let row = (y - list_area.y) as usize;
    let filtered = state.filtered_entries();
    let show_sections = state.input.is_empty();

    // Compute scroll offset to match render_select_list logic
    let visible_count = list_area.height as usize;
    let scroll_offset = if state.selected >= visible_count {
        state.selected - visible_count + 1
    } else {
        0
    };

    // Walk through items accounting for section headers and scroll to map row → item index
    let mut visual_row = 0usize;
    let mut item_idx_visual = 0usize; // counts headers + items for scroll comparison
    let mut last_section: Option<&str> = None;

    for (item_idx, (_, entry)) in filtered.iter().enumerate() {
        if show_sections {
            let section = entry.section.label();
            let need_header = match last_section {
                None => true,
                Some(prev) => prev != section,
            };
            if need_header {
                last_section = Some(section);
                if item_idx_visual >= scroll_offset {
                    if visual_row == row {
                        // Clicked on a section header — ignore
                        return None;
                    }
                    visual_row += 1;
                }
                item_idx_visual += 1;
            }
        }
        if item_idx_visual < scroll_offset {
            item_idx_visual += 1;
            continue;
        }
        if visual_row == row {
            return Some(TabPickerClick::Item(item_idx));
        }
        visual_row += 1;
        item_idx_visual += 1;
    }

    None
}

/// Check whether a click is inside the popup area.
pub fn is_inside_popup(area: Rect, x: u16, y: u16) -> bool {
    let popup = compute_popup_area(area);
    x >= popup.x && x < popup.x + popup.width && y >= popup.y && y < popup.y + popup.height
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
                shell: None,
            },
            TabPickerEntryConfig {
                name: "Editor".into(),
                command: "vim".into(),
                description: None,
                shell: Some("/bin/zsh".into()),
            },
        ]
    }

    fn make_favorites() -> Vec<FavoriteConfig> {
        vec![FavoriteConfig {
            name: "Dev Server".into(),
            command: "npm run dev".into(),
            description: Some("Start dev server".into()),
            shell: Some("/bin/zsh".into()),
        }]
    }

    #[test]
    fn test_new_creates_state_with_entries() {
        let state = TabPickerState::new(&[], &[]);
        assert_eq!(state.mode, TabPickerMode::NewTab);
        assert!(state.input.is_empty());
        // selected defaults to first Recent entry, or 0 if none
        let has_recents = state.entries.iter().any(|e| e.section == TabPickerSection::Recent);
        if has_recents {
            let first_recent = state.entries.iter().position(|e| e.section == TabPickerSection::Recent).unwrap();
            assert_eq!(state.selected, first_recent);
        } else {
            assert_eq!(state.selected, 0);
        }
        // Should always have at least "Shell" entry
        assert!(
            state.entries.iter().any(|e| e.name == "Shell"),
            "should contain the default Shell entry"
        );
    }

    #[test]
    fn test_with_mode_split_horizontal() {
        let state = TabPickerState::with_mode(&[], &[], TabPickerMode::SplitHorizontal);
        assert_eq!(state.mode, TabPickerMode::SplitHorizontal);
    }

    #[test]
    fn test_with_mode_split_vertical() {
        let state = TabPickerState::with_mode(&[], &[], TabPickerMode::SplitVertical);
        assert_eq!(state.mode, TabPickerMode::SplitVertical);
    }

    #[test]
    fn test_custom_entries_included() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&custom, &[]);
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
        let state = TabPickerState::new(&custom, &[]);
        let my_tool = state.entries.iter().find(|e| e.name == "My Tool").unwrap();
        assert_eq!(my_tool.section, TabPickerSection::Custom);
        assert_eq!(my_tool.description, "A custom tool");
        assert_eq!(my_tool.command, Some("mytool".into()));
    }

    #[test]
    fn test_custom_entry_empty_description() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&custom, &[]);
        let editor = state.entries.iter().find(|e| e.name == "Editor").unwrap();
        assert_eq!(editor.description, "");
    }

    #[test]
    fn test_custom_entry_with_explicit_shell() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&custom, &[]);
        let editor = state.entries.iter().find(|e| e.name == "Editor").unwrap();
        assert_eq!(editor.shell, Some("/bin/zsh".into()));
    }

    #[test]
    fn test_custom_entry_inherits_default_shell() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&custom, &[]);
        let my_tool = state.entries.iter().find(|e| e.name == "My Tool").unwrap();
        // Should have inherited the default shell
        assert!(my_tool.shell.is_some(), "should have a shell set");
    }

    #[test]
    fn test_favorites_shown_first() {
        let favs = make_favorites();
        let state = TabPickerState::new(&[], &favs);
        assert_eq!(state.entries[0].name, "Dev Server");
        assert_eq!(state.entries[0].section, TabPickerSection::Favorites);
    }

    #[test]
    fn test_favorites_have_shell() {
        let favs = make_favorites();
        let state = TabPickerState::new(&[], &favs);
        let fav = state.entries.iter().find(|e| e.name == "Dev Server").unwrap();
        assert_eq!(fav.shell, Some("/bin/zsh".into()));
    }

    #[test]
    fn test_filtered_entries_all_initially() {
        let state = TabPickerState::new(&[], &[]);
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), state.entries.len());
    }

    #[test]
    fn test_move_down_increments() {
        let state_entries = make_custom_entries();
        let mut state = TabPickerState::new(&state_entries, &[]);
        // Reset to 0 so we can test relative movement
        state.selected = 0;
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn test_move_down_clamps_at_end() {
        let mut state = TabPickerState::new(&[], &[]);
        let max = state.filtered_entries().len() - 1;
        // Move past the end
        for _ in 0..max + 5 {
            state.move_down();
        }
        assert_eq!(state.selected, max);
    }

    #[test]
    fn test_move_up_decrements() {
        let mut state = TabPickerState::new(&[], &[]);
        state.selected = 2;
        state.move_up();
        assert_eq!(state.selected, 1);
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_move_up_clamps_at_zero() {
        let mut state = TabPickerState::new(&[], &[]);
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
            shell: None,
        }];
        let mut state = TabPickerState::new(&custom, &[]);
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
            shell: None,
        }];
        let mut state = TabPickerState::new(&custom, &[]);
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
            shell: None,
        }];
        let mut state = TabPickerState::new(&custom, &[]);
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
            shell: None,
        }];
        let mut state = TabPickerState::new(&custom, &[]);
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
        let mut state = TabPickerState::new(&[], &[]);
        state.input = "zzzzz_no_match_ever".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_clears_restores_all() {
        let mut state = TabPickerState::new(&[], &[]);
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
            TabPickerEntryConfig { name: "A".into(), command: "a".into(), description: None, shell: None },
            TabPickerEntryConfig { name: "B".into(), command: "b".into(), description: None, shell: None },
        ];
        let mut state = TabPickerState::new(&custom, &[]);
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
        let mut state = TabPickerState::new(&[], &[]);
        // Select the Shell entry (which has no command → returns just base)
        let shell_idx = state.entries.iter().position(|e| e.name == "Shell").unwrap();
        state.selected = state.filtered.iter().position(|&i| i == shell_idx).unwrap();
        let cmd = state.selected_command().unwrap();
        assert_eq!(cmd, "new-window");
    }

    #[test]
    fn test_selected_command_with_shell_wrapping() {
        let custom = vec![TabPickerEntryConfig {
            name: "Htop".into(),
            command: "htop".into(),
            description: None,
            shell: Some("/bin/zsh".into()),
        }];
        let mut state = TabPickerState::new(&custom, &[]);
        // Select the custom entry
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Htop" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert!(cmd.contains("-c \"htop\""), "should include -c flag: {}", cmd);
        assert!(cmd.contains("-s \"/bin/zsh\""), "should include -s flag: {}", cmd);
    }

    #[test]
    fn test_selected_command_split_horizontal() {
        let custom = vec![TabPickerEntryConfig {
            name: "Vim".into(),
            command: "vim".into(),
            description: None,
            shell: None,
        }];
        let mut state = TabPickerState::with_mode(&custom, &[], TabPickerMode::SplitHorizontal);
        // Select the custom entry
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Vim" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert!(cmd.starts_with("split-window -h"), "cmd: {}", cmd);
    }

    #[test]
    fn test_selected_command_split_vertical() {
        let custom = vec![TabPickerEntryConfig {
            name: "Vim".into(),
            command: "vim".into(),
            description: None,
            shell: None,
        }];
        let mut state = TabPickerState::with_mode(&custom, &[], TabPickerMode::SplitVertical);
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Vim" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert!(cmd.starts_with("split-window -v"), "cmd: {}", cmd);
    }

    #[test]
    fn test_section_labels() {
        assert_eq!(TabPickerSection::Favorites.label(), "Favorites");
        assert_eq!(TabPickerSection::Shells.label(), "Shells");
        assert_eq!(TabPickerSection::Custom.label(), "Custom");
        assert_eq!(TabPickerSection::Tools.label(), "Tools");
        assert_eq!(TabPickerSection::Recent.label(), "Recent");
    }

    #[test]
    fn test_build_command_string_no_shell() {
        let cmd = build_command_string(TabPickerMode::NewTab, Some("vim"), None);
        assert_eq!(cmd, "new-window -c \"vim\"");
    }

    #[test]
    fn test_build_command_string_with_shell() {
        let cmd = build_command_string(TabPickerMode::NewTab, Some("htop"), Some("/bin/zsh"));
        assert_eq!(cmd, "new-window -c \"htop\" -s \"/bin/zsh\"");
    }

    #[test]
    fn test_build_command_string_no_command() {
        let cmd = build_command_string(TabPickerMode::NewTab, None, None);
        assert_eq!(cmd, "new-window");
    }

    #[test]
    fn test_shell_entries_have_no_shell_wrapping() {
        let state = TabPickerState::new(&[], &[]);
        let shell_entry = state.entries.iter().find(|e| e.name == "Shell").unwrap();
        assert!(shell_entry.shell.is_none(), "default shell entry should not have shell wrapping");
    }

    #[test]
    fn test_initial_selection_recent() {
        // Build entries with a known Recent entry by using a helper that
        // bypasses build_entries (which reads real shell history).
        let entries = vec![
            TabPickerEntry {
                name: "Shell".into(),
                command: None,
                description: "Default".into(),
                section: TabPickerSection::Shells,
                shell: None,
            },
            TabPickerEntry {
                name: "htop".into(),
                command: Some("htop".into()),
                description: "Process viewer".into(),
                section: TabPickerSection::Tools,
                shell: Some("/bin/zsh".into()),
            },
            TabPickerEntry {
                name: "cargo build".into(),
                command: Some("cargo build".into()),
                description: "From history".into(),
                section: TabPickerSection::Recent,
                shell: Some("/bin/zsh".into()),
            },
        ];
        let filtered: Vec<usize> = (0..entries.len()).collect();
        let selected = entries
            .iter()
            .position(|e| e.section == TabPickerSection::Recent)
            .unwrap_or(0);
        let state = TabPickerState {
            input: String::new(),
            selected,
            entries,
            mode: TabPickerMode::NewTab,
            filtered,
        };
        assert_eq!(state.selected, 2, "should point to first Recent entry");
        assert_eq!(state.entries[state.selected].section, TabPickerSection::Recent);
    }

    #[test]
    fn test_initial_selection_fallback() {
        // No Recent entries — should default to 0
        let entries = vec![
            TabPickerEntry {
                name: "Shell".into(),
                command: None,
                description: "Default".into(),
                section: TabPickerSection::Shells,
                shell: None,
            },
            TabPickerEntry {
                name: "htop".into(),
                command: Some("htop".into()),
                description: "Process viewer".into(),
                section: TabPickerSection::Tools,
                shell: Some("/bin/zsh".into()),
            },
        ];
        let filtered: Vec<usize> = (0..entries.len()).collect();
        let selected = entries
            .iter()
            .position(|e| e.section == TabPickerSection::Recent)
            .unwrap_or(0);
        let state = TabPickerState {
            input: String::new(),
            selected,
            entries,
            mode: TabPickerMode::NewTab,
            filtered,
        };
        assert_eq!(state.selected, 0, "should fall back to 0 when no recents");
    }

    #[test]
    fn test_tools_have_shell_wrapping() {
        // We can't reliably test this since it depends on installed tools,
        // but we can verify the build_entries logic with custom entries
        let state = TabPickerState::new(&[], &[]);
        for entry in &state.entries {
            if entry.section == TabPickerSection::Tools {
                assert!(entry.shell.is_some(), "tool '{}' should have shell wrapping", entry.name);
            }
        }
    }
}

pub fn render(state: &TabPickerState, theme: &Theme, hover: Option<(u16, u16)>, frame: &mut Frame, area: Rect) {
    let popup_area = compute_popup_area(area);

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
    let user_shell = default_shell();
    let shell_hint = std::path::Path::new(&user_shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("shell")
        .to_string();

    let mut items: Vec<dialog::ListItem> = Vec::new();

    // Show "Run '<input>'" at the top when there are no matches
    if has_input && no_matches {
        items.push(dialog::ListItem {
            label: &run_label,
            description: &run_desc,
            section: None,
            hint: Some(&shell_hint),
        });
    }

    // Build hint strings for each entry (needs to live long enough)
    let hints: Vec<Option<String>> = filtered
        .iter()
        .map(|(_, entry)| {
            entry.shell.as_ref().map(|s| {
                std::path::Path::new(s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("shell")
                    .to_string()
            })
        })
        .collect();

    for (idx, (_, entry)) in filtered.iter().enumerate() {
        items.push(dialog::ListItem {
            label: &entry.name,
            description: &entry.description,
            section: if show_sections {
                Some(entry.section.label())
            } else {
                None
            },
            hint: hints[idx].as_deref(),
        });
    }

    let list_area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );
    dialog::render_select_list(frame, list_area, &items, state.selected, show_sections, hover, theme);
}
