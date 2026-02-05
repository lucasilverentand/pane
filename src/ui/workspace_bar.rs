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
    NewWorkspace,
}

struct TabLayout {
    /// (start_x, end_x) for each tab's full span (inclusive of padding)
    tab_ranges: Vec<(u16, u16)>,
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
    area: Rect,
) -> (Vec<Span<'static>>, TabLayout) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut tab_ranges: Vec<(u16, u16)> = Vec::new();
    let max_x = area.x + area.width;
    let plus_reserve = 3u16; // " + "
    let mut cursor_x = area.x;

    // Leading space
    spans.push(Span::raw(" "));
    cursor_x += 1;

    for (i, name) in names.iter().enumerate() {
        let display_name = truncate_name(name, 20);

        let tab_text = format!(" {} ", display_name);
        let tab_width = tab_text.len() as u16;

        // Check if this tab fits (reserve space for +)
        if cursor_x + tab_width + plus_reserve > max_x && i != active_idx {
            // Skip tabs that don't fit (but always show active)
            tab_ranges.push((0, 0));
            continue;
        }

        let tab_start = cursor_x;
        let tab_end = cursor_x + tab_width;

        tab_ranges.push((tab_start, tab_end));
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
            plus_range,
        },
    )
}

pub fn render(
    workspace_names: &[String],
    active_idx: usize,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let (raw_spans, layout) = compute_layout(workspace_names, active_idx, area);

    // Re-style each span based on tab index, with rounded end caps
    let mut styled_spans: Vec<Span<'static>> = Vec::new();
    let mut span_idx = 0;

    // Leading space
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
            continue;
        }

        let is_active = i == active_idx;
        let display_name = truncate_name(&workspace_names[i], 20);

        let bg_color = if is_active {
            theme.workspace_tab_active_bg
        } else {
            theme.workspace_tab_inactive_bg
        };

        // Left rounded cap: fg=tab_bg on transparent bg
        styled_spans.push(Span::styled(
            "\u{E0B6}",
            Style::default().fg(bg_color),
        ));

        // Tab content
        let content_style = if is_active {
            Style::default()
                .fg(Color::Black)
                .bg(bg_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.dim)
                .bg(bg_color)
        };

        styled_spans.push(Span::styled(display_name, content_style));

        // Right rounded cap: fg=tab_bg on transparent bg
        styled_spans.push(Span::styled(
            "\u{E0B4}",
            Style::default().fg(bg_color),
        ));

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
    area: Rect,
    x: u16,
    y: u16,
) -> Option<WorkspaceBarClick> {
    if y < area.y || y >= area.y + area.height {
        return None;
    }

    let (_spans, layout) = compute_layout(workspace_names, active_idx, area);

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

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_hit_test_tab_click() {
        let ws = names(&["alpha", "beta"]);
        let area = Rect::new(0, 0, 80, 1);
        let click = hit_test(&ws, 0, area, 2, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_plus_button() {
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        // " " + " a " (4 chars) → cursor at 5, then + at [5, 8)
        let click = hit_test(&ws, 0, area, 5, 0);
        assert_eq!(click, Some(WorkspaceBarClick::NewWorkspace));
    }

    #[test]
    fn test_hit_test_outside() {
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        let click = hit_test(&ws, 0, area, 70, 0);
        assert_eq!(click, None);
    }

    #[test]
    fn test_hit_test_wrong_row() {
        let ws = names(&["a"]);
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
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        let (_spans, layout) = compute_layout(&ws, 0, area);
        assert!(layout.plus_range.is_some());
    }
}
