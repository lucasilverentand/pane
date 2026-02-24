use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{BaseMode, Overlay};
use crate::client::Client;
use crate::config::Theme;
use crate::ui::format::format_string;

fn format_leader_key(key: &crossterm::event::KeyEvent) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::SHIFT) || c.is_uppercase() {
                c.to_string()
            } else {
                c.to_string()
            }
        }
        _ => "?".to_string(),
    }
}

/// Render the status bar for a daemon-connected client.
pub fn render_client(client: &Client, theme: &Theme, frame: &mut Frame, area: Rect) {
    let (left, right) = if let Some(ref overlay) = client.mode.overlay {
        match overlay {
            Overlay::Scroll => {
                let title = client_pane_title(client);
                let mode_left = format!("SCROLL {}", title);
                let right = "j/k up/down  u/d page  g/G top/end  esc quit ".to_string();
                (mode_left, right)
            }
            Overlay::Copy => {
                let title = client_pane_title(client);
                (
                    format!("COPY {}", title),
                    "hjkl move  v select  y yank  / search  esc quit ".to_string(),
                )
            }
            Overlay::CommandPalette => (
                "CMD ".to_string(),
                "type to filter  enter run  esc cancel ".to_string(),
            ),
            Overlay::Confirm => (String::new(), "enter/y confirm  esc/n cancel ".to_string()),
            Overlay::Leader => {
                let path_str = if let Some(ref ls) = client.leader_state {
                    let keys: Vec<String> =
                        ls.path.iter().map(|k| format_leader_key(k)).collect();
                    if keys.is_empty() {
                        "SPC".to_string()
                    } else {
                        format!("SPC {}", keys.join(" "))
                    }
                } else {
                    "SPC".to_string()
                };
                (format!("LEADER {}", path_str), "esc cancel ".to_string())
            }
            Overlay::TabPicker => (
                "NEW TAB ".to_string(),
                "type to filter  enter spawn  esc cancel ".to_string(),
            ),
        }
    } else {
        match client.mode.base {
            BaseMode::Normal => {
                let vars = build_client_vars(client);
                let left = format!(
                    "NORMAL {}",
                    format_string(&client.config.status_bar.left, &vars)
                );
                let right = format_string(&client.config.status_bar.right, &vars);
                (left, right)
            }
            BaseMode::Interact => {
                let vars = build_client_vars(client);
                let left = format_string(&client.config.status_bar.left, &vars);
                let right = format_string(&client.config.status_bar.right, &vars);
                (left, right)
            }
        }
    };

    // Build plugin segment string
    let plugin_text: String = client
        .plugin_segments
        .iter()
        .flat_map(|segs| segs.iter())
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" │ ");

    let left_len = left.len();
    let right_len = right.len();
    let plugin_len = if plugin_text.is_empty() {
        0
    } else {
        plugin_text.len() + 2
    }; // " │ " prefix
    let padding = (area.width as usize).saturating_sub(left_len + plugin_len + right_len);

    let mut spans = vec![Span::styled(
        left,
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    if !plugin_text.is_empty() {
        spans.push(Span::styled(
            format!(" │ {}", plugin_text),
            Style::default().fg(theme.accent),
        ));
    }
    spans.push(Span::raw(" ".repeat(padding)));
    spans.push(Span::styled(right, Style::default().fg(theme.dim)));

    let line = Line::from(spans);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

fn client_pane_title(client: &Client) -> String {
    if let Some(ws) = client.active_workspace() {
        if let Some(group) = ws.groups.iter().find(|g| g.id == ws.active_group) {
            if let Some(pane) = group.tabs.get(group.active_tab) {
                return pane.title.lines().next().unwrap_or("").to_string();
            }
        }
    }
    String::new()
}

fn build_client_vars(client: &Client) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    let title = client_pane_title(client);
    vars.insert("pane_title".to_string(), title);

    if let Some(ws) = client.active_workspace() {
        vars.insert(
            "session_name".to_string(),
            client.render_state.session_name.clone(),
        );
        vars.insert("window_name".to_string(), ws.name.clone());

        let group_ids = ws.layout.group_ids();
        let pane_count = group_ids.len();
        vars.insert("pane_count".to_string(), pane_count.to_string());

        if let Some(idx) = group_ids.iter().position(|id| *id == ws.active_group) {
            vars.insert("pane_index".to_string(), (idx + 1).to_string());
        }
        vars.insert(
            "window_index".to_string(),
            (client.render_state.active_workspace + 1).to_string(),
        );
    }

    // Client count (available as {client_count} in status bar templates)
    vars.insert("client_count".to_string(), client.client_count.to_string());

    if client.config.status_bar.show_cpu {
        vars.insert("cpu".to_string(), client.system_stats.format_cpu());
    }
    if client.config.status_bar.show_memory {
        vars.insert("mem".to_string(), client.system_stats.format_memory());
    }
    if client.config.status_bar.show_load {
        vars.insert("load".to_string(), client.system_stats.format_load());
    }
    if client.config.status_bar.show_disk {
        vars.insert("disk".to_string(), client.system_stats.format_disk());
    }

    vars
}
