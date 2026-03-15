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
    Resize,
    NewWorkspaceInput,
    ProjectHub,
}

pub struct LeaderState {
    pub path: Vec<KeyEvent>,
    pub current_node: crate::config::LeaderNode,
    pub popup_visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeBorder {
    Left,
    Right,
    Top,
    Bottom,
}

pub struct ResizeState {
    pub selected: Option<ResizeBorder>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use std::collections::HashMap;

    #[test]
    fn mode_equality() {
        assert_eq!(Mode::Normal, Mode::Normal);
        assert_ne!(Mode::Normal, Mode::Interact);
        assert_ne!(Mode::Scroll, Mode::Copy);
    }

    #[test]
    fn mode_clone() {
        let mode = Mode::Leader;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn all_mode_variants_are_distinct() {
        let modes = vec![
            Mode::Normal,
            Mode::Interact,
            Mode::Scroll,
            Mode::Copy,
            Mode::Palette,
            Mode::Confirm,
            Mode::Leader,
            Mode::TabPicker,
            Mode::Rename,
            Mode::ContextMenu,
            Mode::Resize,
            Mode::NewWorkspaceInput,
            Mode::ProjectHub,
        ];
        // Every pair should be different
        for (i, a) in modes.iter().enumerate() {
            for (j, b) in modes.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Mode variants at {} and {} should differ", i, j);
                }
            }
        }
    }

    #[test]
    fn mode_debug_format() {
        let mode = Mode::Interact;
        let debug = format!("{:?}", mode);
        assert_eq!(debug, "Interact");
    }

    #[test]
    fn resize_border_equality() {
        assert_eq!(ResizeBorder::Left, ResizeBorder::Left);
        assert_ne!(ResizeBorder::Left, ResizeBorder::Right);
        assert_ne!(ResizeBorder::Top, ResizeBorder::Bottom);
    }

    #[test]
    fn resize_border_clone_and_copy() {
        let border = ResizeBorder::Top;
        let copied = border; // Copy
        let cloned = border.clone();
        assert_eq!(border, copied);
        assert_eq!(border, cloned);
    }

    #[test]
    fn resize_state_default_selected_is_none() {
        let state = ResizeState { selected: None };
        assert!(state.selected.is_none());
    }

    #[test]
    fn resize_state_with_selected_border() {
        let state = ResizeState {
            selected: Some(ResizeBorder::Left),
        };
        assert_eq!(state.selected, Some(ResizeBorder::Left));

        let state = ResizeState {
            selected: Some(ResizeBorder::Bottom),
        };
        assert_eq!(state.selected, Some(ResizeBorder::Bottom));
    }

    #[test]
    fn leader_state_construction_empty_path() {
        let state = LeaderState {
            path: vec![],
            current_node: crate::config::LeaderNode::PassThrough,
            popup_visible: false,
        };
        assert!(state.path.is_empty());
        assert!(!state.popup_visible);
    }

    #[test]
    fn leader_state_with_key_path() {
        let key = KeyEvent {
            code: KeyCode::Char('w'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let state = LeaderState {
            path: vec![key],
            current_node: crate::config::LeaderNode::Group {
                label: "window".to_string(),
                children: HashMap::new(),
            },
            popup_visible: true,
        };
        assert_eq!(state.path.len(), 1);
        assert_eq!(state.path[0].code, KeyCode::Char('w'));
        assert!(state.popup_visible);
    }

    #[test]
    fn leader_state_with_leaf_node() {
        let state = LeaderState {
            path: vec![],
            current_node: crate::config::LeaderNode::Leaf {
                action: crate::config::Action::Quit,
                label: "quit".to_string(),
            },
            popup_visible: false,
        };
        match &state.current_node {
            crate::config::LeaderNode::Leaf { action, label } => {
                assert_eq!(*action, crate::config::Action::Quit);
                assert_eq!(label, "quit");
            }
            _ => panic!("Expected Leaf node"),
        }
    }
}
