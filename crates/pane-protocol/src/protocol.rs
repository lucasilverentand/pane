use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use serde::{Deserialize, Serialize};

use std::collections::HashSet;

use crate::layout::{LayoutNode, TabId};
use crate::system_stats::SystemStats;
use crate::window_types::{TabKind, WindowId};

// ---------------------------------------------------------------------------
// Serializable wrappers for crossterm types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableKeyEvent {
    pub code: SerializableKeyCode,
    pub modifiers: u8,
}

impl From<KeyEvent> for SerializableKeyEvent {
    fn from(key: KeyEvent) -> Self {
        Self {
            code: SerializableKeyCode::from(key.code),
            modifiers: key.modifiers.bits(),
        }
    }
}

impl From<SerializableKeyEvent> for KeyEvent {
    fn from(sk: SerializableKeyEvent) -> Self {
        KeyEvent {
            code: sk.code.into(),
            modifiers: KeyModifiers::from_bits_truncate(sk.modifiers),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SerializableKeyCode {
    Char(char),
    F(u8),
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Delete,
    Insert,
    Esc,
    Null,
}

impl From<KeyCode> for SerializableKeyCode {
    fn from(code: KeyCode) -> Self {
        match code {
            KeyCode::Char(c) => SerializableKeyCode::Char(c),
            KeyCode::F(n) => SerializableKeyCode::F(n),
            KeyCode::Backspace => SerializableKeyCode::Backspace,
            KeyCode::Enter => SerializableKeyCode::Enter,
            KeyCode::Left => SerializableKeyCode::Left,
            KeyCode::Right => SerializableKeyCode::Right,
            KeyCode::Up => SerializableKeyCode::Up,
            KeyCode::Down => SerializableKeyCode::Down,
            KeyCode::Home => SerializableKeyCode::Home,
            KeyCode::End => SerializableKeyCode::End,
            KeyCode::PageUp => SerializableKeyCode::PageUp,
            KeyCode::PageDown => SerializableKeyCode::PageDown,
            KeyCode::Tab => SerializableKeyCode::Tab,
            KeyCode::BackTab => SerializableKeyCode::BackTab,
            KeyCode::Delete => SerializableKeyCode::Delete,
            KeyCode::Insert => SerializableKeyCode::Insert,
            KeyCode::Esc => SerializableKeyCode::Esc,
            _ => SerializableKeyCode::Null,
        }
    }
}

impl From<SerializableKeyCode> for KeyCode {
    fn from(code: SerializableKeyCode) -> Self {
        match code {
            SerializableKeyCode::Char(c) => KeyCode::Char(c),
            SerializableKeyCode::F(n) => KeyCode::F(n),
            SerializableKeyCode::Backspace => KeyCode::Backspace,
            SerializableKeyCode::Enter => KeyCode::Enter,
            SerializableKeyCode::Left => KeyCode::Left,
            SerializableKeyCode::Right => KeyCode::Right,
            SerializableKeyCode::Up => KeyCode::Up,
            SerializableKeyCode::Down => KeyCode::Down,
            SerializableKeyCode::Home => KeyCode::Home,
            SerializableKeyCode::End => KeyCode::End,
            SerializableKeyCode::PageUp => KeyCode::PageUp,
            SerializableKeyCode::PageDown => KeyCode::PageDown,
            SerializableKeyCode::Tab => KeyCode::Tab,
            SerializableKeyCode::BackTab => KeyCode::BackTab,
            SerializableKeyCode::Delete => KeyCode::Delete,
            SerializableKeyCode::Insert => KeyCode::Insert,
            SerializableKeyCode::Esc => KeyCode::Esc,
            SerializableKeyCode::Null => KeyCode::Null,
        }
    }
}

// ---------------------------------------------------------------------------
// Client → Server messages
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientRequest {
    Attach,
    Detach,
    Resize {
        width: u16,
        height: u16,
    },
    Key(SerializableKeyEvent),
    MouseDown {
        x: u16,
        y: u16,
    },
    MouseDrag {
        x: u16,
        y: u16,
    },
    MouseMove {
        x: u16,
        y: u16,
    },
    MouseUp {
        x: u16,
        y: u16,
    },
    MouseScroll {
        up: bool,
    },
    Command(String),
    /// Paste text directly to the active PTY, bypassing command parsing.
    Paste(String),
    /// Synchronous command: execute and return result on this stream, then disconnect.
    /// Used by the tmux shim for fire-and-forget commands.
    CommandSync(String),
}

// ---------------------------------------------------------------------------
// Server → Client messages
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerResponse {
    Attached,
    PaneOutput {
        pane_id: TabId,
        data: Vec<u8>,
    },
    PaneExited {
        pane_id: TabId,
    },
    LayoutChanged {
        render_state: RenderState,
    },
    StatsUpdate(SerializableSystemStats),
    PluginSegments(Vec<Vec<crate::plugin::PluginSegment>>),
    SessionEnded,
    /// Full screen dump for a pane, sent when a client attaches mid-session.
    FullScreenDump {
        pane_id: TabId,
        data: Vec<u8>,
    },
    /// Notify clients when the number of connected clients changes.
    ClientCountChanged(u32),
    Error(String),
    /// Synchronous command result: output text, optional pane/window IDs, and success flag.
    CommandOutput {
        output: String,
        pane_id: Option<u32>,
        window_id: Option<u32>,
        success: bool,
    },
}

// ---------------------------------------------------------------------------
// RenderState: serializable snapshot for client rendering
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderState {
    pub workspaces: Vec<WorkspaceSnapshot>,
    pub active_workspace: usize,
}

/// Serializable floating window info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FloatingWindowSnapshot {
    pub id: WindowId,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub name: String,
    /// Working directory for this workspace.
    #[serde(default)]
    pub cwd: String,
    pub layout: LayoutNode,
    pub groups: Vec<WindowSnapshot>,
    pub active_group: WindowId,
    pub sync_panes: bool,
    #[serde(default)]
    pub folded_windows: HashSet<WindowId>,
    pub zoomed_window: Option<WindowId>,
    pub floating_windows: Vec<FloatingWindowSnapshot>,
    /// Whether this is the home (project hub) workspace.
    #[serde(default)]
    pub is_home: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowSnapshot {
    pub id: WindowId,
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: usize,
    /// Optional user-assigned window name.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub id: TabId,
    pub kind: TabKind,
    pub title: String,
    pub exited: bool,
    pub foreground_process: Option<String>,
    pub cwd: String,
    /// Current PTY dimensions so the client can size its vt100 parser correctly.
    #[serde(default = "default_pty_cols")]
    pub cols: u16,
    #[serde(default = "default_pty_rows")]
    pub rows: u16,
}

fn default_pty_cols() -> u16 {
    80
}

fn default_pty_rows() -> u16 {
    24
}

// ---------------------------------------------------------------------------
// SystemStats serde wrapper
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableSystemStats {
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub load_avg_1: f64,
    pub disk_usage_percent: f32,
}

impl From<&SystemStats> for SerializableSystemStats {
    fn from(s: &SystemStats) -> Self {
        Self {
            cpu_percent: s.cpu_percent,
            memory_percent: s.memory_percent,
            load_avg_1: s.load_avg_1,
            disk_usage_percent: s.disk_usage_percent,
        }
    }
}

impl From<SerializableSystemStats> for SystemStats {
    fn from(s: SerializableSystemStats) -> Self {
        Self {
            cpu_percent: s.cpu_percent,
            memory_percent: s.memory_percent,
            load_avg_1: s.load_avg_1,
            disk_usage_percent: s.disk_usage_percent,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_key_event_roundtrip() {
        let keys = vec![
            make_key(KeyCode::Char('a'), KeyModifiers::NONE),
            make_key(KeyCode::Char('q'), KeyModifiers::CONTROL),
            make_key(KeyCode::Char('D'), KeyModifiers::CONTROL),
            make_key(KeyCode::F(5), KeyModifiers::NONE),
            make_key(KeyCode::Tab, KeyModifiers::CONTROL),
            make_key(KeyCode::BackTab, KeyModifiers::CONTROL),
            make_key(KeyCode::Enter, KeyModifiers::NONE),
            make_key(KeyCode::Esc, KeyModifiers::NONE),
            make_key(KeyCode::Up, KeyModifiers::SHIFT),
            make_key(KeyCode::Char('h'), KeyModifiers::ALT),
        ];

        for key in keys {
            let serializable: SerializableKeyEvent = key.into();
            let json = serde_json::to_string(&serializable).unwrap();
            let deser: SerializableKeyEvent = serde_json::from_str(&json).unwrap();
            let restored: KeyEvent = deser.into();
            assert_eq!(restored.code, key.code, "KeyCode mismatch for {:?}", key);
            assert_eq!(
                restored.modifiers, key.modifiers,
                "Modifiers mismatch for {:?}",
                key
            );
        }
    }

    #[test]
    fn test_client_request_serialization() {
        let requests = vec![
            ClientRequest::Attach,
            ClientRequest::Detach,
            ClientRequest::Resize {
                width: 120,
                height: 40,
            },
            ClientRequest::Key(make_key(KeyCode::Char('a'), KeyModifiers::NONE).into()),
            ClientRequest::MouseDown { x: 10, y: 5 },
            ClientRequest::MouseDrag { x: 15, y: 8 },
            ClientRequest::MouseMove { x: 20, y: 3 },
            ClientRequest::MouseUp { x: 10, y: 5 },
            ClientRequest::MouseScroll { up: true },
            ClientRequest::Command("list-panes".to_string()),
            ClientRequest::Paste("hello world".to_string()),
        ];

        for req in &requests {
            let json = serde_json::to_string(req).unwrap();
            let restored: ClientRequest = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&restored).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn test_server_response_serialization() {
        let responses = vec![
            ServerResponse::Attached,
            ServerResponse::PaneOutput {
                pane_id: TabId::new_v4(),
                data: vec![0x1b, b'[', b'H', b'e', b'l', b'l', b'o'],
            },
            ServerResponse::PaneExited {
                pane_id: TabId::new_v4(),
            },
            ServerResponse::SessionEnded,
            ServerResponse::Error("something failed".to_string()),
        ];

        for resp in &responses {
            let json = serde_json::to_string(resp).unwrap();
            let restored: ServerResponse = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&restored).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn test_system_stats_roundtrip() {
        let stats = SystemStats {
            cpu_percent: 42.5,
            memory_percent: 67.8,
            load_avg_1: 1.23,
            disk_usage_percent: 55.0,
        };
        let serializable: SerializableSystemStats = (&stats).into();
        let json = serde_json::to_string(&serializable).unwrap();
        let deser: SerializableSystemStats = serde_json::from_str(&json).unwrap();
        let restored: SystemStats = deser.into();
        assert!((restored.cpu_percent - 42.5).abs() < f32::EPSILON);
        assert!((restored.memory_percent - 67.8).abs() < f32::EPSILON);
        assert!((restored.load_avg_1 - 1.23).abs() < f64::EPSILON);
    }

    #[test]
    fn test_render_state_serialization() {
        let state = RenderState {
            workspaces: vec![WorkspaceSnapshot {
                name: "ws1".to_string(),
                cwd: "/tmp".to_string(),
                layout: LayoutNode::Leaf(WindowId::new_v4()),
                groups: vec![WindowSnapshot {
                    id: WindowId::new_v4(),
                    tabs: vec![TabSnapshot {
                        id: TabId::new_v4(),
                        kind: TabKind::Shell,
                        title: "shell".to_string(),
                        exited: false,
                        foreground_process: None,
                        cwd: "/tmp".to_string(),
                        cols: 80,
                        rows: 24,
                    }],
                    active_tab: 0,
                    name: None,
                }],
                active_group: WindowId::new_v4(),
                sync_panes: false,
                folded_windows: HashSet::new(),
                zoomed_window: None,
                floating_windows: vec![],
                is_home: false,
            }],
            active_workspace: 0,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: RenderState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.workspaces.len(), 1);
        assert_eq!(restored.workspaces[0].groups[0].tabs[0].title, "shell");
    }

    // --- Floating window snapshot ---

    #[test]
    fn test_floating_window_snapshot_roundtrip() {
        let id = WindowId::new_v4();
        let snap = FloatingWindowSnapshot {
            id,
            x: 10,
            y: 20,
            width: 80,
            height: 24,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: FloatingWindowSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, id);
        assert_eq!(restored.x, 10);
        assert_eq!(restored.y, 20);
        assert_eq!(restored.width, 80);
        assert_eq!(restored.height, 24);
    }

    #[test]
    fn test_floating_window_snapshot_zero_dimensions() {
        let snap = FloatingWindowSnapshot {
            id: WindowId::new_v4(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: FloatingWindowSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.width, 0);
        assert_eq!(restored.height, 0);
    }

    // --- Workspace snapshot with folds ---

    #[test]
    fn test_workspace_snapshot_with_folded_windows() {
        let w1 = WindowId::new_v4();
        let w2 = WindowId::new_v4();
        let mut folded = HashSet::new();
        folded.insert(w2);

        let snap = WorkspaceSnapshot {
            name: "dev".to_string(),
            cwd: "/home/user".to_string(),
            layout: LayoutNode::Split {
                direction: crate::layout::SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(w1)),
                second: Box::new(LayoutNode::Leaf(w2)),
            },
            groups: vec![],
            active_group: w1,
            sync_panes: false,
            folded_windows: folded.clone(),
            zoomed_window: None,
            floating_windows: vec![],
            is_home: false,
        };

        let json = serde_json::to_string(&snap).unwrap();
        let restored: WorkspaceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.folded_windows.len(), 1);
        assert!(restored.folded_windows.contains(&w2));
        assert!(!restored.folded_windows.contains(&w1));
    }

    #[test]
    fn test_workspace_snapshot_empty_folds_default() {
        // Deserializing a workspace without folded_windows field should default to empty
        let w1 = WindowId::new_v4();
        let snap = WorkspaceSnapshot {
            name: "test".to_string(),
            cwd: "".to_string(),
            layout: LayoutNode::Leaf(w1),
            groups: vec![],
            active_group: w1,
            sync_panes: false,
            folded_windows: HashSet::new(),
            zoomed_window: None,
            floating_windows: vec![],
            is_home: false,
        };
        let mut json_val: serde_json::Value = serde_json::to_value(&snap).unwrap();
        // Remove the folded_windows field to test serde default
        json_val.as_object_mut().unwrap().remove("folded_windows");
        let restored: WorkspaceSnapshot = serde_json::from_value(json_val).unwrap();
        assert!(restored.folded_windows.is_empty());
    }

    #[test]
    fn test_workspace_snapshot_with_zoomed_window() {
        let w1 = WindowId::new_v4();
        let snap = WorkspaceSnapshot {
            name: "zoomed".to_string(),
            cwd: "/tmp".to_string(),
            layout: LayoutNode::Leaf(w1),
            groups: vec![],
            active_group: w1,
            sync_panes: false,
            folded_windows: HashSet::new(),
            zoomed_window: Some(w1),
            floating_windows: vec![],
            is_home: false,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: WorkspaceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.zoomed_window, Some(w1));
    }

    #[test]
    fn test_workspace_snapshot_with_floating_windows() {
        let w1 = WindowId::new_v4();
        let fw = FloatingWindowSnapshot {
            id: WindowId::new_v4(),
            x: 5,
            y: 10,
            width: 60,
            height: 20,
        };
        let snap = WorkspaceSnapshot {
            name: "float".to_string(),
            cwd: "/tmp".to_string(),
            layout: LayoutNode::Leaf(w1),
            groups: vec![],
            active_group: w1,
            sync_panes: false,
            folded_windows: HashSet::new(),
            zoomed_window: None,
            floating_windows: vec![fw],
            is_home: false,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: WorkspaceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.floating_windows.len(), 1);
        assert_eq!(restored.floating_windows[0].x, 5);
        assert_eq!(restored.floating_windows[0].width, 60);
    }

    // --- Tab snapshot with foreground_process ---

    #[test]
    fn test_tab_snapshot_with_foreground_process() {
        let tab = TabSnapshot {
            id: TabId::new_v4(),
            kind: TabKind::Shell,
            title: "zsh".to_string(),
            exited: false,
            foreground_process: Some("vim".to_string()),
            cwd: "/home/user/code".to_string(),
            cols: 120,
            rows: 40,
        };
        let json = serde_json::to_string(&tab).unwrap();
        let restored: TabSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.foreground_process, Some("vim".to_string()));
        assert_eq!(restored.cols, 120);
        assert_eq!(restored.rows, 40);
    }

    #[test]
    fn test_tab_snapshot_without_foreground_process() {
        let tab = TabSnapshot {
            id: TabId::new_v4(),
            kind: TabKind::Shell,
            title: "bash".to_string(),
            exited: false,
            foreground_process: None,
            cwd: "/tmp".to_string(),
            cols: 80,
            rows: 24,
        };
        let json = serde_json::to_string(&tab).unwrap();
        let restored: TabSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.foreground_process, None);
    }

    #[test]
    fn test_tab_snapshot_default_pty_dimensions() {
        // Deserialize without cols/rows fields to test defaults
        let tab = TabSnapshot {
            id: TabId::new_v4(),
            kind: TabKind::Shell,
            title: "test".to_string(),
            exited: false,
            foreground_process: None,
            cwd: "/tmp".to_string(),
            cols: 80,
            rows: 24,
        };
        let mut json_val: serde_json::Value = serde_json::to_value(&tab).unwrap();
        let obj = json_val.as_object_mut().unwrap();
        obj.remove("cols");
        obj.remove("rows");
        let restored: TabSnapshot = serde_json::from_value(json_val).unwrap();
        assert_eq!(restored.cols, 80); // default
        assert_eq!(restored.rows, 24); // default
    }

    #[test]
    fn test_tab_snapshot_exited_flag() {
        let tab = TabSnapshot {
            id: TabId::new_v4(),
            kind: TabKind::Shell,
            title: "dead".to_string(),
            exited: true,
            foreground_process: None,
            cwd: "/tmp".to_string(),
            cols: 80,
            rows: 24,
        };
        let json = serde_json::to_string(&tab).unwrap();
        let restored: TabSnapshot = serde_json::from_str(&json).unwrap();
        assert!(restored.exited);
    }

    #[test]
    fn test_tab_snapshot_all_kinds() {
        for kind in [TabKind::Shell, TabKind::Agent, TabKind::Nvim, TabKind::DevServer] {
            let tab = TabSnapshot {
                id: TabId::new_v4(),
                kind: kind.clone(),
                title: "test".to_string(),
                exited: false,
                foreground_process: None,
                cwd: "/tmp".to_string(),
                cols: 80,
                rows: 24,
            };
            let json = serde_json::to_string(&tab).unwrap();
            let restored: TabSnapshot = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.kind, kind);
        }
    }

    // --- All ClientRequest variants round-trip ---

    #[test]
    fn test_all_client_request_variants_roundtrip() {
        let requests = vec![
            ClientRequest::Attach,
            ClientRequest::Detach,
            ClientRequest::Resize {
                width: 120,
                height: 40,
            },
            ClientRequest::Key(make_key(KeyCode::Char('x'), KeyModifiers::CONTROL).into()),
            ClientRequest::MouseDown { x: 10, y: 5 },
            ClientRequest::MouseDrag { x: 15, y: 8 },
            ClientRequest::MouseMove { x: 20, y: 3 },
            ClientRequest::MouseUp { x: 10, y: 5 },
            ClientRequest::MouseScroll { up: true },
            ClientRequest::MouseScroll { up: false },
            ClientRequest::Command("list-panes".to_string()),
            ClientRequest::Paste("pasted text with spaces\nnewlines".to_string()),
            ClientRequest::CommandSync("split -h".to_string()),
        ];

        for req in &requests {
            let json = serde_json::to_string(req).unwrap();
            let restored: ClientRequest = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&restored).unwrap();
            assert_eq!(json, json2, "Failed round-trip for: {:?}", req);
        }
    }

    // --- All ServerResponse variants round-trip ---

    #[test]
    fn test_all_server_response_variants_roundtrip() {
        let pane_id = TabId::new_v4();
        let window_id = WindowId::new_v4();

        let render_state = RenderState {
            workspaces: vec![WorkspaceSnapshot {
                name: "ws".to_string(),
                cwd: "/tmp".to_string(),
                layout: LayoutNode::Leaf(window_id),
                groups: vec![WindowSnapshot {
                    id: window_id,
                    tabs: vec![TabSnapshot {
                        id: pane_id,
                        kind: TabKind::Shell,
                        title: "sh".to_string(),
                        exited: false,
                        foreground_process: Some("cargo".to_string()),
                        cwd: "/tmp".to_string(),
                        cols: 80,
                        rows: 24,
                    }],
                    active_tab: 0,
                    name: None,
                }],
                active_group: window_id,
                sync_panes: true,
                folded_windows: [window_id].into_iter().collect(),
                zoomed_window: Some(window_id),
                floating_windows: vec![FloatingWindowSnapshot {
                    id: window_id,
                    x: 5,
                    y: 10,
                    width: 40,
                    height: 20,
                }],
                is_home: false,
            }],
            active_workspace: 0,
        };

        let stats = SerializableSystemStats {
            cpu_percent: 50.0,
            memory_percent: 75.0,
            load_avg_1: 1.5,
            disk_usage_percent: 60.0,
        };

        let responses: Vec<ServerResponse> = vec![
            ServerResponse::Attached,
            ServerResponse::PaneOutput {
                pane_id,
                data: vec![b'h', b'i'],
            },
            ServerResponse::PaneExited { pane_id },
            ServerResponse::LayoutChanged { render_state },
            ServerResponse::StatsUpdate(stats),
            ServerResponse::PluginSegments(vec![vec![crate::plugin::PluginSegment {
                text: "hello".to_string(),
                style: "bold".to_string(),
            }]]),
            ServerResponse::SessionEnded,
            ServerResponse::FullScreenDump {
                pane_id,
                data: vec![0x1b, b'[', b'2', b'J'],
            },
            ServerResponse::ClientCountChanged(3),
            ServerResponse::Error("test error".to_string()),
            ServerResponse::CommandOutput {
                output: "ok".to_string(),
                pane_id: Some(42),
                window_id: Some(1),
                success: true,
            },
            ServerResponse::CommandOutput {
                output: "fail".to_string(),
                pane_id: None,
                window_id: None,
                success: false,
            },
        ];

        for resp in &responses {
            let json = serde_json::to_string(resp).unwrap();
            let restored: ServerResponse = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&restored).unwrap();
            assert_eq!(json, json2, "Failed round-trip for: {:?}", resp);
        }
    }

    // --- Key event edge cases ---

    #[test]
    fn test_key_event_with_combined_modifiers() {
        let key = make_key(KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let ser: SerializableKeyEvent = key.into();
        let json = serde_json::to_string(&ser).unwrap();
        let deser: SerializableKeyEvent = serde_json::from_str(&json).unwrap();
        let restored: KeyEvent = deser.into();
        assert!(restored.modifiers.contains(KeyModifiers::CONTROL));
        assert!(restored.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn test_key_event_unmapped_code_becomes_null() {
        // KeyCode::CapsLock or other unmapped codes should map to Null
        let code = KeyCode::Null;
        let ser = SerializableKeyCode::from(code);
        assert!(matches!(ser, SerializableKeyCode::Null));
        let restored: KeyCode = ser.into();
        assert!(matches!(restored, KeyCode::Null));
    }

    #[test]
    fn test_all_serializable_key_codes_roundtrip() {
        let codes = vec![
            KeyCode::Char('z'),
            KeyCode::F(1),
            KeyCode::F(12),
            KeyCode::Backspace,
            KeyCode::Enter,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Esc,
        ];
        for code in codes {
            let ser = SerializableKeyCode::from(code);
            let restored: KeyCode = ser.into();
            assert_eq!(restored, code, "KeyCode mismatch for {:?}", code);
        }
    }

    // --- SystemStats conversion ---

    #[test]
    fn test_system_stats_from_ref_and_back() {
        let stats = SystemStats {
            cpu_percent: 10.5,
            memory_percent: 20.3,
            load_avg_1: 0.99,
            disk_usage_percent: 45.0,
        };
        let ser: SerializableSystemStats = (&stats).into();
        let back: SystemStats = ser.into();
        assert!((back.cpu_percent - 10.5).abs() < f32::EPSILON);
        assert!((back.memory_percent - 20.3).abs() < f32::EPSILON);
        assert!((back.load_avg_1 - 0.99).abs() < f64::EPSILON);
        assert!((back.disk_usage_percent - 45.0).abs() < f32::EPSILON);
    }

    // --- Complex render state ---

    #[test]
    fn test_render_state_multiple_workspaces() {
        let w1 = WindowId::new_v4();
        let w2 = WindowId::new_v4();
        let state = RenderState {
            workspaces: vec![
                WorkspaceSnapshot {
                    name: "1".to_string(),
                    cwd: "/a".to_string(),
                    layout: LayoutNode::Leaf(w1),
                    groups: vec![],
                    active_group: w1,
                    sync_panes: false,
                    folded_windows: HashSet::new(),
                    zoomed_window: None,
                    floating_windows: vec![],
                    is_home: false,
                },
                WorkspaceSnapshot {
                    name: "2".to_string(),
                    cwd: "/b".to_string(),
                    layout: LayoutNode::Leaf(w2),
                    groups: vec![],
                    active_group: w2,
                    sync_panes: true,
                    folded_windows: HashSet::new(),
                    zoomed_window: None,
                    floating_windows: vec![],
                    is_home: false,
                },
            ],
            active_workspace: 1,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: RenderState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.workspaces.len(), 2);
        assert_eq!(restored.active_workspace, 1);
        assert_eq!(restored.workspaces[0].name, "1");
        assert_eq!(restored.workspaces[1].name, "2");
        assert!(restored.workspaces[1].sync_panes);
    }

    #[test]
    fn test_window_snapshot_multiple_tabs() {
        let id = WindowId::new_v4();
        let snap = WindowSnapshot {
            id,
            tabs: vec![
                TabSnapshot {
                    id: TabId::new_v4(),
                    kind: TabKind::Shell,
                    title: "tab1".to_string(),
                    exited: false,
                    foreground_process: None,
                    cwd: "/tmp".to_string(),
                    cols: 80,
                    rows: 24,
                },
                TabSnapshot {
                    id: TabId::new_v4(),
                    kind: TabKind::Nvim,
                    title: "tab2".to_string(),
                    exited: false,
                    foreground_process: Some("nvim".to_string()),
                    cwd: "/home".to_string(),
                    cols: 120,
                    rows: 40,
                },
                TabSnapshot {
                    id: TabId::new_v4(),
                    kind: TabKind::DevServer,
                    title: "tab3".to_string(),
                    exited: true,
                    foreground_process: None,
                    cwd: "/app".to_string(),
                    cols: 80,
                    rows: 24,
                },
            ],
            active_tab: 1,
            name: None,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: WindowSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.tabs.len(), 3);
        assert_eq!(restored.active_tab, 1);
        assert_eq!(restored.tabs[1].kind, TabKind::Nvim);
        assert!(restored.tabs[2].exited);
    }

    // --- cwd default ---

    #[test]
    fn test_workspace_snapshot_cwd_defaults_empty() {
        let w = WindowId::new_v4();
        let snap = WorkspaceSnapshot {
            name: "test".to_string(),
            cwd: "/home".to_string(),
            layout: LayoutNode::Leaf(w),
            groups: vec![],
            active_group: w,
            sync_panes: false,
            folded_windows: HashSet::new(),
            zoomed_window: None,
            floating_windows: vec![],
            is_home: false,
        };
        let mut json_val: serde_json::Value = serde_json::to_value(&snap).unwrap();
        json_val.as_object_mut().unwrap().remove("cwd");
        let restored: WorkspaceSnapshot = serde_json::from_value(json_val).unwrap();
        assert_eq!(restored.cwd, "");
    }
}
