use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::app::Mode;
use crate::config::{Config, Theme};
use crate::copy_mode::CopyModeState;
use crate::layout::SplitDirection;
use crate::pane::{terminal::{render_screen, render_screen_copy_mode}, PaneGroup};

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

/// Layout information for tab bar hit testing.
pub struct TabBarLayout {
    /// Absolute area of the tab bar row.
    pub area: Rect,
    /// (start_x, end_x) for each tab.
    pub tab_ranges: Vec<(u16, u16)>,
    /// (start_x, end_x) for the + button, if present.
    pub plus_range: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabBarClick {
    Tab(usize),
    NewTab,
}

fn build_tab_bar<'a>(
    group: &PaneGroup,
    theme: &Theme,
    area: Rect,
) -> (Vec<Span<'a>>, TabBarLayout) {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut tab_ranges: Vec<(u16, u16)> = Vec::new();
    let mut cursor_x = area.x;
    let max_x = area.x + area.width;
    let sep = " \u{B7} "; // " · "
    let sep_width = 3u16; // 3 display columns
    let plus_text = " + ";
    let plus_reserve = 3u16;

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active_tab = i == group.active_tab;
        let label = format!(" {} ", tab.title);
        let label_width = label.len() as u16;

        // Check if this tab fits (reserve space for + button)
        if cursor_x + label_width + plus_reserve > max_x && !is_active_tab {
            tab_ranges.push((0, 0));
            continue;
        }

        // Separator before non-first tabs
        if i > 0 {
            let sep_style = Style::default().fg(theme.dim);
            spans.push(Span::styled(sep, sep_style));
            cursor_x += sep_width;
        }

        let tab_start = cursor_x;

        let style = if is_active_tab {
            Style::default()
                .fg(theme.tab_active)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.tab_inactive)
        };

        spans.push(Span::styled(label, style));
        cursor_x += label_width;

        tab_ranges.push((tab_start, cursor_x));
    }

    // + button
    let plus_range = if cursor_x + sep_width + plus_reserve <= max_x {
        spans.push(Span::styled(sep, Style::default().fg(theme.dim)));
        cursor_x += sep_width;
        let start = cursor_x;
        spans.push(Span::styled(plus_text, Style::default().fg(theme.accent)));
        cursor_x += plus_reserve;
        Some((start, cursor_x))
    } else {
        None
    };

    let layout = TabBarLayout {
        area,
        tab_ranges,
        plus_range,
    };

    (spans, layout)
}

/// Compute tab bar layout for a given group and area (for hit testing from app.rs).
pub fn tab_bar_layout(group: &PaneGroup, theme: &Theme, area: Rect) -> TabBarLayout {
    let (_spans, layout) = build_tab_bar(group, theme, area);
    layout
}

/// Hit-test the tab bar. Returns which tab or + was clicked.
pub fn tab_bar_hit_test(layout: &TabBarLayout, x: u16, y: u16) -> Option<TabBarClick> {
    if y != layout.area.y {
        return None;
    }
    if x < layout.area.x || x >= layout.area.x + layout.area.width {
        return None;
    }

    // Check + button first
    if let Some((start, end)) = layout.plus_range {
        if x >= start && x < end {
            return Some(TabBarClick::NewTab);
        }
    }

    // Check tabs
    for (i, (start, end)) in layout.tab_ranges.iter().enumerate() {
        if *start == 0 && *end == 0 {
            continue;
        }
        if x >= *start && x < *end {
            return Some(TabBarClick::Tab(i));
        }
    }

    None
}


/// Render a pane group from a snapshot (used by the client).
/// Receives the active tab's vt100 screen directly instead of accessing the Pane struct.
pub fn render_group_from_snapshot(
    group: &crate::server::protocol::GroupSnapshot,
    screen: Option<&vt100::Screen>,
    is_active: bool,
    mode: &Mode,
    copy_mode_state: Option<&CopyModeState>,
    config: &Config,
    frame: &mut Frame,
    area: Rect,
) {
    let theme = &config.theme;

    // Check if the active pane's foreground process has a decoration
    let decoration_color = group.tabs.get(group.active_tab)
        .and_then(|snap| snap.foreground_process.as_deref())
        .and_then(|proc| config.decoration_for(proc))
        .map(|d| d.border_color);

    let border_style = if is_active {
        let color = decoration_color.unwrap_or(theme.border_active);
        Style::default().fg(color)
    } else {
        Style::default().fg(theme.border_inactive)
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    if is_active {
        let indicator = match mode {
            Mode::Copy => " COPY ",
            Mode::Scroll => " SCROLL ",
            Mode::Select => " SELECT ",
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

    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height);

    let cms = if is_active { copy_mode_state } else { None };
    let show_search = cms.map_or(false, |c| c.search_active);

    let constraints = if show_search {
        vec![Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]
    } else {
        vec![Constraint::Length(1), Constraint::Fill(1)]
    };
    let areas = Layout::vertical(constraints).split(padded);
    let tab_area = areas[0];
    let content_area = areas[1];

    // Tab bar from snapshot
    render_tab_bar_from_snapshot(group, theme, frame, tab_area);

    // Content
    if let Some(screen) = screen {
        render_content(screen, cms, frame, content_area);
    }

    if show_search {
        render_search_bar(cms.unwrap(), theme, frame, areas[2]);
    }
}

fn render_tab_bar_from_snapshot(
    group: &crate::server::protocol::GroupSnapshot,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut cursor_x = area.x;
    let max_x = area.x + area.width;
    let sep = " \u{B7} ";
    let sep_width = 3u16;
    let plus_text = " + ";
    let plus_reserve = 3u16;

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active_tab = i == group.active_tab;
        let label = format!(" {} ", tab.title);
        let label_width = label.len() as u16;

        if cursor_x + label_width + plus_reserve > max_x && !is_active_tab {
            continue;
        }

        if i > 0 {
            spans.push(Span::styled(sep.to_string(), Style::default().fg(theme.dim)));
            cursor_x += sep_width;
        }

        let style = if is_active_tab {
            Style::default().fg(theme.tab_active).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.tab_inactive)
        };

        spans.push(Span::styled(label, style));
        cursor_x += label_width;
    }

    if cursor_x + sep_width + plus_reserve <= max_x {
        spans.push(Span::styled(sep.to_string(), Style::default().fg(theme.dim)));
        spans.push(Span::styled(plus_text.to_string(), Style::default().fg(theme.accent)));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render a fold indicator line with 1-cell padding on each side.
pub fn render_folded(
    is_active: bool,
    direction: SplitDirection,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let fg = if is_active {
        theme.accent
    } else {
        theme.dim
    };
    let style = Style::default().fg(fg);
    let buf = frame.buffer_mut();

    match direction {
        SplitDirection::Horizontal => {
            // Vertical line, 1 cell wide. Pad top and bottom by 1.
            if area.height <= 2 {
                return;
            }
            let x = area.x;
            let y_start = area.y + 1;
            let y_end = area.y + area.height - 1;
            for y in y_start..y_end {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_symbol("│");
                    cell.set_style(style);
                }
            }
        }
        SplitDirection::Vertical => {
            // Horizontal line, 1 cell tall. Pad left and right by 1.
            if area.width <= 2 {
                return;
            }
            let y = area.y;
            let x_start = area.x + 1;
            let x_end = area.x + area.width - 1;
            for x in x_start..x_end {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_symbol("─");
                    cell.set_style(style);
                }
            }
        }
    }
}

/// Compute the tab bar area for a given pane group within its visible rect.
/// Returns None if the area is too small.
pub fn tab_bar_area(_group: &PaneGroup, area: Rect) -> Option<Rect> {
    // Matches render_group: Block with Borders::ALL → inner, then 1-cell left/right padding,
    // then tab bar is the first row of the padded area.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(area);
    if inner.width <= 2 || inner.height == 0 {
        return None;
    }
    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, 1);
    Some(padded)
}
