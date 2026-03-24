//! Snapshot tests for dialog overlays (confirm, rename, new workspace).

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::client::Focus;
use pane_protocol::config::Config;
use pane_protocol::layout::{LayoutNode, TabId};
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::{Client, NewWorkspaceInputState, NewWorkspaceStage, RenameTarget};
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
// Confirm dialog tests
// ---------------------------------------------------------------------------

#[test]
fn confirm_default() {
    let mut client = base_client();
    client.focus = Focus::Confirm;
    client.confirm_message = Some("Are you sure?".into());

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("confirm_default", output);
}

#[test]
fn confirm_long_message() {
    let mut client = base_client();
    client.focus = Focus::Confirm;
    client.confirm_message = Some(
        "Are you sure you want to close this workspace? All running processes will be terminated."
            .into(),
    );

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("confirm_long_message", output);
}

#[test]
fn confirm_small_terminal() {
    let mut client = base_client();
    client.focus = Focus::Confirm;
    client.confirm_message = Some("Are you sure?".into());

    let output = render_to_string(&mut client, 60, 20);
    insta::assert_snapshot!("confirm_small_terminal", output);
}

// ---------------------------------------------------------------------------
// Rename dialog tests
// ---------------------------------------------------------------------------

#[test]
fn rename_window_empty() {
    let mut client = base_client();
    client.focus = Focus::Rename;
    client.rename_target = RenameTarget::Window;
    client.rename_input = String::new();

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("rename_window_empty", output);
}

#[test]
fn rename_window_with_text() {
    let mut client = base_client();
    client.focus = Focus::Rename;
    client.rename_target = RenameTarget::Window;
    client.rename_input = "my-server".into();

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("rename_window_with_text", output);
}

#[test]
fn rename_workspace() {
    let mut client = base_client();
    client.focus = Focus::Rename;
    client.rename_target = RenameTarget::Workspace;
    client.rename_input = "production".into();

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("rename_workspace", output);
}

// ---------------------------------------------------------------------------
// New workspace dialog tests
// ---------------------------------------------------------------------------

#[test]
fn new_workspace_dir_stage() {
    let mut client = base_client();
    client.focus = Focus::NewWorkspace;
    client.new_workspace_input =
        Some(NewWorkspaceInputState::for_test(NewWorkspaceStage::Directory));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("new_workspace_dir_stage", output);
}

#[test]
fn new_workspace_name_stage() {
    let mut client = base_client();
    client.focus = Focus::NewWorkspace;
    let mut state = NewWorkspaceInputState::for_test(NewWorkspaceStage::Name);
    state.name = "my-project".into();
    client.new_workspace_input = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("new_workspace_name_stage", output);
}
