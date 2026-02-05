use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::app::Mode;
use crate::config::Theme;
use crate::layout::SplitDirection;
use crate::pane::{terminal::render_screen, PaneGroup};

pub fn render_group(
    group: &PaneGroup,
    is_active: bool,
    mode: &Mode,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let pane = group.active_pane();
    let title = if pane.is_scrolled() {
        format!("  {} [+{}] ", pane.title, pane.scroll_offset)
    } else {
        format!("  {} ", pane.title)
    };

    let border_style = if is_active {
        Style::default().fg(theme.border_active)
    } else {
        Style::default().fg(theme.border_inactive)
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title);

    if is_active {
        let indicator = match mode {
            Mode::Scroll => " SCROLL ",
            _ => " ACTIVE ",
        };
        block = block.title_bottom(Line::styled(
            indicator,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width <= 2 || inner.height == 0 {
        return;
    }

    // 1-cell padding on left and right
    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height);

    let has_tab_bar = group.tab_count() > 1;

    if has_tab_bar {
        let [tab_area, content_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(padded);

        render_tab_bar(group, theme, frame, tab_area);

        let lines: Vec<Line<'static>> = render_screen(pane.screen(), content_area);
        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, content_area);
    } else {
        let lines: Vec<Line<'static>> = render_screen(pane.screen(), padded);
        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, padded);
    }
}

fn render_tab_bar(group: &PaneGroup, theme: &Theme, frame: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active_tab = i == group.active_tab;
        let label = format!("[{}]", tab.title);

        let style = if is_active_tab {
            Style::default()
                .fg(theme.tab_active)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.tab_inactive)
        };

        let prefix = if is_active_tab { "*" } else { " " };
        spans.push(Span::styled(format!("{}{} ", prefix, label), style));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render a folded pane group as a thin line.
pub fn render_folded(
    _group: &PaneGroup,
    is_active: bool,
    direction: SplitDirection,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let fg = if is_active {
        theme.accent
    } else {
        theme.border_inactive
    };
    let style = Style::default().fg(fg);
    let buf = frame.buffer_mut();

    match direction {
        SplitDirection::Horizontal => {
            // Vertical thin line (1 cell wide)
            let x = area.x;
            for y in area.y..area.y + area.height {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_symbol("│");
                    cell.set_style(style);
                }
            }
        }
        SplitDirection::Vertical => {
            // Horizontal thin line (1 cell tall)
            let y = area.y;
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_symbol("─");
                    cell.set_style(style);
                }
            }
        }
    }
}
