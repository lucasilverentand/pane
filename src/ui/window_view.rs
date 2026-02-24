use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{BaseMode, Mode, Overlay};
use crate::config::{Config, Theme};
use crate::copy_mode::CopyModeState;
use crate::layout::SplitDirection;
use crate::window::{
    terminal::{render_screen, render_screen_copy_mode},
    Window,
};

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
        Span::styled(
            "/",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
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

fn display_width_u16(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

fn split_content_and_tab_areas(padded: Rect, show_tab_bar: bool) -> (Option<Rect>, Rect) {
    let constraints = if show_tab_bar {
        vec![Constraint::Length(1), Constraint::Fill(1)]
    } else {
        vec![Constraint::Fill(1)]
    };
    let areas = Layout::vertical(constraints).split(padded);
    if show_tab_bar {
        (Some(areas[0]), areas[1])
    } else {
        (None, areas[0])
    }
}

fn search_overlay_area(content_area: Rect, show_search: bool) -> Option<Rect> {
    if !show_search || content_area.height == 0 {
        return None;
    }
    Some(Rect::new(
        content_area.x,
        content_area.y + content_area.height - 1,
        content_area.width,
        1,
    ))
}

fn build_tab_bar<'a>(group: &Window, theme: &Theme, area: Rect) -> (Vec<Span<'a>>, TabBarLayout) {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut tab_ranges: Vec<(u16, u16)> = Vec::new();
    let mut cursor_x = area.x;
    let max_x = area.x.saturating_add(area.width);
    let sep = " \u{B7} "; // " · "
    let sep_width = display_width_u16(sep);
    let plus_text = " + ";
    let plus_reserve = display_width_u16(plus_text);
    let mut has_visible_tabs = false;

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active_tab = i == group.active_tab;
        let label = format!(" {} ", tab.title);
        let label_width = display_width_u16(&label);
        let sep_before = if has_visible_tabs { sep_width } else { 0 };
        let needed = sep_before
            .saturating_add(label_width)
            .saturating_add(plus_reserve);

        // Check if this tab fits (reserve space for + button)
        if cursor_x.saturating_add(needed) > max_x && !is_active_tab {
            tab_ranges.push((0, 0));
            continue;
        }

        // Separator before non-first tabs
        if has_visible_tabs {
            let sep_style = Style::default().fg(theme.dim);
            spans.push(Span::styled(sep, sep_style));
            cursor_x = cursor_x.saturating_add(sep_width);
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
        cursor_x = cursor_x.saturating_add(label_width);

        tab_ranges.push((tab_start, cursor_x));
        has_visible_tabs = true;
    }

    // + button
    let plus_sep = if has_visible_tabs { sep_width } else { 0 };
    let plus_range = if cursor_x
        .saturating_add(plus_sep)
        .saturating_add(plus_reserve)
        <= max_x
    {
        if has_visible_tabs {
            spans.push(Span::styled(sep, Style::default().fg(theme.dim)));
            cursor_x = cursor_x.saturating_add(sep_width);
        }
        let start = cursor_x;
        spans.push(Span::styled(plus_text, Style::default().fg(theme.accent)));
        cursor_x = cursor_x.saturating_add(plus_reserve);
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
pub fn tab_bar_layout(group: &Window, theme: &Theme, area: Rect) -> TabBarLayout {
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
    group: &crate::server::protocol::WindowSnapshot,
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
    let decoration_color = group
        .tabs
        .get(group.active_tab)
        .and_then(|snap| snap.foreground_process.as_deref())
        .and_then(|proc| config.decoration_for(proc))
        .map(|d| d.border_color);

    let border_style = if is_active {
        let mode_color = if let Some(ref overlay) = mode.overlay {
            match overlay {
                Overlay::Scroll | Overlay::Copy => theme.border_scroll,
                _ => theme.border_active,
            }
        } else {
            match mode.base {
                BaseMode::Normal => theme.border_normal,
                BaseMode::Interact => theme.border_interact,
            }
        };
        let color = decoration_color.unwrap_or(mode_color);
        Style::default().fg(color)
    } else {
        Style::default().fg(theme.border_inactive)
    };

    // Build top title with tab info
    let tab_info = if group.tabs.len() > 1 {
        format!(" [{}/{}] ", group.active_tab + 1, group.tabs.len())
    } else {
        String::new()
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    if !tab_info.is_empty() {
        block = block.title_top(Line::styled(tab_info, Style::default().fg(theme.dim)));
    }

    if is_active {
        let indicator = if let Some(ref overlay) = mode.overlay {
            match overlay {
                Overlay::Copy => " COPY ",
                Overlay::Scroll => " SCROLL ",
                _ => "",
            }
        } else {
            match mode.base {
                BaseMode::Interact => " INTERACT ",
                BaseMode::Normal => "",
            }
        };
        if !indicator.is_empty() {
            block = block.title_bottom(Line::styled(
                indicator,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width <= 2 || inner.height == 0 {
        return;
    }

    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height);

    let cms = if is_active { copy_mode_state } else { None };
    let show_search = cms.map_or(false, |c| c.search_active);
    let show_tab_bar = group.tabs.len() > 1;
    let (tab_bar_area, content_area) = split_content_and_tab_areas(padded, show_tab_bar);

    if let Some(tab_bar_area) = tab_bar_area {
        render_tab_bar_from_snapshot(group, theme, frame, tab_bar_area);
    }

    if let Some(screen) = screen {
        render_content(screen, cms, frame, content_area);
    }

    if let Some(search_area) = search_overlay_area(content_area, show_search) {
        render_search_bar(cms.unwrap(), theme, frame, search_area);
    }
}

fn render_tab_bar_from_snapshot(
    group: &crate::server::protocol::WindowSnapshot,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    const SEP: &str = " \u{B7} ";
    const PLUS_TEXT: &str = " + ";

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut cursor_x = area.x;
    let max_x = area.x.saturating_add(area.width);
    let sep_width = display_width_u16(SEP);
    let plus_reserve = display_width_u16(PLUS_TEXT);
    let mut has_visible_tabs = false;

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active_tab = i == group.active_tab;
        let label = format!(" {} ", tab.title);
        let label_width = display_width_u16(&label);
        let sep_before = if has_visible_tabs { sep_width } else { 0 };
        let needed = sep_before
            .saturating_add(label_width)
            .saturating_add(plus_reserve);

        if cursor_x.saturating_add(needed) > max_x && !is_active_tab {
            continue;
        }

        if has_visible_tabs {
            spans.push(Span::styled(SEP, Style::default().fg(theme.dim)));
            cursor_x = cursor_x.saturating_add(sep_width);
        }

        let style = if is_active_tab {
            Style::default()
                .fg(theme.tab_active)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.tab_inactive)
        };

        spans.push(Span::styled(label, style));
        cursor_x = cursor_x.saturating_add(label_width);
        has_visible_tabs = true;
    }

    let plus_sep = if has_visible_tabs { sep_width } else { 0 };
    if cursor_x
        .saturating_add(plus_sep)
        .saturating_add(plus_reserve)
        <= max_x
    {
        if has_visible_tabs {
            spans.push(Span::styled(SEP, Style::default().fg(theme.dim)));
        }
        spans.push(Span::styled(PLUS_TEXT, Style::default().fg(theme.accent)));
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
    let fg = if is_active { theme.accent } else { theme.dim };
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
pub fn tab_bar_area(_group: &Window, area: Rect) -> Option<Rect> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::TabId;
    use crate::window::{Tab, TabKind, WindowId};

    fn make_window(titles: &[&str], active_tab: usize) -> Window {
        assert!(!titles.is_empty());
        let mut first = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "test");
        first.title = titles[0].to_string();
        let mut group = Window::new(WindowId::new_v4(), first);
        for title in titles.iter().skip(1) {
            let mut tab = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "test");
            tab.title = (*title).to_string();
            group.add_tab(tab);
        }
        group.active_tab = active_tab;
        group
    }

    #[test]
    fn test_tab_bar_hit_test_uses_display_width_for_wide_title() {
        let group = make_window(&["中", "abc"], 0);
        let layout = tab_bar_layout(&group, &Theme::default(), Rect::new(0, 0, 20, 1));

        assert_eq!(layout.tab_ranges[0], (0, 4)); // " 中 " => 4 display cols
        assert_eq!(layout.tab_ranges[1], (7, 12)); // after " · "
        assert_eq!(layout.plus_range, Some((15, 18)));
        assert_eq!(tab_bar_hit_test(&layout, 3, 0), Some(TabBarClick::Tab(0)));
        assert_eq!(tab_bar_hit_test(&layout, 4, 0), None); // separator
        assert_eq!(tab_bar_hit_test(&layout, 8, 0), Some(TabBarClick::Tab(1)));
    }

    #[test]
    fn test_tab_bar_layout_fit_respects_display_width() {
        let group = make_window(&["中", "a"], 0);
        let layout = tab_bar_layout(&group, &Theme::default(), Rect::new(0, 0, 13, 1));

        assert_eq!(layout.tab_ranges[0], (0, 4));
        assert_eq!(layout.tab_ranges[1], (7, 10));
        assert_eq!(layout.plus_range, None);
        assert_eq!(tab_bar_hit_test(&layout, 8, 0), Some(TabBarClick::Tab(1)));
    }

    #[test]
    fn test_search_overlay_area_uses_bottom_row_without_resizing_content() {
        let padded = Rect::new(2, 3, 12, 6);
        let (_tab_bar, content_area) = split_content_and_tab_areas(padded, true);

        let overlay = search_overlay_area(content_area, true).unwrap();
        assert_eq!(content_area, Rect::new(2, 4, 12, 5));
        assert_eq!(overlay, Rect::new(2, 8, 12, 1));

        let (_tab_bar_again, content_area_again) = split_content_and_tab_areas(padded, true);
        assert_eq!(content_area_again, content_area);
    }
}
