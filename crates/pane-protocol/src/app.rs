//! Shared types used by both the TUI client and UI rendering.

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
}
