use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use pane_protocol::config::Theme;

/// State for the fuzzy tab picker overlay.
pub struct TabPickerState {
    pub input: String,
    pub selected: usize,
    pub entries: Vec<TabPickerEntry>,
    filtered: Vec<usize>,
}

#[derive(Clone)]
pub struct TabPickerEntry {
    pub name: String,
    pub command: Option<String>,
    pub description: String,
}

impl TabPickerState {
    pub fn new() -> Self {
        let entries = default_entries();
        let filtered: Vec<usize> = (0..entries.len()).collect();
        Self {
            input: String::new(),
            selected: 0,
            entries,
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
            match &entry.command {
                Some(cmd) => format!("new-window -c {}", cmd),
                None => "new-window".to_string(),
            }
        })
    }
}

fn default_entries() -> Vec<TabPickerEntry> {
    let mut entries = vec![TabPickerEntry {
        name: "Shell".into(),
        command: None,
        description: "Default shell ($SHELL)".into(),
    }];

    // Add common shells if they exist
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
            });
        }
    }

    // Add common tools (check PATH via `command -v`)
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
            });
        }
    }

    entries
}

pub fn render(state: &TabPickerState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup_w = 50u16.min(area.width.saturating_sub(4));
    let popup_h = 15u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" New Tab ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    // Input line
    let input_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent)),
        Span::raw(&state.input),
        Span::styled("_", Style::default().fg(Color::DarkGray)),
    ]);
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(Paragraph::new(input_line), input_area);

    // Separator
    let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    let sep = Line::from("─".repeat(inner.width as usize));
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(Color::DarkGray)),
        sep_area,
    );

    // Entries
    let list_area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );
    let filtered = state.filtered_entries();

    for (i, (_idx, entry)) in filtered.iter().enumerate() {
        if i as u16 >= list_area.height {
            break;
        }
        let is_selected = i == state.selected;
        let style = if is_selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };

        let prefix = if is_selected { "▸ " } else { "  " };
        let line = Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(&entry.name, style),
            Span::styled(
                format!("  {}", &entry.description),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let row = Rect::new(list_area.x, list_area.y + i as u16, list_area.width, 1);
        frame.render_widget(Paragraph::new(line), row);
    }
}
