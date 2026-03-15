use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::client::ProjectHubState;
use pane_protocol::config::Theme;

const SIDEBAR_MIN_WIDTH: u16 = 28;
const SIDEBAR_MAX_WIDTH: u16 = 40;

/// Render the project hub as a full workspace body: left sidebar + right detail panel.
pub fn render_body(
    state: &ProjectHubState,
    theme: &Theme,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    if area.height < 4 || area.width < 20 {
        return;
    }

    // Split into left sidebar and right detail panel
    let sidebar_width = (area.width / 3).clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH).min(area.width);
    let [left, right] = Layout::horizontal([
        Constraint::Length(sidebar_width),
        Constraint::Fill(1),
    ])
    .areas(area);

    render_sidebar(state, theme, hover, frame, left);
    render_detail(state, theme, frame, right);
}

fn render_sidebar(
    state: &ProjectHubState,
    theme: &Theme,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " projects ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 6 {
        return;
    }

    let mut row = inner.y;

    // Search input
    let has_filter = !state.input.is_empty();
    let input_line = if has_filter {
        Line::from(vec![
            Span::styled(" > ", Style::default().fg(theme.accent)),
            Span::styled(&state.input, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" > ", Style::default().fg(theme.accent)),
            Span::styled("search…", Style::default().fg(theme.dim)),
        ])
    };
    frame.render_widget(
        Paragraph::new(input_line),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // Separator
    let sep_line = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            sep_line,
            Style::default().fg(theme.border_inactive),
        ))),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // Project list
    let list_height = inner.y + inner.height - row;
    if list_height == 0 {
        return;
    }
    let list_height = list_height as usize;

    let total = state.filtered.len();
    let selected = state.selected;
    let dim_style = Style::default().fg(theme.dim);
    let selected_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let name_style = Style::default().fg(theme.fg);
    let max_w = inner.width as usize;

    if total == 0 {
        let msg = if has_filter {
            "  (no matches)"
        } else {
            "  (no projects)"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, dim_style))),
            Rect::new(inner.x, row, inner.width, 1),
        );
    } else {
        let mut scroll = state.scroll_offset;
        if selected >= scroll + list_height {
            scroll = selected + 1 - list_height;
        }
        if selected < scroll {
            scroll = selected;
        }

        for (visual, fi) in state.filtered.iter().enumerate() {
            if visual < scroll {
                continue;
            }
            if visual >= scroll + list_height {
                break;
            }

            let project = &state.all_projects[*fi];
            let is_selected = visual == selected;
            let is_hovered = hover
                .map(|(hx, hy)| {
                    let line_y = row + (visual - scroll) as u16;
                    hy == line_y && hx >= inner.x && hx < inner.x + inner.width
                })
                .unwrap_or(false);

            let prefix = if is_selected { " > " } else { "   " };
            let name_part = format!("{}{}", prefix, project.name);
            let display = if name_part.len() > max_w {
                format!("{}…", &name_part[..max_w - 1])
            } else {
                name_part
            };

            let style = if is_selected || is_hovered {
                selected_style
            } else {
                name_style
            };

            let line_y = row + (visual - scroll) as u16;
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(display, style))),
                Rect::new(inner.x, line_y, inner.width, 1),
            );
        }
    }
}

fn render_detail(
    state: &ProjectHubState,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_inactive));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let project = match state.selected_project() {
        Some(p) => p,
        None => {
            let msg = Line::from(Span::styled(
                "  select a project",
                Style::default().fg(theme.dim),
            ));
            frame.render_widget(
                Paragraph::new(msg),
                Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1),
            );
            return;
        }
    };

    let home_dir = std::env::var("HOME").unwrap_or_default();
    let dim_style = Style::default().fg(theme.dim);
    let accent_style = Style::default().fg(theme.accent);
    let bold_style = Style::default()
        .fg(theme.fg)
        .add_modifier(Modifier::BOLD);
    let fg_style = Style::default().fg(theme.fg);
    let max_w = inner.width as usize;

    let mut row = inner.y;
    let pad = " ";

    // ── Project name (large) ──
    let name_line = Line::from(vec![
        Span::raw(pad),
        Span::styled(&project.name, bold_style),
    ]);
    frame.render_widget(
        Paragraph::new(name_line),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // Path
    let path_display = {
        let p = project.path.to_string_lossy();
        if !home_dir.is_empty() && p.starts_with(&home_dir) {
            format!("~{}", &p[home_dir.len()..])
        } else {
            p.to_string()
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(pad),
            Span::styled(&path_display, dim_style),
        ])),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 2;

    // ── Git info ──
    let git_info = match state.selected_git_info() {
        Some(info) => info,
        None => return,
    };

    if git_info.branch.is_empty() {
        return;
    }

    // Branch + sync status
    let mut branch_spans = vec![
        Span::raw(pad),
        Span::styled("branch ", dim_style),
        Span::styled(&git_info.branch, accent_style),
    ];
    if git_info.ahead > 0 || git_info.behind > 0 {
        let mut sync_parts = Vec::new();
        if git_info.ahead > 0 {
            sync_parts.push(format!("{}↑", git_info.ahead));
        }
        if git_info.behind > 0 {
            sync_parts.push(format!("{}↓", git_info.behind));
        }
        branch_spans.push(Span::styled(
            format!("  {}", sync_parts.join(" ")),
            Style::default().fg(Color::Yellow),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(branch_spans)),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // Working tree status summary
    let mut status_parts: Vec<Span> = vec![Span::raw(pad)];
    let has_changes = git_info.staged_count > 0
        || git_info.dirty_count > 0
        || git_info.untracked_count > 0;
    if has_changes {
        if git_info.staged_count > 0 {
            status_parts.push(Span::styled(
                format!("{} staged", git_info.staged_count),
                Style::default().fg(Color::Green),
            ));
            status_parts.push(Span::styled("  ", dim_style));
        }
        if git_info.dirty_count > 0 {
            status_parts.push(Span::styled(
                format!("{} modified", git_info.dirty_count),
                Style::default().fg(Color::Yellow),
            ));
            status_parts.push(Span::styled("  ", dim_style));
        }
        if git_info.untracked_count > 0 {
            status_parts.push(Span::styled(
                format!("{} untracked", git_info.untracked_count),
                Style::default().fg(Color::Red),
            ));
        }
    } else {
        status_parts.push(Span::styled("clean", Style::default().fg(Color::Green)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(status_parts)),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 2;

    // ── Recent commits ──
    if git_info.commits.is_empty() {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(pad),
            Span::styled("recent commits", dim_style),
        ])),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // Separator under heading
    let sep = "─".repeat((inner.width as usize).saturating_sub(2));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(pad),
            Span::styled(sep, Style::default().fg(theme.border_inactive)),
        ])),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    let remaining = (inner.y + inner.height).saturating_sub(row) as usize;
    let commit_count = git_info.commits.len().min(remaining);

    for commit in git_info.commits.iter().take(commit_count) {
        if row >= inner.y + inner.height {
            break;
        }

        // hash + message + author + age on one line
        let hash_w = 8;
        let suffix = format!("{}  {}", commit.author, commit.age);
        let suffix_w = suffix.len() + 2;
        let msg_max = max_w.saturating_sub(hash_w + suffix_w + 2);
        let msg = if commit.message.len() > msg_max {
            format!("{}…", &commit.message[..msg_max.saturating_sub(1)])
        } else {
            commit.message.clone()
        };
        let msg_pad = msg_max.saturating_sub(msg.len());

        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(
                format!("{:<7}", commit.hash),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!(" {}", msg), fg_style),
            Span::raw(" ".repeat(msg_pad)),
            Span::styled(format!("{}  ", commit.author), dim_style),
            Span::styled(&commit.age, dim_style),
        ]);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, row, inner.width, 1),
        );
        row += 1;
    }

    // ── Changed files ──
    if !git_info.status_lines.is_empty() && row + 2 < inner.y + inner.height {
        row += 1; // gap
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(pad),
                Span::styled("changed files", dim_style),
            ])),
            Rect::new(inner.x, row, inner.width, 1),
        );
        row += 1;

        let sep = "─".repeat((inner.width as usize).saturating_sub(2));
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(pad),
                Span::styled(sep, Style::default().fg(theme.border_inactive)),
            ])),
            Rect::new(inner.x, row, inner.width, 1),
        );
        row += 1;

        let remaining = (inner.y + inner.height).saturating_sub(row) as usize;
        let file_count = git_info.status_lines.len().min(remaining);

        for status_line in git_info.status_lines.iter().take(file_count) {
            if row >= inner.y + inner.height {
                break;
            }
            let (indicator, file) = if status_line.len() >= 3 {
                (&status_line[..2], status_line[3..].trim())
            } else {
                (status_line.as_str(), "")
            };

            let indicator_color = match indicator.trim() {
                "M" | "MM" => Color::Yellow,
                "A" | "AM" => Color::Green,
                "D" => Color::Red,
                "R" | "RM" => Color::Cyan,
                "??" => Color::DarkGray,
                _ => Color::Yellow,
            };

            let file_display = if file.len() + 5 > max_w {
                format!("…{}", &file[file.len() + 6 - max_w..])
            } else {
                file.to_string()
            };

            let line = Line::from(vec![
                Span::raw(pad),
                Span::styled(
                    format!("{:<2}", indicator),
                    Style::default().fg(indicator_color),
                ),
                Span::styled(format!(" {}", file_display), fg_style),
            ]);
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(inner.x, row, inner.width, 1),
            );
            row += 1;
        }

        if git_info.status_lines.len() > file_count {
            if row < inner.y + inner.height {
                let more = git_info.status_lines.len() - file_count;
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw(pad),
                        Span::styled(
                            format!("  +{} more", more),
                            dim_style,
                        ),
                    ])),
                    Rect::new(inner.x, row, inner.width, 1),
                );
            }
        }
    }
}
