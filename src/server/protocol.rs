use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use serde::{Deserialize, Serialize};

use crate::layout::{LayoutNode, PaneId};
use crate::pane::{PaneGroupId, PaneKind};
use crate::system_stats::SystemStats;

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
    Attach { session_name: String },
    Detach,
    Resize { width: u16, height: u16 },
    Key(SerializableKeyEvent),
    MouseDown { x: u16, y: u16 },
    MouseDrag { x: u16, y: u16 },
    MouseMove { x: u16, y: u16 },
    MouseUp,
    MouseScroll { up: bool },
    Command(String),
    /// Synchronous command: execute and return result on this stream, then disconnect.
    /// Used by the tmux shim for fire-and-forget commands.
    CommandSync(String),
}

// ---------------------------------------------------------------------------
// Server → Client messages
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerResponse {
    Attached { session_name: String },
    PaneOutput { pane_id: PaneId, data: Vec<u8> },
    PaneExited { pane_id: PaneId },
    LayoutChanged { render_state: RenderState },
    StatsUpdate(SerializableSystemStats),
    SessionEnded,
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
    pub session_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub name: String,
    pub layout: LayoutNode,
    pub groups: Vec<GroupSnapshot>,
    pub active_group: PaneGroupId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupSnapshot {
    pub id: PaneGroupId,
    pub tabs: Vec<PaneSnapshot>,
    pub active_tab: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
    pub exited: bool,
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
// Build RenderState from ServerState
// ---------------------------------------------------------------------------

impl RenderState {
    #[allow(dead_code)]
    pub fn from_server_state(state: &crate::server::state::ServerState) -> Self {
        let workspaces = state
            .workspaces
            .iter()
            .map(|ws| {
                let groups = ws
                    .groups
                    .iter()
                    .map(|(gid, group)| GroupSnapshot {
                        id: *gid,
                        tabs: group
                            .tabs
                            .iter()
                            .map(|pane| PaneSnapshot {
                                id: pane.id,
                                kind: pane.kind.clone(),
                                title: pane.title.clone(),
                                exited: pane.exited,
                            })
                            .collect(),
                        active_tab: group.active_tab,
                    })
                    .collect();
                WorkspaceSnapshot {
                    name: ws.name.clone(),
                    layout: ws.layout.clone(),
                    groups,
                    active_group: ws.active_group,
                }
            })
            .collect();

        RenderState {
            workspaces,
            active_workspace: state.active_workspace,
            session_name: state.session_name.clone(),
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
            ClientRequest::Attach {
                session_name: "test".to_string(),
            },
            ClientRequest::Detach,
            ClientRequest::Resize {
                width: 120,
                height: 40,
            },
            ClientRequest::Key(make_key(KeyCode::Char('a'), KeyModifiers::NONE).into()),
            ClientRequest::MouseDown { x: 10, y: 5 },
            ClientRequest::MouseDrag { x: 15, y: 8 },
            ClientRequest::MouseMove { x: 20, y: 3 },
            ClientRequest::MouseUp,
            ClientRequest::MouseScroll { up: true },
            ClientRequest::Command("list-panes".to_string()),
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
            ServerResponse::Attached {
                session_name: "default".to_string(),
            },
            ServerResponse::PaneOutput {
                pane_id: PaneId::new_v4(),
                data: vec![0x1b, b'[', b'H', b'e', b'l', b'l', b'o'],
            },
            ServerResponse::PaneExited {
                pane_id: PaneId::new_v4(),
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
                layout: LayoutNode::Leaf(PaneGroupId::new_v4()),
                groups: vec![GroupSnapshot {
                    id: PaneGroupId::new_v4(),
                    tabs: vec![PaneSnapshot {
                        id: PaneId::new_v4(),
                        kind: PaneKind::Shell,
                        title: "shell".to_string(),
                        exited: false,
                    }],
                    active_tab: 0,
                }],
                active_group: PaneGroupId::new_v4(),
            }],
            active_workspace: 0,
            session_name: "default".to_string(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: RenderState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.session_name, "default");
        assert_eq!(restored.workspaces.len(), 1);
        assert_eq!(restored.workspaces[0].groups[0].tabs[0].title, "shell");
    }
}
