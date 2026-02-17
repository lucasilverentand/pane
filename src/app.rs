//! Shared types used by both the TUI client and UI rendering.

use crossterm::event::KeyEvent;

use crate::pane::PaneGroupId;

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Mode {
    Normal,
    Select,
    Scroll,
    SessionPicker,
    Help,
    DevServerInput,
    Copy,
    CommandPalette,
    Confirm,
    Leader,
}

pub struct LeaderState {
    pub path: Vec<KeyEvent>,
    pub current_node: crate::config::LeaderNode,
    pub entered_at: std::time::Instant,
    pub popup_visible: bool,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum PendingClose {
    Tab { group_id: PaneGroupId, tab_idx: usize },
    Group { group_id: PaneGroupId },
    Workspace { ws_idx: usize },
}
