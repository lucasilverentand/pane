use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use pane_protocol::config::{Config, Theme};
use crate::copy_mode::CopyModeState;
use pane_protocol::layout::SplitDirection;
use crate::window::terminal::{render_screen, render_screen_copy_mode};

fn render_content(
    screen: &vt100::Screen,
    cms: Option<&CopyModeState>,
    frame: &mut Frame,
    area: Rect,
) {
    // Clamp render area to the VT screen dimensions so the process content
    // renders at its actual size. When a smaller client is connected the PTY
    // is sized to the minimum, and larger clients show only that region
    // instead of stretching the content to fill.
    let (vt_rows, vt_cols) = screen.size();
    let render_area = Rect::new(
        area.x,
        area.y,
        area.width.min(vt_cols),
        area.height.min(vt_rows),
    );
    let lines: Vec<Line<'static>> = match cms {
        Some(cms) => render_screen_copy_mode(screen, render_area, cms),
        None => render_screen(screen, render_area),
    };
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, render_area);
}

fn render_search_bar(cms: &CopyModeState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            "/",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(&cms.search_query, Style::default().fg(theme.fg)),
        Span::styled("_", Style::default().fg(theme.dim)),
    ]);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render a pane group from a snapshot (used by the client).
/// Receives the active tab's vt100 screen directly instead of accessing the Pane struct.
#[allow(clippy::too_many_arguments)]
pub fn render_group_from_snapshot(
    group: &pane_protocol::protocol::WindowSnapshot,
    screen: Option<&vt100::Screen>,
    is_active: bool,
    _is_interact: bool,
    copy_mode_state: Option<&CopyModeState>,
    config: &Config,
    hover: Option<(u16, u16)>,
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

    // Single chrome color: border, active tab, separators, +, indicators all match
    let chrome_color = if is_active {
        decoration_color.unwrap_or(theme.accent)
    } else {
        theme.border_inactive
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(chrome_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width <= 2 || inner.height == 0 {
        return;
    }

    // 1-cell padding inside the border on each side
    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height);

    let cms = if is_active { copy_mode_state } else { None };
    let show_search = cms.is_some_and(|c| c.search_active);
    let mut constraints = vec![Constraint::Length(1), Constraint::Fill(1)];
    if show_search {
        constraints.push(Constraint::Length(1));
    }
    let areas = Layout::vertical(constraints).split(padded);

    render_tab_bar_from_snapshot(group, theme, chrome_color, is_active, hover, frame, areas[0]);
    let content_area = areas[1];
    let search_area = areas.get(2).copied();

    if let Some(screen) = screen {
        render_content(screen, cms, frame, content_area);
    }

    if show_search {
        if let Some(search_area) = search_area {
            render_search_bar(cms.unwrap(), theme, frame, search_area);
        }
    }
}

/// Maximum display width for a single tab title (excluding padding).
const MAX_TAB_TITLE: usize = 20;
/// Ticker scroll speed: characters per second.
const TICKER_CHARS_PER_SEC: f64 = 4.0;
/// Pause at the start/end of a ticker cycle (seconds).
const TICKER_PAUSE_SECS: f64 = 2.0;

/// Truncate a title for an inactive tab, adding "…" if it overflows.
fn truncate_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        title.to_string()
    } else {
        let mut s: String = title.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

/// Produce a ticker-scrolling window into a long title.
fn ticker_title(title: &str, max: usize) -> String {
    let char_count = title.chars().count();
    if char_count <= max {
        return title.to_string();
    }
    let overflow = char_count - max;
    let scroll_duration = overflow as f64 / TICKER_CHARS_PER_SEC;
    let cycle = TICKER_PAUSE_SECS + scroll_duration + TICKER_PAUSE_SECS + scroll_duration;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let t = now % cycle;

    let offset = if t < TICKER_PAUSE_SECS {
        // Pause at start
        0
    } else if t < TICKER_PAUSE_SECS + scroll_duration {
        // Scroll forward
        ((t - TICKER_PAUSE_SECS) * TICKER_CHARS_PER_SEC) as usize
    } else if t < TICKER_PAUSE_SECS + scroll_duration + TICKER_PAUSE_SECS {
        // Pause at end
        overflow
    } else {
        // Scroll back
        let reverse_t = t - TICKER_PAUSE_SECS - scroll_duration - TICKER_PAUSE_SECS;
        overflow - (reverse_t * TICKER_CHARS_PER_SEC) as usize
    };

    let offset = offset.min(overflow);
    title.chars().skip(offset).take(max).collect()
}

fn render_tab_bar_from_snapshot(
    group: &pane_protocol::protocol::WindowSnapshot,
    theme: &Theme,
    chrome_color: Color,
    is_active: bool,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    const SEP: &str = " \u{B7} ";
    const SEP_WIDTH: u16 = 3;
    const PLUS_RESERVE: u16 = 3; // " + "
    const INDICATOR_WIDTH: u16 = 2; // "◂ " or " ▸"

    let n = group.tabs.len();
    let max_x = area.x + area.width;

    // Check if hover is on the tab bar row
    let hover_x = hover.and_then(|(hx, hy)| if hy == area.y { Some(hx) } else { None });

    // Compute label widths (capped at MAX_TAB_TITLE + 2 for padding)
    let label_widths: Vec<u16> = group
        .tabs
        .iter()
        .map(|tab| (tab.title.chars().count().min(MAX_TAB_TITLE) as u16) + 2)
        .collect();

    // Check if everything fits (reserve space for + button)
    let total: u16 = label_widths.iter().sum::<u16>()
        + if n > 1 { SEP_WIDTH * (n as u16 - 1) } else { 0 }
        + PLUS_RESERVE;

    let (lo, hi, hidden_left, hidden_right) = if n == 0 || total <= area.width {
        (0, n.saturating_sub(1), 0usize, 0usize)
    } else {
        // Overflow — find widest contiguous range centered on active tab
        let active = group.active_tab.min(n - 1);
        let range_width = |lo: usize, hi: usize| -> u16 {
            let mut w: u16 = 0;
            for (j, lw) in label_widths[lo..=hi].iter().enumerate() {
                w += lw;
                if j > 0 {
                    w += SEP_WIDTH;
                }
            }
            if lo > 0 {
                w += INDICATOR_WIDTH;
            }
            if hi < n - 1 {
                w += INDICATOR_WIDTH;
            }
            w + PLUS_RESERVE
        };

        let mut lo = active;
        let mut hi = active;
        loop {
            let mut expanded = false;
            if lo > 0 && range_width(lo - 1, hi) <= area.width {
                lo -= 1;
                expanded = true;
            }
            if hi + 1 < n && range_width(lo, hi + 1) <= area.width {
                hi += 1;
                expanded = true;
            }
            if !expanded {
                break;
            }
        }
        (lo, hi, lo, n - 1 - hi)
    };

    // Compute tab_ranges for hit testing
    let mut tab_ranges: Vec<(u16, u16)> = vec![(0, 0); n];
    let mut cursor_x = area.x;
    if hidden_left > 0 {
        cursor_x += INDICATOR_WIDTH;
    }
    if n > 0 {
        for i in lo..=hi {
            if i > lo {
                cursor_x += SEP_WIDTH;
            }
            tab_ranges[i] = (cursor_x, cursor_x + label_widths[i]);
            cursor_x += label_widths[i];
        }
    }

    // Build spans
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Left overflow indicator
    if hidden_left > 0 {
        spans.push(Span::styled("\u{25C2} ", Style::default().fg(chrome_color)));
    }

    let mut first_visible = true;
    if n > 0 {
        for (i, (tab, &(tab_start, tab_end))) in group.tabs[lo..=hi]
            .iter()
            .zip(&tab_ranges[lo..=hi])
            .enumerate()
        {
            if !first_visible {
                spans.push(Span::styled(SEP, Style::default().fg(chrome_color)));
            }
            first_visible = false;

            let is_hovered = hover_x.is_some_and(|hx| hx >= tab_start && hx < tab_end);
            let is_active_tab = (lo + i) == group.active_tab;

            let style = if !is_active {
                Style::default().fg(theme.border_inactive)
            } else if is_active_tab {
                Style::default()
                    .fg(chrome_color)
                    .add_modifier(Modifier::BOLD)
            } else if is_hovered {
                Style::default().fg(theme.fg)
            } else {
                Style::default().fg(theme.tab_inactive)
            };

            let display_title = if is_active_tab {
                ticker_title(&tab.title, MAX_TAB_TITLE)
            } else {
                truncate_title(&tab.title, MAX_TAB_TITLE)
            };
            let label = format!(" {} ", display_title);
            spans.push(Span::styled(label, style));
        }
    }

    // Right overflow indicator
    if hidden_right > 0 {
        spans.push(Span::styled(" \u{25B8}", Style::default().fg(chrome_color)));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);

    // Render + button right-aligned directly into the buffer
    if PLUS_RESERVE <= max_x.saturating_sub(area.x) && !group.tabs.is_empty() {
        let plus_x = max_x - PLUS_RESERVE;
        let plus_hovered = hover_x.is_some_and(|hx| hx >= plus_x && hx < max_x);
        let plus_style = if !is_active {
            Style::default().fg(theme.border_inactive)
        } else if plus_hovered {
            Style::default().fg(theme.fg)
        } else {
            Style::default().fg(chrome_color)
        };
        frame.buffer_mut().set_string(plus_x, area.y, " + ", plus_style);
    }
}

/// Render a fold indicator line with 1-cell padding on each side.
pub fn render_folded(
    is_active: bool,
    direction: SplitDirection,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let _ = is_active;
    let style = Style::default().fg(theme.dim);
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
