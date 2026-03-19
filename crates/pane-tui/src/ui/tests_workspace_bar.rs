//! Snapshot tests for workspace bar rendering.
//!
//! These tests focus on how the workspace bar appears across different
//! configurations: varying counts, overflow, nerd fonts, long names, and
//! narrow terminals.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use pane_protocol::config::Config;
use pane_protocol::layout::LayoutNode;
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::Client;
use crate::ui;

const COLS: u16 = 120;
const ROWS: u16 = 36;

fn new_id() -> uuid::Uuid {
    uuid::Uuid::new_v4()
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

use pane_protocol::layout::TabId;

/// Helper: create a single-window workspace with one shell tab.
fn simple_workspace(name: &str) -> WorkspaceSnapshot {
    let w_id = new_id();
    let t_id = new_id();
    workspace(
        name,
        vec![window(w_id, vec![("zsh", t_id)], None)],
        LayoutNode::Leaf(w_id),
    )
}

// ---------------------------------------------------------------------------
// Workspace bar snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn single_workspace() {
    let mut client = Client::for_test(Config::default());
    client.render_state = RenderState {
        workspaces: vec![simple_workspace("1")],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ws_bar_single_workspace", output);
}

#[test]
fn five_workspaces() {
    let mut client = Client::for_test(Config::default());
    client.render_state = RenderState {
        workspaces: vec![
            simple_workspace("code"),
            simple_workspace("logs"),
            simple_workspace("build"),
            simple_workspace("deploy"),
            simple_workspace("test"),
        ],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ws_bar_five_workspaces", output);
}

#[test]
fn overflow_active_last() {
    let mut client = Client::for_test(Config::default());
    let workspaces: Vec<WorkspaceSnapshot> = (1..=10)
        .map(|i| simple_workspace(&format!("ws{}", i)))
        .collect();
    client.render_state = RenderState {
        workspaces,
        active_workspace: 9,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ws_bar_overflow_active_last", output);
}

#[test]
fn overflow_active_first() {
    let mut client = Client::for_test(Config::default());
    let workspaces: Vec<WorkspaceSnapshot> = (1..=10)
        .map(|i| simple_workspace(&format!("ws{}", i)))
        .collect();
    client.render_state = RenderState {
        workspaces,
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ws_bar_overflow_active_first", output);
}

#[test]
fn nerd_font_home_icon() {
    let mut config = Config::default();
    config.behavior.nerd_fonts = true;
    let mut client = Client::for_test(config);

    let w_id = new_id();
    let t_id = new_id();
    let mut home_ws = workspace(
        "home",
        vec![window(w_id, vec![("zsh", t_id)], None)],
        LayoutNode::Leaf(w_id),
    );
    home_ws.is_home = true;

    client.render_state = RenderState {
        workspaces: vec![home_ws, simple_workspace("dev")],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ws_bar_nerd_font_home_icon", output);
}

#[test]
fn long_workspace_names() {
    let mut client = Client::for_test(Config::default());
    client.render_state = RenderState {
        workspaces: vec![
            simple_workspace("my-very-long-workspace-name"),
            simple_workspace("another-extremely-long-name"),
            simple_workspace("short"),
        ],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ws_bar_long_workspace_names", output);
}

#[test]
fn narrow_terminal() {
    let mut client = Client::for_test(Config::default());
    client.render_state = RenderState {
        workspaces: vec![simple_workspace("alpha"), simple_workspace("beta")],
        active_workspace: 0,
    };

    let output = render_to_string(&mut client, 40, 20);
    insta::assert_snapshot!("ws_bar_narrow_terminal", output);
}

#[test]
fn second_workspace_active() {
    let mut client = Client::for_test(Config::default());
    client.render_state = RenderState {
        workspaces: vec![
            simple_workspace("first"),
            simple_workspace("second"),
            simple_workspace("third"),
        ],
        active_workspace: 1,
    };

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ws_bar_second_workspace_active", output);
}
