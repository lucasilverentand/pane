//! Shared types used by both the TUI client and UI rendering.

use crossterm::event::KeyEvent;

pub struct LeaderState {
    pub path: Vec<KeyEvent>,
    pub current_node: crate::config::LeaderNode,
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
    fn resize_border_equality() {
        assert_eq!(ResizeBorder::Left, ResizeBorder::Left);
        assert_ne!(ResizeBorder::Left, ResizeBorder::Right);
        assert_ne!(ResizeBorder::Top, ResizeBorder::Bottom);
    }

    #[test]
    fn resize_border_clone_and_copy() {
        let border = ResizeBorder::Top;
        let copied = border; // Copy
        let cloned = border;
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
        };
        assert!(state.path.is_empty());
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
        };
        assert_eq!(state.path.len(), 1);
        assert_eq!(state.path[0].code, KeyCode::Char('w'));
    }

    #[test]
    fn leader_state_with_leaf_node() {
        let state = LeaderState {
            path: vec![],
            current_node: crate::config::LeaderNode::Leaf {
                action: crate::config::Action::Quit,
                label: "quit".to_string(),
            },
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
