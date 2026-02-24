use std::collections::HashSet;

use ratatui::layout::{Constraint, Layout, Rect};
use serde::{Deserialize, Serialize};

pub type TabId = uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedPane {
    Visible {
        id: TabId,
        rect: Rect,
    },
    Folded {
        id: TabId,
        rect: Rect,
        direction: SplitDirection,
    },
}

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayoutNode {
    Leaf(TabId),
    Split {
        direction: SplitDirection,
        ratio: f64,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    /// Resolve the layout tree into a flat list of (TabId, Rect) pairs.
    pub fn resolve(&self, area: Rect) -> Vec<(TabId, Rect)> {
        let mut result = Vec::new();
        self.resolve_inner(area, &mut result);
        result
    }

    fn resolve_inner(&self, area: Rect, result: &mut Vec<(TabId, Rect)>) {
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

    /// Compute the two child rects for a Split node given direction, ratio, and area.
    fn split_rects(direction: &SplitDirection, ratio: f64, area: Rect) -> (Rect, Rect) {
        let ratio_pct = (ratio * 100.0) as u32;
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
        (chunks[0], chunks[1])
    }

    /// Resolve layout with manual folds: windows in `folded` are collapsed
    /// to a fold bar while siblings get proportionally more space.
    pub fn resolve_with_folds(
        &self,
        area: Rect,
        folded: &HashSet<TabId>,
    ) -> Vec<ResolvedPane> {
        let mut result = Vec::new();
        self.resolve_folds_inner(area, folded, &mut result);
        result
    }

    fn resolve_folds_inner(
        &self,
        area: Rect,
        folded: &HashSet<TabId>,
        result: &mut Vec<ResolvedPane>,
    ) {
        match self {
            LayoutNode::Leaf(id) => {
                if folded.contains(id) {
                    // Should not happen at top level — a lone leaf can't be folded.
                    // Render it visible as a fallback.
                    result.push(ResolvedPane::Visible {
                        id: *id,
                        rect: area,
                    });
                } else {
                    result.push(ResolvedPane::Visible {
                        id: *id,
                        rect: area,
                    });
                }
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first_all_folded = first.all_leaves_folded(folded);
                let second_all_folded = second.all_leaves_folded(folded);

                if first_all_folded && second_all_folded {
                    // Both sides fully folded — fold everything
                    Self::fold_subtree(self, area, *direction, result);
                } else if first_all_folded {
                    // Fold first, give remaining space to second
                    let fl = first.fold_cell_count(*direction);
                    let (bar, expanded) =
                        Self::fold_redistribute(direction, area, false, fl);
                    Self::fold_subtree(first, bar, *direction, result);
                    second.resolve_folds_inner(expanded, folded, result);
                } else if second_all_folded {
                    // Fold second, give remaining space to first
                    let sl = second.fold_cell_count(*direction);
                    let (expanded, bar) =
                        Self::fold_redistribute(direction, area, true, sl);
                    first.resolve_folds_inner(expanded, folded, result);
                    Self::fold_subtree(second, bar, *direction, result);
                } else {
                    // Neither fully folded — proportional split as normal
                    let (first_rect, second_rect) = Self::split_rects(direction, *ratio, area);
                    first.resolve_folds_inner(first_rect, folded, result);
                    second.resolve_folds_inner(second_rect, folded, result);
                }
            }
        }
    }

    /// Check if all leaves in a subtree are in the folded set.
    fn all_leaves_folded(&self, folded: &HashSet<TabId>) -> bool {
        match self {
            LayoutNode::Leaf(id) => folded.contains(id),
            LayoutNode::Split { first, second, .. } => {
                first.all_leaves_folded(folded) && second.all_leaves_folded(folded)
            }
        }
    }

    fn fold_redistribute(
        direction: &SplitDirection,
        area: Rect,
        fold_second: bool,
        fold_leaf_count: u16,
    ) -> (Rect, Rect) {
        // Each folded leaf gets 1 cell. The expanded pane gets the rest.
        match direction {
            SplitDirection::Horizontal => {
                let bar_w = fold_leaf_count.min(area.width);
                let main_w = area.width.saturating_sub(bar_w);
                if fold_second {
                    (
                        Rect::new(area.x, area.y, main_w, area.height),
                        Rect::new(area.x + main_w, area.y, bar_w, area.height),
                    )
                } else {
                    (
                        Rect::new(area.x, area.y, bar_w, area.height),
                        Rect::new(area.x + bar_w, area.y, main_w, area.height),
                    )
                }
            }
            SplitDirection::Vertical => {
                let bar_h = fold_leaf_count.min(area.height);
                let main_h = area.height.saturating_sub(bar_h);
                if fold_second {
                    (
                        Rect::new(area.x, area.y, area.width, main_h),
                        Rect::new(area.x, area.y + main_h, area.width, bar_h),
                    )
                } else {
                    (
                        Rect::new(area.x, area.y, area.width, bar_h),
                        Rect::new(area.x, area.y + bar_h, area.width, main_h),
                    )
                }
            }
        }
    }

    /// Fold an entire subtree recursively, preserving internal split structure.
    /// - Leaf: gets a single fold bar in the parent direction.
    /// - Split same direction as fold: children tile along fold axis.
    /// - Split cross direction: bar is split perpendicular by ratio, each child gets its portion.
    fn fold_subtree(
        node: &LayoutNode,
        bar_rect: Rect,
        fold_direction: SplitDirection,
        result: &mut Vec<ResolvedPane>,
    ) {
        match node {
            LayoutNode::Leaf(id) => {
                result.push(ResolvedPane::Folded {
                    id: *id,
                    rect: bar_rect,
                    direction: fold_direction,
                });
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
                ..
            } => {
                if *direction == fold_direction {
                    // Same direction: tile children along fold axis
                    let fc = first.fold_cell_count(fold_direction);
                    let sc = second.fold_cell_count(fold_direction);
                    let total = fc + sc;
                    let (first_bar, second_bar) = match fold_direction {
                        SplitDirection::Horizontal => {
                            let fw = if total > 0 {
                                (bar_rect.width as u32 * fc as u32 / total as u32) as u16
                            } else {
                                0
                            };
                            let fw = fw.min(bar_rect.width);
                            (
                                Rect::new(bar_rect.x, bar_rect.y, fw, bar_rect.height),
                                Rect::new(
                                    bar_rect.x + fw,
                                    bar_rect.y,
                                    bar_rect.width.saturating_sub(fw),
                                    bar_rect.height,
                                ),
                            )
                        }
                        SplitDirection::Vertical => {
                            let fh = if total > 0 {
                                (bar_rect.height as u32 * fc as u32 / total as u32) as u16
                            } else {
                                0
                            };
                            let fh = fh.min(bar_rect.height);
                            (
                                Rect::new(bar_rect.x, bar_rect.y, bar_rect.width, fh),
                                Rect::new(
                                    bar_rect.x,
                                    bar_rect.y + fh,
                                    bar_rect.width,
                                    bar_rect.height.saturating_sub(fh),
                                ),
                            )
                        }
                    };
                    Self::fold_subtree(first, first_bar, fold_direction, result);
                    Self::fold_subtree(second, second_bar, fold_direction, result);
                } else {
                    // Cross direction: split bar_rect perpendicular using ratio
                    let (first_bar, second_bar) = Self::split_rects(direction, *ratio, bar_rect);
                    Self::fold_subtree(first, first_bar, fold_direction, result);
                    Self::fold_subtree(second, second_bar, fold_direction, result);
                }
            }
        }
    }

    /// Split a target pane into two, placing the new pane in the second position.
    pub fn split_pane(&mut self, target: TabId, direction: SplitDirection, new_id: TabId) -> bool {
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
    pub fn close_pane(&mut self, target: TabId) -> Option<TabId> {
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
                first
                    .close_pane(target)
                    .or_else(|| second.close_pane(target))
            }
        }
    }

    /// Resize the split containing the target pane by adjusting the ratio.
    pub fn resize(&mut self, target: TabId, delta: f64) -> bool {
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
        target: TabId,
        direction: SplitDirection,
        side: Side,
    ) -> Option<TabId> {
        self.find_neighbor_inner(target, direction, side)
            .and_then(|result| match result {
                NeighborResult::Found(id) => Some(id),
                NeighborResult::NeedFromParent => None,
            })
    }

    fn find_neighbor_inner(
        &self,
        target: TabId,
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
                                return Some(NeighborResult::Found(first.edge_leaf(Side::Second)));
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
    fn edge_leaf(&self, side: Side) -> TabId {
        match self {
            LayoutNode::Leaf(id) => *id,
            LayoutNode::Split { first, second, .. } => match side {
                Side::First => first.edge_leaf(Side::First),
                Side::Second => second.edge_leaf(Side::Second),
            },
        }
    }

    /// How many cells this subtree needs when folded in the given direction.
    /// Same-direction splits tile children along the fold axis (sum),
    /// cross-direction splits stack perpendicular (max).
    fn fold_cell_count(&self, fold_direction: SplitDirection) -> u16 {
        match self {
            LayoutNode::Leaf(_) => 1,
            LayoutNode::Split {
                direction,
                first,
                second,
                ..
            } => {
                let fc = first.fold_cell_count(fold_direction);
                let sc = second.fold_cell_count(fold_direction);
                if *direction == fold_direction {
                    fc + sc
                } else {
                    fc.max(sc)
                }
            }
        }
    }

    /// Get all pane IDs in left-to-right, top-to-bottom order.
    pub fn pane_ids(&self) -> Vec<TabId> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, ids: &mut Vec<TabId>) {
        match self {
            LayoutNode::Leaf(id) => ids.push(*id),
            LayoutNode::Split { first, second, .. } => {
                first.collect_ids(ids);
                second.collect_ids(ids);
            }
        }
    }

    /// Alias for `pane_ids` — leaves now semantically represent WindowIds.
    pub fn group_ids(&self) -> Vec<TabId> {
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

    /// Maximize a specific leaf by pushing split ratios toward it.
    pub fn maximize_leaf(&mut self, target: TabId) {
        if let LayoutNode::Split {
            ratio,
            first,
            second,
            ..
        } = self
        {
            let in_first = first.contains(target);
            let in_second = second.contains(target);
            if in_first {
                *ratio = 0.95;
                first.maximize_leaf(target);
                second.equalize();
            } else if in_second {
                *ratio = 0.05;
                first.equalize();
                second.maximize_leaf(target);
            }
        }
    }

    /// Hit-test split borders. Returns the path to the split node and direction
    /// if (x, y) is within 1 cell of a split border.
    pub fn hit_test_split_border(
        &self,
        area: Rect,
        x: u16,
        y: u16,
    ) -> Option<(Vec<Side>, SplitDirection)> {
        self.hit_test_border_inner(area, x, y, &mut Vec::new())
    }

    fn hit_test_border_inner(
        &self,
        area: Rect,
        x: u16,
        y: u16,
        path: &mut Vec<Side>,
    ) -> Option<(Vec<Side>, SplitDirection)> {
        if let LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
            ..
        } = self
        {
            let (first_rect, second_rect) = Self::split_rects(direction, *ratio, area);

            // Check if the click is on the border between the two children
            let on_border = match direction {
                SplitDirection::Horizontal => {
                    let border_x = first_rect.x + first_rect.width;
                    x >= border_x.saturating_sub(1)
                        && x <= border_x + 1
                        && y >= area.y
                        && y < area.y + area.height
                }
                SplitDirection::Vertical => {
                    let border_y = first_rect.y + first_rect.height;
                    y >= border_y.saturating_sub(1)
                        && y <= border_y + 1
                        && x >= area.x
                        && x < area.x + area.width
                }
            };

            if on_border {
                return Some((path.clone(), *direction));
            }

            // Recurse into children
            path.push(Side::First);
            if let Some(result) = first.hit_test_border_inner(first_rect, x, y, path) {
                return Some(result);
            }
            path.pop();

            path.push(Side::Second);
            if let Some(result) = second.hit_test_border_inner(second_rect, x, y, path) {
                return Some(result);
            }
            path.pop();
        }
        None
    }

    /// Set the ratio at a given path through the tree.
    pub fn set_ratio_at_path(&mut self, path: &[Side], ratio: f64) {
        let ratio = ratio.clamp(0.05, 0.95);
        if path.is_empty() {
            if let LayoutNode::Split { ratio: r, .. } = self {
                *r = ratio;
            }
            return;
        }
        if let LayoutNode::Split { first, second, .. } = self {
            match path[0] {
                Side::First => first.set_ratio_at_path(&path[1..], ratio),
                Side::Second => second.set_ratio_at_path(&path[1..], ratio),
            }
        }
    }

    /// Check if this subtree contains the given pane.
    pub fn contains(&self, target: TabId) -> bool {
        match self {
            LayoutNode::Leaf(id) => *id == target,
            LayoutNode::Split { first, second, .. } => {
                first.contains(target) || second.contains(target)
            }
        }
    }

    /// Get the first leaf in this subtree.
    pub fn first_leaf(&self) -> TabId {
        match self {
            LayoutNode::Leaf(id) => *id,
            LayoutNode::Split { first, .. } => first.first_leaf(),
        }
    }
}

enum NeighborResult {
    Found(TabId),
    NeedFromParent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_single_leaf() {
        let id = TabId::new_v4();
        let node = LayoutNode::Leaf(id);
        let area = Rect::new(0, 0, 100, 50);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, id);
        assert_eq!(resolved[0].1, area);
    }

    #[test]
    fn test_split_pane() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Leaf(id1);
        assert!(node.split_pane(id1, SplitDirection::Horizontal, id2));

        let ids = node.pane_ids();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], id1);
        assert_eq!(ids[1], id2);
    }

    #[test]
    fn test_close_pane() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
    fn build_design_example() -> (LayoutNode, TabId, TabId, TabId, TabId) {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let id4 = TabId::new_v4();

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
        assert!(!node.contains(TabId::new_v4()));
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
        let id = TabId::new_v4();
        let mut node = LayoutNode::Leaf(id);
        assert_eq!(node.close_pane(id), None);
    }

    #[test]
    fn test_close_nonexistent_pane() {
        let (mut node, _, _, _, _) = build_design_example();
        let result = node.close_pane(TabId::new_v4());
        assert_eq!(result, None);
    }

    #[test]
    fn test_split_nonexistent_target() {
        let id = TabId::new_v4();
        let mut node = LayoutNode::Leaf(id);
        assert!(!node.split_pane(TabId::new_v4(), SplitDirection::Horizontal, TabId::new_v4()));
    }

    #[test]
    fn test_split_in_nested_tree() {
        let (mut node, _id1, id2, _id3, _id4) = build_design_example();
        let new_id = TabId::new_v4();
        assert!(node.split_pane(id2, SplitDirection::Vertical, new_id));
        let ids = node.pane_ids();
        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&new_id));
    }

    #[test]
    fn test_resize_clamp_min() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert!(!node.resize(TabId::new_v4(), 0.1));
    }

    #[test]
    fn test_resize_second_child() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
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


    // --- Manual fold tests ---

    #[test]
    fn test_manual_fold_single_leaf_stays_visible() {
        let id = TabId::new_v4();
        let node = LayoutNode::Leaf(id);
        let area = Rect::new(0, 0, 100, 50);
        // A lone leaf in the folded set is still visible (can't fold the only pane)
        let mut folded = HashSet::new();
        folded.insert(id);
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 1);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id: rid, .. } if rid == id));
    }

    #[test]
    fn test_manual_fold_one_side() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 50);
        let mut folded = HashSet::new();
        folded.insert(id2);
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Folded { id, direction: SplitDirection::Horizontal, .. } if id == id2));
        // Visible pane should get most of the space
        if let ResolvedPane::Visible { rect, .. } = &resolved[0] {
            assert!(rect.width > 190, "visible pane should get most space");
        }
    }

    #[test]
    fn test_manual_fold_first_side() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 50);
        let mut folded = HashSet::new();
        folded.insert(id1);
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Folded { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, .. } if id == id2));
    }

    #[test]
    fn test_manual_fold_both_sides() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 50);
        let mut folded = HashSet::new();
        folded.insert(id1);
        folded.insert(id2);
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Folded { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Folded { id, .. } if id == id2));
    }

    #[test]
    fn test_manual_fold_no_folds() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 50);
        let folded = HashSet::new();
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, .. } if id == id2));
    }

    #[test]
    fn test_manual_fold_nested_subtree() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(id2)),
                second: Box::new(LayoutNode::Leaf(id3)),
            }),
        };
        let area = Rect::new(0, 0, 200, 50);
        // Fold entire right subtree
        let mut folded = HashSet::new();
        folded.insert(id2);
        folded.insert(id3);
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 3);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        // Both right panes should be folded
        let fold_count = resolved.iter().filter(|rp| matches!(rp, ResolvedPane::Folded { .. })).count();
        assert_eq!(fold_count, 2);
    }

    #[test]
    fn test_manual_fold_partial_subtree() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(id2)),
                second: Box::new(LayoutNode::Leaf(id3)),
            }),
        };
        let area = Rect::new(0, 0, 200, 50);
        // Fold only id3 — id2 stays visible
        let mut folded = HashSet::new();
        folded.insert(id3);
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 3);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(resolved.iter().any(|rp| matches!(rp, ResolvedPane::Visible { id, .. } if *id == id2)));
        assert!(resolved.iter().any(|rp| matches!(rp, ResolvedPane::Folded { id, .. } if *id == id3)));
    }

    #[test]
    fn test_manual_fold_vertical() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 100, 50);
        let mut folded = HashSet::new();
        folded.insert(id2);
        let resolved = node.resolve_with_folds(area, &folded);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Folded { id, direction: SplitDirection::Vertical, .. } if id == id2));
    }

    #[test]
    fn test_all_leaves_folded() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let empty = HashSet::new();
        assert!(!node.all_leaves_folded(&empty));
        let mut one = HashSet::new();
        one.insert(id1);
        assert!(!node.all_leaves_folded(&one));
        let mut both = HashSet::new();
        both.insert(id1);
        both.insert(id2);
        assert!(node.all_leaves_folded(&both));
    }

    // --- Ratio edge case tests ---

    #[test]
    fn test_resolve_ratio_boundary_0_1() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.1,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 50);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);
        // First gets ~10% = ~20px, second gets ~90% = ~180px
        assert!(resolved[0].1.width <= 25);
        assert!(resolved[1].1.width >= 175);
        // Total width should cover the area
        assert_eq!(resolved[0].1.width + resolved[1].1.width, 200);
    }

    #[test]
    fn test_resolve_ratio_boundary_0_9() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 50);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);
        assert!(resolved[0].1.width >= 175);
        assert!(resolved[1].1.width <= 25);
        assert_eq!(resolved[0].1.width + resolved[1].1.width, 200);
    }

    #[test]
    fn test_resolve_ratio_boundary_vertical() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.1,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 100, 100);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);
        assert!(resolved[0].1.height <= 15);
        assert!(resolved[1].1.height >= 85);
    }

    // --- Tiny area tests ---

    #[test]
    fn test_resolve_1x1_area() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 1, 1);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);
        // Total width must not exceed area
        assert!(resolved[0].1.width + resolved[1].1.width <= 1);
    }

    #[test]
    fn test_resolve_2x2_area() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 2, 2);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);
        // Each pane gets 1 col
        assert_eq!(resolved[0].1.width + resolved[1].1.width, 2);
        assert_eq!(resolved[0].1.height, 2);
        assert_eq!(resolved[1].1.height, 2);
    }

    #[test]
    fn test_resolve_3x3_area() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 3, 3);
        let resolved = node.resolve(area);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].1.width, 3);
        assert_eq!(resolved[1].1.width, 3);
        assert_eq!(resolved[0].1.height + resolved[1].1.height, 3);
    }

    // --- Resize deeply nested tests ---

    #[test]
    fn test_resize_3_levels_deep() {
        // root(H) → [id1 | inner(V) → [id2 | deep(H) → [id3 | id4]]]
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let id4 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
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

        // Resize id3 (3 levels deep) by +0.1
        assert!(node.resize(id3, 0.1));

        // Verify only the deepest split ratio changed
        if let LayoutNode::Split { ratio, second, .. } = &node {
            assert!(
                (*ratio - 0.5).abs() < f64::EPSILON,
                "root ratio should be unchanged"
            );
            if let LayoutNode::Split {
                ratio: mid_ratio,
                second: deep,
                ..
            } = second.as_ref()
            {
                assert!(
                    (*mid_ratio - 0.5).abs() < f64::EPSILON,
                    "middle ratio should be unchanged"
                );
                if let LayoutNode::Split {
                    ratio: deep_ratio, ..
                } = deep.as_ref()
                {
                    assert!(
                        (*deep_ratio - 0.6).abs() < f64::EPSILON,
                        "deep ratio should be 0.6"
                    );
                } else {
                    panic!("Expected deep split");
                }
            }
        }
    }

    #[test]
    fn test_resize_4_levels_deep() {
        let ids: Vec<TabId> = (0..5).map(|_| TabId::new_v4()).collect();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(ids[0])),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(ids[1])),
                second: Box::new(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::Leaf(ids[2])),
                    second: Box::new(LayoutNode::Split {
                        direction: SplitDirection::Horizontal,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf(ids[3])),
                        second: Box::new(LayoutNode::Leaf(ids[4])),
                    }),
                }),
            }),
        };

        // Resize the deepest pane
        assert!(node.resize(ids[4], 0.2));

        // Walk to the deepest split and verify only that ratio changed
        fn get_deepest_ratio(n: &LayoutNode) -> f64 {
            match n {
                LayoutNode::Split { ratio, second, .. } => {
                    if matches!(second.as_ref(), LayoutNode::Leaf(_)) {
                        *ratio
                    } else {
                        get_deepest_ratio(second)
                    }
                }
                _ => panic!("Expected split"),
            }
        }
        let r = get_deepest_ratio(&node);
        assert!(
            (r - 0.3).abs() < f64::EPSILON,
            "deepest ratio should be 0.3 (0.5 - 0.2)"
        );
    }

    // --- Neighbor finding in deeply nested same-direction trees ---

    #[test]
    fn test_neighbor_deep_same_direction_chain() {
        // 4 panes in a horizontal chain: [id1 | [id2 | [id3 | id4]]]
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let id4 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.25,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.33,
                first: Box::new(LayoutNode::Leaf(id2)),
                second: Box::new(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::Leaf(id3)),
                    second: Box::new(LayoutNode::Leaf(id4)),
                }),
            }),
        };

        // Right neighbors
        assert_eq!(
            node.find_neighbor(id1, SplitDirection::Horizontal, Side::Second),
            Some(id2)
        );
        assert_eq!(
            node.find_neighbor(id2, SplitDirection::Horizontal, Side::Second),
            Some(id3)
        );
        assert_eq!(
            node.find_neighbor(id3, SplitDirection::Horizontal, Side::Second),
            Some(id4)
        );
        assert_eq!(
            node.find_neighbor(id4, SplitDirection::Horizontal, Side::Second),
            None
        );

        // Left neighbors
        assert_eq!(
            node.find_neighbor(id1, SplitDirection::Horizontal, Side::First),
            None
        );
        assert_eq!(
            node.find_neighbor(id2, SplitDirection::Horizontal, Side::First),
            Some(id1)
        );
        assert_eq!(
            node.find_neighbor(id3, SplitDirection::Horizontal, Side::First),
            Some(id2)
        );
        assert_eq!(
            node.find_neighbor(id4, SplitDirection::Horizontal, Side::First),
            Some(id3)
        );
    }

    #[test]
    fn test_neighbor_mixed_direction_deep() {
        // H[id1 | V[id2 | H[id3 | id4]]]
        let (node, id1, id2, id3, id4) = build_design_example();

        // id4 → left = id3 (same H split)
        assert_eq!(
            node.find_neighbor(id4, SplitDirection::Horizontal, Side::First),
            Some(id3)
        );
        // id3 → left crosses into id1's territory
        assert_eq!(
            node.find_neighbor(id3, SplitDirection::Horizontal, Side::First),
            Some(id1)
        );
        // id4 → up = id2 (vertical neighbor from bottom-right)
        assert_eq!(
            node.find_neighbor(id4, SplitDirection::Vertical, Side::First),
            Some(id2)
        );
        // id1 → down = None (id1 spans full height, no vertical split at root)
        assert_eq!(
            node.find_neighbor(id1, SplitDirection::Vertical, Side::Second),
            None
        );
    }

    // --- Split/close cascade tests ---

    #[test]
    fn test_split_sibling_is_split_then_close() {
        // Start: H[id1 | id2]
        // Split id1 → H[H[id1 | id3] | id2]
        // Close id3 → H[id1 | id2]
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };

        assert!(node.split_pane(id1, SplitDirection::Horizontal, id3));
        assert_eq!(node.pane_ids(), vec![id1, id3, id2]);

        // Close id3 — inner split collapses, back to [id1 | id2]
        let focus = node.close_pane(id3);
        assert_eq!(focus, Some(id1));
        assert_eq!(node.pane_ids(), vec![id1, id2]);
    }

    #[test]
    fn test_close_cascade_deeply_nested() {
        // Build: H[id1 | V[id2 | H[id3 | id4]]]
        // Close id4 → H[id1 | V[id2 | id3]]
        // Close id3 → H[id1 | id2]
        // Close id2 → Leaf(id1)
        let (mut node, id1, id2, id3, id4) = build_design_example();

        let focus = node.close_pane(id4);
        assert_eq!(focus, Some(id3));
        assert_eq!(node.pane_ids(), vec![id1, id2, id3]);

        let focus = node.close_pane(id3);
        assert_eq!(focus, Some(id2));
        assert_eq!(node.pane_ids(), vec![id1, id2]);

        let focus = node.close_pane(id2);
        assert_eq!(focus, Some(id1));
        assert!(matches!(node, LayoutNode::Leaf(id) if id == id1));
    }

    #[test]
    fn test_split_both_children_are_splits() {
        // H[H[id1 | id2] | H[id3 | id4]]
        // Close id2 → H[id1 | H[id3 | id4]]
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let id4 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(id1)),
                second: Box::new(LayoutNode::Leaf(id2)),
            }),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(id3)),
                second: Box::new(LayoutNode::Leaf(id4)),
            }),
        };

        let focus = node.close_pane(id2);
        assert_eq!(focus, Some(id1));
        assert_eq!(node.pane_ids(), vec![id1, id3, id4]);

        // The first child should now be a leaf
        if let LayoutNode::Split { first, .. } = &node {
            assert!(matches!(first.as_ref(), LayoutNode::Leaf(id) if *id == id1));
        }
    }

    // --- Equalize on complex trees ---

    #[test]
    fn test_equalize_5_pane_layout() {
        // Build 5-pane layout: H[id1 | V[id2 | H[id3 | V[id4 | id5]]]]
        let ids: Vec<TabId> = (0..5).map(|_| TabId::new_v4()).collect();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.3,
            first: Box::new(LayoutNode::Leaf(ids[0])),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.7,
                first: Box::new(LayoutNode::Leaf(ids[1])),
                second: Box::new(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.2,
                    first: Box::new(LayoutNode::Leaf(ids[2])),
                    second: Box::new(LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.8,
                        first: Box::new(LayoutNode::Leaf(ids[3])),
                        second: Box::new(LayoutNode::Leaf(ids[4])),
                    }),
                }),
            }),
        };

        node.equalize();

        fn verify_all_ratios_half(n: &LayoutNode) {
            if let LayoutNode::Split {
                ratio,
                first,
                second,
                ..
            } = n
            {
                assert!(
                    (*ratio - 0.5).abs() < f64::EPSILON,
                    "ratio was {} instead of 0.5",
                    ratio
                );
                verify_all_ratios_half(first);
                verify_all_ratios_half(second);
            }
        }
        verify_all_ratios_half(&node);
        assert_eq!(node.pane_ids().len(), 5);
    }

    #[test]
    fn test_equalize_6_pane_layout() {
        // Balanced binary tree: H[V[id1|id2] | V[H[id3|id4] | H[id5|id6]]]
        let ids: Vec<TabId> = (0..6).map(|_| TabId::new_v4()).collect();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.3,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.8,
                first: Box::new(LayoutNode::Leaf(ids[0])),
                second: Box::new(LayoutNode::Leaf(ids[1])),
            }),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.2,
                first: Box::new(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.9,
                    first: Box::new(LayoutNode::Leaf(ids[2])),
                    second: Box::new(LayoutNode::Leaf(ids[3])),
                }),
                second: Box::new(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.1,
                    first: Box::new(LayoutNode::Leaf(ids[4])),
                    second: Box::new(LayoutNode::Leaf(ids[5])),
                }),
            }),
        };

        node.equalize();

        fn check_all(n: &LayoutNode) {
            if let LayoutNode::Split {
                ratio,
                first,
                second,
                ..
            } = n
            {
                assert!((*ratio - 0.5).abs() < f64::EPSILON);
                check_all(first);
                check_all(second);
            }
        }
        check_all(&node);
    }

    // --- Additional edge cases ---

    #[test]
    fn test_edge_leaf_on_complex_tree() {
        let (node, id1, _, _, id4) = build_design_example();
        assert_eq!(node.edge_leaf(Side::First), id1);
        assert_eq!(node.edge_leaf(Side::Second), id4);
    }

    #[test]
    fn test_fold_cell_count_cross_direction() {
        // V[id1 | id2] folded horizontally → max(1, 1) = 1
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert_eq!(node.fold_cell_count(SplitDirection::Horizontal), 1);
        // Same direction: V folded vertically → 1+1 = 2
        assert_eq!(node.fold_cell_count(SplitDirection::Vertical), 2);
    }

    #[test]
    fn test_fold_cell_count_deep_mixed() {
        // H[V[a|b] | V[c|d]] folded horizontally
        // H-split → sum of children's fold counts
        // V[a|b] folded H → max(1,1) = 1
        // V[c|d] folded H → max(1,1) = 1
        // total = 1+1 = 2
        let ids: Vec<TabId> = (0..4).map(|_| TabId::new_v4()).collect();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(ids[0])),
                second: Box::new(LayoutNode::Leaf(ids[1])),
            }),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(ids[2])),
                second: Box::new(LayoutNode::Leaf(ids[3])),
            }),
        };
        assert_eq!(node.fold_cell_count(SplitDirection::Horizontal), 2);
        // Folded vertically: H-split → max of children
        // V[a|b] folded V → 1+1=2, V[c|d] folded V → 1+1=2
        // H-split → max(2,2) = 2
        assert_eq!(node.fold_cell_count(SplitDirection::Vertical), 2);
    }
    #[test]
    fn test_resolve_with_nonzero_origin() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(10, 20, 100, 50);
        let resolved = node.resolve(area);
        // First pane should start at (10, 20)
        assert_eq!(resolved[0].1.x, 10);
        assert_eq!(resolved[0].1.y, 20);
        // Second pane should start after first
        assert!(resolved[1].1.x > 10);
        assert_eq!(resolved[1].1.y, 20);
    }
}
