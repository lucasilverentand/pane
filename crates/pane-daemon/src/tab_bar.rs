//! Tab bar hit-testing for the server's mouse handling.
//!
//! These functions compute tab bar geometry from the daemon's `Window` struct,
//! allowing the server to detect clicks on tabs and the [+] button.

use ratatui::layout::Rect;
use ratatui::widgets::{Block, BorderType, Borders};

use pane_protocol::config::Theme;
use crate::window::Window;

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

/// Compute the tab bar area for a given pane group within its visible rect.
/// Returns None if the area is too small.
pub fn tab_bar_area(_group: &Window, area: Rect) -> Option<Rect> {
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

/// Compute tab bar layout for a given group and area (for hit testing).
pub fn tab_bar_layout(group: &Window, theme: &Theme, area: Rect) -> TabBarLayout {
    let mut tab_ranges: Vec<(u16, u16)> = Vec::new();
    let mut cursor_x = area.x;
    let max_x = area.x + area.width;
    let sep_width = 3u16;
    let plus_reserve = 3u16;

    for (i, tab) in group.tabs.iter().enumerate() {
        let is_active_tab = i == group.active_tab;
        let label = format!(" {} ", tab.title);
        let label_width = label.len() as u16;

        if cursor_x + label_width + plus_reserve > max_x && !is_active_tab {
            tab_ranges.push((0, 0));
            continue;
        }

        if i > 0 {
            cursor_x += sep_width;
        }

        let tab_start = cursor_x;
        cursor_x += label_width;
        tab_ranges.push((tab_start, cursor_x));
    }

    let plus_range = if cursor_x + sep_width + plus_reserve <= max_x {
        cursor_x += sep_width;
        let start = cursor_x;
        cursor_x += plus_reserve;
        Some((start, cursor_x))
    } else {
        None
    };
    let _ = theme; // used for styling only (not needed for hit-testing geometry)

    TabBarLayout {
        area,
        tab_ranges,
        plus_range,
    }
}

/// Hit-test the tab bar. Returns which tab or + was clicked.
pub fn tab_bar_hit_test(layout: &TabBarLayout, x: u16, y: u16) -> Option<TabBarClick> {
    if y != layout.area.y {
        return None;
    }
    if x < layout.area.x || x >= layout.area.x + layout.area.width {
        return None;
    }

    if let Some((start, end)) = layout.plus_range {
        if x >= start && x < end {
            return Some(TabBarClick::NewTab);
        }
    }

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
