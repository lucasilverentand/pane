use ratatui::layout::{Constraint, Layout, Rect};
use serde::{Deserialize, Serialize};

pub type PaneId = uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    First,
    Second,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LayoutNode {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        ratio: f64,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    /// Resolve the layout tree into a flat list of (PaneId, Rect) pairs.
    pub fn resolve(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut result = Vec::new();
        self.resolve_inner(area, &mut result);
        result
    }

    fn resolve_inner(&self, area: Rect, result: &mut Vec<(PaneId, Rect)>) {
        match self {
            LayoutNode::Leaf(id) => {
                result.push((*id, area));
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let ratio_pct = (*ratio * 100.0) as u32;
                let remainder = 100 - ratio_pct;
                let chunks = match direction {
                    SplitDirection::Horizontal => Layout::horizontal([
                        Constraint::Percentage(ratio_pct as u16),
                        Constraint::Percentage(remainder as u16),
                    ])
                    .split(area),
                    SplitDirection::Vertical => Layout::vertical([
                        Constraint::Percentage(ratio_pct as u16),
                        Constraint::Percentage(remainder as u16),
                    ])
                    .split(area),
                };
                first.resolve_inner(chunks[0], result);
                second.resolve_inner(chunks[1], result);
            }
        }
    }

    /// Split a target pane into two, placing the new pane in the second position.
    pub fn split_pane(
        &mut self,
        target: PaneId,
        direction: SplitDirection,
        new_id: PaneId,
    ) -> bool {
        match self {
            LayoutNode::Leaf(id) if *id == target => {
                *self = LayoutNode::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::Leaf(target)),
                    second: Box::new(LayoutNode::Leaf(new_id)),
                };
                true
            }
            LayoutNode::Split { first, second, .. } => {
                first.split_pane(target, direction, new_id)
                    || second.split_pane(target, direction, new_id)
            }
            _ => false,
        }
    }

    /// Close a pane, replacing its parent split with the sibling.
    /// Returns the sibling's first leaf ID (for focusing).
    pub fn close_pane(&mut self, target: PaneId) -> Option<PaneId> {
        match self {
            LayoutNode::Leaf(_) => None,
            LayoutNode::Split { first, second, .. } => {
                // Check if either child is the target leaf
                if matches!(first.as_ref(), LayoutNode::Leaf(id) if *id == target) {
                    let sibling = *second.clone();
                    let focus = sibling.first_leaf();
                    *self = sibling;
                    return Some(focus);
                }
                if matches!(second.as_ref(), LayoutNode::Leaf(id) if *id == target) {
                    let sibling = *first.clone();
                    let focus = sibling.first_leaf();
                    *self = sibling;
                    return Some(focus);
                }
                // Recurse
                first.close_pane(target).or_else(|| second.close_pane(target))
            }
        }
    }

    /// Resize the split containing the target pane by adjusting the ratio.
    pub fn resize(&mut self, target: PaneId, delta: f64) -> bool {
        match self {
            LayoutNode::Leaf(_) => false,
            LayoutNode::Split {
                ratio,
                first,
                second,
                ..
            } => {
                let in_first = first.contains(target);
                let in_second = second.contains(target);
                if in_first || in_second {
                    // If the target is directly in this split, adjust ratio
                    let is_direct = matches!(first.as_ref(), LayoutNode::Leaf(id) if *id == target)
                        || matches!(second.as_ref(), LayoutNode::Leaf(id) if *id == target);
                    if is_direct {
                        let adjusted = if in_first { delta } else { -delta };
                        *ratio = (*ratio + adjusted).clamp(0.1, 0.9);
                        return true;
                    }
                    // Recurse into the subtree containing the target
                    if in_first {
                        return first.resize(target, delta);
                    }
                    return second.resize(target, delta);
                }
                false
            }
        }
    }

    /// Find a neighbor pane in the given direction from the target.
    pub fn find_neighbor(
        &self,
        target: PaneId,
        direction: SplitDirection,
        side: Side,
    ) -> Option<PaneId> {
        self.find_neighbor_inner(target, direction, side)
            .and_then(|result| match result {
                NeighborResult::Found(id) => Some(id),
                NeighborResult::NeedFromParent => None,
            })
    }

    fn find_neighbor_inner(
        &self,
        target: PaneId,
        direction: SplitDirection,
        side: Side,
    ) -> Option<NeighborResult> {
        match self {
            LayoutNode::Leaf(id) => {
                if *id == target {
                    Some(NeighborResult::NeedFromParent)
                } else {
                    None
                }
            }
            LayoutNode::Split {
                direction: split_dir,
                first,
                second,
                ..
            } => {
                // Try first subtree
                if let Some(result) = first.find_neighbor_inner(target, direction, side) {
                    match result {
                        NeighborResult::Found(id) => return Some(NeighborResult::Found(id)),
                        NeighborResult::NeedFromParent => {
                            if *split_dir == direction && side == Side::Second {
                                // The neighbor is in the second subtree
                                return Some(NeighborResult::Found(second.edge_leaf(Side::First)));
                            }
                            return Some(NeighborResult::NeedFromParent);
                        }
                    }
                }
                // Try second subtree
                if let Some(result) = second.find_neighbor_inner(target, direction, side) {
                    match result {
                        NeighborResult::Found(id) => return Some(NeighborResult::Found(id)),
                        NeighborResult::NeedFromParent => {
                            if *split_dir == direction && side == Side::First {
                                // The neighbor is in the first subtree
                                return Some(NeighborResult::Found(
                                    first.edge_leaf(Side::Second),
                                ));
                            }
                            return Some(NeighborResult::NeedFromParent);
                        }
                    }
                }
                None
            }
        }
    }

    /// Get the leaf at the edge of this subtree.
    fn edge_leaf(&self, side: Side) -> PaneId {
        match self {
            LayoutNode::Leaf(id) => *id,
            LayoutNode::Split { first, second, .. } => match side {
                Side::First => first.edge_leaf(Side::First),
                Side::Second => second.edge_leaf(Side::Second),
            },
        }
    }

    /// Get all pane IDs in left-to-right, top-to-bottom order.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, ids: &mut Vec<PaneId>) {
        match self {
            LayoutNode::Leaf(id) => ids.push(*id),
            LayoutNode::Split { first, second, .. } => {
                first.collect_ids(ids);
                second.collect_ids(ids);
            }
        }
    }

    /// Alias for `pane_ids` — leaves now semantically represent PaneGroupIds.
    pub fn group_ids(&self) -> Vec<PaneId> {
        self.pane_ids()
    }

    /// Set all split ratios to 0.5.
    pub fn equalize(&mut self) {
        if let LayoutNode::Split {
            ratio,
            first,
            second,
            ..
        } = self
        {
            *ratio = 0.5;
            first.equalize();
            second.equalize();
        }
    }

    /// Check if this subtree contains the given pane.
    pub fn contains(&self, target: PaneId) -> bool {
        match self {
            LayoutNode::Leaf(id) => *id == target,
            LayoutNode::Split { first, second, .. } => {
                first.contains(target) || second.contains(target)
            }
        }
    }

    /// Get the first leaf in this subtree.
    pub fn first_leaf(&self) -> PaneId {
        match self {
            LayoutNode::Leaf(id) => *id,
            LayoutNode::Split { first, .. } => first.first_leaf(),
        }
    }
}

enum NeighborResult {
    Found(PaneId),
    NeedFromParent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_single_leaf() {
        let id = PaneId::new_v4();
        let node = LayoutNode::Leaf(id);
        let area = Rect::new(0, 0, 100, 50);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, id);
        assert_eq!(resolved[0].1, area);
    }

    #[test]
    fn test_split_pane() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Leaf(id1);
        assert!(node.split_pane(id1, SplitDirection::Horizontal, id2));

        let ids = node.pane_ids();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], id1);
        assert_eq!(ids[1], id2);
    }

    #[test]
    fn test_close_pane() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        let focus = node.close_pane(id2);
        assert_eq!(focus, Some(id1));
        assert!(matches!(node, LayoutNode::Leaf(id) if id == id1));
    }

    #[test]
    fn test_resize() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        assert!(node.resize(id1, 0.1));
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((ratio - 0.6).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_equalize() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.7,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        node.equalize();
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((ratio - 0.5).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_find_neighbor() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        assert_eq!(
            node.find_neighbor(id1, SplitDirection::Horizontal, Side::Second),
            Some(id2)
        );
        assert_eq!(
            node.find_neighbor(id2, SplitDirection::Horizontal, Side::First),
            Some(id1)
        );
        // No vertical neighbor in a horizontal split
        assert_eq!(
            node.find_neighbor(id1, SplitDirection::Vertical, Side::Second),
            None
        );
    }

    #[test]
    fn test_resolve_split() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        let area = Rect::new(0, 0, 100, 50);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].0, id1);
        assert_eq!(resolved[1].0, id2);
        // Each should get roughly half the width
        assert!(resolved[0].1.width >= 45 && resolved[0].1.width <= 55);
        assert!(resolved[1].1.width >= 45 && resolved[1].1.width <= 55);
    }

    // --- Deep nesting tests ---

    /// Build the DESIGN.md example layout:
    /// root split(H) → [pane1, split(V) → [pane2, split(H) → [pane3, pane4]]]
    fn build_design_example() -> (LayoutNode, PaneId, PaneId, PaneId, PaneId) {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let id3 = PaneId::new_v4();
        let id4 = PaneId::new_v4();

        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.3,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(id2)),
                second: Box::new(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::Leaf(id3)),
                    second: Box::new(LayoutNode::Leaf(id4)),
                }),
            }),
        };
        (node, id1, id2, id3, id4)
    }

    #[test]
    fn test_resolve_deep_nesting() {
        let (node, id1, id2, id3, id4) = build_design_example();
        let area = Rect::new(0, 0, 200, 60);
        let resolved = node.resolve(area);

        assert_eq!(resolved.len(), 4);
        assert_eq!(resolved[0].0, id1);
        assert_eq!(resolved[1].0, id2);
        assert_eq!(resolved[2].0, id3);
        assert_eq!(resolved[3].0, id4);

        // id1 is the left 30%, should be roughly 60px wide
        assert!(resolved[0].1.width >= 55 && resolved[0].1.width <= 65);
        // id1 gets full height
        assert_eq!(resolved[0].1.height, 60);
    }

    #[test]
    fn test_pane_ids_ordering() {
        let (node, id1, id2, id3, id4) = build_design_example();
        let ids = node.pane_ids();
        assert_eq!(ids, vec![id1, id2, id3, id4]);
    }

    #[test]
    fn test_contains() {
        let (node, id1, _id2, _id3, id4) = build_design_example();
        assert!(node.contains(id1));
        assert!(node.contains(id4));
        assert!(!node.contains(PaneId::new_v4()));
    }

    #[test]
    fn test_first_leaf() {
        let (node, id1, _, _, _) = build_design_example();
        assert_eq!(node.first_leaf(), id1);
    }

    #[test]
    fn test_close_in_nested_tree() {
        let (mut node, id1, id2, id3, id4) = build_design_example();

        // Close id3 — its parent split should collapse to id4
        let focus = node.close_pane(id3);
        assert_eq!(focus, Some(id4));
        let ids = node.pane_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
        assert!(ids.contains(&id4));
        assert!(!ids.contains(&id3));
    }

    #[test]
    fn test_close_first_child_in_nested() {
        let (mut node, _id1, id2, id3, id4) = build_design_example();

        // Close id2 — its parent split(V) collapses, replacing it with the sibling subtree
        let focus = node.close_pane(id2);
        assert!(focus.is_some());
        let ids = node.pane_ids();
        assert_eq!(ids.len(), 3);
        assert!(!ids.contains(&id2));
        assert!(ids.contains(&id3));
        assert!(ids.contains(&id4));
    }

    #[test]
    fn test_close_returns_none_for_single_leaf() {
        let id = PaneId::new_v4();
        let mut node = LayoutNode::Leaf(id);
        assert_eq!(node.close_pane(id), None);
    }

    #[test]
    fn test_close_nonexistent_pane() {
        let (mut node, _, _, _, _) = build_design_example();
        let result = node.close_pane(PaneId::new_v4());
        assert_eq!(result, None);
    }

    #[test]
    fn test_split_nonexistent_target() {
        let id = PaneId::new_v4();
        let mut node = LayoutNode::Leaf(id);
        assert!(!node.split_pane(PaneId::new_v4(), SplitDirection::Horizontal, PaneId::new_v4()));
    }

    #[test]
    fn test_split_in_nested_tree() {
        let (mut node, _id1, id2, _id3, _id4) = build_design_example();
        let new_id = PaneId::new_v4();
        assert!(node.split_pane(id2, SplitDirection::Vertical, new_id));
        let ids = node.pane_ids();
        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&new_id));
    }

    #[test]
    fn test_resize_clamp_min() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.15,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        // Try to shrink below minimum
        node.resize(id1, -0.2);
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!(*ratio >= 0.1);
        }
    }

    #[test]
    fn test_resize_clamp_max() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.85,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        node.resize(id1, 0.2);
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!(*ratio <= 0.9);
        }
    }

    #[test]
    fn test_resize_nonexistent_returns_false() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert!(!node.resize(PaneId::new_v4(), 0.1));
    }

    #[test]
    fn test_resize_second_child() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        // Resizing the second child should shrink first (negative delta applied)
        node.resize(id2, 0.1);
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((ratio - 0.4).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_equalize_deep() {
        let (mut node, _, _, _, _) = build_design_example();
        // Set various ratios
        if let LayoutNode::Split { ratio, .. } = &mut node {
            *ratio = 0.2;
        }
        node.equalize();

        // Check that all ratios are 0.5
        fn check_ratios(node: &LayoutNode) {
            if let LayoutNode::Split {
                ratio,
                first,
                second,
                ..
            } = node
            {
                assert!((*ratio - 0.5).abs() < f64::EPSILON);
                check_ratios(first);
                check_ratios(second);
            }
        }
        check_ratios(&node);
    }

    #[test]
    fn test_resolve_vertical_split() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        let area = Rect::new(0, 0, 100, 60);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);

        // Both should get full width
        assert_eq!(resolved[0].1.width, 100);
        assert_eq!(resolved[1].1.width, 100);

        // Each should get roughly half the height
        assert!(resolved[0].1.height >= 25 && resolved[0].1.height <= 35);
        assert!(resolved[1].1.height >= 25 && resolved[1].1.height <= 35);
    }

    #[test]
    fn test_find_neighbor_in_nested() {
        let (node, id1, id2, id3, id4) = build_design_example();

        // id1 → right should find id2 (first leaf in right subtree)
        assert_eq!(
            node.find_neighbor(id1, SplitDirection::Horizontal, Side::Second),
            Some(id2)
        );

        // id3 → left should find id1 (closest horizontal neighbor going left)
        // Actually id3 is in the right subtree, its horizontal parent puts id3 left of id4
        assert_eq!(
            node.find_neighbor(id3, SplitDirection::Horizontal, Side::Second),
            Some(id4)
        );
        assert_eq!(
            node.find_neighbor(id4, SplitDirection::Horizontal, Side::First),
            Some(id3)
        );

        // id2 → down should find id3 (or id4, whichever is the first leaf below)
        assert_eq!(
            node.find_neighbor(id2, SplitDirection::Vertical, Side::Second),
            Some(id3)
        );
    }

    #[test]
    fn test_find_neighbor_no_neighbor() {
        let (node, id1, _, _, _) = build_design_example();
        // id1 is the leftmost pane, no left neighbor
        assert_eq!(
            node.find_neighbor(id1, SplitDirection::Horizontal, Side::First),
            None
        );
    }

    #[test]
    fn test_serialization_roundtrip() {
        let (node, _, _, _, _) = build_design_example();
        let json = serde_json::to_string(&node).unwrap();
        let deserialized: LayoutNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node.pane_ids(), deserialized.pane_ids());
    }
}
