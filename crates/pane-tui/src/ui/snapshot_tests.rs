//! Snapshot tests for the pane TUI renderer.
//!
//! These tests render the UI to a `TestBackend` and capture the output
//! as text snapshots using `insta`. This lets us:
//! - Visually inspect the rendered UI layout
//! - Catch unintended regressions in rendering
//!
//! Run `cargo insta review` to interactively accept/reject snapshot changes.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

use ratatui::layout::Rect;

use pane_protocol::config::Config;
use pane_protocol::layout::{LayoutNode, Side, SplitDirection, TabId};
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::Client;
use crate::ui;

/// Standard terminal size for snapshot tests.
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

/// Render a `Client` to a text string via `TestBackend`.
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
        // Trim trailing whitespace per line for cleaner snapshots
        let trimmed = output.trim_end();
        output.truncate(trimmed.len());
        output.push('\n');
    }
    output
}

/// Render with focus markers: active borders use heavy box drawing (┏┓┗┛┃━),
/// inactive borders stay as thin rounded (╭╮╰╯│─). This makes focus state
/// visible in text snapshots.
#[allow(dead_code)]
fn render_to_styled_string(client: &mut Client, cols: u16, rows: u16) -> String {
    let border_chars: &[&str] = &["╭", "╮", "╰", "╯", "│", "─", "┤", "├"];

    let is_bright_fg = |fg: Color| -> bool {
        let (r, g, b) = match fg {
            Color::Rgb(r, g, b) => (r as u16, g as u16, b as u16),
            Color::White => (255, 255, 255),
            Color::Gray => (229, 229, 229),
            Color::DarkGray => (127, 127, 127),
            Color::Cyan | Color::Yellow | Color::Green | Color::Magenta | Color::Blue | Color::Red => {
                return true;
            }
            _ => return false,
        };
        let avg = (r + g + b) / 3;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let saturation = max - min;
        avg > 100 || (max > 80 && saturation > 30)
    };

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
            if border_chars.contains(&sym) && is_bright_fg(cell.fg) {
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

/// Create a basic workspace snapshot with the given windows.
fn workspace(name: &str, windows: Vec<WindowSnapshot>, layout: LayoutNode) -> WorkspaceSnapshot {
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
    }
}

/// Create a window snapshot with the given tabs.
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
// Snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn single_workspace_single_window() {
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

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("single_workspace_single_window", output);
}

#[test]
fn single_workspace_horizontal_split() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("nvim", t1)], None),
                window(w2, vec![("cargo watch", t2)], None),
            ],
            hsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("single_workspace_hsplit", output);
}

#[test]
fn single_workspace_vertical_split() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("editor", t1)], None),
                window(w2, vec![("tests", t2)], None),
            ],
            vsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("single_workspace_vsplit", output);
}

#[test]
fn multiple_workspaces() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    client.render_state = RenderState {
        workspaces: vec![
            workspace(
                "code",
                vec![window(w1, vec![("zsh", t1)], None)],
                LayoutNode::Leaf(w1),
            ),
            workspace(
                "logs",
                vec![window(w2, vec![("tail -f", t2)], None)],
                LayoutNode::Leaf(w2),
            ),
        ],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("multiple_workspaces", output);
}

#[test]
fn window_with_multiple_tabs() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t1 = new_id();
    let t2 = new_id();
    let t3 = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "work",
            vec![window(
                w_id,
                vec![("zsh", t1), ("vim", t2), ("htop", t3)],
                None,
            )],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("window_with_multiple_tabs", output);
}

#[test]
fn three_way_split() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let w3 = new_id();
    let t1 = new_id();
    let t2 = new_id();
    let t3 = new_id();

    // Left pane | right top / right bottom
    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("editor", t1)], None),
                window(w2, vec![("build", t2)], None),
                window(w3, vec![("logs", t3)], None),
            ],
            hsplit(
                LayoutNode::Leaf(w1),
                vsplit(LayoutNode::Leaf(w2), LayoutNode::Leaf(w3)),
            ),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("three_way_split", output);
}

#[test]
fn folded_window() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    let mut ws = workspace(
        "dev",
        vec![
            window(w1, vec![("editor", t1)], None),
            window(w2, vec![("folded", t2)], None),
        ],
        hsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
    );
    ws.folded_windows.insert(w2);

    client.render_state = RenderState {
        workspaces: vec![ws],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("folded_window", output);
}

#[test]
fn small_terminal() {
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

    let output = render_to_string(&mut client, 60, 20);
    insta::assert_snapshot!("small_terminal", output);
}

/// Verify rendering doesn't panic at extreme terminal sizes.
#[test]
fn tiny_terminal_no_panic() {
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

    // These should not panic even at degenerate sizes
    for (cols, rows) in [(1, 1), (3, 3), (5, 5), (10, 4), (4, 10), (0, 0)] {
        let _output = render_to_string(&mut client, cols, rows);
    }
}

/// Verify split layouts don't panic at tiny sizes.
#[test]
fn tiny_terminal_split_no_panic() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    let layout = LayoutNode::Split {
        direction: pane_protocol::layout::SplitDirection::Horizontal,
        ratio: 0.5,
        first: Box::new(LayoutNode::Leaf(w1)),
        second: Box::new(LayoutNode::Leaf(w2)),
    };

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![
                window(w1, vec![("zsh", t1)], None),
                window(w2, vec![("zsh", t2)], None),
            ],
            layout,
        )],
        active_workspace: 0,
    };

    for (cols, rows) in [(1, 1), (3, 3), (5, 5), (10, 5), (0, 0)] {
        let _output = render_to_string(&mut client, cols, rows);
    }
}

#[test]
fn named_window() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t_id = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w_id, vec![("zsh", t_id)], Some("my-server"))],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("named_window", output);
}

// ---------------------------------------------------------------------------
// Resize behavior tests
// ---------------------------------------------------------------------------
//
// These tests verify that dragging a split border only moves THAT border,
// not others. The bug was: ratio calculation used the entire layout_body
// rect instead of the split node's own rect, causing nested splits to
// compute wrong ratios.

/// Helper: simulate a drag on a split border using the correct (fixed) approach.
/// Returns the new ratio, calculated relative to the split node's own rect.
fn simulate_drag_correct(
    layout: &LayoutNode,
    layout_body: Rect,
    drag_to_x: u16,
    drag_to_y: u16,
    path: &[Side],
    direction: SplitDirection,
) -> f64 {
    // Get the split node's rect by resolving the layout
    let split_rect = layout.rect_at_path(layout_body, path);
    match direction {
        SplitDirection::Horizontal => {
            let clamped = drag_to_x.clamp(split_rect.x, split_rect.x + split_rect.width);
            (clamped - split_rect.x) as f64 / split_rect.width.max(1) as f64
        }
        SplitDirection::Vertical => {
            let clamped = drag_to_y.clamp(split_rect.y, split_rect.y + split_rect.height);
            (clamped - split_rect.y) as f64 / split_rect.height.max(1) as f64
        }
    }
}

/// Helper: simulate a drag with the OLD broken approach (ratio relative to layout_body).
fn simulate_drag_broken(
    layout_body: Rect,
    drag_to_x: u16,
    drag_to_y: u16,
    direction: SplitDirection,
) -> f64 {
    match direction {
        SplitDirection::Horizontal => {
            let clamped = drag_to_x.clamp(layout_body.x, layout_body.x + layout_body.width);
            (clamped - layout_body.x) as f64 / layout_body.width.max(1) as f64
        }
        SplitDirection::Vertical => {
            let clamped = drag_to_y.clamp(layout_body.y, layout_body.y + layout_body.height);
            (clamped - layout_body.y) as f64 / layout_body.height.max(1) as f64
        }
    }
}

/// Nested horizontal splits: HSplit [ A | HSplit [ B | C ] ]
/// Dragging the inner border (between B and C) should only move that border.
#[test]
fn resize_nested_hsplit_before() {
    let mut client = Client::for_test(Config::default());
    let (w1, w2, w3) = (new_id(), new_id(), new_id());
    let (t1, t2, t3) = (new_id(), new_id(), new_id());

    // Initial layout: HSplit(0.5) [ A | HSplit(0.5) [ B | C ] ]
    let layout = hsplit(LayoutNode::Leaf(w1), hsplit(LayoutNode::Leaf(w2), LayoutNode::Leaf(w3)));

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("A", t1)], None),
                window(w2, vec![("B", t2)], None),
                window(w3, vec![("C", t3)], None),
            ],
            layout,
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_nested_hsplit_before", output);
}

/// After correctly dragging the inner border (between B and C) to the right.
/// Only the inner border should move — A's width stays the same.
#[test]
fn resize_nested_hsplit_correct() {
    let mut client = Client::for_test(Config::default());
    let (w1, w2, w3) = (new_id(), new_id(), new_id());
    let (t1, t2, t3) = (new_id(), new_id(), new_id());

    let mut layout = hsplit(LayoutNode::Leaf(w1), hsplit(LayoutNode::Leaf(w2), LayoutNode::Leaf(w3)));

    // Simulate dragging the inner HSplit border (path=[Second]) to the right.
    // Layout body: the area excluding workspace bar (3) and status bar (1).
    // At 120 cols, 36 rows: body is Rect(0, 3, 120, 32).
    let layout_body = Rect::new(0, 3, 120, 32);
    // The inner split occupies roughly x=60..120 (right half).
    // Drag to x=100 — this should move the B/C border to 2/3 of the right half.
    let path = vec![Side::Second];
    let new_ratio = simulate_drag_correct(&layout, layout_body, 100, 0, &path, SplitDirection::Horizontal);
    layout.set_ratio_at_path(&path, new_ratio);

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("A", t1)], None),
                window(w2, vec![("B", t2)], None),
                window(w3, vec![("C", t3)], None),
            ],
            layout,
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_nested_hsplit_correct", output);
}

/// After applying the BROKEN ratio calculation (relative to layout_body).
/// This demonstrates the bug: the inner border overshoots because the
/// ratio is calculated against the full width instead of the right half.
#[test]
fn resize_nested_hsplit_broken() {
    let mut client = Client::for_test(Config::default());
    let (w1, w2, w3) = (new_id(), new_id(), new_id());
    let (t1, t2, t3) = (new_id(), new_id(), new_id());

    let mut layout = hsplit(LayoutNode::Leaf(w1), hsplit(LayoutNode::Leaf(w2), LayoutNode::Leaf(w3)));

    // Same drag to x=100, but using the broken calculation (relative to full layout_body).
    let layout_body = Rect::new(0, 3, 120, 32);
    let path = vec![Side::Second];
    let broken_ratio = simulate_drag_broken(layout_body, 100, 0, SplitDirection::Horizontal);
    layout.set_ratio_at_path(&path, broken_ratio);

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("A", t1)], None),
                window(w2, vec![("B", t2)], None),
                window(w3, vec![("C", t3)], None),
            ],
            layout,
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_nested_hsplit_broken", output);
}

/// Nested vertical splits: VSplit [ Top | VSplit [ Mid | Bot ] ]
/// Dragging the inner border should only move it, not the outer border.
#[test]
fn resize_nested_vsplit_correct() {
    let mut client = Client::for_test(Config::default());
    let (w1, w2, w3) = (new_id(), new_id(), new_id());
    let (t1, t2, t3) = (new_id(), new_id(), new_id());

    let mut layout = vsplit(LayoutNode::Leaf(w1), vsplit(LayoutNode::Leaf(w2), LayoutNode::Leaf(w3)));

    let layout_body = Rect::new(0, 3, 120, 32);
    // Inner VSplit occupies bottom half (roughly y=19..35).
    // Drag to y=28 — move Mid/Bot border down.
    let path = vec![Side::Second];
    let new_ratio = simulate_drag_correct(&layout, layout_body, 0, 28, &path, SplitDirection::Vertical);
    layout.set_ratio_at_path(&path, new_ratio);

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("Top", t1)], None),
                window(w2, vec![("Mid", t2)], None),
                window(w3, vec![("Bot", t3)], None),
            ],
            layout,
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_nested_vsplit_correct", output);
}

/// Four-way layout: HSplit [ VSplit[A|B] | VSplit[C|D] ]
/// Dragging the right VSplit border (between C and D) should not affect A/B.
#[test]
fn resize_four_way_inner_correct() {
    let mut client = Client::for_test(Config::default());
    let (w1, w2, w3, w4) = (new_id(), new_id(), new_id(), new_id());
    let (t1, t2, t3, t4) = (new_id(), new_id(), new_id(), new_id());

    let mut layout = hsplit(
        vsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
        vsplit(LayoutNode::Leaf(w3), LayoutNode::Leaf(w4)),
    );

    let layout_body = Rect::new(0, 3, 120, 32);
    // Drag the right VSplit (path=[Second]) border down from center to ~75%.
    let path = vec![Side::Second];
    let new_ratio = simulate_drag_correct(&layout, layout_body, 0, 27, &path, SplitDirection::Vertical);
    layout.set_ratio_at_path(&path, new_ratio);

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("A", t1)], None),
                window(w2, vec![("B", t2)], None),
                window(w3, vec![("C", t3)], None),
                window(w4, vec![("D", t4)], None),
            ],
            layout,
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("resize_four_way_inner_correct", output);
}

