use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use pane_protocol::config::Theme;

pub const HEIGHT: u16 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBarClick {
    Home,
    Tab(usize),
    NewWorkspace,
}

struct TabLayout {
    /// (start_x, end_x) for each tab's full span (inclusive of padding).
    /// Hidden tabs have (0, 0).
    tab_ranges: Vec<(u16, u16)>,
    /// (start_x, end_x) for the + button
    plus_range: Option<(u16, u16)>,
    /// Number of tabs hidden to the left (before the visible window).
    hidden_left: usize,
    /// Number of tabs hidden to the right (after the visible window).
    hidden_right: usize,
}

/// Compute the inner content area for the workspace bar, inset by 1 extra
/// cell on each side for visual padding (on top of any border inset).
fn padded_tab_area(area: Rect) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }

    // For bordered bars (HEIGHT=3), start from the inner text row.
    let has_border = area.width > 2 && area.height > 2;
    let x = if has_border { area.x + 1 } else { area.x };
    let width = if has_border {
        area.width - 2
    } else {
        area.width
    };
    let y = if has_border {
        area.y + (area.height / 2)
    } else {
        area.y
    };

    // Extra 1-cell padding on each side
    Rect::new(x + 1, y, width.saturating_sub(2), 1)
}

fn truncate_name(name: &str, max: usize) -> String {
    if name.len() <= max {
        name.to_string()
    } else {
        format!("{}...", &name[..max - 3])
    }
}

const SEP_WIDTH: u16 = 3; // " · "
const PLUS_RESERVE: u16 = 3; // " + "
const INDICATOR_WIDTH: u16 = 2; // "◂ " or " ▸"

fn compute_layout(names: &[&str], active_idx: usize, area: Rect) -> TabLayout {
    let n = names.len();
    let max_x = area.x + area.width;

    if n == 0 {
        let plus_range = if PLUS_RESERVE <= area.width {
            Some((max_x - PLUS_RESERVE, max_x))
        } else {
            None
        };
        return TabLayout {
            tab_ranges: vec![],
            plus_range,
            hidden_left: 0,
            hidden_right: 0,
        };
    }

    // Compute label widths
    let label_widths: Vec<u16> = names
        .iter()
        .map(|name| truncate_name(name, 20).len() as u16 + 2)
        .collect();

    // Check if everything fits without overflow
    let total: u16 = label_widths.iter().sum::<u16>()
        + if n > 1 {
            SEP_WIDTH * (n as u16 - 1)
        } else {
            0
        }
        + PLUS_RESERVE;

    if total <= area.width {
        // Everything fits — lay out left-to-right
        let mut tab_ranges = Vec::new();
        let mut cursor_x = area.x;
        for (i, &w) in label_widths.iter().enumerate() {
            if i > 0 {
                cursor_x += SEP_WIDTH;
            }
            tab_ranges.push((cursor_x, cursor_x + w));
            cursor_x += w;
        }
        let plus_range = Some((max_x - PLUS_RESERVE, max_x));
        return TabLayout {
            tab_ranges,
            plus_range,
            hidden_left: 0,
            hidden_right: 0,
        };
    }

    // Overflow — find the widest contiguous range centered on active_idx
    let active = active_idx.min(n - 1);
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

    let hidden_left = lo;
    let hidden_right = n - 1 - hi;

    // Build tab_ranges
    let mut tab_ranges = vec![(0u16, 0u16); n];
    let mut cursor_x = area.x;
    if hidden_left > 0 {
        cursor_x += INDICATOR_WIDTH;
    }
    for i in lo..=hi {
        if i > lo {
            cursor_x += SEP_WIDTH;
        }
        tab_ranges[i] = (cursor_x, cursor_x + label_widths[i]);
        cursor_x += label_widths[i];
    }

    let plus_range = if PLUS_RESERVE <= area.width {
        Some((max_x - PLUS_RESERVE, max_x))
    } else {
        None
    };

    TabLayout {
        tab_ranges,
        plus_range,
        hidden_left,
        hidden_right,
    }
}

/// Width of the home button box (including borders): " 󰋜 Home " = 8 + 2 borders = 10
const HOME_WIDTH: u16 = 10;
/// Gap between home button and workspace bar
const HOME_GAP: u16 = 1;

pub fn render(
    workspace_names: &[&str],
    active_idx: usize,
    theme: &Theme,
    focused: bool,
    home_active: bool,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    // Split area: [Home button] [gap] [workspace bar]
    let home_total = HOME_WIDTH + HOME_GAP;
    if area.width <= home_total + 4 {
        // Too narrow to render both; just render workspace bar
        render_workspace_tabs(workspace_names, active_idx, theme, focused && !home_active, hover, frame, area);
        return;
    }

    let home_area = Rect::new(area.x, area.y, HOME_WIDTH, area.height);
    let bar_area = Rect::new(area.x + home_total, area.y, area.width - home_total, area.height);

    // Render home button
    let home_hovered = hover
        .map(|(hx, hy)| {
            hy >= home_area.y
                && hy < home_area.y + home_area.height
                && hx >= home_area.x
                && hx < home_area.x + home_area.width
        })
        .unwrap_or(false);

    let home_border_color = if home_active && focused {
        theme.accent
    } else {
        theme.border_inactive
    };
    let home_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(home_border_color));
    let home_inner = home_block.inner(home_area);
    frame.render_widget(home_block, home_area);

    if home_inner.width > 0 && home_inner.height > 0 {
        let home_style = if home_active {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if home_hovered {
            Style::default().fg(theme.fg)
        } else {
            Style::default().fg(theme.dim)
        };
        let label = Line::from(Span::styled(" \u{f015} Home", home_style));
        let label_y = home_inner.y + home_inner.height / 2;
        frame.render_widget(
            Paragraph::new(label),
            Rect::new(home_inner.x, label_y, home_inner.width, 1),
        );
    }

    // Render workspace tabs
    render_workspace_tabs(workspace_names, active_idx, theme, focused && !home_active, hover, frame, bar_area);
}

fn render_workspace_tabs(
    workspace_names: &[&str],
    active_idx: usize,
    theme: &Theme,
    focused: bool,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    let border_color = if focused {
        theme.accent
    } else {
        theme.border_inactive
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    let tab_area = padded_tab_area(area);
    let layout = compute_layout(workspace_names, active_idx, tab_area);
    let sep = " \u{B7} "; // " · "

    // Determine which element is hovered
    let hovered = hover.and_then(|(hx, hy)| hit_test_bar(workspace_names, active_idx, area, hx, hy));

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut first_visible = true;
    let mut content_end = tab_area.x;

    // Left overflow indicator
    if layout.hidden_left > 0 {
        spans.push(Span::styled("\u{25C2} ", Style::default().fg(theme.dim)));
        content_end = tab_area.x + INDICATOR_WIDTH;
    }

    for (i, name) in workspace_names.iter().enumerate() {
        if i >= layout.tab_ranges.len() {
            break;
        }
        let (start, end) = layout.tab_ranges[i];
        if start == 0 && end == 0 {
            continue;
        }

        // Separator before non-first visible tabs
        if !first_visible {
            spans.push(Span::styled(sep, Style::default().fg(theme.dim)));
        }
        first_visible = false;

        let is_active = i == active_idx;
        let is_hovered = matches!(hovered, Some(WorkspaceBarClick::Tab(t)) if t == i);
        let display_name = truncate_name(name, 20);
        let label = format!(" {} ", display_name);

        let style = if is_active {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if is_hovered {
            Style::default().fg(theme.fg)
        } else {
            Style::default().fg(theme.dim)
        };

        spans.push(Span::styled(label, style));
        content_end = end;
    }

    // Right overflow indicator
    if layout.hidden_right > 0 {
        spans.push(Span::styled(" \u{25B8}", Style::default().fg(theme.dim)));
        content_end += INDICATOR_WIDTH;
    }

    // Right-align the + button with padding
    if let Some((plus_start, _)) = layout.plus_range {
        let gap = plus_start.saturating_sub(content_end);
        if gap > 0 {
            spans.push(Span::raw(" ".repeat(gap as usize)));
        }
        let plus_hovered = matches!(hovered, Some(WorkspaceBarClick::NewWorkspace));
        let plus_style = if plus_hovered {
            Style::default().fg(theme.fg)
        } else {
            Style::default().fg(theme.accent)
        };
        spans.push(Span::styled(" + ", plus_style));
    }

    let line = Line::from(spans);
    frame.render_widget(block, area);
    if tab_area.width > 0 && tab_area.height > 0 {
        frame.render_widget(Paragraph::new(line), tab_area);
    }
}

/// Hit test for the full header area (home button + workspace bar).
pub fn hit_test(
    workspace_names: &[&str],
    active_idx: usize,
    area: Rect,
    x: u16,
    y: u16,
) -> Option<WorkspaceBarClick> {
    // Check home button first
    let home_total = HOME_WIDTH + HOME_GAP;
    if area.width > home_total + 4 {
        let home_area = Rect::new(area.x, area.y, HOME_WIDTH, area.height);
        if x >= home_area.x
            && x < home_area.x + home_area.width
            && y >= home_area.y
            && y < home_area.y + home_area.height
        {
            return Some(WorkspaceBarClick::Home);
        }

        // Check workspace bar area
        let bar_area = Rect::new(area.x + home_total, area.y, area.width - home_total, area.height);
        return hit_test_bar(workspace_names, active_idx, bar_area, x, y);
    }

    // Narrow fallback: no home button, full area is workspace bar
    hit_test_bar(workspace_names, active_idx, area, x, y)
}

/// Hit test within the workspace bar only (no home button).
fn hit_test_bar(
    workspace_names: &[&str],
    active_idx: usize,
    area: Rect,
    x: u16,
    y: u16,
) -> Option<WorkspaceBarClick> {
    let tab_area = padded_tab_area(area);

    if y < tab_area.y || y >= tab_area.y + tab_area.height {
        return None;
    }
    if x < tab_area.x || x >= tab_area.x + tab_area.width {
        return None;
    }

    let layout = compute_layout(workspace_names, active_idx, tab_area);

    // Check + button first
    if let Some((start, end)) = layout.plus_range {
        if x >= start && x < end {
            return Some(WorkspaceBarClick::NewWorkspace);
        }
    }

    // Check tab bodies
    for (i, (start, end)) in layout.tab_ranges.iter().enumerate() {
        if *start == 0 && *end == 0 {
            continue;
        }
        if x >= *start && x < *end {
            return Some(WorkspaceBarClick::Tab(i));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Home button takes HOME_WIDTH (10) + HOME_GAP (1) = 11 columns from the left.
    // The workspace bar area starts at x = 9 for a full-width area starting at 0.
    // Within that bar, padded_tab_area insets by border (if any) + 1 cell each side.

    // Helper: compute the bar-only area (after home button) for a given full area.
    fn bar_area(full: Rect) -> Rect {
        let home_total = HOME_WIDTH + HOME_GAP;
        if full.width > home_total + 4 {
            Rect::new(full.x + home_total, full.y, full.width - home_total, full.height)
        } else {
            full
        }
    }

    #[test]
    fn test_hit_test_home_click() {
        let ws: Vec<&str> = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        // Home button occupies x=[0, 8)
        let click = hit_test(&ws, 0, area, 3, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Home));
    }

    #[test]
    fn test_hit_test_tab_click() {
        let ws: Vec<&str> = vec!["alpha", "beta"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        // Use hit_test_bar to test workspace tab clicks
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (start, _) = layout.tab_ranges[0];
        let click = hit_test_bar(&ws, 0, ba, start + 1, ba.y);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_plus_button() {
        let ws: Vec<&str> = vec!["a"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (plus_start, _) = layout.plus_range.unwrap();
        let click = hit_test_bar(&ws, 0, ba, plus_start, ba.y);
        assert_eq!(click, Some(WorkspaceBarClick::NewWorkspace));
    }

    #[test]
    fn test_hit_test_outside() {
        let ws: Vec<&str> = vec!["a"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        let click = hit_test_bar(&ws, 0, ba, 70, 0);
        assert_eq!(click, None);
    }

    #[test]
    fn test_hit_test_wrong_row() {
        let ws: Vec<&str> = vec!["a"];
        let area = Rect::new(0, 0, 80, 1);
        let click = hit_test(&ws, 0, area, 2, 1);
        assert_eq!(click, None);
    }

    #[test]
    fn test_truncate_name_short() {
        assert_eq!(truncate_name("hello", 20), "hello");
    }

    #[test]
    fn test_truncate_name_long() {
        let long = "a_very_long_workspace_name_here";
        let result = truncate_name(long, 20);
        assert_eq!(result.len(), 20);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_plus_button_present() {
        let ws = vec!["a"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        assert!(layout.plus_range.is_some());
    }

    // --- Hit test at exact boundary positions (using bar-only area) ---

    #[test]
    fn test_hit_test_at_tab_start() {
        let ws = vec!["alpha"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (start, _) = layout.tab_ranges[0];
        let click = hit_test_bar(&ws, 0, ba, start, ba.y);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_tab_end_exclusive() {
        let ws = vec!["alpha"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (_, end) = layout.tab_ranges[0];
        let click = hit_test_bar(&ws, 0, ba, end, ba.y);
        assert_ne!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_last_pixel_of_tab() {
        let ws = vec!["alpha"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (_, end) = layout.tab_ranges[0];
        let click = hit_test_bar(&ws, 0, ba, end - 1, ba.y);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_second_tab_boundary() {
        let ws = vec!["ab", "cd"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (start, end) = layout.tab_ranges[1];
        // Click at start of second tab
        assert_eq!(
            hit_test_bar(&ws, 0, ba, start, ba.y),
            Some(WorkspaceBarClick::Tab(1))
        );
        // Click at end-1 of second tab
        assert_eq!(
            hit_test_bar(&ws, 0, ba, end - 1, ba.y),
            Some(WorkspaceBarClick::Tab(1))
        );
        // Click at end is outside
        assert_ne!(
            hit_test_bar(&ws, 0, ba, end, ba.y),
            Some(WorkspaceBarClick::Tab(1))
        );
    }

    // --- Many workspaces (overflow behavior) ---

    #[test]
    fn test_many_workspaces_overflow() {
        let ws = vec![
            "workspace_one",
            "workspace_two",
            "workspace_three",
            "workspace_four",
            "workspace_five",
        ];
        let area = Rect::new(0, 0, 40, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let skipped = layout
            .tab_ranges
            .iter()
            .filter(|&&(s, e)| s == 0 && e == 0)
            .count();
        assert!(
            skipped > 0,
            "expected some tabs to be skipped in narrow area"
        );
    }

    #[test]
    fn test_active_workspace_always_visible() {
        let ws = vec![
            "workspace_one",
            "workspace_two",
            "active_workspace",
            "workspace_four",
        ];
        let area = Rect::new(0, 0, 40, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 2, padded);
        let (start, end) = layout.tab_ranges[2];
        assert!(start != 0 || end != 0, "active tab should be visible");
    }

    // --- Name truncation ---

    #[test]
    fn test_truncate_name_exact_max() {
        let name = "12345678901234567890"; // 20 chars
        assert_eq!(truncate_name(name, 20), name);
    }

    #[test]
    fn test_truncate_name_one_over_max() {
        let name = "123456789012345678901"; // 21 chars
        let result = truncate_name(name, 20);
        assert_eq!(result.len(), 20);
        assert!(result.ends_with("..."));
        assert_eq!(result, "12345678901234567...");
    }

    #[test]
    fn test_truncate_name_empty() {
        assert_eq!(truncate_name("", 20), "");
    }

    #[test]
    fn test_truncate_name_small_max() {
        let result = truncate_name("hello", 4);
        assert_eq!(result.len(), 4);
        assert_eq!(result, "h...");
    }

    // --- Compute layout with empty names ---

    #[test]
    fn test_compute_layout_empty_name() {
        let ws = vec![""];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, end) = layout.tab_ranges[0];
        assert_eq!(start, 1);
        assert_eq!(end, 3); // " " + "" + " " = 2 chars, starting at 1
        assert!(layout.plus_range.is_some());
    }

    #[test]
    fn test_compute_layout_all_empty_names() {
        let ws = vec!["", "", ""];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        for (start, end) in &layout.tab_ranges {
            assert!(*start != 0 || *end != 0, "all tiny tabs should fit");
        }
    }

    #[test]
    fn test_compute_layout_no_workspaces() {
        let ws: Vec<&str> = vec![];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        assert!(layout.tab_ranges.is_empty());
        assert!(layout.plus_range.is_some());
    }

    #[test]
    fn test_hit_test_with_area_offset() {
        // Area starts at x=10, y=5
        let ws = vec!["ab"];
        let full = Rect::new(10, 5, 80, 1);
        let ba = bar_area(full);
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (start, end) = layout.tab_ranges[0];
        assert_eq!(
            hit_test_bar(&ws, 0, ba, start, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(
            hit_test_bar(&ws, 0, ba, end - 1, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        // Outside area y
        assert_eq!(hit_test_bar(&ws, 0, ba, start, 4), None);
        assert_eq!(hit_test_bar(&ws, 0, ba, start, 6), None);
    }

    #[test]
    fn test_hit_test_bordered_bar_uses_inner_row() {
        let ws = vec!["alpha"];
        let full = Rect::new(0, 0, 80, HEIGHT);
        let ba = bar_area(full);
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);
        let (start, _) = layout.tab_ranges[0];
        // Bordered: inner row is at y=1 (HEIGHT=3, middle row)
        assert_eq!(
            hit_test_bar(&ws, 0, ba, start, 1),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(hit_test_bar(&ws, 0, ba, start, 0), None);
        assert_eq!(hit_test_bar(&ws, 0, ba, start, 2), None);
    }

    // --- Many workspaces overflow (additional) ---

    #[test]
    fn test_many_workspaces_active_is_last() {
        let ws: Vec<&str> = (0..10).map(|_| "workspace_long").collect();
        let area = Rect::new(0, 0, 50, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 9, padded);
        // The active (last) tab should be visible even when others overflow
        let (start, end) = layout.tab_ranges[9];
        assert!(start != 0 || end != 0, "active tab at end should be visible");
    }

    #[test]
    fn test_overflow_skipped_tabs_not_clickable() {
        let ws = vec![
            "workspace_one",
            "workspace_two",
            "workspace_three",
            "workspace_four",
            "workspace_five",
        ];
        let ba = bar_area(Rect::new(0, 0, 60, 1));
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);

        // For any skipped tab, hit_test_bar should not return it
        for (i, &(start, end)) in layout.tab_ranges.iter().enumerate() {
            if start == 0 && end == 0 {
                for x in ba.x..ba.x + ba.width {
                    assert_ne!(
                        hit_test_bar(&ws, 0, ba, x, ba.y),
                        Some(WorkspaceBarClick::Tab(i)),
                        "skipped tab {} should not be clickable at x={}",
                        i,
                        x
                    );
                }
            }
        }
    }

    // --- Single-char workspace names ---

    #[test]
    fn test_single_char_names() {
        let ws = vec!["1", "2", "3", "4", "5"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        // All single-char names should fit easily in 80 cols
        for (start, end) in &layout.tab_ranges {
            assert!(
                *start != 0 || *end != 0,
                "all single-char tabs should be visible"
            );
        }
    }

    #[test]
    fn test_single_char_names_hit_test() {
        let ws = vec!["1", "2", "3"];
        let ba = bar_area(Rect::new(0, 0, 80, 1));
        let padded = padded_tab_area(ba);
        let layout = compute_layout(&ws, 0, padded);

        let (start0, end0) = layout.tab_ranges[0];
        assert_eq!(
            hit_test_bar(&ws, 0, ba, start0, ba.y),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(
            hit_test_bar(&ws, 0, ba, end0 - 1, ba.y),
            Some(WorkspaceBarClick::Tab(0))
        );

        let (start1, _) = layout.tab_ranges[1];
        assert_eq!(
            hit_test_bar(&ws, 0, ba, start1, ba.y),
            Some(WorkspaceBarClick::Tab(1))
        );
    }

    #[test]
    fn test_many_single_char_names_narrow_area() {
        // 20 single-char workspaces in a narrow area
        let names: Vec<&str> = vec![
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F", "G", "H",
            "I", "J", "K",
        ];
        let area = Rect::new(0, 0, 30, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&names, 0, padded);

        let visible: usize = layout
            .tab_ranges
            .iter()
            .filter(|&&(s, e)| s != 0 || e != 0)
            .count();
        assert!(
            visible < 20,
            "not all 20 tabs should fit in 30 cols"
        );
        assert!(visible > 0, "at least some tabs should be visible");
    }

    // --- Truncation edge cases ---

    #[test]
    fn test_truncate_name_max_3() {
        // max=3 means "..." = exactly 3 chars
        let result = truncate_name("abcdef", 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_name_unicode() {
        // Note: truncate_name uses byte-level slicing, so this tests basic ASCII behavior
        let result = truncate_name("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(result, "abcdefg...");
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_truncate_single_char_name() {
        assert_eq!(truncate_name("x", 20), "x");
        assert_eq!(truncate_name("x", 1), "x");
    }

    // --- Plus button in narrow area ---

    #[test]
    fn test_plus_button_very_narrow() {
        let ws = vec!["workspace"];
        // Area barely fits anything
        let area = Rect::new(0, 0, 6, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        // Plus button takes 3 chars, area is only 4 after padding (6-2)
        // Behavior depends on whether there's room
        if let Some((start, end)) = layout.plus_range {
            assert!(end <= padded.x + padded.width);
            assert!(start < end);
        }
    }

    // --- padded_tab_area edge cases ---

    #[test]
    fn test_padded_tab_area_zero_width() {
        let area = Rect::new(0, 0, 0, 0);
        let result = padded_tab_area(area);
        assert_eq!(result, Rect::new(0, 0, 0, 0));
    }

    #[test]
    fn test_padded_tab_area_minimum_bordered() {
        // 3x3 is the minimum bordered area (borders take 2, padding takes 2 more)
        let area = Rect::new(0, 0, 3, 3);
        let result = padded_tab_area(area);
        // inner after border: width=1, then padding subtracts 2 → 0 (saturating)
        // This just shouldn't panic
        assert!(result.width == 0 || result.width <= 1);
    }

    // --- Home button specific tests ---

    #[test]
    fn test_home_button_not_clickable_in_narrow_area() {
        let ws = vec!["a"];
        // Too narrow for home + bar (need > HOME_WIDTH + HOME_GAP + 4 = 15)
        let area = Rect::new(0, 0, 14, 1);
        let click = hit_test(&ws, 0, area, 2, 0);
        // Should fall through to bar-only mode, no Home click
        assert_ne!(click, Some(WorkspaceBarClick::Home));
    }

    #[test]
    fn test_home_vs_bar_boundary() {
        let ws = vec!["test"];
        let area = Rect::new(0, 0, 80, 1);
        // x=9 is last pixel of home button (HOME_WIDTH=10, x=[0,10))
        assert_eq!(hit_test(&ws, 0, area, 9, 0), Some(WorkspaceBarClick::Home));
        // x=10 is in the gap
        assert_ne!(hit_test(&ws, 0, area, 10, 0), Some(WorkspaceBarClick::Home));
    }
}
