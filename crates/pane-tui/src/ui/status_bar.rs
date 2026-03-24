use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::client::{Client, Focus};
use pane_protocol::config::Theme;

/// Return the button definitions for the current client state.
pub fn get_buttons(client: &Client) -> &'static [(&'static str, &'static str)] {
    match &client.focus {
        Focus::WorkspaceBar => &[
            ("h/l", "switch"),
            ("d", "close"),
            ("n", "new"),
            ("j", "exit bar"),
        ],
        Focus::Normal => &[
            ("\u{2423}", "leader"),
            (":", "commands"),
            ("n", "new tab"),
            ("s/v", "split"),
            ("q", "quit"),
        ],
        Focus::Interact => &[
            ("^\u{2423}", "normal"),
            ("\u{2423}", "leader"),
            (":", "commands"),
        ],
        Focus::Scroll => &[
            ("j/k", "up/down"),
            ("u/d", "page"),
            ("g/G", "top/end"),
            ("Esc", "quit"),
        ],
        Focus::Copy => &[
            ("hjkl", "move"),
            ("v", "select"),
            ("y", "yank"),
            ("/", "search"),
            ("Esc", "quit"),
        ],
        Focus::Palette => &[
            ("type", "filter"),
            ("Enter", "run"),
            ("Esc", "cancel"),
        ],
        Focus::Confirm => &[
            ("Enter/y", "confirm"),
            ("Esc/n", "cancel"),
        ],
        Focus::Leader => &[
            ("Esc", "cancel"),
        ],
        Focus::TabPicker => &[
            ("type", "filter"),
            ("Enter", "spawn"),
            ("Esc", "cancel"),
        ],
        Focus::Rename => &[
            ("Enter", "confirm"),
            ("Esc", "cancel"),
        ],
        Focus::NewWorkspace => {
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
        Focus::ContextMenu => &[
            ("j/k", "navigate"),
            ("Enter", "select"),
            ("Esc", "cancel"),
        ],
        Focus::WidgetPicker => &[
            ("j/k", "navigate"),
            ("Enter", "select"),
            ("Esc", "cancel"),
        ],
        Focus::Resize => {
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
    }
}

/// Render the status bar for a daemon-connected client.
/// Uses a unified button-style bar for all modes and workspaces.
pub fn render_client(client: &Client, theme: &Theme, frame: &mut Frame, area: Rect) {
    let buttons = get_buttons(client);
    let hovered = client.hover.and_then(|(hx, hy)| hit_test(buttons, area, hx, hy));
    render_button_bar(buttons, hovered, theme, frame, area);
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

/// Compute the x-range `(start, end)` for each button in the bar.
/// Ranges are absolute (offset by `area.x`).
fn compute_button_ranges(buttons: &[(&str, &str)], area: Rect) -> Vec<(u16, u16)> {
    let mut ranges = Vec::with_capacity(buttons.len());
    let mut offset = 1u16; // initial " " padding
    for (i, (key, label)) in buttons.iter().enumerate() {
        if i > 0 {
            offset += 3; // " · " separator
        }
        let start = offset;
        offset += key.width() as u16 + 2; // " key "
        offset += label.width() as u16 + 1; // " label"
        ranges.push((area.x + start, area.x + offset));
    }
    ranges
}

/// Hit-test the status bar. Returns the index of the clicked button, if any.
pub fn hit_test(buttons: &[(&str, &str)], area: Rect, x: u16, y: u16) -> Option<usize> {
    if y < area.y || y >= area.y + area.height {
        return None;
    }
    let ranges = compute_button_ranges(buttons, area);
    for (i, (start, end)) in ranges.iter().enumerate() {
        if x >= *start && x < *end {
            return Some(i);
        }
    }
    None
}

/// Render a styled button bar with key-label pairs.
fn render_button_bar(
    buttons: &[(&str, &str)],
    hovered: Option<usize>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let key_style = Style::default()
        .fg(theme.status_bar_key_fg())
        .bg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(theme.dim).bg(Color::Reset);
    let sep_style = Style::default().fg(theme.dim).bg(Color::Reset);
    let hovered_key_style = Style::default()
        .fg(theme.status_bar_key_fg())
        .bg(theme.fg)
        .add_modifier(Modifier::BOLD);
    let hovered_label_style = Style::default().fg(theme.fg).bg(Color::Reset);

    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (i, (key, label)) in buttons.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" \u{b7} ", sep_style));
        }
        let is_hovered = hovered == Some(i);
        let ks = if is_hovered { hovered_key_style } else { key_style };
        let ls = if is_hovered { hovered_label_style } else { label_style };
        spans.push(Span::styled(format!(" {} ", key), ks));
        spans.push(Span::styled(format!(" {}", label), ls));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn area(w: u16) -> Rect {
        Rect::new(0, 24, w, 1)
    }

    #[test]
    fn hit_test_returns_correct_button() {
        // Normal mode buttons with ⎵ (display width 1):
        // " " (1) | " ⎵ " (3) " leader" (7) = [1,11)
        // " · " (3) | " : " (3) " commands" (9) = [14,26)
        // " · " (3) | " n " (3) " new tab" (8) = [29,40)
        // " · " (3) | " s/v " (5) " split" (6) = [43,54)
        // " · " (3) | " q " (3) " quit" (5) = [57,65)
        let buttons: &[(&str, &str)] = &[
            ("\u{2423}", "leader"),
            (":", "commands"),
            ("n", "new tab"),
            ("s/v", "split"),
            ("q", "quit"),
        ];
        let a = area(80);

        // Verify ranges to understand layout
        let ranges = compute_button_ranges(buttons, a);
        assert_eq!(ranges[0], (1, 11));  // ⎵ leader
        assert_eq!(ranges[1], (14, 26)); // : commands

        // Click on first button key area
        assert_eq!(hit_test(buttons, a, 1, 24), Some(0));
        assert_eq!(hit_test(buttons, a, 3, 24), Some(0));
        // Click on first button label
        assert_eq!(hit_test(buttons, a, 5, 24), Some(0));

        // Click on "commands" button
        assert_eq!(hit_test(buttons, a, 14, 24), Some(1));
        assert_eq!(hit_test(buttons, a, 25, 24), Some(1));

        // Click on "new tab" button
        assert_eq!(hit_test(buttons, a, 29, 24), Some(2));

        // Click on "quit" button (last)
        assert_eq!(hit_test(buttons, a, 57, 24), Some(4));

        // Click in separator area → None
        assert_eq!(hit_test(buttons, a, 11, 24), None);
        assert_eq!(hit_test(buttons, a, 12, 24), None);
        assert_eq!(hit_test(buttons, a, 13, 24), None);

        // Click past all buttons → None
        assert_eq!(hit_test(buttons, a, 65, 24), None);

        // Click on padding → None
        assert_eq!(hit_test(buttons, a, 0, 24), None);
    }

    #[test]
    fn hit_test_wrong_row() {
        let buttons: &[(&str, &str)] = &[("q", "quit")];
        let a = area(80);
        // Wrong y
        assert_eq!(hit_test(buttons, a, 1, 23), None);
        assert_eq!(hit_test(buttons, a, 1, 25), None);
    }

    #[test]
    fn compute_ranges_single_button() {
        let buttons: &[(&str, &str)] = &[("Esc", "cancel")];
        let a = Rect::new(0, 0, 80, 1);
        let ranges = compute_button_ranges(buttons, a);
        // " " (1) + " Esc " (5) + " cancel" (7) = range [1, 13)
        assert_eq!(ranges, vec![(1, 13)]);
    }

    #[test]
    fn compute_ranges_two_buttons() {
        let buttons: &[(&str, &str)] = &[("Enter", "confirm"), ("Esc", "cancel")];
        let a = Rect::new(0, 0, 80, 1);
        let ranges = compute_button_ranges(buttons, a);
        // Button 0: offset=1, " Enter " (7) + " confirm" (8) → [1, 16)
        // Button 1: offset=16+3=19, " Esc " (5) + " cancel" (7) → [19, 31)
        assert_eq!(ranges, vec![(1, 16), (19, 31)]);
    }

    #[test]
    fn hit_test_area_offset() {
        // Status bar at x=5 (e.g. in a split)
        let buttons: &[(&str, &str)] = &[("q", "quit")];
        let a = Rect::new(5, 10, 80, 1);
        // Button range: [5+1, 5+9) = [6, 14)
        assert_eq!(hit_test(buttons, a, 5, 10), None); // padding
        assert_eq!(hit_test(buttons, a, 6, 10), Some(0));
        assert_eq!(hit_test(buttons, a, 13, 10), Some(0));
        assert_eq!(hit_test(buttons, a, 14, 10), None);
    }
}
