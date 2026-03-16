use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use pane_protocol::app::Mode;
use crate::client::Client;
use pane_protocol::config::Theme;

/// Render the status bar for a daemon-connected client.
/// Uses a unified button-style bar for all modes and workspaces.
pub fn render_client(client: &Client, theme: &Theme, frame: &mut Frame, area: Rect) {
    let is_home = client.is_home_active();

    let buttons: &[(&str, &str)] = match &client.mode {
        Mode::Normal if client.workspace_bar_focused => &[
            ("h/l", "switch"),
            ("d", "close"),
            ("n", "new"),
            ("j", "exit bar"),
        ],
        Mode::Normal if is_home => &[
            ("/", "search"),
            ("Enter", "open"),
            ("n", "new"),
            (":", "commands"),
            ("q", "quit"),
        ],
        Mode::Normal => &[
            ("\u{2423}", "leader"),
            (":", "commands"),
            ("t", "new tab"),
            ("-", "split"),
            ("n", "new ws"),
            ("q", "quit"),
        ],
        Mode::Interact => &[
            ("^\u{2423}", "normal"),
            ("\u{2423}", "leader"),
            (":", "commands"),
        ],
        Mode::Scroll => &[
            ("j/k", "up/down"),
            ("u/d", "page"),
            ("g/G", "top/end"),
            ("Esc", "quit"),
        ],
        Mode::Copy => &[
            ("hjkl", "move"),
            ("v", "select"),
            ("y", "yank"),
            ("/", "search"),
            ("Esc", "quit"),
        ],
        Mode::Palette => &[
            ("type", "filter"),
            ("Enter", "run"),
            ("Esc", "cancel"),
        ],
        Mode::Confirm => &[
            ("Enter/y", "confirm"),
            ("Esc/n", "cancel"),
        ],
        Mode::Leader => &[
            ("Esc", "cancel"),
        ],
        Mode::TabPicker => &[
            ("type", "filter"),
            ("Enter", "spawn"),
            ("Esc", "cancel"),
        ],
        Mode::Rename => &[
            ("Enter", "confirm"),
            ("Esc", "cancel"),
        ],
        Mode::NewWorkspaceInput => {
            if let Some(ref nw) = client.new_workspace_input {
                match nw.stage {
                    crate::client::NewWorkspaceStage::Directory => &[
                        ("Enter", "select"),
                        ("Tab", "complete"),
                        ("\u{2192}", "open"),
                        ("Esc", "cancel"),
                    ],
                    crate::client::NewWorkspaceStage::Name => &[
                        ("Enter", "create"),
                        ("Esc", "back"),
                    ],
                }
            } else {
                &[("Esc", "cancel")]
            }
        }
        Mode::ContextMenu => &[
            ("j/k", "navigate"),
            ("Enter", "select"),
            ("Esc", "cancel"),
        ],
        Mode::ProjectHub => &[
            ("type", "search"),
            ("Enter", "open"),
            ("Esc", "cancel"),
        ],
        Mode::Resize => {
            let selected = client.resize_state.as_ref().and_then(|rs| rs.selected);
            match selected {
                None => &[
                    ("hjkl", "select border"),
                    ("=", "equalize"),
                    ("Esc", "quit"),
                ],
                Some(b) => match b {
                    pane_protocol::app::ResizeBorder::Left
                    | pane_protocol::app::ResizeBorder::Right => &[
                        ("h/l", "move"),
                        ("Esc", "quit"),
                    ],
                    pane_protocol::app::ResizeBorder::Top
                    | pane_protocol::app::ResizeBorder::Bottom => &[
                        ("j/k", "move"),
                        ("Esc", "quit"),
                    ],
                },
            }
        }
    };

    render_button_bar(buttons, theme, frame, area);
}

#[allow(dead_code)]
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

/// Render a styled button bar with key-label pairs.
fn render_button_bar(buttons: &[(&str, &str)], theme: &Theme, frame: &mut Frame, area: Rect) {
    let key_style = Style::default()
        .fg(Color::Black)
        .bg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(theme.dim);
    let sep_style = Style::default().fg(theme.dim);

    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (i, (key, label)) in buttons.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" \u{b7} ", sep_style));
        }
        spans.push(Span::styled(format!(" {} ", key), key_style));
        spans.push(Span::styled(format!(" {}", label), label_style));
    }

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

#[allow(dead_code)]
fn build_client_vars(client: &Client) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();

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
