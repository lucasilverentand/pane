pub mod command_palette;
pub mod format;
pub mod help;
pub mod layout_render;
pub mod pane_view;
pub mod session_picker;
pub mod status_bar;
pub mod workspace_bar;

use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;

use crate::app::{App, Mode};
use crate::layout::LayoutParams;

pub fn render(app: &App, frame: &mut Frame) {
    let theme = &app.state.config.theme;

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    {
        let names: Vec<String> = app.state.workspaces.iter().map(|ws| ws.name.clone()).collect();
        workspace_bar::render(
            &names,
            app.state.active_workspace,
            theme,
            frame,
            header,
        );
    }

    if !app.state.workspaces.is_empty() && app.mode != Mode::SessionPicker {
        layout_render::render_workspace(app, frame, body);
    }
    status_bar::render(app, theme, frame, footer);

    // Set cursor position from active pane's vt100 screen (not in scroll mode).
    // Only show the host cursor when the child process hasn't hidden it (DECTCEM).
    // Fullscreen apps like neovim/claude-code manage their own cursor via escape
    // sequences; showing the host cursor on top causes a visible ghost cursor.
    if app.mode == Mode::Normal && !app.state.workspaces.is_empty() {
        let ws = app.active_workspace();
        if let Some(group) = ws.groups.get(&ws.active_group) {
            let pane = group.active_pane();
            if !pane.screen().hide_cursor() {
                let params = LayoutParams::from(&app.state.config.behavior);
                let resolved = ws.layout.resolve_with_fold(body, params, &ws.leaf_min_sizes);
                for rp in &resolved {
                    if let crate::layout::ResolvedPane::Visible { id, rect } = rp {
                        if *id == ws.active_group {
                            let (vt_row, vt_col) = pane.screen().cursor_position();
                            let tab_bar_offset: u16 = 1;
                            let cursor_x = rect.x + 2 + vt_col;
                            let cursor_y = rect.y + 1 + tab_bar_offset + vt_row;
                            if cursor_x < rect.x + rect.width
                                && cursor_y < rect.y + rect.height
                            {
                                frame.set_cursor_position(ratatui::layout::Position {
                                    x: cursor_x,
                                    y: cursor_y,
                                });
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    // Render overlays on top
    match &app.mode {
        Mode::SessionPicker => {
            session_picker::render(
                &app.session_list,
                app.session_selected,
                theme,
                frame,
                frame.area(),
            );
        }
        Mode::DevServerInput => {
            render_dev_server_input(app, theme, frame, frame.area());
        }
        Mode::Help => {
            help::render(
                &app.state.config.keys,
                &app.help_state,
                theme,
                frame,
                frame.area(),
            );
        }
        Mode::CommandPalette => {
            if let Some(ref cp_state) = app.command_palette_state {
                command_palette::render(cp_state, theme, frame, frame.area());
            }
        }
        Mode::Confirm => {
            render_confirm_dialog(app, theme, frame, frame.area());
        }
        Mode::Normal | Mode::Select | Mode::Scroll | Mode::Copy => {}
    }
}

fn render_dev_server_input(
    app: &App,
    theme: &crate::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Clear, Paragraph},
    };

    let popup_area = centered_rect(50, 15, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" enter command ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let lines = vec![
        Line::raw(""),
        Line::styled(
            format!("  > {}_", app.dev_server_input),
            Style::default().fg(Color::White),
        ),
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
pub enum DevServerDialogClick {
    Cancel,
    Confirm,
}

/// Hit-test the dev server input dialog buttons.
pub fn dev_server_dialog_hit_test(
    area: ratatui::layout::Rect,
    x: u16,
    y: u16,
) -> Option<DevServerDialogClick> {
    use ratatui::widgets::{Block, BorderType, Borders};

    let popup_area = centered_rect(50, 15, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(popup_area);

    // Buttons are on line index 3 (0-indexed) of the inner area
    let button_y = inner.y + 3;
    if y != button_y {
        return None;
    }

    let cancel_start = inner.x + 2;
    let cancel_end = cancel_start + 8;
    let confirm_start = cancel_end + 2;
    let confirm_end = confirm_start + 9;

    if x >= cancel_start && x < cancel_end {
        Some(DevServerDialogClick::Cancel)
    } else if x >= confirm_start && x < confirm_end {
        Some(DevServerDialogClick::Confirm)
    } else {
        None
    }
}

fn render_confirm_dialog(
    app: &App,
    theme: &crate::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Clear, Paragraph},
    };

    let message = match &app.pending_close {
        Some(crate::app::PendingClose::Workspace { .. }) => {
            "Close workspace with running processes?"
        }
        _ => "Close tab with running process?",
    };

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
        Line::styled(
            format!("  {}", message),
            Style::default().fg(Color::White),
        ),
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
