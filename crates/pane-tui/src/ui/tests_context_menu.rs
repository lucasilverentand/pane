//! Snapshot tests for context menu and widget picker overlays.

use std::collections::HashSet;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::client::Focus;
use pane_protocol::config::{Config, HubWidget};
use pane_protocol::layout::{LayoutNode, TabId};
use pane_protocol::protocol::{RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot};
use pane_protocol::window_types::{TabKind, WindowId};

use crate::client::{Client, ProjectHubState};
use crate::ui;
use crate::ui::context_menu;
use crate::ui::widget_picker::{WidgetPickerMode, WidgetPickerState};

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

fn home_workspace(windows: Vec<WindowSnapshot>, layout: LayoutNode) -> WorkspaceSnapshot {
    let active_group = windows.first().map(|w| w.id).unwrap_or_else(new_id);
    WorkspaceSnapshot {
        name: "home".to_string(),
        cwd: String::new(),
        layout,
        groups: windows,
        active_group,
        sync_panes: false,
        folded_windows: HashSet::new(),
        zoomed_window: None,
        floating_windows: Vec::new(),
        is_home: true,
    }
}

fn widget_window(id: WindowId, tab_id: TabId, widget: HubWidget) -> WindowSnapshot {
    let title = format!("{:?}", widget);
    WindowSnapshot {
        id,
        tabs: vec![TabSnapshot {
            id: tab_id,
            kind: TabKind::Widget(widget),
            title,
            exited: false,
            foreground_process: None,
            cwd: String::new(),
            cols: 80,
            rows: 24,
        }],
        active_tab: 0,
        name: None,
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
// Context menu snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn ctx_menu_tab_bar() {
    let mut client = base_client();
    client.focus = Focus::ContextMenu;
    client.context_menu_state = Some(context_menu::tab_bar_menu(40, 5));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ctx_menu_tab_bar", output);
}

#[test]
fn ctx_menu_workspace_bar() {
    let mut client = base_client();
    client.focus = Focus::ContextMenu;
    client.context_menu_state = Some(context_menu::workspace_bar_menu(20, 1));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ctx_menu_workspace_bar", output);
}

#[test]
fn ctx_menu_pane_body() {
    let mut client = base_client();
    client.focus = Focus::ContextMenu;
    client.context_menu_state = Some(context_menu::home_body_menu(60, 18));
    client.project_hub_state = Some(ProjectHubState::for_test());

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ctx_menu_pane_body", output);
}

#[test]
fn ctx_menu_home_body() {
    let mut client = Client::for_test(Config::default());
    let w = new_id();
    let t = new_id();
    client.render_state = RenderState {
        workspaces: vec![home_workspace(
            vec![widget_window(w, t, HubWidget::RecentCommits)],
            LayoutNode::Leaf(w),
        )],
        active_workspace: 0,
    };
    client.project_hub_state = Some(ProjectHubState::for_test());
    client.focus = Focus::ContextMenu;
    client.context_menu_state = Some(context_menu::home_body_menu(30, 10));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ctx_menu_home_body", output);
}

#[test]
fn ctx_menu_clamped_position() {
    let mut client = base_client();
    client.focus = Focus::ContextMenu;
    client.context_menu_state = Some(context_menu::tab_bar_menu(115, 30));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ctx_menu_clamped_position", output);
}

#[test]
fn ctx_menu_selected_second() {
    let mut client = base_client();
    client.focus = Focus::ContextMenu;
    let mut menu = context_menu::workspace_bar_menu(20, 1);
    menu.selected = 1;
    client.context_menu_state = Some(menu);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("ctx_menu_selected_second", output);
}

// ---------------------------------------------------------------------------
// Widget picker snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn widget_picker_change_mode() {
    let mut client = base_client();
    client.focus = Focus::WidgetPicker;
    client.widget_picker_state = Some(WidgetPickerState::new(WidgetPickerMode::Change));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("widget_picker_change_mode", output);
}

#[test]
fn widget_picker_split_h() {
    let mut client = base_client();
    client.focus = Focus::WidgetPicker;
    client.widget_picker_state = Some(WidgetPickerState::new(WidgetPickerMode::SplitHorizontal));

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("widget_picker_split_h", output);
}

#[test]
fn widget_picker_selected_fifth() {
    let mut client = base_client();
    client.focus = Focus::WidgetPicker;
    let mut state = WidgetPickerState::new(WidgetPickerMode::Change);
    state.selected = 4;
    client.widget_picker_state = Some(state);

    let output = render_to_string(&mut client, COLS, ROWS);
    insta::assert_snapshot!("widget_picker_selected_fifth", output);
}
