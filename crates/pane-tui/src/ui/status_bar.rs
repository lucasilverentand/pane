use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use pane_protocol::app::Mode;
use crate::client::Client;
use pane_protocol::config::Theme;
use crate::ui::format::format_string;

fn format_leader_key(key: &crossterm::event::KeyEvent) -> String {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Char(c) => c.to_string(),
        _ => "?".to_string(),
    }
}

/// Render the status bar for a daemon-connected client.
pub fn render_client(client: &Client, theme: &Theme, frame: &mut Frame, area: Rect) {
    let (left, right) = if client.hub_active && client.mode != Mode::Palette && client.mode != Mode::Confirm {
        (" Hub".to_string(), "type to search  enter open  esc back ".to_string())
    } else { match &client.mode {
        Mode::Normal => {
            let vars = build_client_vars(client);
            let left = format_string(&client.config.status_bar.left, &vars);
            let right = if client.workspace_bar_focused {
                "h/l switch  d close  n new  j exit bar ".to_string()
            } else {
                format_string(&client.config.status_bar.right, &vars)
            };
            (left, right)
        }
        Mode::Interact => {
            let vars = build_client_vars(client);
            let left = format_string(&client.config.status_bar.left, &vars);
            let right = format_string(&client.config.status_bar.right, &vars);
            (left, right)
        }
        Mode::Scroll => {
            let title = client_pane_title(client);
            let right = "j/k up/down  u/d page  g/G top/end  esc quit ".to_string();
            (title, right)
        }
        Mode::Copy => {
            let title = client_pane_title(client);
            (
                title,
                "hjkl move  v select  y yank  / search  esc quit ".to_string(),
            )
        }
        Mode::Palette => (
            String::new(),
            "type to filter  enter run  esc cancel ".to_string(),
        ),
        Mode::Confirm => (String::new(), "enter/y confirm  esc/n cancel ".to_string()),
        Mode::Leader => {
            let path_str = if let Some(ref ls) = client.leader_state {
                let keys: Vec<String> = ls.path.iter().map(format_leader_key).collect();
                if keys.is_empty() {
                    "⎵".to_string()
                } else {
                    format!("⎵ {}", keys.join(" "))
                }
            } else {
                "⎵".to_string()
            };
            (path_str, "esc cancel ".to_string())
        }
        Mode::TabPicker => (
            String::new(),
            "type to filter  enter spawn  esc cancel ".to_string(),
        ),
        Mode::Rename => (
            String::new(),
            "enter confirm  esc cancel ".to_string(),
        ),
        Mode::NewWorkspaceInput => {
            let hint = if let Some(ref nw) = client.new_workspace_input {
                match nw.stage {
                    crate::client::NewWorkspaceStage::Directory => {
                        "enter select  tab complete  \u{2192} open  esc cancel "
                    }
                    crate::client::NewWorkspaceStage::Name => {
                        "enter create  esc back "
                    }
                }
            } else {
                "esc cancel "
            };
            (String::new(), hint.to_string())
        }
        Mode::ContextMenu => (
            String::new(),
            "j/k navigate  enter select  esc cancel ".to_string(),
        ),
        Mode::ProjectHub => (
            String::new(),
            "type to search  enter open  esc cancel ".to_string(),
        ),
        Mode::Resize => {
            let selected = client.resize_state.as_ref().and_then(|rs| rs.selected);
            match selected {
                None => (
                    " RESIZE".to_string(),
                    "hjkl select border  = equalize  esc quit ".to_string(),
                ),
                Some(b) => {
                    let label = match b {
                        pane_protocol::app::ResizeBorder::Left => "LEFT",
                        pane_protocol::app::ResizeBorder::Right => "RIGHT",
                        pane_protocol::app::ResizeBorder::Top => "TOP",
                        pane_protocol::app::ResizeBorder::Bottom => "BOTTOM",
                    };
                    let hint = match b {
                        pane_protocol::app::ResizeBorder::Left
                        | pane_protocol::app::ResizeBorder::Right => "h/l move  esc quit ",
                        pane_protocol::app::ResizeBorder::Top
                        | pane_protocol::app::ResizeBorder::Bottom => "j/k move  esc quit ",
                    };
                    (format!(" RESIZE [{}]", label), hint.to_string())
                }
            }
        }
    } };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_leader_key_lowercase() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('a'),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "a");
    }

    #[test]
    fn format_leader_key_uppercase() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('A'),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "A");
    }

    #[test]
    fn format_leader_key_with_shift() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('b'),
            modifiers: crossterm::event::KeyModifiers::SHIFT,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "b");
    }

    #[test]
    fn format_leader_key_non_char() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "?");
    }

    #[test]
    fn format_leader_key_special_char() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('/'),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "/");
    }

    #[test]
    fn format_leader_key_esc() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Esc,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "?");
    }

    #[test]
    fn format_leader_key_tab() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Tab,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "?");
    }

    #[test]
    fn format_leader_key_digit() {
        let key = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('5'),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(format_leader_key(&key), "5");
    }
}

fn build_client_vars(client: &Client) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    let title = client_pane_title(client);
    vars.insert("pane_title".to_string(), title);

    if let Some(ws) = client.active_workspace() {
        vars.insert(
            "session_name".to_string(),
            "pane".to_string(),
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
