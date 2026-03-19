//! Snapshot tests for resize mode border highlighting.
//!
//! When `Mode::Resize` is active, `render_resize_borders` paints the selected
//! border of the active window with the theme accent colour.  These tests
//! capture that behaviour by rendering to a `TestBackend` and replacing
//! accent-coloured border characters with heavy box-drawing equivalents
//! (┏┓┗┛┃━) so the highlight is visible in plain-text snapshots.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

use pane_protocol::app::{Mode, ResizeBorder, ResizeState};
use pane_protocol::config::Config;
use pane_protocol::layout::{LayoutNode, SplitDirection, TabId};
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::Client;
use crate::ui;

const COLS: u16 = 120;
const ROWS: u16 = 36;

fn new_id() -> uuid::Uuid {
    uuid::Uuid::new_v4()
}

fn hsplit(first: LayoutNode, second: LayoutNode) -> LayoutNode {
    LayoutNode::Split {
        direction: SplitDirection::Horizontal,
        ratio: 0.5,
        first: Box::new(first),
        second: Box::new(second),
    }
}

fn vsplit(first: LayoutNode, second: LayoutNode) -> LayoutNode {
    LayoutNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.5,
        first: Box::new(first),
        second: Box::new(second),
    }
}

fn render_to_string(client: &mut Client, cols: u16, rows: u16) -> String {
    let backend = TestBackend::new(cols, rows);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| ui::render_client(client, frame))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    buffer_to_string(&buf)
}

fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
    let mut output = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            output.push_str(cell.symbol());
        }
        let trimmed = output.trim_end();
        output.truncate(trimmed.len());
        output.push('\n');
    }
    output
}

/// Box-drawing characters used for window borders (rounded style).
const BORDER_CHARS: &[&str] = &["╭", "╮", "╰", "╯", "│", "─", "┤", "├"];

/// Returns `true` when the foreground colour matches the theme accent.
///
/// The default theme uses `Color::Cyan`; named presets use `Color::Rgb`.
/// Both variants are handled here.
fn is_accent_fg(fg: Color) -> bool {
    match fg {
        Color::Cyan => true,
        Color::Rgb(r, g, b) => {
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            max > 100 && (max - min) > 30
        }
        _ => false,
    }
}

/// Render with colour-aware output: cells whose foreground matches the theme
/// accent get their border characters replaced with heavy box-drawing
/// equivalents so highlighted borders are visible in text snapshots.
fn render_to_accent_string(client: &mut Client, cols: u16, rows: u16) -> String {
    let backend = TestBackend::new(cols, rows);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| ui::render_client(client, frame))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut output = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let sym = cell.symbol();
            if BORDER_CHARS.contains(&sym) && is_accent_fg(cell.fg) {
                let heavy = match sym {
                    "╭" => "┏",
                    "╮" => "┓",
                    "╰" => "┗",
                    "╯" => "┛",
                    "│" => "┃",
                    "─" => "━",
                    "┤" => "┫",
                    "├" => "┣",
                    _ => sym,
                };
                output.push_str(heavy);
            } else {
                output.push_str(sym);
            }
        }
        let trimmed = output.trim_end();
        output.truncate(trimmed.len());
        output.push('\n');
    }
    output
}

fn workspace(
    name: &str,
    windows: Vec<WindowSnapshot>,
    layout: LayoutNode,
) -> WorkspaceSnapshot {
    let active_group = windows.first().map(|w| w.id).unwrap_or_else(new_id);
    WorkspaceSnapshot {
        name: name.to_string(),
        cwd: String::new(),
        layout,
        groups: windows,
        active_group,
        sync_panes: false,
        folded_windows: HashSet::new(),
        zoomed_window: None,
        floating_windows: Vec::new(),
        is_home: false,
    }
}

fn window(id: WindowId, tabs: Vec<(&str, TabId)>, name: Option<&str>) -> WindowSnapshot {
    let tab_snapshots: Vec<TabSnapshot> = tabs
        .into_iter()
        .map(|(title, tab_id)| TabSnapshot {
            id: tab_id,
            kind: TabKind::Shell,
            title: title.to_string(),
            exited: false,
            foreground_process: None,
            cwd: String::new(),
            cols: 80,
            rows: 24,
        })
        .collect();
    WindowSnapshot {
        id,
        tabs: tab_snapshots,
        active_tab: 0,
        name: name.map(|s| s.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn resize_no_border_selected() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t_id = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w_id, vec![("zsh", t_id)], None)],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState { selected: None });

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_no_border_selected", output);
}

#[test]
fn resize_border_left() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t_id = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w_id, vec![("zsh", t_id)], None)],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Left),
    });

    let output = render_to_accent_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_border_left", output);
}

#[test]
fn resize_border_right() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t_id = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w_id, vec![("zsh", t_id)], None)],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Right),
    });

    let output = render_to_accent_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_border_right", output);
}

#[test]
fn resize_border_top() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t_id = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w_id, vec![("zsh", t_id)], None)],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Top),
    });

    let output = render_to_accent_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_border_top", output);
}

#[test]
fn resize_border_bottom() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t_id = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w_id, vec![("zsh", t_id)], None)],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Bottom),
    });

    let output = render_to_accent_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_border_bottom", output);
}

#[test]
fn resize_hsplit_left_pane() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    let mut ws = workspace(
        "1",
        vec![
            window(w1, vec![("zsh", t1)], None),
            window(w2, vec![("zsh", t2)], None),
        ],
        hsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
    );
    ws.active_group = w1;

    client.render_state = RenderState {
        workspaces: vec![ws],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Right),
    });

    let output = render_to_accent_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_hsplit_left_pane", output);
}

#[test]
fn resize_vsplit_top_pane() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    let mut ws = workspace(
        "1",
        vec![
            window(w1, vec![("zsh", t1)], None),
            window(w2, vec![("zsh", t2)], None),
        ],
        vsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
    );
    ws.active_group = w1;

    client.render_state = RenderState {
        workspaces: vec![ws],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Bottom),
    });

    let output = render_to_accent_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_vsplit_top_pane", output);
}

#[test]
fn resize_small_terminal() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t_id = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w_id, vec![("zsh", t_id)], None)],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };
    client.mode = Mode::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Left),
    });

    let output = render_to_accent_string(&mut client, 60, 20);
    insta::assert_snapshot!("resize_small_terminal", output);
}
