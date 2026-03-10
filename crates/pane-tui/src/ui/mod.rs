pub mod context_menu;
pub mod dialog;
pub mod format;
pub mod layout_render;
pub mod palette;
pub mod status_bar;
pub mod tab_picker;
pub mod window_view;
pub mod workspace_bar;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use pane_protocol::app::Mode;
use crate::client::Client;

/// Render the TUI for a connected client (daemon mode).
pub fn render_client(client: &Client, frame: &mut Frame) {
    let theme = &client.config.theme;

    let show_workspace_bar = !client.render_state.workspaces.is_empty();

    let (header, body, footer) = if show_workspace_bar {
        let [h, b, f] = Layout::vertical([
            Constraint::Length(workspace_bar::HEIGHT),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        (Some(h), b, f)
    } else {
        let [b, f] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        (None, b, f)
    };

    // Workspace bar
    if let Some(header) = header {
        let names: Vec<&str> = client
            .render_state
            .workspaces
            .iter()
            .map(|ws| ws.name.as_str())
            .collect();
        workspace_bar::render(
            &names,
            client.render_state.active_workspace,
            theme,
            client.workspace_bar_focused,
            frame,
            header,
        );
    }

    // Status bar
    status_bar::render_client(client, theme, frame, footer);

    // Render workspace body + cursor
    if let Some(ws) = client.active_workspace() {
        let copy_mode_state = if client.mode == Mode::Copy {
            client.copy_mode_state.as_ref()
        } else {
            None
        };
        let ws_bar_focused = client.workspace_bar_focused;

        // Check for zoom mode
        if let Some(zoomed_id) = ws.zoomed_window {
            // Render only the zoomed window filling the body
            if let Some(group) = ws.groups.iter().find(|g| g.id == zoomed_id) {
                let pane = group.tabs.get(group.active_tab);
                let screen = pane.and_then(|p| client.pane_screen(p.id));
                window_view::render_group_from_snapshot(
                    group,
                    screen,
                    !ws_bar_focused,
                    &client.mode,
                    copy_mode_state,
                    &client.config,
                    frame,
                    body,
                );

                // Cursor for zoomed window
                if !ws_bar_focused && (client.mode == Mode::Interact || client.mode == Mode::Normal) {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                let (vt_row, vt_col) = screen.cursor_position();
                                let cursor_x = body.x + 2 + vt_col;
                                let cursor_y = body.y + 3 + vt_row;
                                if cursor_x < body.x + body.width && cursor_y < body.y + body.height
                                {
                                    frame.set_cursor_position(ratatui::layout::Position {
                                        x: cursor_x,
                                        y: cursor_y,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let resolved = ws
                .layout
                .resolve_with_folds(body, &ws.folded_windows);

            // First pass: visible panes
            for rp in &resolved {
                if let pane_protocol::layout::ResolvedPane::Visible { id: group_id, rect } = rp {
                    if let Some(group) = ws.groups.iter().find(|g| g.id == *group_id) {
                        let is_active = *group_id == ws.active_group && !ws_bar_focused;
                        let pane = group.tabs.get(group.active_tab);
                        let screen = pane.and_then(|p| client.pane_screen(p.id));
                        window_view::render_group_from_snapshot(
                            group,
                            screen,
                            is_active,
                            &client.mode,
                            copy_mode_state,
                            &client.config,
                            frame,
                            *rect,
                        );
                    }
                }
            }

            // Second pass: fold bars
            for rp in &resolved {
                if let pane_protocol::layout::ResolvedPane::Folded {
                    id: group_id,
                    rect,
                    direction,
                } = rp
                {
                    if rect.width == 0 || rect.height == 0 {
                        continue;
                    }
                    let is_active = *group_id == ws.active_group && !ws_bar_focused;
                    window_view::render_folded(is_active, *direction, theme, frame, *rect);
                }
            }

            // Cursor position
            if !ws_bar_focused && (client.mode == Mode::Interact || client.mode == Mode::Normal) {
                if let Some(group) = ws.groups.iter().find(|g| g.id == ws.active_group) {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                for rp in &resolved {
                                    if let pane_protocol::layout::ResolvedPane::Visible { id, rect } = rp {
                                        if *id == ws.active_group {
                                            let (vt_row, vt_col) = screen.cursor_position();
                                            let cursor_x = rect.x + 2 + vt_col;
                                            let cursor_y = rect.y + 3 + vt_row;
                                            if cursor_x < rect.x + rect.width
                                                && cursor_y < rect.y + rect.height
                                            {
                                                frame.set_cursor_position(
                                                    ratatui::layout::Position {
                                                        x: cursor_x,
                                                        y: cursor_y,
                                                    },
                                                );
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Render floating windows on top of tiled layout
        for fw in &ws.floating_windows {
            if let Some(group) = ws.groups.iter().find(|g| g.id == fw.id) {
                let is_active = fw.id == ws.active_group && !ws_bar_focused;
                let pane = group.tabs.get(group.active_tab);
                let screen = pane.and_then(|p| client.pane_screen(p.id));
                let fw_rect = ratatui::layout::Rect::new(fw.x, fw.y, fw.width, fw.height);
                use ratatui::widgets::Clear;
                frame.render_widget(Clear, fw_rect);
                window_view::render_group_from_snapshot(
                    group,
                    screen,
                    is_active,
                    &client.mode,
                    copy_mode_state,
                    &client.config,
                    frame,
                    fw_rect,
                );
            }
        }
    }

    // Overlays
    match &client.mode {
        Mode::Palette => {
            if let Some(ref palette_state) = client.palette_state {
                palette::render(palette_state, theme, frame, frame.area());
            }
        }
        Mode::Confirm => {
            render_confirm_dialog(client, theme, frame, frame.area());
        }
        Mode::Leader => {
            if let Some(ref ls) = client.leader_state {
                if ls.popup_visible {
                    if let pane_protocol::config::LeaderNode::Group { ref children, .. } =
                        ls.current_node
                    {
                        // Build display path: "\u{2389}" for root, "\u{2389} w" for leader->w, etc.
                        let mut path_parts = vec!["\u{2389}".to_string()];
                        for k in &ls.path {
                            path_parts.push(palette::key_event_to_string(k));
                        }
                        // If in a subgroup, show the group label
                        let path = if let pane_protocol::config::LeaderNode::Group { ref label, .. } = ls.current_node {
                            if ls.path.is_empty() {
                                "\u{2389}".to_string()
                            } else {
                                format!("{} \u{2192} {}", path_parts.join(" "), label)
                            }
                        } else {
                            path_parts.join(" ")
                        };
                        let compact =
                            palette::UnifiedPaletteState::new_compact_hints(children, path);
                        palette::render(&compact, theme, frame, frame.area());
                    }
                }
            }
        }
        Mode::TabPicker => {
            if let Some(ref tp_state) = client.tab_picker_state {
                // Render inside the active window's rect
                let picker_area = active_window_rect(client, body)
                    .unwrap_or(frame.area());
                tab_picker::render(tp_state, theme, frame, picker_area);
            }
        }
        Mode::ContextMenu => {
            if let Some(ref cm_state) = client.context_menu_state {
                context_menu::render(cm_state, theme, frame, frame.area());
            }
        }
        _ => {}
    }
}

/// Find the active window's rect from the current render state.
fn active_window_rect(client: &Client, body: Rect) -> Option<Rect> {
    let ws = client.active_workspace()?;
    // Check floating windows first
    for fw in &ws.floating_windows {
        if fw.id == ws.active_group {
            return Some(Rect::new(fw.x, fw.y, fw.width, fw.height));
        }
    }
    let resolved = ws.layout.resolve_with_folds(body, &ws.folded_windows);
    for rp in &resolved {
        if let pane_protocol::layout::ResolvedPane::Visible { id, rect } = rp {
            if *id == ws.active_group {
                return Some(*rect);
            }
        }
    }
    None
}

fn render_confirm_dialog(
    client: &Client,
    theme: &pane_protocol::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::Paragraph,
    };

    let message = client
        .confirm_message
        .as_deref()
        .unwrap_or("Are you sure?");

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Percent { width: 40, height: 15 },
        dialog::PopupAnchor::Center,
        area,
    );
    let inner = dialog::render_popup(frame, popup_area, "confirm", theme);

    let lines = vec![
        Line::raw(""),
        Line::styled(format!("  {}", message), Style::default().fg(Color::White)),
        Line::raw(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                " Cancel ",
                Style::default()
                    .fg(Color::White)
                    .bg(theme.dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                " Confirm ",
                Style::default()
                    .fg(Color::White)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfirmDialogClick {
    Cancel,
    Confirm,
}

/// Hit-test the confirm dialog buttons. Returns which button was clicked, if any.
pub fn confirm_dialog_hit_test(
    area: ratatui::layout::Rect,
    x: u16,
    y: u16,
) -> Option<ConfirmDialogClick> {
    use ratatui::widgets::{Block, BorderType, Borders};

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Percent { width: 40, height: 15 },
        dialog::PopupAnchor::Center,
        area,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(popup_area);

    // Buttons are on line index 3 (0-indexed) of the inner area
    let button_y = inner.y + 3;
    if y != button_y {
        return None;
    }

    // Layout: "  " (2) + " Cancel " (8) + "  " (2) + " Confirm " (9)
    let cancel_start = inner.x + 2;
    let cancel_end = cancel_start + 8;
    let confirm_start = cancel_end + 2;
    let confirm_end = confirm_start + 9;

    if x >= cancel_start && x < cancel_end {
        Some(ConfirmDialogClick::Cancel)
    } else if x >= confirm_start && x < confirm_end {
        Some(ConfirmDialogClick::Confirm)
    } else {
        None
    }
}
