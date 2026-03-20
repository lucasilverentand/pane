//! Snapshot tests for the tab picker overlay.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::client::Focus;
use pane_protocol::config::{Config, TabPickerEntryConfig};
use pane_protocol::layout::LayoutNode;
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::Client;
use crate::ui;
use crate::ui::tab_picker::{TabPickerEntry, TabPickerMode, TabPickerSection, TabPickerState};

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

fn window(id: WindowId, tabs: Vec<(&str, uuid::Uuid)>, name: Option<&str>) -> WindowSnapshot {
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

fn test_programs() -> Vec<TabPickerEntry> {
    vec![
        TabPickerEntry {
            name: "zsh".into(),
            command: Some("zsh".into()),
            description: "Z Shell".into(),
            section: TabPickerSection::Shells,
            shell: None,
            favorite: false,
        },
        TabPickerEntry {
            name: "bash".into(),
            command: Some("bash".into()),
            description: "Bourne Again Shell".into(),
            section: TabPickerSection::Shells,
            shell: None,
            favorite: false,
        },
        TabPickerEntry {
            name: "nvim".into(),
            command: Some("nvim".into()),
            description: "Neovim".into(),
            section: TabPickerSection::Editors,
            shell: None,
            favorite: false,
        },
        TabPickerEntry {
            name: "htop".into(),
            command: Some("htop".into()),
            description: "Process viewer".into(),
            section: TabPickerSection::System,
            shell: None,
            favorite: false,
        },
        TabPickerEntry {
            name: "python3".into(),
            command: Some("python3".into()),
            description: "Python REPL".into(),
            section: TabPickerSection::Repls,
            shell: None,
            favorite: false,
        },
    ]
}

#[test]
fn tab_picker_new_tab() {
    let mut client = base_client();
    client.focus = Focus::TabPicker;
    let state = TabPickerState::new(&test_programs(), &[], &HashSet::new());
    client.tab_picker_state = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("tab_picker_new_tab", output);
}

#[test]
fn tab_picker_split_right() {
    let mut client = base_client();
    client.focus = Focus::TabPicker;
    let state = TabPickerState::with_mode(
        &test_programs(),
        &[],
        &HashSet::new(),
        TabPickerMode::SplitHorizontal,
    );
    client.tab_picker_state = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("tab_picker_split_right", output);
}

#[test]
fn tab_picker_split_down() {
    let mut client = base_client();
    client.focus = Focus::TabPicker;
    let state = TabPickerState::with_mode(
        &test_programs(),
        &[],
        &HashSet::new(),
        TabPickerMode::SplitVertical,
    );
    client.tab_picker_state = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("tab_picker_split_down", output);
}

#[test]
fn tab_picker_custom_entries() {
    let mut client = base_client();
    client.focus = Focus::TabPicker;
    let custom = vec![
        TabPickerEntryConfig {
            name: "dev-server".into(),
            command: "npm run dev".into(),
            description: Some("Start dev server".into()),
            category: Some("scripts".into()),
            shell: None,
        },
        TabPickerEntryConfig {
            name: "build".into(),
            command: "cargo build".into(),
            description: Some("Build project".into()),
            category: Some("scripts".into()),
            shell: None,
        },
    ];
    let state = TabPickerState::new(&test_programs(), &custom, &HashSet::new());
    client.tab_picker_state = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("tab_picker_custom_entries", output);
}

#[test]
fn tab_picker_filtered() {
    let mut client = base_client();
    client.focus = Focus::TabPicker;
    let mut state = TabPickerState::new(&test_programs(), &[], &HashSet::new());
    state.input = "sh".into();
    state.update_filter();
    client.tab_picker_state = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("tab_picker_filtered", output);
}

#[test]
fn tab_picker_no_match_run_entry() {
    let mut client = base_client();
    client.focus = Focus::TabPicker;
    let mut state = TabPickerState::new(&test_programs(), &[], &HashSet::new());
    state.input = "docker compose up".into();
    state.update_filter();
    client.tab_picker_state = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("tab_picker_no_match_run_entry", output);
}

#[test]
fn tab_picker_small_terminal() {
    let mut client = base_client();
    client.focus = Focus::TabPicker;
    let state = TabPickerState::new(&test_programs(), &[], &HashSet::new());
    client.tab_picker_state = Some(state);

    let output = render_to_string(&mut client, 60, 20);
    insta::assert_snapshot!("tab_picker_small_terminal", output);
}
