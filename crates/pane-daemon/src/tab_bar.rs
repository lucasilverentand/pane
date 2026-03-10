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

    // The + button is right-aligned in the render, so match that here.
    let plus_range = if plus_reserve <= max_x.saturating_sub(area.x) {
        let plus_start = max_x - plus_reserve;
        Some((plus_start, max_x))
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

#[cfg(test)]
mod tests {
    use super::*;
    use pane_protocol::config::Theme;
    use pane_protocol::layout::TabId;
    use crate::window::{Tab, TabKind, Window, WindowId};

    fn make_group_with_tabs(titles: &[&str]) -> Window {
        let gid = WindowId::new_v4();
        let first = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "err");
        let mut group = Window::new(gid, first);
        group.tabs[0].title = titles[0].to_string();
        for title in &titles[1..] {
            let mut tab = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "err");
            tab.title = title.to_string();
            group.add_tab(tab);
        }
        group.active_tab = 0;
        group
    }

    // ---- tab_bar_area ----

    #[test]
    fn test_tab_bar_area_normal() {
        let group = make_group_with_tabs(&["shell"]);
        let area = Rect::new(0, 0, 80, 24);
        let result = tab_bar_area(&group, area);
        assert!(result.is_some());
        let bar = result.unwrap();
        // The bar should be 1 row tall
        assert_eq!(bar.height, 1);
        // Should be within the inner area (offset by border + padding)
        assert!(bar.x > 0);
        assert!(bar.width > 0);
    }

    #[test]
    fn test_tab_bar_area_too_small_width() {
        let group = make_group_with_tabs(&["shell"]);
        // Width too narrow: border (2) + padding (2) = need at least 5 width for inner > 2
        let area = Rect::new(0, 0, 4, 10);
        let result = tab_bar_area(&group, area);
        assert!(result.is_none());
    }

    #[test]
    fn test_tab_bar_area_too_small_height() {
        let group = make_group_with_tabs(&["shell"]);
        // Height 2: border takes 2, inner height = 0
        let area = Rect::new(0, 0, 80, 2);
        let result = tab_bar_area(&group, area);
        assert!(result.is_none());
    }

    #[test]
    fn test_tab_bar_area_minimum_viable() {
        let group = make_group_with_tabs(&["shell"]);
        // Border 2 + padding 2 inner = 1, border 2 inner height = 1
        let area = Rect::new(0, 0, 6, 3);
        let result = tab_bar_area(&group, area);
        assert!(result.is_some());
        let bar = result.unwrap();
        assert_eq!(bar.height, 1);
        assert_eq!(bar.width, 2); // inner.width(4) - 2 padding
    }

    // ---- tab_bar_layout ----

    #[test]
    fn test_tab_bar_layout_single_tab() {
        let group = make_group_with_tabs(&["shell"]);
        let theme = Theme::default();
        let area = Rect::new(5, 2, 60, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        assert_eq!(layout.area, area);
        assert_eq!(layout.tab_ranges.len(), 1);
        // Tab label is " shell " = 7 chars
        let (start, end) = layout.tab_ranges[0];
        assert_eq!(start, 5); // starts at area.x
        assert_eq!(end - start, 7); // " shell " = 7
        assert!(layout.plus_range.is_some());
    }

    #[test]
    fn test_tab_bar_layout_multiple_tabs() {
        let group = make_group_with_tabs(&["tab1", "tab2", "tab3"]);
        let theme = Theme::default();
        let area = Rect::new(0, 0, 80, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        assert_eq!(layout.tab_ranges.len(), 3);
        // All tabs should have non-zero ranges
        for (start, end) in &layout.tab_ranges {
            assert!(end > start, "tab range should be non-empty");
        }
        // Tabs should not overlap
        for i in 1..layout.tab_ranges.len() {
            let (_, prev_end) = layout.tab_ranges[i - 1];
            let (curr_start, _) = layout.tab_ranges[i];
            // The separator is 3 wide, added before tab i>0
            assert!(curr_start >= prev_end, "tabs should not overlap");
        }
    }

    #[test]
    fn test_tab_bar_layout_plus_button_right_aligned() {
        let group = make_group_with_tabs(&["shell"]);
        let theme = Theme::default();
        let area = Rect::new(0, 0, 60, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        let (plus_start, plus_end) = layout.plus_range.unwrap();
        assert_eq!(plus_end, 60); // right edge of area
        assert_eq!(plus_end - plus_start, 3); // plus_reserve = 3
    }

    #[test]
    fn test_tab_bar_layout_very_narrow_no_plus() {
        let group = make_group_with_tabs(&["x"]);
        let theme = Theme::default();
        // Area just 2 wide: too small for plus_reserve (3)
        let area = Rect::new(0, 0, 2, 1);
        let layout = tab_bar_layout(&group, &theme, area);
        assert!(layout.plus_range.is_none());
    }

    // ---- tab_bar_hit_test ----

    #[test]
    fn test_hit_test_click_on_tab() {
        let group = make_group_with_tabs(&["shell"]);
        let theme = Theme::default();
        let area = Rect::new(0, 0, 60, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        let (start, _end) = layout.tab_ranges[0];
        let result = tab_bar_hit_test(&layout, start, 0);
        assert_eq!(result, Some(TabBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_click_on_plus() {
        let group = make_group_with_tabs(&["shell"]);
        let theme = Theme::default();
        let area = Rect::new(0, 0, 60, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        let (plus_start, _) = layout.plus_range.unwrap();
        let result = tab_bar_hit_test(&layout, plus_start, 0);
        assert_eq!(result, Some(TabBarClick::NewTab));
    }

    #[test]
    fn test_hit_test_wrong_row() {
        let group = make_group_with_tabs(&["shell"]);
        let theme = Theme::default();
        let area = Rect::new(0, 5, 60, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        // Click on row 0, but bar is at row 5
        let result = tab_bar_hit_test(&layout, 10, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_hit_test_outside_area_left() {
        let group = make_group_with_tabs(&["shell"]);
        let theme = Theme::default();
        let area = Rect::new(10, 0, 60, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        let result = tab_bar_hit_test(&layout, 5, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_hit_test_outside_area_right() {
        let group = make_group_with_tabs(&["shell"]);
        let theme = Theme::default();
        let area = Rect::new(0, 0, 60, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        let result = tab_bar_hit_test(&layout, 60, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_hit_test_gap_between_tabs() {
        let group = make_group_with_tabs(&["a", "b"]);
        let theme = Theme::default();
        let area = Rect::new(0, 0, 80, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        // Find the gap: after end of tab 0 and before start of tab 1
        let (_, end0) = layout.tab_ranges[0];
        let (start1, _) = layout.tab_ranges[1];
        if start1 > end0 {
            // Click in the separator gap
            let result = tab_bar_hit_test(&layout, end0, 0);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_hit_test_multiple_tabs_click_second() {
        let mut group = make_group_with_tabs(&["first", "second", "third"]);
        group.active_tab = 0;
        let theme = Theme::default();
        let area = Rect::new(0, 0, 80, 1);
        let layout = tab_bar_layout(&group, &theme, area);

        let (start1, _) = layout.tab_ranges[1];
        if start1 > 0 {
            let result = tab_bar_hit_test(&layout, start1, 0);
            assert_eq!(result, Some(TabBarClick::Tab(1)));
        }
    }

    #[test]
    fn test_tab_bar_click_equality() {
        assert_eq!(TabBarClick::Tab(0), TabBarClick::Tab(0));
        assert_ne!(TabBarClick::Tab(0), TabBarClick::Tab(1));
        assert_ne!(TabBarClick::Tab(0), TabBarClick::NewTab);
        assert_eq!(TabBarClick::NewTab, TabBarClick::NewTab);
    }
}
