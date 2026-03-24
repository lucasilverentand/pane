//! Snapshot tests for the palette rendering.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::client::Focus;
use pane_protocol::config::{Config, LeaderConfig};
use pane_protocol::layout::{LayoutNode, TabId};
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::Client;
use crate::ui;
use crate::ui::palette::UnifiedPaletteState;

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
// Palette snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn palette_full_search_empty() {
    let mut client = base_client();
    client.focus = Focus::Palette;
    client.palette_state = Some(UnifiedPaletteState::new_full_search(
        &Config::default().keys,
        &LeaderConfig::default(),
    ));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("palette_full_search_empty", output);
}

#[test]
fn palette_filtered_split() {
    let mut client = base_client();
    client.focus = Focus::Palette;
    let mut palette_state = UnifiedPaletteState::new_full_search(
        &Config::default().keys,
        &LeaderConfig::default(),
    );
    palette_state.input = "split".to_string();
    palette_state.update_filter();
    client.palette_state = Some(palette_state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("palette_filtered_split", output);
}

#[test]
fn palette_no_match() {
    let mut client = base_client();
    client.focus = Focus::Palette;
    let mut palette_state = UnifiedPaletteState::new_full_search(
        &Config::default().keys,
        &LeaderConfig::default(),
    );
    palette_state.input = "xyznonexistent".to_string();
    palette_state.update_filter();
    client.palette_state = Some(palette_state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("palette_no_match", output);
}

#[test]
fn palette_selected_third() {
    let mut client = base_client();
    client.focus = Focus::Palette;
    let mut palette_state = UnifiedPaletteState::new_full_search(
        &Config::default().keys,
        &LeaderConfig::default(),
    );
    palette_state.selected = 2;
    client.palette_state = Some(palette_state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("palette_selected_third", output);
}

#[test]
fn palette_small_terminal() {
    let mut client = base_client();
    client.focus = Focus::Palette;
    client.palette_state = Some(UnifiedPaletteState::new_full_search(
        &Config::default().keys,
        &LeaderConfig::default(),
    ));

    let output = render_to_string(&mut client, 60, 20);
    insta::assert_snapshot!("palette_small_terminal", output);
}
