//! Shared types used by both the TUI client and UI rendering.

use crossterm::event::KeyEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Interact,
    Scroll,
    Copy,
    Palette,
    Confirm,
    Leader,
    TabPicker,
    Rename,
    ContextMenu,
}

pub struct LeaderState {
    pub path: Vec<KeyEvent>,
    pub current_node: crate::config::LeaderNode,
    pub entered_at: std::time::Instant,
    pub popup_visible: bool,
}
