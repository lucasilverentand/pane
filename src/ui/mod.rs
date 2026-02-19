pub mod command_palette;
pub mod format;
pub mod help;
pub mod layout_render;
pub mod status_bar;
pub mod tab_picker;
pub mod which_key;
pub mod window_view;
pub mod workspace_bar;

use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;

use crate::app::Mode;
use crate::client::Client;
use crate::layout::LayoutParams;

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
            frame,
            header,
        );
    }

    // Status bar
    status_bar::render_client(client, theme, frame, footer);

    // Render workspace body + cursor
    if let Some(ws) = client.active_workspace() {
        let params = LayoutParams::from(&client.config.behavior);
        let copy_mode_state = if client.mode == Mode::Copy {
            client.copy_mode_state.as_ref()
        } else {
            None
        };

        // Check for zoom mode
        if let Some(zoomed_id) = ws.zoomed_window {
            // Render only the zoomed window filling the body
            if let Some(group) = ws.groups.iter().find(|g| g.id == zoomed_id) {
                let pane = group.tabs.get(group.active_tab);
                let screen = pane.and_then(|p| client.pane_screen(p.id));
                window_view::render_group_from_snapshot(
                    group,
                    screen,
                    true,
                    &client.mode,
                    copy_mode_state,
                    &client.config,
                    frame,
                    body,
                );

                // Cursor for zoomed window
                if client.mode == Mode::Interact || client.mode == Mode::Normal {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                let (vt_row, vt_col) = screen.cursor_position();
                                let tab_bar_offset: u16 = if group.tabs.len() > 1 { 1 } else { 0 };
                                let cursor_x = body.x + 2 + vt_col;
                                let cursor_y = body.y + 1 + tab_bar_offset + vt_row;
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
                .resolve_with_fold(body, params, &ws.leaf_min_sizes);

            // First pass: visible panes
            for rp in &resolved {
                if let crate::layout::ResolvedPane::Visible { id: group_id, rect } = rp {
                    if let Some(group) = ws.groups.iter().find(|g| g.id == *group_id) {
                        let is_active = *group_id == ws.active_group;
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
                if let crate::layout::ResolvedPane::Folded {
                    id: group_id,
                    rect,
                    direction,
                } = rp
                {
                    if rect.width == 0 || rect.height == 0 {
                        continue;
                    }
                    let is_active = *group_id == ws.active_group;
                    window_view::render_folded(is_active, *direction, theme, frame, *rect);
                }
            }

            // Cursor position
            if client.mode == Mode::Interact || client.mode == Mode::Normal {
                if let Some(group) = ws.groups.iter().find(|g| g.id == ws.active_group) {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                for rp in &resolved {
                                    if let crate::layout::ResolvedPane::Visible { id, rect } = rp {
                                        if *id == ws.active_group {
                                            let (vt_row, vt_col) = screen.cursor_position();
                                            let tab_bar_offset: u16 =
                                                if group.tabs.len() > 1 { 1 } else { 0 };
                                            let cursor_x = rect.x + 2 + vt_col;
                                            let cursor_y = rect.y + 1 + tab_bar_offset + vt_row;
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
                let is_active = fw.id == ws.active_group;
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
        Mode::Help => {
            help::render(
                &client.config.keys,
                &client.help_state,
                theme,
                frame,
                frame.area(),
            );
        }
        Mode::CommandPalette => {
            if let Some(ref cp_state) = client.command_palette_state {
                command_palette::render(cp_state, theme, frame, frame.area());
            }
        }
        Mode::Confirm => {
            render_confirm_dialog(client, theme, frame, frame.area());
        }
        Mode::Leader => {
            if let Some(ref ls) = client.leader_state {
                which_key::render(ls, theme, frame, frame.area());
            }
        }
        Mode::TabPicker => {
            if let Some(ref tp_state) = client.tab_picker_state {
                tab_picker::render(tp_state, theme, frame, frame.area());
            }
        }
        _ => {}
    }
}

fn render_confirm_dialog(
    _client: &Client,
    theme: &crate::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Clear, Paragraph},
    };

    let message = "Close tab with running process?";

    let popup_area = centered_rect(40, 15, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" confirm ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

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

    let popup_area = centered_rect(40, 15, area);
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

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    area: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
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
    use ratatui::layout::Rect;

    #[test]
    fn test_centered_rect_is_within_area() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_rect(50, 50, area);
        assert!(result.x >= area.x);
        assert!(result.y >= area.y);
        assert!(result.x + result.width <= area.x + area.width);
        assert!(result.y + result.height <= area.y + area.height);
    }

    #[test]
    fn test_centered_rect_is_roughly_centered() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered_rect(50, 50, area);
        let result_center_x = result.x + result.width / 2;
        let result_center_y = result.y + result.height / 2;
        let area_center_x = area.width / 2;
        let area_center_y = area.height / 2;
        assert!((result_center_x as i16 - area_center_x as i16).unsigned_abs() <= 2);
        assert!((result_center_y as i16 - area_center_y as i16).unsigned_abs() <= 2);
    }

    #[test]
    fn test_centered_rect_respects_percentages() {
        let area = Rect::new(0, 0, 200, 100);
        let result = centered_rect(50, 60, area);
        assert!(result.width >= 95 && result.width <= 105);
        assert!(result.height >= 55 && result.height <= 65);
    }

    #[test]
    fn test_centered_rect_with_offset_area() {
        let area = Rect::new(10, 5, 100, 50);
        let result = centered_rect(50, 50, area);
        assert!(result.x >= area.x);
        assert!(result.y >= area.y);
        assert!(result.x + result.width <= area.x + area.width);
        assert!(result.y + result.height <= area.y + area.height);
    }
}
