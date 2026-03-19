use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use pane_protocol::app::Mode;
use pane_protocol::config::{Config, Theme};
use pane_protocol::window_types::TabKind;
use crate::client::ProjectHubState;
use crate::copy_mode::CopyModeState;
use pane_protocol::layout::SplitDirection;
use crate::window::terminal::{render_screen, render_screen_copy_mode};

// Typewriter animation timing (milliseconds).
const LABEL_HOLD_MS: u128 = 5000;
const LABEL_TYPE_MS: u128 = 60;
const LABEL_DELETE_MS: u128 = 30;
const LABEL_PAUSE_MS: u128 = 200;

fn interact_labels(foreground_process: Option<&str>) -> &'static [&'static str] {
    match foreground_process {
        Some("claude") => &["VIBING", "THINKING", "SCHEMING", "COOKING", "JAMMING"],
        Some("aider" | "codex" | "goose" | "cline" | "mentat" | "gpt-engineer" | "gemini") =>
            &["PROMPTING", "CONJURING", "SUMMONING", "CHANNELING"],
        Some("nvim" | "vim" | "helix" | "hx" | "kak") =>
            &["EDITING", "HACKING", "CRAFTING", "SHAPING"],
        Some("nano" | "micro" | "emacs") =>
            &["WRITING", "TYPING", "COMPOSING"],
        Some("python3" | "python" | "node" | "bun" | "deno" | "irb" | "iex" | "erl"
            | "ghci" | "julia" | "lua" | "luajit" | "R" | "swift" | "scala" | "amm") =>
            &["REPL", "EVAL", "TINKERING"],
        Some("htop" | "btop" | "top" | "btm" | "glances" | "zenith" | "bandwhich") =>
            &["MONITORING", "WATCHING", "OBSERVING"],
        Some("lazygit" | "tig" | "gitui") =>
            &["GITTING", "COMMITTING", "BRANCHING", "REBASING"],
        Some("yazi" | "ranger" | "lf" | "nnn" | "mc" | "broot" | "spf") =>
            &["BROWSING", "EXPLORING", "NAVIGATING"],
        Some("lazydocker") =>
            &["DOCKING", "CONTAINING", "SHIPPING"],
        Some("k9s" | "kdash") =>
            &["STEERING", "HELMING", "ORCHESTRATING"],
        Some("sqlite3" | "psql" | "mysql" | "redis-cli" | "mongosh" | "pgcli" | "mycli" | "litecli") =>
            &["QUERYING", "SELECTING", "JOINING"],
        Some("ssh") =>
            &["REMOTE", "TUNNELING", "CONNECTING"],
        Some("man" | "less" | "more") =>
            &["READING", "STUDYING", "LEARNING"],
        Some("make" | "cargo" | "npm" | "go" | "gradle" | "mvn") =>
            &["BUILDING", "COMPILING", "ASSEMBLING"],
        _ => &["INTERACT"],
    }
}

/// Animate a typewriter cycling through labels.
/// Returns the current display string with leading/trailing spaces for padding.
fn animate_interact_label(foreground_process: Option<&str>) -> String {
    let labels = interact_labels(foreground_process);
    if labels.len() <= 1 {
        return format!(" {} ", labels[0]);
    }

    // Compute per-label durations and total cycle time.
    let durations: Vec<u128> = labels
        .iter()
        .map(|w| {
            let n = w.len() as u128;
            n * LABEL_TYPE_MS + LABEL_HOLD_MS + n * LABEL_DELETE_MS + LABEL_PAUSE_MS
        })
        .collect();
    let total: u128 = durations.iter().sum();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut t = now % total;

    for (i, word) in labels.iter().enumerate() {
        let dur = durations[i];
        if t >= dur {
            t -= dur;
            continue;
        }
        let n = word.len();
        // Phase 1: type in
        let type_time = n as u128 * LABEL_TYPE_MS;
        if t < type_time {
            let chars = (t / LABEL_TYPE_MS) as usize;
            return format!(" {} ", &word[..chars]);
        }
        t -= type_time;
        // Phase 2: hold
        if t < LABEL_HOLD_MS {
            return format!(" {} ", word);
        }
        t -= LABEL_HOLD_MS;
        // Phase 3: delete
        let delete_time = n as u128 * LABEL_DELETE_MS;
        if t < delete_time {
            let remaining = n.saturating_sub((t / LABEL_DELETE_MS) as usize);
            return format!(" {} ", &word[..remaining]);
        }
        // Phase 4: pause (empty)
        return " ".to_string();
    }
    " ".to_string()
}

fn render_content(
    screen: &vt100::Screen,
    cms: Option<&CopyModeState>,
    frame: &mut Frame,
    area: Rect,
) {
    // Clamp render area to the VT screen dimensions so the process content
    // renders at its actual size. When a smaller client is connected the PTY
    // is sized to the minimum, and larger clients show only that region
    // instead of stretching the content to fill.
    let (vt_rows, vt_cols) = screen.size();
    let render_area = Rect::new(
        area.x,
        area.y,
        area.width.min(vt_cols),
        area.height.min(vt_rows),
    );
    let lines: Vec<Line<'static>> = match cms {
        Some(cms) => render_screen_copy_mode(screen, render_area, cms),
        None => render_screen(screen, render_area),
    };
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, render_area);
}

fn render_search_bar(cms: &CopyModeState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            "/",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}_", cms.search_query),
            Style::default().fg(Color::White),
        ),
    ]);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render a pane group from a snapshot (used by the client).
/// Receives the active tab's vt100 screen directly instead of accessing the Pane struct.
/// For widget tabs, pass `hub_state` to render widget content instead of terminal content.
#[allow(clippy::too_many_arguments)]
pub fn render_group_from_snapshot(
    group: &pane_protocol::protocol::WindowSnapshot,
    screen: Option<&vt100::Screen>,
    is_active: bool,
    mode: &Mode,
    copy_mode_state: Option<&CopyModeState>,
    config: &Config,
    hover: Option<(u16, u16)>,
    hub_state: Option<&ProjectHubState>,
    frame: &mut Frame,
    area: Rect,
) {
    let theme = &config.theme;

    // Widget tabs: skip window chrome, render widget directly with its own borders
    let active_tab = group.tabs.get(group.active_tab);
    if let Some(TabKind::Widget(ref w)) = active_tab.map(|t| &t.kind) {
        if let Some(hub) = hub_state {
            let is_focused = hub.focused_widget == Some(group.id);
            let interact = hub.widget_interact.get(&group.id);
            super::project_hub::render_single_widget(hub, w, is_focused, interact, theme, frame, area);
        }
        return;
    }

    // Check if the active pane's foreground process has a decoration
    let decoration_color = group
        .tabs
        .get(group.active_tab)
        .and_then(|snap| snap.foreground_process.as_deref())
        .and_then(|proc| config.decoration_for(proc))
        .map(|d| d.border_color);

    let border_style = if is_active {
        let mode_color = match mode {
            Mode::Normal => theme.border_normal,
            Mode::Interact => theme.border_interact,
            Mode::Scroll => theme.border_scroll,
            Mode::Copy => theme.border_scroll,
            _ => theme.border_active,
        };
        let color = decoration_color.unwrap_or(mode_color);
        Style::default().fg(color)
    } else if let Some(dec_color) = decoration_color {
        Style::default().fg(dec_color)
    } else {
        Style::default().fg(theme.border_inactive)
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    if is_active && matches!(mode, Mode::Interact) {
        let fg_process = group
            .tabs
            .get(group.active_tab)
            .and_then(|snap| snap.foreground_process.as_deref());
        let label = animate_interact_label(fg_process);
        let label_color = decoration_color.unwrap_or(theme.accent);
        block = block.title_bottom(Line::styled(
            label,
            Style::default()
                .fg(label_color)
                .add_modifier(Modifier::BOLD),
        ));
    } else if is_active && matches!(mode, Mode::Resize) {
        block = block.title_bottom(Line::styled(
            " RESIZE ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width <= 2 || inner.height == 0 {
        return;
    }

    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height);

    let cms = if is_active { copy_mode_state } else { None };
    let show_search = cms.is_some_and(|c| c.search_active);
    let mut constraints = vec![Constraint::Length(1), Constraint::Length(1), Constraint::Fill(1)];
    if show_search {
        constraints.push(Constraint::Length(1));
    }
    let areas = Layout::vertical(constraints).split(padded);

    let tab_accent = decoration_color;
    render_tab_bar_from_snapshot(group, theme, tab_accent, hover, frame, areas[0]);
    render_tab_separator(theme, frame, areas[1]);
    let content_area = areas[2];
    let search_area = areas.get(3).copied();

    if let Some(screen) = screen {
        render_content(screen, cms, frame, content_area);
    }

    if show_search {
        if let Some(search_area) = search_area {
            render_search_bar(cms.unwrap(), theme, frame, search_area);
        }
    }
}

fn render_tab_separator(theme: &Theme, frame: &mut Frame, area: Rect) {
    let style = Style::default().fg(theme.dim);
    let buf = frame.buffer_mut();
    for x in area.x..area.x + area.width {
        if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y: area.y }) {
            cell.set_symbol("─");
            cell.set_style(style);
        }
    }
}

/// Maximum display width for a single tab title (excluding padding).
const MAX_TAB_TITLE: usize = 20;
/// Ticker scroll speed: characters per second.
const TICKER_CHARS_PER_SEC: f64 = 4.0;
/// Pause at the start/end of a ticker cycle (seconds).
const TICKER_PAUSE_SECS: f64 = 2.0;

/// Truncate a title for an inactive tab, adding "…" if it overflows.
fn truncate_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        title.to_string()
    } else {
        let mut s: String = title.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

/// Produce a ticker-scrolling window into a long title.
fn ticker_title(title: &str, max: usize) -> String {
    let char_count = title.chars().count();
    if char_count <= max {
        return title.to_string();
    }
    let overflow = char_count - max;
    let scroll_duration = overflow as f64 / TICKER_CHARS_PER_SEC;
    let cycle = TICKER_PAUSE_SECS + scroll_duration + TICKER_PAUSE_SECS + scroll_duration;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let t = now % cycle;

    let offset = if t < TICKER_PAUSE_SECS {
        // Pause at start
        0
    } else if t < TICKER_PAUSE_SECS + scroll_duration {
        // Scroll forward
        ((t - TICKER_PAUSE_SECS) * TICKER_CHARS_PER_SEC) as usize
    } else if t < TICKER_PAUSE_SECS + scroll_duration + TICKER_PAUSE_SECS {
        // Pause at end
        overflow
    } else {
        // Scroll back
        let reverse_t = t - TICKER_PAUSE_SECS - scroll_duration - TICKER_PAUSE_SECS;
        overflow - (reverse_t * TICKER_CHARS_PER_SEC) as usize
    };

    let offset = offset.min(overflow);
    title.chars().skip(offset).take(max).collect()
}

fn render_tab_bar_from_snapshot(
    group: &pane_protocol::protocol::WindowSnapshot,
    theme: &Theme,
    decoration_color: Option<Color>,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    const SEP: &str = " \u{B7} ";
    const SEP_WIDTH: u16 = 3;
    const INDICATOR_WIDTH: u16 = 2; // "◂ " or " ▸"

    let n = group.tabs.len();

    // Check if hover is on the tab bar row
    let hover_x = hover.and_then(|(hx, hy)| if hy == area.y { Some(hx) } else { None });

    // Compute label widths (capped at MAX_TAB_TITLE + 2 for padding)
    let label_widths: Vec<u16> = group
        .tabs
        .iter()
        .map(|tab| (tab.title.chars().count().min(MAX_TAB_TITLE) as u16) + 2)
        .collect();

    // Check if everything fits
    let total: u16 = label_widths.iter().sum::<u16>()
        + if n > 1 { SEP_WIDTH * (n as u16 - 1) } else { 0 };

    let (lo, hi, hidden_left, hidden_right) = if n == 0 || total <= area.width {
        (0, n.saturating_sub(1), 0usize, 0usize)
    } else {
        // Overflow — find widest contiguous range centered on active tab
        let active = group.active_tab.min(n - 1);
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
            w
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
        (lo, hi, lo, n - 1 - hi)
    };

    // Compute tab_ranges for hit testing
    let mut tab_ranges: Vec<(u16, u16)> = vec![(0, 0); n];
    let mut cursor_x = area.x;
    if hidden_left > 0 {
        cursor_x += INDICATOR_WIDTH;
    }
    if n > 0 {
        for i in lo..=hi {
            if i > lo {
                cursor_x += SEP_WIDTH;
            }
            tab_ranges[i] = (cursor_x, cursor_x + label_widths[i]);
            cursor_x += label_widths[i];
        }
    }

    // Build spans
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Left overflow indicator
    if hidden_left > 0 {
        spans.push(Span::styled("\u{25C2} ", Style::default().fg(theme.dim)));
    }

    let mut first_visible = true;
    if n > 0 {
        for (i, (tab, &(tab_start, tab_end))) in group.tabs[lo..=hi]
            .iter()
            .zip(&tab_ranges[lo..=hi])
            .enumerate()
        {
            if !first_visible {
                spans.push(Span::styled(SEP, Style::default().fg(theme.dim)));
            }
            first_visible = false;

            let is_hovered = hover_x.is_some_and(|hx| hx >= tab_start && hx < tab_end);
            let is_active_tab = (lo + i) == group.active_tab;

            let style = if is_active_tab {
                let color = decoration_color.unwrap_or(theme.tab_active);
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD)
            } else if is_hovered {
                Style::default().fg(theme.fg)
            } else {
                Style::default().fg(theme.tab_inactive)
            };

            let display_title = if is_active_tab {
                ticker_title(&tab.title, MAX_TAB_TITLE)
            } else {
                truncate_title(&tab.title, MAX_TAB_TITLE)
            };
            let label = format!(" {} ", display_title);
            spans.push(Span::styled(label, style));
        }
    }

    // Right overflow indicator
    if hidden_right > 0 {
        spans.push(Span::styled(" \u{25B8}", Style::default().fg(theme.dim)));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render a fold indicator line with 1-cell padding on each side.
pub fn render_folded(
    is_active: bool,
    direction: SplitDirection,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let fg = if is_active { theme.accent } else { theme.dim };
    let style = Style::default().fg(fg);
    let buf = frame.buffer_mut();

    match direction {
        SplitDirection::Horizontal => {
            // Vertical line, 1 cell wide. Pad top and bottom by 1.
            if area.height <= 2 {
                return;
            }
            let x = area.x;
            let y_start = area.y + 1;
            let y_end = area.y + area.height - 1;
            for y in y_start..y_end {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_symbol("│");
                    cell.set_style(style);
                }
            }
        }
        SplitDirection::Vertical => {
            // Horizontal line, 1 cell tall. Pad left and right by 1.
            if area.width <= 2 {
                return;
            }
            let y = area.y;
            let x_start = area.x + 1;
            let x_end = area.x + area.width - 1;
            for x in x_start..x_end {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position { x, y }) {
                    cell.set_symbol("─");
                    cell.set_style(style);
                }
            }
        }
    }
}
