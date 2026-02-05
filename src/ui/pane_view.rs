use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::app::Mode;
use crate::config::Theme;
use crate::copy_mode::CopyModeState;
use crate::layout::SplitDirection;
use crate::pane::{terminal::{render_screen, render_screen_copy_mode}, PaneGroup};

pub fn render_group(
    group: &PaneGroup,
    is_active: bool,
    mode: &Mode,
    copy_mode_state: Option<&CopyModeState>,
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
            Mode::Copy => " COPY ",
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

    // Only apply copy mode rendering to the active pane
    let cms = if is_active { copy_mode_state } else { None };

    // Reserve a row for search bar if search is active
    let show_search = cms.map_or(false, |c| c.search_active);

    let has_tab_bar = group.tab_count() > 1;

    if has_tab_bar {
        let constraints = if show_search {
            vec![
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
            ]
        } else {
            vec![Constraint::Length(1), Constraint::Fill(1)]
        };
        let areas = Layout::vertical(constraints).split(padded);
        let tab_area = areas[0];
        let content_area = areas[1];

        render_tab_bar(group, theme, frame, tab_area);
        render_content(pane.screen(), cms, frame, content_area);

        if show_search {
            render_search_bar(cms.unwrap(), theme, frame, areas[2]);
        }
    } else {
        let constraints = if show_search {
            vec![Constraint::Fill(1), Constraint::Length(1)]
        } else {
            vec![Constraint::Fill(1)]
        };
        let areas = Layout::vertical(constraints).split(padded);
        let content_area = areas[0];

        render_content(pane.screen(), cms, frame, content_area);

        if show_search {
            render_search_bar(cms.unwrap(), theme, frame, areas[1]);
        }
    }
}

fn render_content(
    screen: &vt100::Screen,
    cms: Option<&CopyModeState>,
    frame: &mut Frame,
    area: Rect,
) {
    let lines: Vec<Line<'static>> = match cms {
        Some(cms) => render_screen_copy_mode(screen, area, cms),
        None => render_screen(screen, area),
    };
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn render_search_bar(cms: &CopyModeState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("/", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("{}_", cms.search_query),
            Style::default().fg(Color::White),
        ),
    ]);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
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
