pub mod context_menu;
pub mod dialog;
pub mod format;
pub mod layout_render;
pub mod palette;
pub mod status_bar;
pub mod tab_picker;
pub mod window_view;
pub mod workspace_bar;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use pane_protocol::app::Mode;
use crate::client::Client;

/// Render the TUI for a connected client (daemon mode).
pub fn render_client(client: &Client, frame: &mut Frame) {
    let theme = &client.config.theme;

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
        let names: Vec<&str> = client
            .render_state
            .workspaces
            .iter()
            .map(|ws| ws.name.as_str())
            .collect();
        workspace_bar::render(
            &names,
            client.render_state.active_workspace,
            theme,
            client.workspace_bar_focused,
            client.hover,
            frame,
            header,
        );
    }

    // Status bar
    status_bar::render_client(client, theme, frame, footer);

    // Render workspace body + cursor
    if let Some(ws) = client.active_workspace() {
        let copy_mode_state = if client.mode == Mode::Copy {
            client.copy_mode_state.as_ref()
        } else {
            None
        };
        let ws_bar_focused = client.workspace_bar_focused;

        // Check for zoom mode
        if let Some(zoomed_id) = ws.zoomed_window {
            // Render only the zoomed window filling the body
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
                    frame,
                    body,
                );

                // Cursor for zoomed window
                if !ws_bar_focused && (client.mode == Mode::Interact || client.mode == Mode::Normal) {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                let (vt_row, vt_col) = screen.cursor_position();
                                let cursor_x = body.x + 2 + vt_col;
                                let cursor_y = body.y + 3 + vt_row;
                                if cursor_x < body.x + body.width && cursor_y < body.y + body.height
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
                .resolve_with_folds(body, &ws.folded_windows);

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

            // Cursor position
            if !ws_bar_focused && (client.mode == Mode::Interact || client.mode == Mode::Normal) {
                if let Some(group) = ws.groups.iter().find(|g| g.id == ws.active_group) {
                    if let Some(pane) = group.tabs.get(group.active_tab) {
                        if let Some(screen) = client.pane_screen(pane.id) {
                            if !screen.hide_cursor() {
                                for rp in &resolved {
                                    if let pane_protocol::layout::ResolvedPane::Visible { id, rect } = rp {
                                        if *id == ws.active_group {
                                            let (vt_row, vt_col) = screen.cursor_position();
                                            let cursor_x = rect.x + 2 + vt_col;
                                            let cursor_y = rect.y + 3 + vt_row;
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
                palette::render(palette_state, theme, frame, frame.area());
            }
        }
        Mode::Confirm => {
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
                        palette::render(&compact, theme, frame, frame.area());
                    }
                }
            }
        }
        Mode::TabPicker => {
            if let Some(ref tp_state) = client.tab_picker_state {
                // Render inside the active window's rect
                let picker_area = active_window_rect(client, body)
                    .unwrap_or(frame.area());
                tab_picker::render(tp_state, theme, client.hover, frame, picker_area);
            }
        }
        Mode::ContextMenu => {
            if let Some(ref cm_state) = client.context_menu_state {
                context_menu::render(cm_state, theme, client.hover, frame, frame.area());
            }
        }
        Mode::Rename => {
            render_rename_dialog(client, theme, frame, frame.area());
        }
        Mode::NewWorkspaceInput => {
            render_new_workspace_dialog(client, theme, frame, frame.area());
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
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::Paragraph,
    };

    let message = client
        .confirm_message
        .as_deref()
        .unwrap_or("Are you sure?");

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Percent { width: 40, height: 15 },
        dialog::PopupAnchor::Center,
        area,
    );
    let inner = dialog::render_popup(frame, popup_area, "confirm", theme);

    let hovered = client.hover.and_then(|(hx, hy)| {
        confirm_dialog_hit_test(area, hx, hy)
    });

    let cancel_bg = if matches!(hovered, Some(ConfirmDialogClick::Cancel)) {
        theme.fg
    } else {
        theme.dim
    };
    let confirm_bg = if matches!(hovered, Some(ConfirmDialogClick::Confirm)) {
        theme.fg
    } else {
        theme.accent
    };

    let lines = vec![
        Line::raw(""),
        Line::styled(format!("  {}", message), Style::default().fg(Color::White)),
        Line::raw(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                " Cancel ",
                Style::default()
                    .fg(Color::White)
                    .bg(cancel_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                " Confirm ",
                Style::default()
                    .fg(Color::White)
                    .bg(confirm_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
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

    let max_w = inner.width.saturating_sub(4) as usize;
    let display = if path_display.len() > max_w {
        format!("…{}", &path_display[path_display.len() - max_w + 1..])
    } else {
        path_display
    };

    let input_line = if search_mode {
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
        Line::from(vec![
            Span::raw(" "),
            Span::styled(display, Style::default().fg(Color::White)),
            if has_filter {
                Span::styled("_", Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ])
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

#[derive(Debug, PartialEq, Eq)]
pub enum ConfirmDialogClick {
    Cancel,
    Confirm,
}

/// Hit-test the confirm dialog buttons. Returns which button was clicked, if any.
pub fn confirm_dialog_hit_test(
    area: ratatui::layout::Rect,
    x: u16,
    y: u16,
) -> Option<ConfirmDialogClick> {
    use ratatui::widgets::{Block, BorderType, Borders};

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Percent { width: 40, height: 15 },
        dialog::PopupAnchor::Center,
        area,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(popup_area);

    // Buttons are on line index 3 (0-indexed) of the inner area
    let button_y = inner.y + 3;
    if y != button_y {
        return None;
    }

    // Layout: "  " (2) + " Cancel " (8) + "  " (2) + " Confirm " (9)
    let cancel_start = inner.x + 2;
    let cancel_end = cancel_start + 8;
    let confirm_start = cancel_end + 2;
    let confirm_end = confirm_start + 9;

    if x >= cancel_start && x < cancel_end {
        Some(ConfirmDialogClick::Cancel)
    } else if x >= confirm_start && x < confirm_end {
        Some(ConfirmDialogClick::Confirm)
    } else {
        None
    }
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

