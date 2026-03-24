//! Snapshot tests for status bar mode rendering.
//!
//! Each test sets the client to a specific `Focus` and captures a full-frame
//! snapshot so we can verify the status bar reflects the correct mode label.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use pane_protocol::app::{ResizeBorder, ResizeState};
use pane_protocol::config::Config;
use pane_protocol::layout::{LayoutNode, TabId};
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::{Client, Focus};
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

/// Create a base client with a single workspace containing one window.
fn base_client() -> Client {
    let mut client = Client::for_test(Config::default());
    let w = new_id();
    let t = new_id();
    client.render_state = RenderState {
        workspaces: vec![workspace(
            "1",
            vec![window(w, vec![("zsh", t)], None)],
            LayoutNode::Leaf(w),
        )],
        active_workspace: 0,
    };
    client
}

// ---------------------------------------------------------------------------
// Status bar mode snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn status_bar_normal_mode() {
    let mut client = base_client();
    // Focus::Normal is the default from for_test
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_normal_mode", output);
}

#[test]
fn status_bar_interact_mode() {
    let mut client = base_client();
    client.focus = Focus::Interact;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_interact_mode", output);
}

#[test]
fn status_bar_scroll_mode() {
    let mut client = base_client();
    client.focus = Focus::Scroll;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_scroll_mode", output);
}

#[test]
fn status_bar_copy_mode() {
    let mut client = base_client();
    client.focus = Focus::Copy;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_copy_mode", output);
}

#[test]
fn status_bar_palette_mode() {
    let mut client = base_client();
    client.focus = Focus::Palette;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_palette_mode", output);
}

#[test]
fn status_bar_confirm_mode() {
    let mut client = base_client();
    client.focus = Focus::Confirm;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_confirm_mode", output);
}

#[test]
fn status_bar_leader_mode() {
    let mut client = base_client();
    client.focus = Focus::Leader;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_leader_mode", output);
}

#[test]
fn status_bar_rename_mode() {
    let mut client = base_client();
    client.focus = Focus::Rename;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_rename_mode", output);
}

#[test]
fn status_bar_ws_bar_focused() {
    let mut client = base_client();
    client.focus = Focus::WorkspaceBar;
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_ws_bar_focused", output);
}

#[test]
fn status_bar_resize_mode() {
    let mut client = base_client();
    client.focus = Focus::Resize;
    client.resize_state = Some(ResizeState {
        selected: Some(ResizeBorder::Left),
    });
    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("status_bar_resize_mode", output);
}
