use crate::layout::{LayoutNode, SplitDirection};
use crate::pane::PaneGroupId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutPreset {
    EvenHorizontal,
    EvenVertical,
    MainHorizontal,
    MainVertical,
    Tiled,
}

impl LayoutPreset {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "even-horizontal" | "even_horizontal" => Some(Self::EvenHorizontal),
            "even-vertical" | "even_vertical" => Some(Self::EvenVertical),
            "main-horizontal" | "main_horizontal" => Some(Self::MainHorizontal),
            "main-vertical" | "main_vertical" => Some(Self::MainVertical),
            "tiled" => Some(Self::Tiled),
            _ => None,
        }
    }

    pub fn build(&self, group_ids: &[PaneGroupId]) -> LayoutNode {
        if group_ids.is_empty() {
            // Should not happen, but return a safe default
            return LayoutNode::Leaf(PaneGroupId::new_v4());
        }
        if group_ids.len() == 1 {
            return LayoutNode::Leaf(group_ids[0]);
        }

        match self {
            LayoutPreset::EvenHorizontal => build_even(group_ids, SplitDirection::Horizontal),
            LayoutPreset::EvenVertical => build_even(group_ids, SplitDirection::Vertical),
            LayoutPreset::MainHorizontal => build_main(group_ids, SplitDirection::Vertical),
            LayoutPreset::MainVertical => build_main(group_ids, SplitDirection::Horizontal),
            LayoutPreset::Tiled => build_tiled(group_ids),
        }
    }
}

/// Build an even split: all panes split equally in the given direction.
/// For N panes, creates a right-leaning binary tree with equal ratios.
fn build_even(ids: &[PaneGroupId], direction: SplitDirection) -> LayoutNode {
    if ids.len() == 1 {
        return LayoutNode::Leaf(ids[0]);
    }
    if ids.len() == 2 {
        return LayoutNode::Split {
            direction,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(ids[0])),
            second: Box::new(LayoutNode::Leaf(ids[1])),
        };
    }
    // Split first pane off, rest go into second subtree
    let ratio = 1.0 / ids.len() as f64;
    LayoutNode::Split {
        direction,
        ratio,
        first: Box::new(LayoutNode::Leaf(ids[0])),
        second: Box::new(build_even(&ids[1..], direction)),
    }
}

/// Build main layout: first pane gets 60% in the given direction,
/// rest are evenly split in the opposite direction.
fn build_main(ids: &[PaneGroupId], direction: SplitDirection) -> LayoutNode {
    if ids.len() == 2 {
        return LayoutNode::Split {
            direction,
            ratio: 0.6,
            first: Box::new(LayoutNode::Leaf(ids[0])),
            second: Box::new(LayoutNode::Leaf(ids[1])),
        };
    }
    let opposite = match direction {
        SplitDirection::Horizontal => SplitDirection::Vertical,
        SplitDirection::Vertical => SplitDirection::Horizontal,
    };
    LayoutNode::Split {
        direction,
        ratio: 0.6,
        first: Box::new(LayoutNode::Leaf(ids[0])),
        second: Box::new(build_even(&ids[1..], opposite)),
    }
}

/// Build tiled layout: recursive even splitting alternating directions.
fn build_tiled(ids: &[PaneGroupId]) -> LayoutNode {
    build_tiled_inner(ids, SplitDirection::Horizontal)
}

fn build_tiled_inner(ids: &[PaneGroupId], direction: SplitDirection) -> LayoutNode {
    if ids.len() == 1 {
        return LayoutNode::Leaf(ids[0]);
    }
    if ids.len() == 2 {
        return LayoutNode::Split {
            direction,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(ids[0])),
            second: Box::new(LayoutNode::Leaf(ids[1])),
        };
    }
    let mid = ids.len() / 2;
    let next_dir = match direction {
        SplitDirection::Horizontal => SplitDirection::Vertical,
        SplitDirection::Vertical => SplitDirection::Horizontal,
    };
    LayoutNode::Split {
        direction,
        ratio: 0.5,
        first: Box::new(build_tiled_inner(&ids[..mid], next_dir)),
        second: Box::new(build_tiled_inner(&ids[mid..], next_dir)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ids(n: usize) -> Vec<PaneGroupId> {
        (0..n).map(|_| PaneGroupId::new_v4()).collect()
    }

    // --- EvenHorizontal ---

    #[test]
    fn test_even_horizontal_1() {
        let ids = make_ids(1);
        let layout = LayoutPreset::EvenHorizontal.build(&ids);
        assert_eq!(layout, LayoutNode::Leaf(ids[0]));
    }

    #[test]
    fn test_even_horizontal_2() {
        let ids = make_ids(2);
        let layout = LayoutPreset::EvenHorizontal.build(&ids);
        let leaves = layout.pane_ids();
        assert_eq!(leaves, ids);
        if let LayoutNode::Split { direction, ratio, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!((ratio - 0.5).abs() < f64::EPSILON);
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_even_horizontal_3() {
        let ids = make_ids(3);
        let layout = LayoutPreset::EvenHorizontal.build(&ids);
        let leaves = layout.pane_ids();
        assert_eq!(leaves, ids);
    }

    #[test]
    fn test_even_horizontal_5() {
        let ids = make_ids(5);
        let layout = LayoutPreset::EvenHorizontal.build(&ids);
        let leaves = layout.pane_ids();
        assert_eq!(leaves, ids);
    }

    // --- EvenVertical ---

    #[test]
    fn test_even_vertical_2() {
        let ids = make_ids(2);
        let layout = LayoutPreset::EvenVertical.build(&ids);
        if let LayoutNode::Split { direction, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Vertical);
        } else {
            panic!("Expected Split");
        }
        assert_eq!(layout.pane_ids(), ids);
    }

    #[test]
    fn test_even_vertical_4() {
        let ids = make_ids(4);
        let layout = LayoutPreset::EvenVertical.build(&ids);
        assert_eq!(layout.pane_ids(), ids);
    }

    // --- MainHorizontal ---

    #[test]
    fn test_main_horizontal_2() {
        let ids = make_ids(2);
        let layout = LayoutPreset::MainHorizontal.build(&ids);
        let leaves = layout.pane_ids();
        assert_eq!(leaves, ids);
        if let LayoutNode::Split { direction, ratio, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert!((ratio - 0.6).abs() < f64::EPSILON);
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_main_horizontal_3() {
        let ids = make_ids(3);
        let layout = LayoutPreset::MainHorizontal.build(&ids);
        let leaves = layout.pane_ids();
        assert_eq!(leaves, ids);
        // First pane gets 60% vertically, rest split horizontally
        if let LayoutNode::Split { direction, ratio, second, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert!((ratio - 0.6).abs() < f64::EPSILON);
            // Second child should be horizontal split
            if let LayoutNode::Split { direction: d2, .. } = second.as_ref() {
                assert_eq!(*d2, SplitDirection::Horizontal);
            } else {
                panic!("Expected inner Split");
            }
        }
    }

    #[test]
    fn test_main_horizontal_5() {
        let ids = make_ids(5);
        let layout = LayoutPreset::MainHorizontal.build(&ids);
        assert_eq!(layout.pane_ids(), ids);
    }

    // --- MainVertical ---

    #[test]
    fn test_main_vertical_2() {
        let ids = make_ids(2);
        let layout = LayoutPreset::MainVertical.build(&ids);
        if let LayoutNode::Split { direction, ratio, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!((ratio - 0.6).abs() < f64::EPSILON);
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_main_vertical_3() {
        let ids = make_ids(3);
        let layout = LayoutPreset::MainVertical.build(&ids);
        let leaves = layout.pane_ids();
        assert_eq!(leaves, ids);
        // First pane gets 60% horizontally, rest split vertically
        if let LayoutNode::Split { direction, second, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Horizontal);
            if let LayoutNode::Split { direction: d2, .. } = second.as_ref() {
                assert_eq!(*d2, SplitDirection::Vertical);
            }
        }
    }

    #[test]
    fn test_main_vertical_4() {
        let ids = make_ids(4);
        let layout = LayoutPreset::MainVertical.build(&ids);
        assert_eq!(layout.pane_ids(), ids);
    }

    // --- Tiled ---

    #[test]
    fn test_tiled_1() {
        let ids = make_ids(1);
        let layout = LayoutPreset::Tiled.build(&ids);
        assert_eq!(layout, LayoutNode::Leaf(ids[0]));
    }

    #[test]
    fn test_tiled_2() {
        let ids = make_ids(2);
        let layout = LayoutPreset::Tiled.build(&ids);
        assert_eq!(layout.pane_ids(), ids);
        if let LayoutNode::Split { direction, ratio, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!((ratio - 0.5).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_tiled_4() {
        let ids = make_ids(4);
        let layout = LayoutPreset::Tiled.build(&ids);
        let leaves = layout.pane_ids();
        assert_eq!(leaves, ids);
        // Should produce a 2x2 grid: horizontal split, each side vertical split
        if let LayoutNode::Split { direction, first, second, .. } = &layout {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!(matches!(first.as_ref(), LayoutNode::Split { direction: SplitDirection::Vertical, .. }));
            assert!(matches!(second.as_ref(), LayoutNode::Split { direction: SplitDirection::Vertical, .. }));
        }
    }

    #[test]
    fn test_tiled_5() {
        let ids = make_ids(5);
        let layout = LayoutPreset::Tiled.build(&ids);
        assert_eq!(layout.pane_ids(), ids);
    }

    #[test]
    fn test_tiled_3() {
        let ids = make_ids(3);
        let layout = LayoutPreset::Tiled.build(&ids);
        assert_eq!(layout.pane_ids(), ids);
    }

    // --- from_name ---

    #[test]
    fn test_from_name() {
        assert_eq!(LayoutPreset::from_name("even-horizontal"), Some(LayoutPreset::EvenHorizontal));
        assert_eq!(LayoutPreset::from_name("even_vertical"), Some(LayoutPreset::EvenVertical));
        assert_eq!(LayoutPreset::from_name("main-horizontal"), Some(LayoutPreset::MainHorizontal));
        assert_eq!(LayoutPreset::from_name("main-vertical"), Some(LayoutPreset::MainVertical));
        assert_eq!(LayoutPreset::from_name("tiled"), Some(LayoutPreset::Tiled));
        assert_eq!(LayoutPreset::from_name("unknown"), None);
    }

    // --- Preserves all IDs ---

    #[test]
    fn test_all_presets_preserve_ids() {
        for n in 1..=6 {
            let ids = make_ids(n);
            for preset in &[
                LayoutPreset::EvenHorizontal,
                LayoutPreset::EvenVertical,
                LayoutPreset::MainHorizontal,
                LayoutPreset::MainVertical,
                LayoutPreset::Tiled,
            ] {
                let layout = preset.build(&ids);
                let leaves = layout.pane_ids();
                assert_eq!(
                    leaves, ids,
                    "Preset {:?} with {} groups should preserve all IDs",
                    preset, n
                );
            }
        }
    }
}
