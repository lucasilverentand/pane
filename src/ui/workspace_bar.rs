use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::config::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBarClick {
    Tab(usize),
    CloseTab(usize),
    NewWorkspace,
}

struct TabLayout {
    /// (start_x, end_x) for each tab's full span (inclusive of padding)
    tab_ranges: Vec<(u16, u16)>,
    /// (start_x, end_x) for each tab's × close button (if shown)
    close_ranges: Vec<Option<(u16, u16)>>,
    /// (start_x, end_x) for the + button
    plus_range: Option<(u16, u16)>,
}

fn truncate_name(name: &str, max: usize) -> String {
    if name.len() <= max {
        name.to_string()
    } else {
        format!("{}...", &name[..max - 3])
    }
}

fn compute_layout(
    names: &[String],
    active_idx: usize,
    hovered_tab: Option<usize>,
    area: Rect,
) -> (Vec<Span<'static>>, TabLayout) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut tab_ranges: Vec<(u16, u16)> = Vec::new();
    let mut close_ranges: Vec<Option<(u16, u16)>> = Vec::new();
    let max_x = area.x + area.width;
    let plus_reserve = 3u16; // " + "
    let mut cursor_x = area.x;
    let only_one = names.len() <= 1;

    // Leading space
    spans.push(Span::raw(" "));
    cursor_x += 1;

    for (i, name) in names.iter().enumerate() {
        let display_name = truncate_name(name, 20);
        let is_active = i == active_idx;
        let is_hovered = hovered_tab == Some(i);
        let show_close = !only_one && (is_active || is_hovered);

        // Tab content: " name × " or " name "
        let tab_text = if show_close {
            format!(" {} × ", display_name)
        } else {
            format!(" {} ", display_name)
        };

        let tab_width = tab_text.len() as u16;

        // Check if this tab fits (reserve space for +)
        if cursor_x + tab_width + plus_reserve > max_x && i != active_idx {
            // Skip tabs that don't fit (but always show active)
            tab_ranges.push((0, 0));
            close_ranges.push(None);
            continue;
        }

        let tab_start = cursor_x;
        let tab_end = cursor_x + tab_width;

        // Close button range: the "× " part is at the end, 2 chars before trailing space
        let close_range = if show_close {
            // The × is at tab_end - 3 (the × char), tab_end - 2 (the space after ×)
            Some((tab_end - 3, tab_end - 1))
        } else {
            None
        };

        tab_ranges.push((tab_start, tab_end));
        close_ranges.push(close_range);

        spans.push(Span::styled(tab_text, Style::default()));

        cursor_x = tab_end;

        // 1-char gap between tabs
        if i + 1 < names.len() {
            spans.push(Span::raw(" "));
            cursor_x += 1;
        }
    }

    // + button
    let plus_range = if cursor_x + plus_reserve <= max_x {
        let start = cursor_x;
        spans.push(Span::styled(" + ", Style::default()));
        Some((start, start + plus_reserve))
    } else {
        None
    };

    (
        spans,
        TabLayout {
            tab_ranges,
            close_ranges,
            plus_range,
        },
    )
}

pub fn render(
    workspace_names: &[String],
    active_idx: usize,
    hovered_tab: Option<usize>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let (raw_spans, layout) = compute_layout(workspace_names, active_idx, hovered_tab, area);
    let only_one = workspace_names.len() <= 1;

    // Re-style each span based on tab index
    let mut styled_spans: Vec<Span<'static>> = Vec::new();
    let mut span_idx = 0;

    // Leading space - style with reset bg
    if !raw_spans.is_empty() {
        styled_spans.push(Span::styled(" ", Style::default()));
        span_idx += 1;
    }

    for (i, _name) in workspace_names.iter().enumerate() {
        if i >= layout.tab_ranges.len() {
            break;
        }
        let (start, end) = layout.tab_ranges[i];
        if start == 0 && end == 0 {
            // Skipped tab (didn't fit)
            continue;
        }

        let is_active = i == active_idx;
        let is_hovered = hovered_tab == Some(i);
        let show_close = !only_one && (is_active || is_hovered);

        let display_name = truncate_name(&workspace_names[i], 20);

        if is_active {
            let style = Style::default()
                .fg(Color::Black)
                .bg(theme.workspace_tab_active_bg)
                .add_modifier(Modifier::BOLD);
            if show_close {
                styled_spans.push(Span::styled(format!(" {} × ", display_name), style));
            } else {
                styled_spans.push(Span::styled(format!(" {} ", display_name), style));
            }
        } else {
            let style = Style::default()
                .fg(theme.dim)
                .bg(theme.workspace_tab_inactive_bg);
            if show_close {
                styled_spans.push(Span::styled(format!(" {} × ", display_name), style));
            } else {
                styled_spans.push(Span::styled(format!(" {} ", display_name), style));
            }
        }
        span_idx += 1;

        // Gap span
        if i + 1 < workspace_names.len() && span_idx < raw_spans.len() {
            styled_spans.push(Span::styled(" ", Style::default()));
            span_idx += 1;
        }
    }

    // + button
    if layout.plus_range.is_some() {
        styled_spans.push(Span::styled(
            " + ",
            Style::default().fg(theme.accent),
        ));
    }

    let line = Line::from(styled_spans);
    frame.render_widget(Paragraph::new(line), area);
}

pub fn hit_test(
    workspace_names: &[String],
    active_idx: usize,
    hovered_tab: Option<usize>,
    area: Rect,
    x: u16,
    y: u16,
) -> Option<WorkspaceBarClick> {
    if y < area.y || y >= area.y + area.height {
        return None;
    }

    let (_spans, layout) = compute_layout(workspace_names, active_idx, hovered_tab, area);

    // Check + button first
    if let Some((start, end)) = layout.plus_range {
        if x >= start && x < end {
            return Some(WorkspaceBarClick::NewWorkspace);
        }
    }

    // Check close buttons (priority over tab click)
    for (i, close_range) in layout.close_ranges.iter().enumerate() {
        if let Some((start, end)) = close_range {
            if x >= *start && x < *end {
                return Some(WorkspaceBarClick::CloseTab(i));
            }
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

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_hit_test_tab_click() {
        let ws = names(&["alpha", "beta"]);
        let area = Rect::new(0, 0, 80, 1);
        // Tab 0 starts at x=1 (" " prefix), content " alpha × " = 9 chars → range [1, 10)
        let click = hit_test(&ws, 0, None, area, 2, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_close_active() {
        let ws = names(&["alpha", "beta"]);
        let area = Rect::new(0, 0, 80, 1);
        // Active tab 0: " alpha × " → close at positions 7,8 from tab start
        // Tab starts at x=1, × at x=8 (1 + 7), range [8, 10)
        // Actually: " alpha × " is 10 chars. tab_end = 1+10=11. close = (11-3, 11-1) = (8, 10)
        let click = hit_test(&ws, 0, None, area, 8, 0);
        assert_eq!(click, Some(WorkspaceBarClick::CloseTab(0)));
    }

    #[test]
    fn test_hit_test_plus_button() {
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        // " " + " a " (4 chars) → cursor at 5, then + at [5, 8)
        let click = hit_test(&ws, 0, None, area, 5, 0);
        assert_eq!(click, Some(WorkspaceBarClick::NewWorkspace));
    }

    #[test]
    fn test_hit_test_outside() {
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        let click = hit_test(&ws, 0, None, area, 70, 0);
        assert_eq!(click, None);
    }

    #[test]
    fn test_hit_test_wrong_row() {
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        let click = hit_test(&ws, 0, None, area, 2, 1);
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
    fn test_no_close_on_single_workspace() {
        let ws = names(&["only"]);
        let area = Rect::new(0, 0, 80, 1);
        let (_spans, layout) = compute_layout(&ws, 0, None, area);
        // Single workspace: no close button
        assert_eq!(layout.close_ranges[0], None);
    }

    #[test]
    fn test_close_shown_on_active_with_multiple() {
        let ws = names(&["one", "two"]);
        let area = Rect::new(0, 0, 80, 1);
        let (_spans, layout) = compute_layout(&ws, 0, None, area);
        // Active tab should have close
        assert!(layout.close_ranges[0].is_some());
        // Inactive (not hovered) should not
        assert!(layout.close_ranges[1].is_none());
    }

    #[test]
    fn test_close_shown_on_hovered_inactive() {
        let ws = names(&["one", "two"]);
        let area = Rect::new(0, 0, 80, 1);
        let (_spans, layout) = compute_layout(&ws, 0, Some(1), area);
        // Hovered inactive tab should have close
        assert!(layout.close_ranges[1].is_some());
    }

    #[test]
    fn test_plus_button_present() {
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        let (_spans, layout) = compute_layout(&ws, 0, None, area);
        assert!(layout.plus_range.is_some());
    }
}
