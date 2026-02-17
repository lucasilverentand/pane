use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Mode};
use crate::config::Theme;
use crate::ui::format::format_string;

pub fn render(app: &App, theme: &Theme, frame: &mut Frame, area: Rect) {
    let (left, right) = match &app.mode {
        Mode::Normal => {
            let vars = build_vars(app);
            let left = format_string(&app.state.config.status_bar.left, &vars);
            let right = format_string(&app.state.config.status_bar.right, &vars);
            (left, right)
        }
        Mode::Scroll => {
            let mode_left = build_mode_left(app, "[SCROLL] ");
            let right = "j/k up/down  u/d page  g/G top/end  esc quit ".to_string();
            (mode_left, right)
        }
        Mode::SessionPicker => (
            String::new(),
            "up/down navigate  enter open  n new  d delete  q quit ".to_string(),
        ),
        Mode::Help => (String::new(), "esc close  / search  j/k scroll ".to_string()),
        Mode::DevServerInput => (
            String::new(),
            "type command, enter to confirm, esc to cancel ".to_string(),
        ),
        Mode::Select => {
            let mode_left = build_mode_left(app, "[SELECT] ");
            (
                mode_left,
                "hjkl nav  n tab  d split  w close  1-9 pane  esc back ".to_string(),
            )
        }
        Mode::Copy => {
            let mode_left = build_mode_left(app, "[COPY] ");
            (
                mode_left,
                "hjkl move  v select  y yank  / search  esc quit ".to_string(),
            )
        }
        Mode::CommandPalette => (
            "[CMD] ".to_string(),
            "type to filter  enter run  esc cancel ".to_string(),
        ),
        Mode::Confirm => (
            String::new(),
            "enter/y confirm  esc/n cancel ".to_string(),
        ),
        Mode::Leader => {
            let path_str = if let Some(ref ls) = app.leader_state {
                let keys: Vec<String> = ls.path.iter().map(|k| format_leader_key(k)).collect();
                if keys.is_empty() {
                    "\\".to_string()
                } else {
                    format!("\\ {}", keys.join(" "))
                }
            } else {
                "\\".to_string()
            };
            (
                format!("[LEADER] {}", path_str),
                "esc cancel ".to_string(),
            )
        }
    };

    let left_len = left.len();
    let right_len = right.len();
    let padding = (area.width as usize).saturating_sub(left_len + right_len);

    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(padding)),
        Span::styled(right, Style::default().fg(theme.dim)),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

fn build_mode_left(app: &App, prefix: &str) -> String {
    let title = pane_title(app);
    format!("{}{}", prefix, title)
}

fn pane_title(app: &App) -> String {
    if app.state.workspaces.is_empty() {
        return String::new();
    }
    let ws = app.active_workspace();
    let title = ws
        .groups
        .get(&ws.active_group)
        .map(|g| g.active_pane().title.clone())
        .unwrap_or_default();
    // Only use the first line to avoid multi-line bleed in the status bar
    title.lines().next().unwrap_or("").to_string()
}

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

/// Build the template variables HashMap from app state.
fn build_vars(app: &App) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // Pane info
    let title = pane_title(app);
    vars.insert("pane_title".to_string(), title);

    if !app.state.workspaces.is_empty() {
        let ws = app.active_workspace();
        vars.insert("session_name".to_string(), app.state.session_name.clone());
        vars.insert("window_name".to_string(), ws.name.clone());

        let group_ids = ws.layout.group_ids();
        let pane_count = group_ids.len();
        vars.insert("pane_count".to_string(), pane_count.to_string());

        if let Some(idx) = group_ids.iter().position(|id| *id == ws.active_group) {
            vars.insert("pane_index".to_string(), (idx + 1).to_string());
        }
        vars.insert(
            "window_index".to_string(),
            (app.state.active_workspace + 1).to_string(),
        );
    }

    // System stats (conditionally include based on config)
    if app.state.config.status_bar.show_cpu {
        vars.insert("cpu".to_string(), app.state.system_stats.format_cpu());
    }
    if app.state.config.status_bar.show_memory {
        vars.insert("mem".to_string(), app.state.system_stats.format_memory());
    }
    if app.state.config.status_bar.show_load {
        vars.insert("load".to_string(), app.state.system_stats.format_load());
    }
    if app.state.config.status_bar.show_disk {
        vars.insert("disk".to_string(), app.state.system_stats.format_disk());
    }

    // Build a combined stats string with separators for backward compat
    let mut stat_parts: Vec<String> = Vec::new();
    if app.state.config.status_bar.show_cpu {
        stat_parts.push(app.state.system_stats.format_cpu());
    }
    if app.state.config.status_bar.show_memory {
        stat_parts.push(app.state.system_stats.format_memory());
    }
    if app.state.config.status_bar.show_load {
        stat_parts.push(app.state.system_stats.format_load());
    }
    if app.state.config.status_bar.show_disk {
        stat_parts.push(app.state.system_stats.format_disk());
    }

    vars
}
