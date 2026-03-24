//! Snapshot tests for window view and tab bar edge cases.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

use crate::client::Focus;
use pane_protocol::config::Config;
use pane_protocol::layout::{LayoutNode, SplitDirection, TabId};
use pane_protocol::protocol::{
    FloatingWindowSnapshot, RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot,
};
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

fn hsplit_ratio(first: LayoutNode, second: LayoutNode, ratio: f64) -> LayoutNode {
    LayoutNode::Split {
        direction: SplitDirection::Horizontal,
        ratio,
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

const BORDER_CHARS: &[&str] = &["╭", "╮", "╰", "╯", "│", "─", "┤", "├"];

fn is_bright_fg(fg: Color) -> bool {
    let (r, g, b) = match fg {
        Color::Rgb(r, g, b) => (r as u16, g as u16, b as u16),
        Color::White => (255, 255, 255),
        Color::Gray => (229, 229, 229),
        Color::DarkGray => (127, 127, 127),
        // Named accent colors used for active/focused borders
        Color::Cyan | Color::Yellow | Color::Green | Color::Magenta | Color::Blue | Color::Red => {
            return true;
        }
        _ => return false,
    };
    let avg = (r + g + b) / 3;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let saturation = max - min;
    // Bright grayscale (dimmed white, DarkGray) or saturated color (accent / dimmed accent)
    avg > 100 || (max > 80 && saturation > 30)
}

fn render_to_styled_string(client: &mut Client, cols: u16, rows: u16) -> String {
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
            if BORDER_CHARS.contains(&sym) && is_bright_fg(cell.fg) {
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

/// Window with 8 tabs to test tab bar overflow.
#[test]
fn many_tabs() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let tabs: Vec<(&str, TabId)> = vec![
        ("zsh", new_id()),
        ("nvim", new_id()),
        ("htop", new_id()),
        ("cargo", new_id()),
        ("git", new_id()),
        ("docker", new_id()),
        ("npm", new_id()),
        ("python", new_id()),
    ];

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![window(w_id, tabs, None)],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("many_tabs", output);
}

/// Window with 3 tabs where the second tab is active.
#[test]
fn active_tab_second() {
    let mut client = Client::for_test(Config::default());
    let w_id = new_id();
    let t1 = new_id();
    let t2 = new_id();
    let t3 = new_id();

    let mut win = window(w_id, vec![("zsh", t1), ("nvim", t2), ("htop", t3)], None);
    win.active_tab = 1;

    client.render_state = RenderState {
        workspaces: vec![workspace("dev", vec![win], LayoutNode::Leaf(w_id))],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("active_tab_second", output);
}

/// Named window with 3 tabs.
#[test]
fn named_window_with_tabs() {
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
                vec![("zsh", t1), ("nvim", t2), ("htop", t3)],
                Some("my-server"),
            )],
            LayoutNode::Leaf(w_id),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("named_window_with_tabs", output);
}

/// Interact mode: active window should have heavy (bright) borders.
#[test]
fn interact_mode_border() {
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
            hsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
        )],
        active_workspace: 0,
    };
    client.focus = Focus::Interact;

    let output = render_to_styled_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("interact_mode_border", output);
}

/// Horizontal split with unequal ratio (0.3): left pane narrow, right pane wide.
#[test]
fn unequal_ratio() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("sidebar", t1)], None),
                window(w2, vec![("main", t2)], None),
            ],
            hsplit_ratio(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2), 0.3),
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("unequal_ratio", output);
}

/// Deeply nested layout: 4 levels deep with different tab names.
#[test]
fn deeply_nested() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let w3 = new_id();
    let w4 = new_id();
    let t1 = new_id();
    let t2 = new_id();
    let t3 = new_id();
    let t4 = new_id();

    let layout = hsplit(
        LayoutNode::Leaf(w1),
        vsplit(
            LayoutNode::Leaf(w2),
            hsplit(LayoutNode::Leaf(w3), LayoutNode::Leaf(w4)),
        ),
    );

    client.render_state = RenderState {
        workspaces: vec![workspace(
            "dev",
            vec![
                window(w1, vec![("editor", t1)], None),
                window(w2, vec![("build", t2)], None),
                window(w3, vec![("logs", t3)], None),
                window(w4, vec![("tests", t4)], None),
            ],
            layout,
        )],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("deeply_nested", output);
}

/// Zoomed window: first window fills entire body area.
#[test]
fn zoomed_window() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let t1 = new_id();
    let t2 = new_id();

    let mut ws = workspace(
        "dev",
        vec![
            window(w1, vec![("editor", t1)], None),
            window(w2, vec![("tests", t2)], None),
        ],
        hsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
    );
    ws.zoomed_window = Some(w1);

    client.render_state = RenderState {
        workspaces: vec![ws],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("zoomed_window", output);
}

/// Three windows in nested hsplit, two folded, one visible.
#[test]
fn two_folded_one_visible() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let w3 = new_id();
    let t1 = new_id();
    let t2 = new_id();
    let t3 = new_id();

    let layout = hsplit(
        LayoutNode::Leaf(w1),
        hsplit(LayoutNode::Leaf(w2), LayoutNode::Leaf(w3)),
    );

    let mut ws = workspace(
        "dev",
        vec![
            window(w1, vec![("editor", t1)], None),
            window(w2, vec![("build", t2)], None),
            window(w3, vec![("logs", t3)], None),
        ],
        layout,
    );
    ws.folded_windows.insert(w2);
    ws.folded_windows.insert(w3);

    client.render_state = RenderState {
        workspaces: vec![ws],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("two_folded_one_visible", output);
}

/// Two windows in vsplit, second folded — fold bar should be horizontal.
#[test]
fn vertical_fold_bar() {
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
        vsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
    );
    ws.folded_windows.insert(w2);

    client.render_state = RenderState {
        workspaces: vec![ws],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("vertical_fold_bar", output);
}

/// Two tiled windows in hsplit plus a floating window overlay.
#[test]
fn floating_window_overlay() {
    let mut client = Client::for_test(Config::default());
    let w1 = new_id();
    let w2 = new_id();
    let w3 = new_id();
    let t1 = new_id();
    let t2 = new_id();
    let t3 = new_id();

    let mut ws = workspace(
        "dev",
        vec![
            window(w1, vec![("editor", t1)], None),
            window(w2, vec![("tests", t2)], None),
            window(w3, vec![("floating", t3)], None),
        ],
        hsplit(LayoutNode::Leaf(w1), LayoutNode::Leaf(w2)),
    );
    ws.floating_windows = vec![FloatingWindowSnapshot {
        id: w3,
        x: 30,
        y: 10,
        width: 40,
        height: 15,
    }];

    client.render_state = RenderState {
        workspaces: vec![ws],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("floating_window_overlay", output);
}
