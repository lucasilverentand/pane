use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::client::{ProjectGitInfo, ProjectHubState, WidgetInteractState};
use pane_protocol::config::{HubLayout, HubWidget, Theme};

pub const SIDEBAR_MIN_WIDTH: u16 = 20;
pub const SIDEBAR_MAX_WIDTH: u16 = 80;
const SIDEBAR_DEFAULT_FRACTION: u16 = 4; // body.width / 4

/// Compute the sidebar width for the home workspace.
/// If `user_width` is set, clamp it to valid bounds; otherwise auto-calculate.
pub fn sidebar_width(body_width: u16, user_width: Option<u16>) -> u16 {
    match user_width {
        Some(w) => w.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH).min(body_width.saturating_sub(20)),
        None => (body_width / SIDEBAR_DEFAULT_FRACTION)
            .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH.min(40))
            .min(body_width),
    }
}

/// Hit test the project sidebar. Returns the filtered index of the project clicked.
pub fn sidebar_hit_test(
    state: &ProjectHubState,
    area: Rect,
    x: u16,
    y: u16,
    user_sidebar_width: Option<u16>,
) -> Option<usize> {
    if area.height < 4 || area.width < 20 {
        return None;
    }

    let sw = sidebar_width(area.width, user_sidebar_width);
    let sidebar_area = Rect::new(area.x, area.y, sw, area.height);

    // Compute inner area (inside border)
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(sidebar_area);

    if x < inner.x || x >= inner.x + inner.width || y < inner.y || y >= inner.y + inner.height {
        return None;
    }

    // List starts after search input (1 line) + separator (1 line)
    let list_start_y = inner.y + 2;
    if y < list_start_y {
        return None;
    }

    let list_height = (inner.y + inner.height).saturating_sub(list_start_y) as usize;
    if list_height == 0 || state.filtered.is_empty() {
        return None;
    }

    // Compute scroll offset (same logic as render_sidebar)
    let mut scroll = state.scroll_offset;
    if state.selected >= scroll + list_height {
        scroll = state.selected + 1 - list_height;
    }
    if state.selected < scroll {
        scroll = state.selected;
    }

    let visual_row = (y - list_start_y) as usize;
    let filtered_idx = scroll + visual_row;
    if filtered_idx < state.filtered.len() {
        Some(filtered_idx)
    } else {
        None
    }
}

/// Render a single widget into a given area. Used by window_view for widget tabs.
pub fn render_single_widget(
    hub_state: &ProjectHubState,
    widget: &HubWidget,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let project = match hub_state.selected_project() {
        Some(p) => p,
        None => {
            let msg = Line::from(Span::styled(
                "  select a project",
                Style::default().fg(theme.dim),
            ));
            frame.render_widget(
                Paragraph::new(msg),
                Rect::new(area.x, area.y + area.height / 2, area.width, 1),
            );
            return;
        }
    };

    let git_info = hub_state.selected_git_info();

    if git_info.is_none() && hub_state.is_loading_git_info() {
        let msg = Line::from(Span::styled(
            "  loading...",
            Style::default().fg(theme.dim),
        ));
        frame.render_widget(
            Paragraph::new(msg),
            Rect::new(area.x, area.y + area.height / 2, area.width, 1),
        );
        return;
    }

    match widget {
        HubWidget::ProjectInfo => {
            render_widget_project_info(project, git_info, is_focused, theme, frame, area);
        }
        HubWidget::RecentCommits => {
            render_widget_recent_commits(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::ChangedFiles => {
            render_widget_changed_files(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::Branches => {
            render_widget_branches(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::Stashes => {
            render_widget_stashes(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::Tags => {
            render_widget_tags(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::GitGraph => {
            render_widget_git_graph(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::Contributors => {
            render_widget_contributors(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::Todos => {
            render_widget_todos(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::Readme => {
            render_widget_readme(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::Languages => {
            render_widget_languages(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::DiskUsage => {
            render_widget_disk_usage(git_info, is_focused, theme, frame, area);
        }
        HubWidget::CiStatus => {
            render_widget_ci_status(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::OpenIssues => {
            render_widget_open_issues(git_info, is_focused, interact, theme, frame, area);
        }
        HubWidget::RunningProcesses => {
            render_widget_running_processes(git_info, is_focused, interact, theme, frame, area);
        }
    }
}

/// Return the number of selectable list items for a widget.
pub fn widget_item_count(widget: &HubWidget, git_info: Option<&ProjectGitInfo>) -> usize {
    let git = match git_info {
        Some(g) => g,
        None => return 0,
    };
    match widget {
        HubWidget::RecentCommits => git.commits.len(),
        HubWidget::ChangedFiles => git.status_lines.len(),
        HubWidget::Branches => git.branches.len(),
        HubWidget::Stashes => git.stashes.len(),
        HubWidget::Tags => git.tags.len(),
        HubWidget::GitGraph => git.graph_lines.len(),
        HubWidget::Contributors => git.contributors.len(),
        HubWidget::Todos => git.todos.len(),
        HubWidget::Readme => git.readme_lines.len(),
        HubWidget::Languages => git.languages.len(),
        HubWidget::CiStatus => git.ci_runs.len(),
        HubWidget::OpenIssues => git.gh_issues.len(),
        HubWidget::RunningProcesses => git.processes.len(),
        HubWidget::ProjectInfo | HubWidget::DiskUsage => 0,
    }
}

/// Return the text to copy for the selected item in a widget.
pub fn widget_selected_text(
    widget: &HubWidget,
    git_info: Option<&ProjectGitInfo>,
    selected: usize,
) -> Option<String> {
    let git = git_info?;
    match widget {
        HubWidget::RecentCommits => git
            .commits
            .get(selected)
            .map(|c| format!("{} {}", c.hash, c.message)),
        HubWidget::ChangedFiles => git.status_lines.get(selected).map(|s| {
            if s.len() >= 3 {
                s[3..].trim().to_string()
            } else {
                s.clone()
            }
        }),
        HubWidget::Branches => git.branches.get(selected).map(|b| b.name.clone()),
        HubWidget::Stashes => git
            .stashes
            .get(selected)
            .map(|s| format!("{} {}", s.id, s.message)),
        HubWidget::Tags => git.tags.get(selected).map(|t| t.name.clone()),
        HubWidget::GitGraph => git.graph_lines.get(selected).cloned(),
        HubWidget::Contributors => git.contributors.get(selected).map(|c| c.name.clone()),
        HubWidget::Todos => git
            .todos
            .get(selected)
            .map(|t| format!("{}:{}", t.file, t.line_num)),
        HubWidget::Readme => git.readme_lines.get(selected).cloned(),
        HubWidget::Languages => git.languages.get(selected).map(|l| l.extension.clone()),
        HubWidget::CiStatus => git.ci_runs.get(selected).map(|r| r.name.clone()),
        HubWidget::OpenIssues => git
            .gh_issues
            .get(selected)
            .map(|i| format!("#{} {}", i.number, i.title)),
        HubWidget::RunningProcesses => git.processes.get(selected).map(|p| p.command.clone()),
        HubWidget::ProjectInfo | HubWidget::DiskUsage => None,
    }
}

/// Render the project hub sidebar only (for the home workspace layout).
pub fn render_sidebar_only(
    state: &ProjectHubState,
    theme: &Theme,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    render_sidebar(state, theme, hover, frame, area);
}

/// Render the project hub as a full workspace body: left sidebar + right widget panel.
/// Kept for reference — the home workspace now uses render_single_widget per window.
#[allow(dead_code)]
pub fn render_body(
    state: &mut ProjectHubState,
    theme: &Theme,
    layout: &HubLayout,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    if area.height < 4 || area.width < 20 {
        return;
    }

    let sidebar_width = (area.width / 3)
        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
        .min(area.width);
    let [left, right] = Layout::horizontal([
        Constraint::Length(sidebar_width),
        Constraint::Fill(1),
    ])
    .areas(area);

    render_sidebar(state, theme, hover, frame, left);
    render_widgets(state, theme, layout, frame, right);
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
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
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

// ---------------------------------------------------------------------------
// Widget grid layout
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn render_widgets(
    state: &ProjectHubState,
    theme: &Theme,
    layout: &HubLayout,
    frame: &mut Frame,
    area: Rect,
) {
    if area.height < 3 || area.width < 10 {
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
                Rect::new(area.x, area.y + area.height / 2, area.width, 1),
            );
            return;
        }
    };

    let git_info = state.selected_git_info();
    let refreshing = state.is_refreshing_git_info();

    // Show loading indicator only when we have no data at all
    if git_info.is_none() && state.is_loading_git_info() {
        let msg = Line::from(Span::styled(
            "  loading...",
            Style::default().fg(theme.dim),
        ));
        frame.render_widget(
            Paragraph::new(msg),
            Rect::new(area.x, area.y + area.height / 2, area.width, 1),
        );
        return;
    }

    // Show "updating..." indicator in top-right when refreshing with stale/partial data
    if refreshing && area.width > 14 {
        let label = " updating... ";
        let label_width = label.len() as u16;
        frame.render_widget(
            Paragraph::new(Span::styled(
                label,
                Style::default().fg(theme.dim),
            )),
            Rect::new(
                area.x + area.width - label_width,
                area.y,
                label_width,
                1,
            ),
        );
    }

    // Split area vertically into rows, each getting equal space
    let row_constraints: Vec<Constraint> = layout
        .rows
        .iter()
        .map(|row| {
            // Give ProjectInfo row a smaller fixed height, others fill
            if row.len() == 1 && row[0] == HubWidget::ProjectInfo {
                // ProjectInfo needs ~6 lines + 2 border = 8
                Constraint::Length(8)
            } else {
                Constraint::Fill(1)
            }
        })
        .collect();

    let row_areas = Layout::vertical(row_constraints).split(area);

    for (row_idx, widgets) in layout.rows.iter().enumerate() {
        let row_area = row_areas[row_idx];
        if row_area.height < 3 {
            continue;
        }

        // Split row horizontally into columns
        let col_constraints: Vec<Constraint> =
            widgets.iter().map(|_| Constraint::Fill(1)).collect();
        let col_areas = Layout::horizontal(col_constraints).split(row_area);

        for (col_idx, widget) in widgets.iter().enumerate() {
            let cell = col_areas[col_idx];
            if cell.width < 6 {
                continue;
            }
            match widget {
                HubWidget::ProjectInfo => {
                    render_widget_project_info(project, git_info, false, theme, frame, cell);
                }
                HubWidget::RecentCommits => {
                    render_widget_recent_commits(git_info, false, None, theme, frame, cell);
                }
                HubWidget::ChangedFiles => {
                    render_widget_changed_files(git_info, false, None, theme, frame, cell);
                }
                HubWidget::Branches => {
                    render_widget_branches(git_info, false, None, theme, frame, cell);
                }
                HubWidget::Stashes => {
                    render_widget_stashes(git_info, false, None, theme, frame, cell);
                }
                HubWidget::Tags => {
                    render_widget_tags(git_info, false, None, theme, frame, cell);
                }
                HubWidget::GitGraph => {
                    render_widget_git_graph(git_info, false, None, theme, frame, cell);
                }
                HubWidget::Contributors => {
                    render_widget_contributors(git_info, false, None, theme, frame, cell);
                }
                HubWidget::Todos => {
                    render_widget_todos(git_info, false, None, theme, frame, cell);
                }
                HubWidget::Readme => {
                    render_widget_readme(git_info, false, None, theme, frame, cell);
                }
                HubWidget::Languages => {
                    render_widget_languages(git_info, false, None, theme, frame, cell);
                }
                HubWidget::DiskUsage => {
                    render_widget_disk_usage(git_info, false, theme, frame, cell);
                }
                HubWidget::CiStatus => {
                    render_widget_ci_status(git_info, false, None, theme, frame, cell);
                }
                HubWidget::OpenIssues => {
                    render_widget_open_issues(git_info, false, None, theme, frame, cell);
                }
                HubWidget::RunningProcesses => {
                    render_widget_running_processes(git_info, false, None, theme, frame, cell);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Individual widget renderers — each draws inside a bordered box
// ---------------------------------------------------------------------------

fn render_widget_project_info(
    project: &crate::client::ProjectEntry,
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" project ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 || inner.width < 8 {
        return;
    }

    let dim_style = Style::default().fg(theme.dim);
    let accent_style = Style::default().fg(theme.accent);
    let bold_style = Style::default()
        .fg(theme.fg)
        .add_modifier(Modifier::BOLD);
    let pad = " ";

    let home_dir = std::env::var("HOME").unwrap_or_default();
    let mut row = inner.y;

    // Project name
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(pad),
            Span::styled(&project.name, bold_style),
        ])),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // Path
    if row < inner.y + inner.height {
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
                Span::styled(path_display, dim_style),
            ])),
            Rect::new(inner.x, row, inner.width, 1),
        );
        row += 1;
    }

    // Git info
    if let Some(git) = git_info {
        if !git.branch.is_empty() && row + 1 < inner.y + inner.height {
            row += 1; // gap

            // Branch + sync status
            let mut branch_spans = vec![
                Span::raw(pad),
                Span::styled("branch ", dim_style),
                Span::styled(&git.branch, accent_style),
            ];
            if git.ahead > 0 || git.behind > 0 {
                let mut sync_parts = Vec::new();
                if git.ahead > 0 {
                    sync_parts.push(format!("{}↑", git.ahead));
                }
                if git.behind > 0 {
                    sync_parts.push(format!("{}↓", git.behind));
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
            if row < inner.y + inner.height {
                let mut status_parts: Vec<Span> = vec![Span::raw(pad)];
                let has_changes = git.staged_count > 0
                    || git.dirty_count > 0
                    || git.untracked_count > 0;
                if has_changes {
                    if git.staged_count > 0 {
                        status_parts.push(Span::styled(
                            format!("{} staged", git.staged_count),
                            Style::default().fg(Color::Green),
                        ));
                        status_parts.push(Span::styled("  ", dim_style));
                    }
                    if git.dirty_count > 0 {
                        status_parts.push(Span::styled(
                            format!("{} modified", git.dirty_count),
                            Style::default().fg(Color::Yellow),
                        ));
                        status_parts.push(Span::styled("  ", dim_style));
                    }
                    if git.untracked_count > 0 {
                        status_parts.push(Span::styled(
                            format!("{} untracked", git.untracked_count),
                            Style::default().fg(Color::Red),
                        ));
                    }
                } else {
                    status_parts
                        .push(Span::styled("clean", Style::default().fg(Color::Green)));
                }
                frame.render_widget(
                    Paragraph::new(Line::from(status_parts)),
                    Rect::new(inner.x, row, inner.width, 1),
                );
            }
        }
    }
}

fn render_widget_recent_commits(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" recent commits ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.branch.is_empty() && !g.commits.is_empty() => g,
        _ => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " no commits",
                    Style::default().fg(theme.dim),
                ))),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            return;
        }
    };

    let dim_style = Style::default().fg(theme.dim);
    let fg_style = Style::default().fg(theme.fg);
    let max_w = inner.width as usize;
    let pad = " ";

    let total = git.commits.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let commit = match git.commits.get(item_idx) {
            Some(c) => c,
            None => break,
        };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;

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
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

fn render_widget_changed_files(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" changed files ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.status_lines.is_empty() => g,
        _ => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " no changes",
                    Style::default().fg(theme.dim),
                ))),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let max_w = inner.width as usize;
    let pad = " ";

    let total = git.status_lines.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let status_line = match git.status_lines.get(item_idx) {
            Some(s) => s,
            None => break,
        };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;

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
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

// ---------------------------------------------------------------------------
// Unit 1: Branches, Stashes, Tags
// ---------------------------------------------------------------------------

fn render_widget_branches(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" branches ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.branches.is_empty() => g,
        _ => {
            render_empty_placeholder("no branches", theme, frame, inner);
            return;
        }
    };

    let dim_style = Style::default().fg(theme.dim);
    let fg_style = Style::default().fg(theme.fg);
    let accent_bold = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let pad = " ";

    let total = git.branches.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let branch = match git.branches.get(item_idx) {
            Some(b) => b,
            None => break,
        };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let name_style = if branch.is_current { accent_bold } else { fg_style };
        let prefix = if branch.is_current { "* " } else { "  " };
        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(prefix, name_style),
            Span::styled(&branch.name, name_style),
            Span::styled(format!("  {}", branch.last_commit_date), dim_style),
        ]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

fn render_widget_stashes(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" stashes ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.stashes.is_empty() => g,
        _ => {
            render_empty_placeholder("no stashes", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let pad = " ";

    let total = git.stashes.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let stash = match git.stashes.get(item_idx) { Some(s) => s, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(&stash.id, Style::default().fg(Color::Yellow)),
            Span::styled(format!(" {}", stash.message), fg_style),
        ]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

fn render_widget_tags(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" tags ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.tags.is_empty() => g,
        _ => {
            render_empty_placeholder("no tags", theme, frame, inner);
            return;
        }
    };

    let accent_style = Style::default().fg(theme.accent);
    let dim_style = Style::default().fg(theme.dim);
    let pad = " ";

    let total = git.tags.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let tag = match git.tags.get(item_idx) { Some(t) => t, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(&tag.name, accent_style),
            Span::styled(format!("  {}", tag.date), dim_style),
        ]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

// ---------------------------------------------------------------------------
// Unit 2: Git Graph, Contributors
// ---------------------------------------------------------------------------

fn render_widget_git_graph(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" git graph ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.graph_lines.is_empty() => g,
        _ => {
            render_empty_placeholder("no graph", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let accent_style = Style::default().fg(theme.accent);
    let max_w = inner.width as usize;

    let total = git.graph_lines.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let gline = match git.graph_lines.get(item_idx) { Some(l) => l, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::raw(" "));
        let display = if gline.len() > max_w.saturating_sub(1) {
            &gline[..max_w.saturating_sub(1)]
        } else {
            gline.as_str()
        };
        for ch in display.chars() {
            match ch {
                '*' | '|' | '/' | '\\' | '_' => {
                    spans.push(Span::styled(ch.to_string(), accent_style));
                }
                _ => {
                    spans.push(Span::styled(ch.to_string(), fg_style));
                }
            }
        }
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(row_style),
            Rect::new(inner.x, row, inner.width, 1),
        );
    }
}

fn render_widget_contributors(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" contributors ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.contributors.is_empty() => g,
        _ => {
            render_empty_placeholder("no contributors", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let count_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let pad = " ";

    let total = git.contributors.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let contrib = match git.contributors.get(item_idx) { Some(c) => c, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let mut spans = vec![
            Span::raw(pad),
            Span::styled(format!("{:>5}", contrib.count), count_style),
            Span::styled(format!("  {}", contrib.name), fg_style),
        ];
        if !contrib.email.is_empty() {
            spans.push(Span::styled(format!("  <{}>", contrib.email), dim_style));
        }
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(row_style),
            Rect::new(inner.x, row, inner.width, 1),
        );
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

// ---------------------------------------------------------------------------
// Unit 3: Todos, Readme
// ---------------------------------------------------------------------------

fn render_widget_todos(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" todos ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.todos.is_empty() => g,
        _ => {
            render_empty_placeholder("no TODOs found", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let pad = " ";

    let total = git.todos.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let todo = match git.todos.get(item_idx) { Some(t) => t, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let kind_color = match todo.kind.as_str() {
            "FIXME" => Color::Red,
            "HACK" => Color::Cyan,
            _ => Color::Yellow,
        };
        let max_w = inner.width as usize;
        let loc = format!("{}:{}", todo.file, todo.line_num);
        let loc_display = if loc.len() > max_w / 3 {
            format!("…{}", &loc[loc.len().saturating_sub(max_w / 3)..])
        } else {
            loc
        };
        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(
                format!("{:<5}", todo.kind),
                Style::default().fg(kind_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}", loc_display), dim_style),
            Span::styled(
                format!(
                    " {}",
                    truncate_str(&todo.text, max_w.saturating_sub(8 + loc_display.len()))
                ),
                fg_style,
            ),
        ]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

fn render_widget_readme(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" readme ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.readme_lines.is_empty() => g,
        _ => {
            render_empty_placeholder("no README.md", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let bold_style = Style::default()
        .fg(theme.fg)
        .add_modifier(Modifier::BOLD);

    let total = git.readme_lines.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let line_text = match git.readme_lines.get(item_idx) { Some(l) => l, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let style = if item_idx == 0 { bold_style } else { fg_style };
        let display = truncate_str(line_text, inner.width as usize - 1);
        let line = Line::from(vec![Span::raw(" "), Span::styled(display, style)]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

// ---------------------------------------------------------------------------
// Unit 4: Languages, Disk Usage
// ---------------------------------------------------------------------------

fn render_widget_languages(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" languages ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.languages.is_empty() => g,
        _ => {
            render_empty_placeholder("no files tracked", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let accent_style = Style::default().fg(theme.accent);
    let pad = " ";
    let bar_max = 20usize.min(inner.width as usize / 3);

    let total = git.languages.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let lang = match git.languages.get(item_idx) { Some(l) => l, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let filled = ((lang.percentage / 100.0) * bar_max as f32).round() as usize;
        let bar: String = "\u{2588}"
            .repeat(filled)
            + &"\u{2591}".repeat(bar_max.saturating_sub(filled));
        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(format!("{:<8}", lang.extension), fg_style),
            Span::styled(format!("{:>4} ", lang.file_count), dim_style),
            Span::styled(bar, accent_style),
            Span::styled(format!(" {:.0}%", lang.percentage), dim_style),
        ]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

fn render_widget_disk_usage(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" disk usage ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let du = match git_info.and_then(|g| g.disk_usage.as_ref()) {
        Some(du) => du,
        None => {
            render_empty_placeholder("unavailable", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let pad = " ";
    let mut row = inner.y;

    let size_line = |label: &str, size: &str, style: Style| -> Line<'_> {
        Line::from(vec![
            Span::raw(pad.to_string()),
            Span::styled(format!("{:<14}", label), dim_style),
            Span::styled(size.to_string(), style),
        ])
    };

    frame.render_widget(
        Paragraph::new(size_line("total", &du.total, fg_style)),
        Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    if row < inner.y + inner.height {
        frame.render_widget(
            Paragraph::new(size_line(".git", &du.git_size, fg_style)),
            Rect::new(inner.x, row, inner.width, 1),
        );
        row += 1;
    }

    if let (Some(size), Some(name)) = (&du.build_size, &du.build_dir_name) {
        if row < inner.y + inner.height {
            // Highlight build dir in yellow if it's large (> 100M or G)
            let large = size.contains('G') || {
                size.trim_end_matches('M')
                    .parse::<f32>()
                    .map(|v| v > 100.0)
                    .unwrap_or(false)
            };
            let style = if large {
                Style::default().fg(Color::Yellow)
            } else {
                fg_style
            };
            frame.render_widget(
                Paragraph::new(size_line(name, size, style)),
                Rect::new(inner.x, row, inner.width, 1),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Unit 5: CI Status, Open Issues
// ---------------------------------------------------------------------------

fn render_widget_ci_status(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" ci status ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) => g,
        None => {
            render_empty_placeholder("no data", theme, frame, inner);
            return;
        }
    };

    if !git.gh_available {
        render_empty_placeholder("gh CLI not available", theme, frame, inner);
        return;
    }

    if git.ci_runs.is_empty() {
        render_empty_placeholder("no CI runs", theme, frame, inner);
        return;
    }

    let fg_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let pad = " ";

    let total = git.ci_runs.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let run = match git.ci_runs.get(item_idx) { Some(r) => r, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let (icon, icon_color) = match (run.status.as_str(), run.conclusion.as_str()) {
            ("completed", "success") => ("✓", Color::Green),
            ("completed", "failure") | ("completed", "cancelled") => ("✗", Color::Red),
            ("in_progress", _) | ("queued", _) | ("waiting", _) => ("●", Color::Yellow),
            _ => ("?", Color::DarkGray),
        };

        let time_display = format_relative_time(&run.created_at);
        let max_name = inner
            .width
            .saturating_sub(6 + run.branch.len() as u16 + time_display.len() as u16)
            as usize;
        let name_display = truncate_str(&run.name, max_name);

        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(icon, Style::default().fg(icon_color)),
            Span::styled(format!(" {}", name_display), fg_style),
            Span::styled(format!("  {}", run.branch), dim_style),
            Span::styled(format!("  {}", time_display), dim_style),
        ]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

fn render_widget_open_issues(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" open issues ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) => g,
        None => {
            render_empty_placeholder("no data", theme, frame, inner);
            return;
        }
    };

    if !git.gh_available {
        render_empty_placeholder("gh CLI not available", theme, frame, inner);
        return;
    }

    if git.gh_issues.is_empty() {
        render_empty_placeholder("no open issues", theme, frame, inner);
        return;
    }

    let fg_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let accent_style = Style::default().fg(theme.accent);
    let pad = " ";

    let total = git.gh_issues.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let issue = match git.gh_issues.get(item_idx) { Some(i) => i, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let mut spans = vec![
            Span::raw(pad),
            Span::styled(format!("#{}", issue.number), accent_style),
            Span::styled(
                format!(
                    " {}",
                    truncate_str(&issue.title, inner.width as usize / 2)
                ),
                fg_style,
            ),
            Span::styled(format!("  {}", issue.author), dim_style),
        ];
        if !issue.labels.is_empty() {
            spans.push(Span::styled(
                format!("  [{}]", issue.labels.join(", ")),
                Style::default().fg(Color::Cyan),
            ));
        }
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(row_style),
            Rect::new(inner.x, row, inner.width, 1),
        );
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

// ---------------------------------------------------------------------------
// Unit 6: Quick Actions, Running Processes
// ---------------------------------------------------------------------------

fn render_widget_running_processes(
    git_info: Option<&ProjectGitInfo>,
    is_focused: bool,
    interact: Option<&WidgetInteractState>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let block = widget_block(" running processes ", theme, is_focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 10 {
        return;
    }

    let git = match git_info {
        Some(g) if !g.processes.is_empty() => g,
        _ => {
            render_empty_placeholder("no processes found", theme, frame, inner);
            return;
        }
    };

    let fg_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let pad = " ";

    let total = git.processes.len();
    let visible = (inner.height as usize).min(total);
    let selected = interact.map(|s| s.selected).unwrap_or(usize::MAX);
    let scroll = selected.checked_sub(visible.saturating_sub(1)).map(|v| v.saturating_add(1)).unwrap_or(0).min(total.saturating_sub(visible));

    for vis_i in 0..visible {
        let item_idx = scroll + vis_i;
        let proc = match git.processes.get(item_idx) { Some(p) => p, None => break };
        let row = inner.y + vis_i as u16;
        let is_selected = item_idx == selected;
        let cpu_val: f32 = proc.cpu.parse().unwrap_or(0.0);
        let cpu_style = if cpu_val > 5.0 {
            Style::default().fg(Color::Yellow)
        } else {
            dim_style
        };
        let cmd_display = truncate_str(&proc.command, inner.width as usize / 2);
        let line = Line::from(vec![
            Span::raw(pad),
            Span::styled(format!("{:>6}", proc.pid), dim_style),
            Span::styled(format!("  {:>5}%", proc.cpu), cpu_style),
            Span::styled(format!("  {}", cmd_display), fg_style),
        ]);
        let row_style = if is_selected { Style::default().bg(theme.accent) } else { Style::default() };
        frame.render_widget(Paragraph::new(line).style(row_style), Rect::new(inner.x, row, inner.width, 1));
    }

    render_overflow(total, scroll + visible, theme, frame, inner);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn widget_block<'a>(title: &'a str, theme: &Theme, is_focused: bool) -> Block<'a> {
    if is_focused {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
    } else {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_inactive))
            .title(Span::styled(title, Style::default().fg(theme.dim)))
    }
}

fn render_empty_placeholder(msg: &str, theme: &Theme, frame: &mut Frame, inner: Rect) {
    if inner.height >= 1 && inner.width >= 4 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", msg),
                Style::default().fg(theme.dim),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }
}

fn render_overflow(total: usize, visible: usize, theme: &Theme, frame: &mut Frame, inner: Rect) {
    if total > visible && inner.height >= 1 {
        let more = total - visible;
        let row = inner.y + inner.height - 1;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("+{} more", more),
                    Style::default().fg(theme.dim),
                ),
            ])),
            Rect::new(inner.x, row, inner.width, 1),
        );
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() > max && max > 1 {
        let end = s.char_indices()
            .map(|(i, _)| i)
            .take(max - 1)
            .last()
            .unwrap_or(0);
        format!("{}…", &s[..end])
    } else {
        s.to_string()
    }
}

/// Format an ISO 8601 timestamp to a relative time string.
fn format_relative_time(iso: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);
    let secs = diff.num_seconds();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
