use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
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
) -> TabLayout {
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
    workspace_names: &[String],
    active_idx: usize,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let layout = compute_layout(workspace_names, active_idx, area);
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

    let layout = compute_layout(workspace_names, active_idx, area);

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
        // First tab: " alpha " starts at x=0, 7 chars wide → [0, 7)
        let click = hit_test(&ws, 0, area, 1, 0);
        assert_eq!(click, Some(WorkspaceBarClick::Tab(0)));
    }

    #[test]
    fn test_hit_test_plus_button() {
        let ws = names(&["a"]);
        let area = Rect::new(0, 0, 80, 1);
        // " a " (3 chars) + " · " (3 chars) + " + " starts at x=6
        let click = hit_test(&ws, 0, area, 6, 0);
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
        let layout = compute_layout(&ws, 0, area);
        assert!(layout.plus_range.is_some());
    }
}
