pub mod help;
pub mod layout_render;
pub mod new_pane_menu;
pub mod pane_view;
pub mod session_picker;
pub mod status_bar;

use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;

use crate::app::{App, Mode};

pub fn render(app: &App, frame: &mut Frame) {
    let [body, footer] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    layout_render::render_workspace(app, frame, body);
    status_bar::render(app, frame, footer);

    // Set cursor position from active pane's vt100 screen
    if app.mode == Mode::Normal {
        let ws = app.active_workspace();
        if let Some(group) = ws.groups.get(&ws.active_group) {
            let pane = group.active_pane();
            let resolved = ws.layout.resolve(body);
            if let Some((_, rect)) = resolved.iter().find(|(id, _)| *id == ws.active_group) {
                let (vt_row, vt_col) = pane.screen().cursor_position();
                // Offset by rect position + 1 for border
                let tab_bar_offset: u16 = if group.tab_count() > 1 { 1 } else { 0 };
                let cursor_x = rect.x + 1 + vt_col;
                let cursor_y = rect.y + 1 + tab_bar_offset + vt_row;
                if cursor_x < rect.x + rect.width && cursor_y < rect.y + rect.height {
                    frame.set_cursor_position(ratatui::layout::Position {
                        x: cursor_x,
                        y: cursor_y,
                    });
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
                frame,
                frame.area(),
            );
        }
        Mode::NewPane | Mode::DevServerInput => {
            new_pane_menu::render(frame, frame.area());
            if let Mode::DevServerInput = &app.mode {
                render_dev_server_input(app, frame, frame.area());
            }
        }
        Mode::Help => {
            help::render(frame, frame.area());
        }
        Mode::Normal => {}
    }
}

fn render_dev_server_input(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::{
        style::{Color, Style},
        text::Line,
        widgets::{Block, BorderType, Borders, Clear, Paragraph},
    };

    let popup_area = centered_rect(50, 15, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" enter command ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let lines = vec![
        Line::raw(""),
        Line::styled(
            format!("  > {}_", app.dev_server_input),
            Style::default().fg(Color::White),
        ),
        Line::raw(""),
        Line::styled(
            "  enter to confirm, esc to cancel",
            Style::default().fg(Color::DarkGray),
        ),
    ];
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
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
