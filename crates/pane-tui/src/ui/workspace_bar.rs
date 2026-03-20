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
    Tab(usize),
    NewWorkspace,
}

struct TabLayout {
    /// (start_x, end_x) for each tab's full span (inclusive of padding).
    /// Hidden tabs have (0, 0).
    tab_ranges: Vec<(u16, u16)>,
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
    let count = name.chars().count();
    if count <= max {
        return name.to_string();
    }
    if max < 4 {
        return name.chars().take(max).collect();
    }
    let mut s: String = name.chars().take(max - 3).collect();
    s.push_str("...");
    s
}

const SEP_WIDTH: u16 = 3; // " · "
const INDICATOR_WIDTH: u16 = 2; // "◂ " or " ▸"
const PLUS_RESERVE: u16 = 3; // " + "

fn compute_layout(names: &[&str], active_idx: usize, area: Rect) -> TabLayout {
    let n = names.len();

    if n == 0 {
        return TabLayout {
            tab_ranges: vec![],
            hidden_left: 0,
            hidden_right: 0,
        };
    }

    // Compute label widths
    let label_widths: Vec<u16> = names
        .iter()
        .map(|name| truncate_name(name, 20).chars().count() as u16 + 2)
        .collect();

    // Check if everything fits without overflow (reserve space for + button)
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
        return TabLayout {
            tab_ranges,
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

    TabLayout {
        tab_ranges,
        hidden_left,
        hidden_right,
    }
}

pub fn render(
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
    let hovered = hover.and_then(|(hx, hy)| hit_test(workspace_names, active_idx, area, hx, hy));

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut first_visible = true;

    // Left overflow indicator
    if layout.hidden_left > 0 {
        spans.push(Span::styled("\u{25C2} ", Style::default().fg(theme.dim)));
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
        let _ = end;
    }

    // Right overflow indicator
    if layout.hidden_right > 0 {
        spans.push(Span::styled(" \u{25B8}", Style::default().fg(theme.dim)));
    }

    let line = Line::from(spans);
    frame.render_widget(block, area);
    if tab_area.width > 0 && tab_area.height > 0 {
        frame.render_widget(Paragraph::new(line), tab_area);
    }

    // Render + button right-aligned directly into the buffer
    if PLUS_RESERVE <= tab_area.width {
        let plus_x = tab_area.x + tab_area.width - PLUS_RESERVE;
        let plus_hovered = matches!(hovered, Some(WorkspaceBarClick::NewWorkspace));
        let plus_style = if plus_hovered {
            Style::default().fg(theme.fg)
        } else if focused {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.border_inactive)
        };
        frame.buffer_mut().set_string(plus_x, tab_area.y, " + ", plus_style);
    }
}

/// Hit test for the workspace bar area.
pub fn hit_test(
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

    // Check + button (right-aligned)
    if PLUS_RESERVE <= tab_area.width {
        let plus_x = tab_area.x + tab_area.width - PLUS_RESERVE;
        if x >= plus_x && x < tab_area.x + tab_area.width {
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

    #[test]
    fn test_hit_test_tab_click() {
        let ws: Vec<&str> = vec!["Home", "alpha", "beta"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, _) = layout.tab_ranges[0];
        let click = hit_test(&ws, 0, area, start + 1, area.y);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_second_tab() {
        let ws: Vec<&str> = vec!["Home", "alpha"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, _) = layout.tab_ranges[1];
        let click = hit_test(&ws, 0, area, start + 1, area.y);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(1)));
    }

    #[test]
    fn test_hit_test_outside() {
        let ws: Vec<&str> = vec!["a"];
        let area = Rect::new(0, 0, 80, 1);
        let click = hit_test(&ws, 0, area, 70, 0);
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
    fn test_hit_test_at_tab_start() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, _) = layout.tab_ranges[0];
        let click = hit_test(&ws, 0, area, start, area.y);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_tab_end_exclusive() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (_, end) = layout.tab_ranges[0];
        let click = hit_test(&ws, 0, area, end, area.y);
        assert_ne!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_last_pixel_of_tab() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (_, end) = layout.tab_ranges[0];
        let click = hit_test(&ws, 0, area, end - 1, area.y);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_second_tab_boundary() {
        let ws = vec!["ab", "cd"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, end) = layout.tab_ranges[1];
        assert_eq!(
            hit_test(&ws, 0, area, start, area.y),
            Some(WorkspaceBarClick::Tab(1))
        );
        assert_eq!(
            hit_test(&ws, 0, area, end - 1, area.y),
            Some(WorkspaceBarClick::Tab(1))
        );
        assert_ne!(
            hit_test(&ws, 0, area, end, area.y),
            Some(WorkspaceBarClick::Tab(1))
        );
    }

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

    #[test]
    fn test_compute_layout_empty_name() {
        let ws = vec![""];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, end) = layout.tab_ranges[0];
        assert_eq!(start, 1);
        assert_eq!(end, 3);
    }

    #[test]
    fn test_compute_layout_all_empty_names() {
        let ws = vec!["", "", ""];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        for (start, end) in &layout.tab_ranges {
            assert!(
                *start != 0 || *end != 0,
                "all tiny tabs should fit"
            );
        }
    }

    #[test]
    fn test_compute_layout_no_workspaces() {
        let ws: Vec<&str> = vec![];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        assert!(layout.tab_ranges.is_empty());
    }

    #[test]
    fn test_hit_test_with_area_offset() {
        let ws = vec!["ab"];
        let area = Rect::new(10, 5, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, end) = layout.tab_ranges[0];
        assert_eq!(
            hit_test(&ws, 0, area, start, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(
            hit_test(&ws, 0, area, end - 1, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(hit_test(&ws, 0, area, start, 4), None);
        assert_eq!(hit_test(&ws, 0, area, start, 6), None);
    }

    #[test]
    fn test_hit_test_bordered_bar_uses_inner_row() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, HEIGHT);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (start, _) = layout.tab_ranges[0];
        assert_eq!(
            hit_test(&ws, 0, area, start, 1),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(hit_test(&ws, 0, area, start, 0), None);
        assert_eq!(hit_test(&ws, 0, area, start, 2), None);
    }

    #[test]
    fn test_many_workspaces_active_is_last() {
        let ws: Vec<&str> = (0..10).map(|_| "workspace_long").collect();
        let area = Rect::new(0, 0, 50, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 9, padded);
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
        let area = Rect::new(0, 0, 60, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);

        for (i, &(start, end)) in layout.tab_ranges.iter().enumerate() {
            if start == 0 && end == 0 {
                for x in area.x..area.x + area.width {
                    assert_ne!(
                        hit_test(&ws, 0, area, x, area.y),
                        Some(WorkspaceBarClick::Tab(i)),
                        "skipped tab {} should not be clickable at x={}",
                        i,
                        x
                    );
                }
            }
        }
    }

    #[test]
    fn test_single_char_names() {
        let ws = vec!["1", "2", "3", "4", "5"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
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
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);

        let (start0, end0) = layout.tab_ranges[0];
        assert_eq!(
            hit_test(&ws, 0, area, start0, area.y),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(
            hit_test(&ws, 0, area, end0 - 1, area.y),
            Some(WorkspaceBarClick::Tab(0))
        );

        let (start1, _) = layout.tab_ranges[1];
        assert_eq!(
            hit_test(&ws, 0, area, start1, area.y),
            Some(WorkspaceBarClick::Tab(1))
        );
    }

    #[test]
    fn test_many_single_char_names_narrow_area() {
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
        assert!(visible < 20, "not all 20 tabs should fit in 30 cols");
        assert!(visible > 0, "at least some tabs should be visible");
    }

    #[test]
    fn test_truncate_name_max_3() {
        // max < 4: truncate without ellipsis
        let result = truncate_name("abcdef", 3);
        assert_eq!(result, "abc");
    }

    #[test]
    fn test_truncate_name_unicode() {
        let result = truncate_name("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(result, "abcdefg...");
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_truncate_single_char_name() {
        assert_eq!(truncate_name("x", 20), "x");
        assert_eq!(truncate_name("x", 1), "x");
    }

    #[test]
    fn test_padded_tab_area_zero_width() {
        let area = Rect::new(0, 0, 0, 0);
        let result = padded_tab_area(area);
        assert_eq!(result, Rect::new(0, 0, 0, 0));
    }

    #[test]
    fn test_padded_tab_area_minimum_bordered() {
        let area = Rect::new(0, 0, 3, 3);
        let result = padded_tab_area(area);
        assert!(result.width == 0 || result.width <= 1);
    }

    #[test]
    fn test_truncate_name_emoji() {
        // Each emoji is 1 char but multi-byte in UTF-8
        let name = "🔥🔥🔥🔥🔥🔥";
        assert_eq!(truncate_name(name, 6), "🔥🔥🔥🔥🔥🔥");
        assert_eq!(truncate_name(name, 5), "🔥🔥...");
        assert_eq!(truncate_name(name, 4), "🔥...");
    }

    #[test]
    fn test_truncate_name_cjk() {
        let name = "日本語のワークスペース";
        // 11 chars
        assert_eq!(truncate_name(name, 11), name);
        let result = truncate_name(name, 8);
        assert_eq!(result, "日本語のワ...");
        assert_eq!(result.chars().count(), 8);
    }

    #[test]
    fn test_truncate_name_max_less_than_3() {
        assert_eq!(truncate_name("hello", 2), "he");
        assert_eq!(truncate_name("hello", 1), "h");
    }

    #[test]
    fn test_truncate_name_max_zero() {
        assert_eq!(truncate_name("hello", 0), "");
        assert_eq!(truncate_name("", 0), "");
    }

    #[test]
    fn test_truncate_name_ascii_exact() {
        assert_eq!(truncate_name("abcde", 5), "abcde");
        assert_eq!(truncate_name("abcdef", 5), "ab...");
    }

    #[test]
    fn test_truncate_name_mixed_multibyte() {
        let name = "a🔥b🔥c🔥d";
        // 7 chars
        assert_eq!(truncate_name(name, 7), name);
        let result = truncate_name(name, 6);
        assert_eq!(result.chars().count(), 6);
        assert_eq!(result, "a🔥b...");
    }
}
