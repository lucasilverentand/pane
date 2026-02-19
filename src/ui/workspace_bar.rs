use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::config::Theme;

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

fn tab_line_area(area: Rect) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }

    // For bordered bars (HEIGHT=3), render/hit-test on the inner text row.
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

    Rect::new(x, y, width, 1)
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

        // Check if this tab fits (reserve space for + button)
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

    // + button
    let plus_range = if cursor_x + sep_width + plus_reserve <= max_x {
        cursor_x += sep_width;
        let start = cursor_x;
        cursor_x += plus_reserve;
        Some((start, cursor_x))
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
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_inactive));
    let tab_area = tab_line_area(area);
    let layout = compute_layout(workspace_names, active_idx, tab_area);
    let sep = " \u{B7} "; // " · "

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut first = true;

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
    }

    // + button
    if layout.plus_range.is_some() {
        spans.push(Span::styled(sep, Style::default().fg(theme.dim)));
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
    let tab_area = tab_line_area(area);

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

    #[test]
    fn test_hit_test_tab_click() {
        let ws: Vec<&str> = vec!["alpha", "beta"];
        let area = Rect::new(0, 0, 80, 1);
        // First tab: " alpha " starts at x=0, 7 chars wide → [0, 7)
        let click = hit_test(&ws, 0, area, 1, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_plus_button() {
        let ws: Vec<&str> = vec!["a"];
        let area = Rect::new(0, 0, 80, 1);
        // " a " (3 chars) + " · " (3 chars) + " + " starts at x=6
        let click = hit_test(&ws, 0, area, 6, 0);
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
        let layout = compute_layout(&ws, 0, area);
        assert!(layout.plus_range.is_some());
    }

    // --- Hit test at exact boundary positions ---

    #[test]
    fn test_hit_test_at_tab_start() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        // " alpha " starts at x=0
        let click = hit_test(&ws, 0, area, 0, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_tab_end_exclusive() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        // " alpha " is 7 chars wide → range [0, 7), so x=7 is outside
        let layout = compute_layout(&ws, 0, area);
        let (_, end) = layout.tab_ranges[0];
        assert_eq!(end, 7);
        let click = hit_test(&ws, 0, area, 7, 0);
        // x=7 is in the separator or gap, not the tab
        assert_ne!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_at_last_pixel_of_tab() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, 1);
        // range is [0, 7), x=6 is the last pixel inside the tab
        let click = hit_test(&ws, 0, area, 6, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_second_tab_boundary() {
        let ws = vec!["ab", "cd"];
        let area = Rect::new(0, 0, 80, 1);
        let layout = compute_layout(&ws, 0, area);
        // " ab " = 4 chars [0, 4), then " · " (3 chars), then " cd " starts at 7
        let (start, end) = layout.tab_ranges[1];
        assert_eq!(start, 7);
        assert_eq!(end, 11);
        // Click at start of second tab
        assert_eq!(
            hit_test(&ws, 0, area, 7, 0),
            Some(WorkspaceBarClick::Tab(1))
        );
        // Click at end-1 of second tab
        assert_eq!(
            hit_test(&ws, 0, area, 10, 0),
            Some(WorkspaceBarClick::Tab(1))
        );
        // Click at end is outside
        assert_ne!(
            hit_test(&ws, 0, area, 11, 0),
            Some(WorkspaceBarClick::Tab(1))
        );
    }

    // --- Many workspaces (overflow behavior) ---

    #[test]
    fn test_many_workspaces_overflow() {
        // Create many workspaces that don't all fit in 40 cols
        let ws = vec![
            "workspace_one",
            "workspace_two",
            "workspace_three",
            "workspace_four",
            "workspace_five",
        ];
        let area = Rect::new(0, 0, 40, 1);
        let layout = compute_layout(&ws, 0, area);
        // Some tabs should be skipped (0,0 ranges)
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
        // Active tab should always appear even if space is tight
        let ws = vec![
            "workspace_one",
            "workspace_two",
            "active_workspace",
            "workspace_four",
        ];
        let area = Rect::new(0, 0, 40, 1);
        let layout = compute_layout(&ws, 2, area);
        // The active tab (index 2) should not be skipped
        let (start, end) = layout.tab_ranges[2];
        assert!(start != 0 || end != 0, "active tab should be visible");
    }

    // --- Name truncation ---

    #[test]
    fn test_truncate_name_exact_max() {
        // Exactly at max length — no truncation
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
        let layout = compute_layout(&ws, 0, area);
        // " " (empty name with padding) = 2 chars
        let (start, end) = layout.tab_ranges[0];
        assert_eq!(start, 0);
        assert_eq!(end, 2); // " " + "" + " " = 2 chars
        assert!(layout.plus_range.is_some());
    }

    #[test]
    fn test_compute_layout_all_empty_names() {
        let ws = vec!["", "", ""];
        let area = Rect::new(0, 0, 80, 1);
        let layout = compute_layout(&ws, 0, area);
        // All tabs should be visible since they're tiny
        for (start, end) in &layout.tab_ranges {
            assert!(*start != 0 || *end != 0, "all tiny tabs should fit");
        }
    }

    #[test]
    fn test_compute_layout_no_workspaces() {
        let ws: Vec<&str> = vec![];
        let area = Rect::new(0, 0, 80, 1);
        let layout = compute_layout(&ws, 0, area);
        assert!(layout.tab_ranges.is_empty());
        assert!(layout.plus_range.is_some());
    }

    #[test]
    fn test_hit_test_with_area_offset() {
        // Area starts at x=10, y=5
        let ws = vec!["ab"];
        let area = Rect::new(10, 5, 80, 1);
        // " ab " = 4 chars, tab at [10, 14)
        assert_eq!(
            hit_test(&ws, 0, area, 10, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(
            hit_test(&ws, 0, area, 13, 5),
            Some(WorkspaceBarClick::Tab(0))
        );
        // Outside area y
        assert_eq!(hit_test(&ws, 0, area, 10, 4), None);
        assert_eq!(hit_test(&ws, 0, area, 10, 6), None);
        // Before area x — should be None
        assert_eq!(hit_test(&ws, 0, area, 9, 5), None);
    }

    #[test]
    fn test_hit_test_bordered_bar_uses_inner_row() {
        let ws = vec!["alpha"];
        let area = Rect::new(0, 0, 80, HEIGHT);
        assert_eq!(
            hit_test(&ws, 0, area, 1, 1),
            Some(WorkspaceBarClick::Tab(0))
        );
        assert_eq!(hit_test(&ws, 0, area, 1, 0), None);
        assert_eq!(hit_test(&ws, 0, area, 1, 2), None);
    }
}
