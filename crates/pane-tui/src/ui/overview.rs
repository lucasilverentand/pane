use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use pane_protocol::config::Theme;

use crate::client::Client;

/// Compute grid dimensions (cols, rows) for `count` tiles.
pub fn compute_grid(count: usize) -> (usize, usize) {
    match count {
        0 => (1, 1),
        1 => (1, 1),
        2 => (2, 1),
        3..=4 => (2, 2),
        5..=6 => (3, 2),
        7..=9 => (3, 3),
        _ => {
            let cols = (count as f64).sqrt().ceil() as usize;
            let rows = (count + cols - 1) / cols;
            (cols, rows)
        }
    }
}

/// Navigate the grid in a direction, returning the new selected index.
pub fn grid_navigate(
    selected: usize,
    count: usize,
    cols: usize,
    dx: isize,
    dy: isize,
) -> usize {
    if count == 0 {
        return 0;
    }
    let rows = (count + cols - 1) / cols;
    let col = (selected % cols) as isize;
    let row = (selected / cols) as isize;

    let new_col = (col + dx).rem_euclid(cols as isize) as usize;
    let new_row = (row + dy).rem_euclid(rows as isize) as usize;
    let new_idx = new_row * cols + new_col;

    if new_idx < count {
        new_idx
    } else {
        selected
    }
}

/// Hit-test which tile contains the point (x, y).
pub fn hit_test_tile(count: usize, body: Rect, x: u16, y: u16) -> Option<usize> {
    if count == 0 || !body.intersects(Rect::new(x, y, 1, 1)) {
        return None;
    }
    let (cols, rows) = compute_grid(count);
    let col_width = body.width / cols as u16;
    let row_height = body.height / rows as u16;

    if col_width == 0 || row_height == 0 {
        return None;
    }

    let col = ((x - body.x) / col_width) as usize;
    let row = ((y - body.y) / row_height) as usize;

    let col = col.min(cols - 1);
    let row = row.min(rows - 1);
    let idx = row * cols + col;

    if idx < count {
        Some(idx)
    } else {
        None
    }
}

/// Render the overview grid showing one tile per workspace.
pub fn render_overview(client: &Client, frame: &mut Frame, body: Rect) {
    let theme = &client.config.theme;
    let workspaces = &client.render_state.workspaces;
    let count = workspaces.len();

    if count == 0 {
        render_empty(theme, frame, body);
        return;
    }

    let (cols, rows) = compute_grid(count);
    let row_rects = Layout::vertical(
        (0..rows).map(|_| Constraint::Ratio(1, rows as u32)),
    )
    .split(body);

    for (i, ws) in workspaces.iter().enumerate() {
        let row = i / cols;
        let col = i % cols;

        let col_rects = Layout::horizontal(
            (0..cols).map(|_| Constraint::Ratio(1, cols as u32)),
        )
        .split(row_rects[row]);

        let tile_rect = col_rects[col];
        if tile_rect.width < 3 || tile_rect.height < 3 {
            continue;
        }

        let is_selected = i == client.overview_selected;
        render_tile(client, ws, is_selected, theme, frame, tile_rect);
    }
}

fn render_tile(
    client: &Client,
    ws: &pane_protocol::protocol::WorkspaceSnapshot,
    is_selected: bool,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let border_color = if is_selected {
        theme.accent
    } else {
        theme.border_inactive
    };

    let title_style = if is_selected {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(ratatui::text::Span::styled(&ws.name, title_style));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Find the active pane's screen and render its content
    let group = ws.groups.iter().find(|g| g.id == ws.active_group);
    if let Some(group) = group {
        if let Some(pane) = group.tabs.get(group.active_tab) {
            if let Some(screen) = client.pane_screen(pane.id) {
                super::window_view::render_content(screen, None, frame, inner);
            }
        }
    }
}

fn render_empty(theme: &Theme, frame: &mut Frame, body: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_inactive))
        .title(ratatui::text::Span::styled(
            "No workspaces",
            Style::default().fg(theme.dim),
        ));
    let inner = block.inner(body);
    frame.render_widget(block, body);

    let msg = Paragraph::new("Press 'n' to create a workspace")
        .style(Style::default().fg(theme.dim));
    frame.render_widget(msg, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dimensions() {
        assert_eq!(compute_grid(0), (1, 1));
        assert_eq!(compute_grid(1), (1, 1));
        assert_eq!(compute_grid(2), (2, 1));
        assert_eq!(compute_grid(3), (2, 2));
        assert_eq!(compute_grid(4), (2, 2));
        assert_eq!(compute_grid(5), (3, 2));
        assert_eq!(compute_grid(6), (3, 2));
        assert_eq!(compute_grid(7), (3, 3));
        assert_eq!(compute_grid(9), (3, 3));
        assert_eq!(compute_grid(10), (4, 3));
    }

    #[test]
    fn navigate_wraps_horizontally() {
        // 2x1 grid: [0, 1]
        assert_eq!(grid_navigate(0, 2, 2, 1, 0), 1);
        assert_eq!(grid_navigate(1, 2, 2, -1, 0), 0);
    }

    #[test]
    fn navigate_wraps_vertically() {
        // 2x2 grid: [0,1] [2,3]
        assert_eq!(grid_navigate(0, 4, 2, 0, 1), 2);
        assert_eq!(grid_navigate(2, 4, 2, 0, -1), 0);
    }

    #[test]
    fn navigate_stays_on_invalid() {
        // 3x2 grid with 5 items: [0,1,2] [3,4,_]
        // Moving right from 4 would land on 5 which doesn't exist
        assert_eq!(grid_navigate(4, 5, 3, 1, 0), 4);
    }

    #[test]
    fn navigate_empty() {
        assert_eq!(grid_navigate(0, 0, 1, 1, 0), 0);
    }

    #[test]
    fn hit_test_basic() {
        let body = Rect::new(0, 0, 120, 36);
        // 2 workspaces → 2x1 grid, each 60 wide
        assert_eq!(hit_test_tile(2, body, 10, 10), Some(0));
        assert_eq!(hit_test_tile(2, body, 70, 10), Some(1));
    }

    #[test]
    fn hit_test_out_of_bounds() {
        let body = Rect::new(0, 0, 120, 36);
        assert_eq!(hit_test_tile(2, body, 200, 10), None);
    }

    #[test]
    fn hit_test_empty() {
        let body = Rect::new(0, 0, 120, 36);
        assert_eq!(hit_test_tile(0, body, 10, 10), None);
    }
}
