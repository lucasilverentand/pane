pub mod context_menu;
pub mod dialog;
#[allow(dead_code)]
pub mod format;
pub mod layout_render;
pub mod palette;
pub mod project_hub;
pub mod status_bar;
pub mod tab_picker;
pub mod widget_picker;
pub mod window_view;
pub mod workspace_bar;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use pane_protocol::app::Mode;
use crate::client::Client;

/// Cursor X offset within a window: 1 (border) + 1 (padding).
const WINDOW_CONTENT_X_OFFSET: u16 = 2;
/// Cursor Y offset within a window: 1 (border) + 1 (tab bar) + 1 (separator).
const WINDOW_CONTENT_Y_OFFSET: u16 = 3;

/// Render the TUI for a connected client (daemon mode).
pub fn render_client(client: &mut Client, frame: &mut Frame) {
    let theme = &client.config.theme;

    // Workspace bar always shows (home workspace always exists)
    let show_workspace_bar = !client.render_state.workspaces.is_empty();

    let (header, body, footer) = if show_workspace_bar {
        let [h, b, f] = Layout::vertical([
            Constraint::Length(workspace_bar::HEIGHT),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        (Some(h), b, f)
    } else {
        let [b, f] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        (None, b, f)
    };

    // Workspace bar
    if let Some(header) = header {
        let names: Vec<String> = client
            .render_state
            .workspaces
            .iter()
            .map(|ws| ws.name.clone())
            .collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let active_idx = client.render_state.active_workspace;
        workspace_bar::render(
            &name_refs,
            active_idx,
            theme,
            client.workspace_bar_focused,
            client.hover,
            frame,
            header,
        );
    }

    // Status bar
    status_bar::render_client(client, theme, frame, footer);

    // Determine if the active workspace is the home workspace
    let is_home = client.is_home_active();
    let hub_state_ref = if is_home {
        client.project_hub_state.as_ref()
    } else {
        None
    };

    if let Some(ws) = client.active_workspace() {
        let copy_mode_state = if client.mode == Mode::Copy {
            client.copy_mode_state.as_ref()
        } else {
            None
        };
        let ws_bar_focused = client.workspace_bar_focused;

        // For home workspace: split body into sidebar + layout area
        // Skip sidebar when terminal is too narrow
        let sidebar_w = project_hub::sidebar_width(body.width, client.sidebar_width);
        let layout_body = if is_home && body.width >= sidebar_w + 20 {
            let [left, right] = Layout::horizontal([
                Constraint::Length(sidebar_w),
                Constraint::Fill(1),
            ])
            .areas(body);

            if let Some(ref hub) = client.project_hub_state {
                project_hub::render_sidebar_only(hub, theme, client.hover, frame, left);
            }
            right
        } else {
            body
        };

        // Check for zoom mode
        if let Some(zoomed_id) = ws.zoomed_window {
            if let Some(group) = ws.groups.iter().find(|g| g.id == zoomed_id) {
                let pane = group.tabs.get(group.active_tab);
                let screen = pane.and_then(|p| client.pane_screen(p.id));
                window_view::render_group_from_snapshot(
                    group,
                    screen,
                    !ws_bar_focused,
                    &client.mode,
                    copy_mode_state,
                    &client.config,
                    client.hover,
                    hub_state_ref,
                    frame,
                    layout_body,
                );

                if !ws_bar_focused && (client.mode == Mode::Interact || client.mode == Mode::Normal) {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                let (vt_row, vt_col) = screen.cursor_position();
                                let cursor_x = layout_body.x + WINDOW_CONTENT_X_OFFSET + vt_col;
                                let cursor_y = layout_body.y + WINDOW_CONTENT_Y_OFFSET + vt_row;
                                if cursor_x < layout_body.x + layout_body.width && cursor_y < layout_body.y + layout_body.height
                                {
                                    frame.set_cursor_position(ratatui::layout::Position {
                                        x: cursor_x,
                                        y: cursor_y,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let resolved = ws
                .layout
                .resolve_with_folds(layout_body, &ws.folded_windows);

            // First pass: visible panes
            for rp in &resolved {
                if let pane_protocol::layout::ResolvedPane::Visible { id: group_id, rect } = rp {
                    if let Some(group) = ws.groups.iter().find(|g| g.id == *group_id) {
                        let is_active = *group_id == ws.active_group && !ws_bar_focused;
                        let pane = group.tabs.get(group.active_tab);
                        let screen = pane.and_then(|p| client.pane_screen(p.id));
                        window_view::render_group_from_snapshot(
                            group,
                            screen,
                            is_active,
                            &client.mode,
                            copy_mode_state,
                            &client.config,
                            client.hover,
                            hub_state_ref,
                            frame,
                            *rect,
                        );
                    }
                }
            }

            // Second pass: fold bars
            for rp in &resolved {
                if let pane_protocol::layout::ResolvedPane::Folded {
                    id: group_id,
                    rect,
                    direction,
                } = rp
                {
                    if rect.width == 0 || rect.height == 0 {
                        continue;
                    }
                    let is_active = *group_id == ws.active_group && !ws_bar_focused;
                    window_view::render_folded(is_active, *direction, theme, frame, *rect);
                }
            }

            // Cursor position (only for non-home workspaces with terminal tabs)
            if !is_home && !ws_bar_focused && (client.mode == Mode::Interact || client.mode == Mode::Normal) {
                if let Some(group) = ws.groups.iter().find(|g| g.id == ws.active_group) {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                for rp in &resolved {
                                    if let pane_protocol::layout::ResolvedPane::Visible { id, rect } = rp {
                                        if *id == ws.active_group {
                                            let (vt_row, vt_col) = screen.cursor_position();
                                            let cursor_x = rect.x + WINDOW_CONTENT_X_OFFSET + vt_col;
                                            let cursor_y = rect.y + WINDOW_CONTENT_Y_OFFSET + vt_row;
                                            if cursor_x < rect.x + rect.width
                                                && cursor_y < rect.y + rect.height
                                            {
                                                frame.set_cursor_position(
                                                    ratatui::layout::Position {
                                                        x: cursor_x,
                                                        y: cursor_y,
                                                    },
                                                );
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Render floating windows on top of tiled layout
        for fw in &ws.floating_windows {
            if let Some(group) = ws.groups.iter().find(|g| g.id == fw.id) {
                let is_active = fw.id == ws.active_group && !ws_bar_focused;
                let pane = group.tabs.get(group.active_tab);
                let screen = pane.and_then(|p| client.pane_screen(p.id));
                let fw_rect = ratatui::layout::Rect::new(fw.x, fw.y, fw.width, fw.height);
                use ratatui::widgets::Clear;
                frame.render_widget(Clear, fw_rect);
                window_view::render_group_from_snapshot(
                    group,
                    screen,
                    is_active,
                    &client.mode,
                    copy_mode_state,
                    &client.config,
                    client.hover,
                    hub_state_ref,
                    frame,
                    fw_rect,
                );
            }
        }
    }

    // Overlays
    match &client.mode {
        Mode::Palette => {
            if let Some(ref palette_state) = client.palette_state {
                dialog::dim_background(frame, frame.area());
                palette::render(palette_state, theme, frame, frame.area());
            }
        }
        Mode::Confirm => {
            dialog::dim_background(frame, frame.area());
            render_confirm_dialog(client, theme, frame, frame.area());
        }
        Mode::Leader => {
            if let Some(ref ls) = client.leader_state {
                if ls.popup_visible {
                    if let pane_protocol::config::LeaderNode::Group { ref children, .. } =
                        ls.current_node
                    {
                        // Build display path: "⎵" for root, "⎵ w" for leader→w, etc.
                        let mut path_parts = vec!["⎵".to_string()];
                        for k in &ls.path {
                            path_parts.push(palette::key_event_to_string(k));
                        }
                        // If in a subgroup, show the group label
                        let path = if let pane_protocol::config::LeaderNode::Group { ref label, .. } = ls.current_node {
                            if ls.path.is_empty() {
                                "⎵".to_string()
                            } else {
                                format!("{} → {}", path_parts.join(" "), label)
                            }
                        } else {
                            path_parts.join(" ")
                        };
                        let compact =
                            palette::UnifiedPaletteState::new_compact_hints(children, path);
                        dialog::dim_background(frame, frame.area());
                        palette::render(&compact, theme, frame, frame.area());
                    }
                }
            }
        }
        Mode::TabPicker => {
            if let Some(ref tp_state) = client.tab_picker_state {
                let picker_area = active_window_rect(client, body)
                    .unwrap_or(frame.area());
                dialog::dim_background(frame, frame.area());
                tab_picker::render(tp_state, theme, client.hover, frame, picker_area);
            }
        }
        Mode::ContextMenu => {
            if let Some(ref cm_state) = client.context_menu_state {
                dialog::dim_background(frame, frame.area());
                context_menu::render(cm_state, theme, client.hover, frame, frame.area());
            }
        }
        Mode::Rename => {
            dialog::dim_background(frame, frame.area());
            render_rename_dialog(client, theme, frame, frame.area());
        }
        Mode::NewWorkspaceInput => {
            dialog::dim_background(frame, frame.area());
            render_new_workspace_dialog(client, theme, frame, frame.area());
        }
        Mode::ProjectHub => {
            // Legacy: hub is now rendered as workspace body, not overlay
        }
        Mode::WidgetPicker => {
            if let Some(ref wp_state) = client.widget_picker_state {
                dialog::dim_background(frame, frame.area());
                widget_picker::render(wp_state, theme, frame, frame.area());
            }
        }
        Mode::Resize => {
            if let Some(ref rs) = client.resize_state {
                if let Some(ws) = client.active_workspace() {
                    // Find the active window's rect
                    let resolved = ws.layout.resolve_with_folds(body, &ws.folded_windows);
                    for rp in &resolved {
                        if let pane_protocol::layout::ResolvedPane::Visible { id, rect } = rp {
                            if *id == ws.active_group {
                                render_resize_borders(
                                    &ws.layout,
                                    ws.active_group,
                                    rs,
                                    theme,
                                    frame,
                                    *rect,
                                );
                                break;
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Compute the body area (below workspace bar, above status bar).
pub fn body_rect(client: &Client, full_area: Rect) -> Rect {
    let show_workspace_bar = !client.render_state.workspaces.is_empty();
    if show_workspace_bar {
        let [_h, b, _f] = Layout::vertical([
            Constraint::Length(workspace_bar::HEIGHT),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(full_area);
        b
    } else {
        let [b, _f] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(full_area);
        b
    }
}

/// Compute the area used to anchor the tab picker popup (must match render logic).
pub fn tab_picker_area(client: &Client, full_area: Rect) -> Rect {
    let body = body_rect(client, full_area);
    active_window_rect(client, body).unwrap_or(full_area)
}

/// Find the active window's rect from the current render state.
fn active_window_rect(client: &Client, body: Rect) -> Option<Rect> {
    let ws = client.active_workspace()?;
    // Check floating windows first
    for fw in &ws.floating_windows {
        if fw.id == ws.active_group {
            return Some(Rect::new(fw.x, fw.y, fw.width, fw.height));
        }
    }
    let resolved = ws.layout.resolve_with_folds(body, &ws.folded_windows);
    for rp in &resolved {
        if let pane_protocol::layout::ResolvedPane::Visible { id, rect } = rp {
            if *id == ws.active_group {
                return Some(*rect);
            }
        }
    }
    None
}

fn render_confirm_dialog(
    client: &Client,
    theme: &pane_protocol::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    let message = client
        .confirm_message
        .as_deref()
        .unwrap_or("Are you sure?");

    let hovered = client.hover.and_then(|(hx, hy)| {
        dialog::confirm_hit_test(area, message, hx, hy)
    });

    dialog::render_confirm(frame, area, message, hovered, theme);
}

fn render_rename_dialog(
    client: &Client,
    theme: &pane_protocol::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        style::{Color, Style},
        text::{Line, Span},
        widgets::Paragraph,
    };

    let target = match client.rename_target {
        crate::client::RenameTarget::Window => "window",
        crate::client::RenameTarget::Workspace => "workspace",
    };

    let title = format!("rename {}", target);
    // Width: title + border padding + some input room, at least 30 for typing
    let title_w = title.len() as u16 + 4; // " title " + borders
    let popup_w = title_w.max(30);
    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed { width: popup_w, height: 3 },
        dialog::PopupAnchor::Center,
        area,
    );
    let inner = dialog::render_popup(frame, popup_area, &title, theme);

    if inner.height >= 1 {
        let input_line = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{}_", client.rename_input),
                Style::default().fg(Color::White),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(input_line),
            ratatui::layout::Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }
}

fn render_new_workspace_dialog(
    client: &crate::client::Client,
    theme: &pane_protocol::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    let state = match &client.new_workspace_input {
        Some(s) => s,
        None => return,
    };

    match state.stage {
        crate::client::NewWorkspaceStage::Directory => {
            render_new_workspace_dir_stage(state, theme, frame, area);
        }
        crate::client::NewWorkspaceStage::Name => {
            render_new_workspace_name_stage(state, theme, frame, area);
        }
    }
}

/// Stage 1: directory picker with type-to-filter and zoxide search.
///
/// Default mode is browse (filesystem navigation). Press Ctrl+F to toggle
/// zoxide search mode where typing queries zoxide frecency results.
fn render_new_workspace_dir_stage(
    state: &crate::client::NewWorkspaceInputState,
    theme: &pane_protocol::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::Paragraph,
    };

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::FixedClamped { width: 56, height: 22, pad: 4 },
        dialog::PopupAnchor::Center,
        area,
    );
    let title = if state.browser.search_mode {
        "new workspace — zoxide search"
    } else {
        "new workspace"
    };
    let inner = dialog::render_popup(frame, popup_area, title, theme);

    if inner.height < 5 || inner.width < 10 {
        return;
    }

    let mut row = inner.y;
    let search_mode = state.browser.search_mode;
    let has_filter = !state.browser.input.is_empty();

    // ── Search / path bar ──
    let path_display = if search_mode {
        state.browser.input.clone()
    } else if has_filter {
        let base = state.browser.display_path();
        let base_slash = if base.ends_with('/') { base } else { format!("{}/", base) };
        format!("{}{}", base_slash, state.browser.input)
    } else {
        state.browser.display_path_with_selected()
    };

    let has_parent = state.browser.current_dir.parent().is_some();

    let input_line = if search_mode {
        let max_w = inner.width.saturating_sub(4) as usize;
        let display = if path_display.len() > max_w {
            format!("…{}", &path_display[path_display.len() - max_w + 1..])
        } else {
            path_display
        };
        if has_filter {
            Line::from(vec![
                Span::styled(" > ", Style::default().fg(theme.accent)),
                Span::styled(display, Style::default().fg(Color::White)),
                Span::styled("_", Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(vec![
                Span::styled(" > ", Style::default().fg(theme.accent)),
                Span::styled("type to search…", Style::default().fg(theme.dim)),
            ])
        }
    } else {
        // Reserve space for ← (3) and → (3) buttons around the path
        let back_prefix = if has_parent { "← " } else { "  " };
        let suffix = if has_filter { "_" } else { " →" };
        let reserved = back_prefix.len() + suffix.len();
        let max_w = (inner.width as usize).saturating_sub(reserved + 1);
        let display = if path_display.len() > max_w {
            format!("…{}", &path_display[path_display.len() - max_w + 1..])
        } else {
            path_display
        };
        let mut spans = vec![
            Span::styled(
                format!(" {}", back_prefix),
                if has_parent {
                    Style::default().fg(theme.accent)
                } else {
                    Style::default().fg(theme.dim)
                },
            ),
            Span::styled(display, Style::default().fg(Color::White)),
        ];
        if has_filter {
            spans.push(Span::styled("_", Style::default().fg(Color::DarkGray)));
        } else {
            spans.push(Span::styled(" →", Style::default().fg(theme.accent)));
        }
        Line::from(spans)
    };
    frame.render_widget(
        Paragraph::new(input_line),
        ratatui::layout::Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // ── Separator ──
    dialog::render_separator(
        frame,
        ratatui::layout::Rect::new(inner.x, row, inner.width, 1),
    );
    row += 1;

    // ── List area ──
    let hint_height = 2u16;
    let list_height = (inner.y + inner.height)
        .saturating_sub(row + hint_height) as usize;
    if list_height == 0 {
        return;
    }

    let entries = state.browser.visible_entries();
    let selected = state.browser.selected;
    let zoxide_results = &state.browser.zoxide_results;
    let total_items = state.browser.total_count();

    let dir_style = Style::default().fg(theme.fg);
    let dim_style = Style::default().fg(theme.dim);
    let selected_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let home_dir = std::env::var("HOME").unwrap_or_default();
    let max_w = inner.width as usize;

    // Helper: shorten a path with ~ for home directory
    let shorten_path = |path: &str| -> String {
        if !home_dir.is_empty() && path.starts_with(&home_dir) {
            format!("~{}", &path[home_dir.len()..])
        } else {
            path.to_string()
        }
    };

    if total_items == 0 {
        let msg = if search_mode && !has_filter {
            "  type to search…"
        } else if state.browser.input.is_empty() {
            "  (empty)"
        } else {
            "  (no matches)"
        };
        let empty_line = Line::from(vec![Span::styled(msg, dim_style)]);
        frame.render_widget(
            Paragraph::new(empty_line),
            ratatui::layout::Rect::new(inner.x, row, inner.width, 1),
        );
    } else {
        let scroll = {
            let mut s = state.browser.scroll_offset;
            if selected >= s + list_height {
                s = selected + 1 - list_height;
            }
            if selected < s {
                s = selected;
            }
            s
        };

        let mut visual_row = 0usize;

        if search_mode {
            // ── Zoxide search results ──
            for (zi, zpath) in zoxide_results.iter().enumerate() {
                if visual_row >= scroll + list_height { break; }
                if visual_row >= scroll {
                    let is_selected = zi == selected;
                    let prefix = if is_selected { " > " } else { "   " };
                    let short = shorten_path(zpath);
                    let display = format!("{}{}", prefix, short);
                    let display = if display.len() > max_w {
                        format!("{}…", &display[..max_w - 1])
                    } else {
                        display
                    };
                    let style = if is_selected { selected_style } else { dir_style };
                    let line = Line::from(vec![Span::styled(display, style)]);
                    let line_y = row + (visual_row - scroll) as u16;
                    frame.render_widget(
                        Paragraph::new(line),
                        ratatui::layout::Rect::new(inner.x, line_y, inner.width, 1),
                    );
                }
                visual_row += 1;
            }
        } else {
            // ── Browse mode: local dirs ──
            for (i, entry) in entries.iter().enumerate() {
                if visual_row >= scroll + list_height { break; }
                if visual_row >= scroll {
                    let is_selected = i == selected;
                    let prefix = if is_selected { " > " } else { "   " };
                    let display = format!("{}{}/", prefix, entry.name);
                    let display = if display.len() > max_w {
                        format!("{}…", &display[..max_w - 1])
                    } else {
                        display
                    };
                    let style = if is_selected { selected_style } else { dir_style };
                    let line = Line::from(vec![Span::styled(display, style)]);
                    let line_y = row + (visual_row - scroll) as u16;
                    frame.render_widget(
                        Paragraph::new(line),
                        ratatui::layout::Rect::new(inner.x, line_y, inner.width, 1),
                    );
                }
                visual_row += 1;
            }
        }
    }

    // ── Hint bar at bottom ──
    let hint_y = inner.y + inner.height - hint_height;
    dialog::render_separator(
        frame,
        ratatui::layout::Rect::new(inner.x, hint_y, inner.width, 1),
    );
    let hint_line = if search_mode {
        Line::from(vec![
            Span::raw("  "),
            Span::styled("enter", Style::default().fg(theme.accent)),
            Span::styled(" select  ", dim_style),
            Span::styled("esc", Style::default().fg(theme.accent)),
            Span::styled(" back  ", dim_style),
            Span::styled("^F", Style::default().fg(theme.accent)),
            Span::styled(" browse", dim_style),
        ])
    } else if state.browser.has_zoxide {
        Line::from(vec![
            Span::raw("  "),
            Span::styled("enter", Style::default().fg(theme.accent)),
            Span::styled(" select  ", dim_style),
            Span::styled("tab/\u{2192}", Style::default().fg(theme.accent)),
            Span::styled(" open  ", dim_style),
            Span::styled("^F", Style::default().fg(theme.accent)),
            Span::styled(" search  ", dim_style),
            Span::styled("esc", Style::default().fg(theme.accent)),
            Span::styled(" cancel", dim_style),
        ])
    } else {
        Line::from(vec![
            Span::raw("  "),
            Span::styled("enter", Style::default().fg(theme.accent)),
            Span::styled(" select  ", dim_style),
            Span::styled("tab/\u{2192}", Style::default().fg(theme.accent)),
            Span::styled(" open  ", dim_style),
            Span::styled("esc", Style::default().fg(theme.accent)),
            Span::styled(" cancel", dim_style),
        ])
    };
    frame.render_widget(
        Paragraph::new(hint_line),
        ratatui::layout::Rect::new(inner.x, hint_y + 1, inner.width, 1),
    );
}

/// Stage 2: name input.
fn render_new_workspace_name_stage(
    state: &crate::client::NewWorkspaceInputState,
    theme: &pane_protocol::config::Theme,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        style::Style,
        text::{Line, Span},
        widgets::Paragraph,
    };

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed { width: 44, height: 9 },
        dialog::PopupAnchor::Center,
        area,
    );
    let inner = dialog::render_popup(frame, popup_area, "name workspace", theme);

    if inner.height < 5 || inner.width < 10 {
        return;
    }

    let mut row = inner.y;

    // ── Selected directory (dimmed context) ──
    let path_display = state.browser.display_path();
    let max_path = inner.width.saturating_sub(4) as usize;
    let path_short = if path_display.len() > max_path {
        format!("…{}", &path_display[path_display.len() - max_path + 1..])
    } else {
        path_display
    };
    let dir_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(path_short, Style::default().fg(theme.dim)),
    ]);
    frame.render_widget(
        Paragraph::new(dir_line),
        ratatui::layout::Rect::new(inner.x, row, inner.width, 1),
    );
    row += 2; // gap

    // ── Name input ──
    dialog::render_text_field(
        frame, inner, row,
        "name", &state.name, "(press enter to skip)",
        true, theme,
    );

    // ── Hint bar at bottom ──
    let hint_y = inner.y + inner.height - 1;
    let hint_line = Line::from(vec![
        Span::raw("  "),
        Span::styled("enter", Style::default().fg(theme.accent)),
        Span::styled(" create  ", Style::default().fg(theme.dim)),
        Span::styled("esc", Style::default().fg(theme.accent)),
        Span::styled(" back", Style::default().fg(theme.dim)),
    ]);
    frame.render_widget(
        Paragraph::new(hint_line),
        ratatui::layout::Rect::new(inner.x, hint_y, inner.width, 1),
    );
}

// Re-export from dialog for callers that reference the old name.
pub use dialog::ConfirmButton as ConfirmDialogClick;

/// Result of hit-testing the directory picker popup.
pub enum DirPickerClick {
    /// Clicked on a list item at the given index (relative to scroll).
    Item(usize),
    /// Clicked the ← back button to go to the parent directory.
    Back,
    /// Clicked the → confirm button on the path bar.
    Confirm,
    /// Clicked "enter" (select) in the hint bar.
    HintEnter,
    /// Clicked "tab/→" (open) in the hint bar.
    HintOpen,
    /// Clicked "^F" (search/browse toggle) in the hint bar.
    HintSearch,
    /// Clicked "esc" (cancel/back) in the hint bar.
    HintEsc,
}

pub enum NamePickerClick {
    /// Clicked "enter" (create) in the hint bar.
    HintEnter,
    /// Clicked "esc" (back) in the hint bar.
    HintEsc,
}

/// Hit-test the directory picker list. Returns which item was clicked, if any.
/// The geometry must match `render_new_workspace_dir_stage`.
pub fn dir_picker_hit_test(
    browser: &crate::client::DirBrowser,
    area: ratatui::layout::Rect,
    x: u16,
    y: u16,
) -> Option<DirPickerClick> {
    let popup_area = dialog::popup_rect(
        dialog::PopupSize::FixedClamped { width: 56, height: 22, pad: 4 },
        dialog::PopupAnchor::Center,
        area,
    );
    let inner = dialog::inner_rect(popup_area);
    if inner.height < 5 || inner.width < 10 {
        return None;
    }

    // Must be within the popup horizontally
    if x < inner.x || x >= inner.x + inner.width {
        return None;
    }

    // Path bar is at inner.y — ← on the left, → on the right
    if y == inner.y && !browser.search_mode && browser.input.is_empty() {
        // "← " occupies columns inner.x..inner.x+4 (space + "← " = 4 chars)
        let has_parent = browser.current_dir.parent().is_some();
        if has_parent && x < inner.x + 4 {
            return Some(DirPickerClick::Back);
        }
        // " →" occupies the last 3 columns
        if x >= inner.x + inner.width.saturating_sub(3) {
            return Some(DirPickerClick::Confirm);
        }
        // Click on the path text itself also confirms
        return Some(DirPickerClick::Confirm);
    }

    // Hint bar is the last row of inner
    let hint_y = inner.y + inner.height - 1;
    if y == hint_y {
        let col = x.saturating_sub(inner.x) as usize;
        return hit_test_dir_hint_bar(col, browser.search_mode, browser.has_zoxide);
    }

    // List starts after path bar (1 row) + separator (1 row)
    let list_y = inner.y + 2;
    let hint_height = 2u16;
    let list_height = (inner.y + inner.height).saturating_sub(list_y + hint_height) as usize;
    if list_height == 0 {
        return None;
    }

    if y < list_y || y >= list_y + list_height as u16 {
        return None;
    }

    let total_items = browser.total_count();
    if total_items == 0 {
        return None;
    }

    // Compute scroll offset (same logic as render)
    let selected = browser.selected;
    let mut scroll = browser.scroll_offset;
    if selected >= scroll + list_height {
        scroll = selected + 1 - list_height;
    }
    if selected < scroll {
        scroll = selected;
    }

    let visual_idx = (y - list_y) as usize;
    let item_idx = scroll + visual_idx;
    if item_idx < total_items {
        Some(DirPickerClick::Item(item_idx))
    } else {
        None
    }
}

/// Hit-test the hint bar buttons in the directory picker.
/// Column offsets must match the spans in `render_new_workspace_dir_stage`.
fn hit_test_dir_hint_bar(col: usize, search_mode: bool, has_zoxide: bool) -> Option<DirPickerClick> {
    if search_mode {
        // "  enter select  esc back  ^F browse"
        //  01234567890123456789012345678901234567
        //  ^^enter^^^^^^^^^esc^^^^^^^F
        if col < 15 { return Some(DirPickerClick::HintEnter); }   // "  enter select"
        if col < 25 { return Some(DirPickerClick::HintEsc); }     // "  esc back"
        return Some(DirPickerClick::HintSearch);                    // "  ^F browse"
    }
    if has_zoxide {
        // "  enter select  tab/→ open  ^F search  esc cancel"
        //  0         1         2         3         4
        if col < 16 { return Some(DirPickerClick::HintEnter); }   // "  enter select  "
        if col < 27 { return Some(DirPickerClick::HintOpen); }    // "tab/→ open  "
        if col < 39 { return Some(DirPickerClick::HintSearch); }  // "^F search  "
        return Some(DirPickerClick::HintEsc);                      // "esc cancel"
    }
    // "  enter select  tab/→ open  esc cancel"
    if col < 16 { return Some(DirPickerClick::HintEnter); }
    if col < 27 { return Some(DirPickerClick::HintOpen); }
    Some(DirPickerClick::HintEsc)
}

/// Check if a click is inside the directory picker popup area.
pub fn dir_picker_is_inside(area: ratatui::layout::Rect, x: u16, y: u16) -> bool {
    let popup_area = dialog::popup_rect(
        dialog::PopupSize::FixedClamped { width: 56, height: 22, pad: 4 },
        dialog::PopupAnchor::Center,
        area,
    );
    x >= popup_area.x
        && x < popup_area.x + popup_area.width
        && y >= popup_area.y
        && y < popup_area.y + popup_area.height
}

/// Check if a click is inside the name stage popup area.
pub fn name_picker_is_inside(area: ratatui::layout::Rect, x: u16, y: u16) -> bool {
    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed { width: 44, height: 9 },
        dialog::PopupAnchor::Center,
        area,
    );
    x >= popup_area.x
        && x < popup_area.x + popup_area.width
        && y >= popup_area.y
        && y < popup_area.y + popup_area.height
}

/// Hit-test the name picker hint bar buttons.
pub fn name_picker_hit_test(area: ratatui::layout::Rect, x: u16, y: u16) -> Option<NamePickerClick> {
    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed { width: 44, height: 9 },
        dialog::PopupAnchor::Center,
        area,
    );
    let inner = dialog::inner_rect(popup_area);
    if inner.height < 5 || inner.width < 10 {
        return None;
    }
    if x < inner.x || x >= inner.x + inner.width {
        return None;
    }
    // Hint bar: last row of inner
    // "  enter create  esc back"
    let hint_y = inner.y + inner.height - 1;
    if y == hint_y {
        let col = x.saturating_sub(inner.x) as usize;
        if col < 16 {
            return Some(NamePickerClick::HintEnter);
        }
        return Some(NamePickerClick::HintEsc);
    }
    None
}

fn render_resize_borders(
    _layout: &pane_protocol::layout::LayoutNode,
    _active_group: pane_protocol::layout::TabId,
    resize_state: &pane_protocol::app::ResizeState,
    theme: &pane_protocol::config::Theme,
    frame: &mut Frame,
    rect: ratatui::layout::Rect,
) {
    use pane_protocol::app::ResizeBorder;
    use ratatui::style::Style;

    let selected = match resize_state.selected {
        Some(b) => b,
        None => return,
    };

    let style = Style::default().fg(theme.border_interact);
    let buf = frame.buffer_mut();

    match selected {
        ResizeBorder::Left => {
            for y in rect.y..rect.y + rect.height {
                if let Some(cell) =
                    buf.cell_mut(ratatui::layout::Position { x: rect.x, y })
                {
                    cell.set_style(style);
                }
            }
        }
        ResizeBorder::Right => {
            let x = rect.x + rect.width.saturating_sub(1);
            for y in rect.y..rect.y + rect.height {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_style(style);
                }
            }
        }
        ResizeBorder::Top => {
            for x in rect.x..rect.x + rect.width {
                if let Some(cell) =
                    buf.cell_mut(ratatui::layout::Position { x, y: rect.y })
                {
                    cell.set_style(style);
                }
            }
        }
        ResizeBorder::Bottom => {
            let y = rect.y + rect.height.saturating_sub(1);
            for x in rect.x..rect.x + rect.width {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_style(style);
                }
            }
        }
    }
}

