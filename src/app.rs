//! Shared types used by both the TUI client and UI rendering.

use crossterm::event::KeyEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseMode {
    Normal,
    Interact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overlay {
    Scroll,
    Copy,
    Resize,
    ClientPicker,
    CommandPalette,
    Confirm,
    Leader,
    TabPicker,
    NoWorkspaces,
    NewPane,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mode {
    pub base: BaseMode,
    pub overlay: Option<Overlay>,
}

impl Mode {
    pub fn normal() -> Self {
        Self {
            base: BaseMode::Normal,
            overlay: None,
        }
    }

    pub fn interact() -> Self {
        Self {
            base: BaseMode::Interact,
            overlay: None,
        }
    }

    /// Push an overlay, preserving the current base mode.
    pub fn push_overlay(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
    }

    /// Dismiss the current overlay, returning to the base mode.
    pub fn dismiss_overlay(&mut self) {
        self.overlay = None;
    }
}

pub struct LeaderState {
    pub path: Vec<KeyEvent>,
    pub current_node: crate::config::LeaderNode,
    pub entered_at: std::time::Instant,
    pub popup_visible: bool,
}
