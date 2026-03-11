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
    /// (start_x, end_x) for each tab's full span (inclusive of padding)
    tab_ranges: Vec<(u16, u16)>,
    /// (start_x, end_x) for the + button
    plus_range: Option<(u16, u16)>,
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

fn compute_layout(names: &[&str], active_idx: usize, area: Rect) -> TabLayout {
    let mut tab_ranges: Vec<(u16, u16)> = Vec::new();
    let max_x = area.x + area.width;
    let plus_reserve = 3u16; // " + "
    let sep_width = 3u16; // " · " — 3 display columns
    let mut cursor_x = area.x;

    for (i, name) in names.iter().enumerate() {
        let display_name = truncate_name(name, 20);
        let label = format!(" {} ", display_name);
        let label_width = label.len() as u16;

        // Check if this tab fits (reserve space for + button on the right)
        if cursor_x + label_width + plus_reserve > max_x && i != active_idx {
            tab_ranges.push((0, 0));
            continue;
        }

        // Separator before non-first tabs
        if i > 0 {
            cursor_x += sep_width;
        }

        let tab_start = cursor_x;
        cursor_x += label_width;
        tab_ranges.push((tab_start, cursor_x));
    }

    // Right-align the + button
    let plus_range = if plus_reserve <= max_x.saturating_sub(area.x) {
        let plus_start = max_x - plus_reserve;
        Some((plus_start, max_x))
    } else {
        None
    };

    TabLayout {
        tab_ranges,
        plus_range,
    }
}

pub fn render(
    workspace_names: &[&str],
    active_idx: usize,
    theme: &Theme,
    focused: bool,
    frame: &mut Frame,
    area: Rect,
) {
    let border_color = if focused {
        theme.border_normal
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

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut first = true;
    let mut content_end = tab_area.x;

    for (i, name) in workspace_names.iter().enumerate() {
        if i >= layout.tab_ranges.len() {
            break;
        }
        let (start, end) = layout.tab_ranges[i];
        if start == 0 && end == 0 {
            continue;
        }

        // Separator before non-first visible tabs
        if !first {
            spans.push(Span::styled(sep, Style::default().fg(theme.dim)));
        }
        first = false;

        let is_active = i == active_idx;
        let display_name = truncate_name(name, 20);
        let label = format!(" {} ", display_name);

        let style = if is_active {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.dim)
        };

        spans.push(Span::styled(label, style));
        content_end = end;
    }

    // Right-align the + button with padding
    if let Some((plus_start, _)) = layout.plus_range {
        let gap = plus_start.saturating_sub(content_end);
        if gap > 0 {
            spans.push(Span::raw(" ".repeat(gap as usize)));
        }
        spans.push(Span::styled(" + ", Style::default().fg(theme.accent)));
    }

    let line = Line::from(spans);
    frame.render_widget(block, area);
    if tab_area.width > 0 && tab_area.height > 0 {
        frame.render_widget(Paragraph::new(line), tab_area);
    }
}

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

    // Note: padded_tab_area insets by 1 on each side, so for a non-bordered
    // area Rect(0,0,80,1) the effective area is Rect(1,0,78,1).
    // Tabs start at x=1, + button ends at x=79 (i.e. 1+78).

    #[test]
    fn test_hit_test_tab_click() {
        let ws: Vec<&str> = vec!["alpha", "beta"];
        let area = Rect::new(0, 0, 80, 1);
        // First tab " alpha " starts at x=1 (after padding)
        let click = hit_test(&ws, 0, area, 2, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_plus_button() {
        let ws: Vec<&str> = vec!["a"];
        let area = Rect::new(0, 0, 80, 1);
        // Padded area is [1, 79), + button at end: starts at 79-3=76
        let click = hit_test(&ws, 0, area, 76, 0);
        assert_eq!(click, Some(WorkspaceBarClick::NewWorkspace));
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
    fn test_plus_button_present() {
        let ws = vec!["a"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        assert!(layout.plus_range.is_some());
    }

    // --- Hit test at exact boundary positions ---

    #[test]
    fn test_hit_test_at_tab_start() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        // " alpha " starts at x=1 (padded area starts at 1)
        let click = hit_test(&ws, 0, area, 1, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_tab_end_exclusive() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        let (_, end) = layout.tab_ranges[0];
        // " alpha " = 7 chars, starts at 1, so end = 8
        assert_eq!(end, 8);
        let click = hit_test(&ws, 0, area, 8, 0);
        // x=8 is outside the tab
        assert_ne!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_last_pixel_of_tab() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        // " alpha " = 7 chars starting at x=1, last pixel is x=7
        let click = hit_test(&ws, 0, area, 7, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_second_tab_boundary() {
        let ws = vec!["ab", "cd"];
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);
        // " ab " = 4 chars [1, 5), then " · " (3 chars), then " cd " starts at 8
        let (start, end) = layout.tab_ranges[1];
        assert_eq!(start, 8);
        assert_eq!(end, 12);
        // Click at start of second tab
        assert_eq!(
            hit_test(&ws, 0, area, 8, 0),
            Some(WorkspaceBarClick::Tab(1))
        );
        // Click at end-1 of second tab
        assert_eq!(
            hit_test(&ws, 0, area, 11, 0),
            Some(WorkspaceBarClick::Tab(1))
        );
        // Click at end is outside
        assert_ne!(
            hit_test(&ws, 0, area, 12, 0),
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
        let area = Rect::new(10, 5, 80, 1);
        // Padded area starts at x=11. " ab " = 4 chars → [11, 15)
        assert_eq!(
            hit_test(&ws, 0, area, 11, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(
            hit_test(&ws, 0, area, 14, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        // Outside area y
        assert_eq!(hit_test(&ws, 0, area, 11, 4), None);
        assert_eq!(hit_test(&ws, 0, area, 11, 6), None);
        // Before padded area x — should be None
        assert_eq!(hit_test(&ws, 0, area, 10, 5), None);
    }

    #[test]
    fn test_hit_test_bordered_bar_uses_inner_row() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, HEIGHT);
        // Bordered: inner starts at x=1, then +1 padding = x=2. Inner row y=1.
        assert_eq!(
            hit_test(&ws, 0, area, 2, 1),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(hit_test(&ws, 0, area, 2, 0), None);
        assert_eq!(hit_test(&ws, 0, area, 2, 2), None);
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
        let area = Rect::new(0, 0, 40, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);

        // For any skipped tab, hit_test should not return it
        for (i, &(start, end)) in layout.tab_ranges.iter().enumerate() {
            if start == 0 && end == 0 {
                // This tab is skipped; no x position should resolve to it
                for x in 0..40 {
                    assert_ne!(
                        hit_test(&ws, 0, area, x, 0),
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
        let area = Rect::new(0, 0, 80, 1);
        let padded = padded_tab_area(area);
        let layout = compute_layout(&ws, 0, padded);

        // Each " X " = 3 chars, separators " · " = 3 chars
        // First tab: [1, 4)
        assert_eq!(layout.tab_ranges[0], (1, 4));
        assert_eq!(
            hit_test(&ws, 0, area, 1, 0),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(
            hit_test(&ws, 0, area, 3, 0),
            Some(WorkspaceBarClick::Tab(0))
        );
        // Gap between tabs (separator at 4,5,6)
        assert_ne!(
            hit_test(&ws, 0, area, 5, 0),
            Some(WorkspaceBarClick::Tab(0))
        );
        // Second tab: starts at 7 = 4 + 3 (sep)
        assert_eq!(layout.tab_ranges[1], (7, 10));
        assert_eq!(
            hit_test(&ws, 0, area, 7, 0),
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
}
