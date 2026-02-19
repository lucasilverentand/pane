use std::collections::HashMap;

use ratatui::layout::{Constraint, Layout, Rect};
use serde::{Deserialize, Serialize};

use crate::config::Behavior;

pub type TabId = uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub struct LayoutParams {
    pub min_pane_width: u16,
    pub min_pane_height: u16,
    #[allow(dead_code)]
    pub fold_bar_size: u16,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            min_pane_width: 80,
            min_pane_height: 20,
            fold_bar_size: 1,
        }
    }
}

impl From<&Behavior> for LayoutParams {
    fn from(b: &Behavior) -> Self {
        Self {
            min_pane_width: b.min_pane_width,
            min_pane_height: b.min_pane_height,
            fold_bar_size: b.fold_bar_size,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedPane {
    Visible { id: TabId, rect: Rect },
    Folded { id: TabId, rect: Rect, direction: SplitDirection },
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

    /// Resolve with automatic folding: panes too small get collapsed to a fold bar.
    /// `leaf_mins` provides per-leaf custom minimum sizes (from user drag/resize),
    /// falling back to params.min_pane_width / min_pane_height when absent.
    pub fn resolve_with_fold(
        &self,
        area: Rect,
        params: LayoutParams,
        leaf_mins: &HashMap<TabId, (u16, u16)>,
    ) -> Vec<ResolvedPane> {
        let mut result = Vec::new();
        self.resolve_fold_inner(area, params, leaf_mins, &mut result);
        result
    }

    fn resolve_fold_inner(
        &self,
        area: Rect,
        params: LayoutParams,
        leaf_mins: &HashMap<TabId, (u16, u16)>,
        result: &mut Vec<ResolvedPane>,
    ) {
        match self {
            LayoutNode::Leaf(id) => {
                result.push(ResolvedPane::Visible { id: *id, rect: area });
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_rect, second_rect) = Self::split_rects(direction, *ratio, area);

                let (first_min_w, first_min_h) = first.subtree_min_size(params, leaf_mins);
                let (second_min_w, second_min_h) = second.subtree_min_size(params, leaf_mins);

                let (total, first_size, first_min, second_min) = match direction {
                    SplitDirection::Horizontal => (
                        area.width,
                        first_rect.width,
                        first_min_w,
                        second_min_w,
                    ),
                    SplitDirection::Vertical => (
                        area.height,
                        first_rect.height,
                        first_min_h,
                        second_min_h,
                    ),
                };
                let second_size = total.saturating_sub(first_size);

                // Case 1: Both fit at proportional sizes — no fold
                if first_size >= first_min && second_size >= second_min {
                    first.resolve_fold_inner(first_rect, params, leaf_mins, result);
                    second.resolve_fold_inner(second_rect, params, leaf_mins, result);
                }
                // Case 2: Total can fit both minimums — clamp sizes
                else if total >= first_min + second_min {
                    let (adj_first, adj_second) = if first_size < first_min {
                        Self::split_rects_clamped(direction, area, first_min, total - first_min)
                    } else {
                        Self::split_rects_clamped(direction, area, total - second_min, second_min)
                    };
                    first.resolve_fold_inner(adj_first, params, leaf_mins, result);
                    second.resolve_fold_inner(adj_second, params, leaf_mins, result);
                }
                // Case 3: Not enough space for both — must fold one
                else {
                    let fl = first.fold_cell_count(*direction);
                    let sl = second.fold_cell_count(*direction);
                    // Folding a subtree costs fold_cell_count cells
                    let first_fits_alone = total >= first_min + sl;
                    let second_fits_alone = total >= second_min + fl;

                    match (first_fits_alone, second_fits_alone) {
                        (true, true) => {
                            // Both could fit alone. Use ratio to decide which to fold.
                            // ratio < 0.5 → first intended small → fold first
                            // ratio >= 0.5 → second intended small → fold second (right/bottom)
                            if *ratio < 0.5 {
                                let (bar, expanded) =
                                    Self::fold_redistribute(direction, area, false, fl);
                                Self::fold_subtree(first, bar, *direction, result);
                                second.resolve_fold_inner(expanded, params, leaf_mins, result);
                            } else {
                                let (expanded, bar) =
                                    Self::fold_redistribute(direction, area, true, sl);
                                first.resolve_fold_inner(expanded, params, leaf_mins, result);
                                Self::fold_subtree(second, bar, *direction, result);
                            }
                        }
                        (true, false) => {
                            // Only first fits alone — fold second
                            let (expanded, bar) =
                                Self::fold_redistribute(direction, area, true, sl);
                            first.resolve_fold_inner(expanded, params, leaf_mins, result);
                            Self::fold_subtree(second, bar, *direction, result);
                        }
                        (false, true) => {
                            // Only second fits alone — fold first
                            let (bar, expanded) =
                                Self::fold_redistribute(direction, area, false, fl);
                            Self::fold_subtree(first, bar, *direction, result);
                            second.resolve_fold_inner(expanded, params, leaf_mins, result);
                        }
                        (false, false) => {
                            // Neither fits alone — keep first (left/top), fold second
                            let (expanded, bar) =
                                Self::fold_redistribute(direction, area, true, sl);
                            Self::fold_subtree(second, bar, *direction, result);
                            first.resolve_fold_inner(expanded, params, leaf_mins, result);
                        }
                    }
                }
            }
        }
    }

    /// Compute the minimum size a subtree needs to display at least one pane.
    fn subtree_min_size(
        &self,
        params: LayoutParams,
        leaf_mins: &HashMap<TabId, (u16, u16)>,
    ) -> (u16, u16) {
        match self {
            LayoutNode::Leaf(id) => leaf_mins
                .get(id)
                .copied()
                .unwrap_or((params.min_pane_width, params.min_pane_height)),
            LayoutNode::Split {
                direction,
                first,
                second,
                ..
            } => {
                let (fw, fh) = first.subtree_min_size(params, leaf_mins);
                let (sw, sh) = second.subtree_min_size(params, leaf_mins);
                let fl = first.fold_cell_count(*direction);
                let sl = second.fold_cell_count(*direction);
                // When one child folds, it takes fold_cell_count cells.
                // Option A: show first, fold second → first_min + second_fold_cells
                // Option B: show second, fold first → second_min + first_fold_cells
                // Min = whichever option is smaller.
                match direction {
                    SplitDirection::Horizontal => {
                        let a = fw + sl;
                        let b = sw + fl;
                        (a.min(b), fh.max(sh))
                    }
                    SplitDirection::Vertical => {
                        let a = fh + sl;
                        let b = sh + fl;
                        (fw.max(sw), a.min(b))
                    }
                }
            }
        }
    }

    /// Create two rects with exact pixel sizes (for clamping).
    fn split_rects_clamped(
        direction: &SplitDirection,
        area: Rect,
        first_size: u16,
        second_size: u16,
    ) -> (Rect, Rect) {
        match direction {
            SplitDirection::Horizontal => (
                Rect::new(area.x, area.y, first_size, area.height),
                Rect::new(area.x + first_size, area.y, second_size, area.height),
            ),
            SplitDirection::Vertical => (
                Rect::new(area.x, area.y, area.width, first_size),
                Rect::new(area.x, area.y + first_size, area.width, second_size),
            ),
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
                    let (first_bar, second_bar) =
                        Self::split_rects(direction, *ratio, bar_rect);
                    Self::fold_subtree(first, first_bar, fold_direction, result);
                    Self::fold_subtree(second, second_bar, fold_direction, result);
                }
            }
        }
    }

    /// Unfold a pane by resetting its parent split ratio to 0.5.
    /// Returns true if the target was found and the parent ratio was reset.
    #[cfg(test)]
    pub fn unfold(&mut self, target: TabId) -> bool {
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
                    // Check if target is a direct child
                    let is_direct =
                        matches!(first.as_ref(), LayoutNode::Leaf(id) if *id == target)
                            || matches!(second.as_ref(), LayoutNode::Leaf(id) if *id == target);
                    if is_direct {
                        *ratio = 0.5;
                        return true;
                    }
                    // Also check if target is the first leaf of a subtree child
                    if (in_first && first.first_leaf() == target)
                        || (in_second && second.first_leaf() == target)
                    {
                        *ratio = 0.5;
                        return true;
                    }
                    // Recurse
                    if in_first {
                        return first.unfold(target);
                    }
                    return second.unfold(target);
                }
                false
            }
        }
    }

    /// Unfold a pane by skewing the ratio at the split responsible for
    /// folding the target. Skews only that one split — inner ratios are
    /// preserved so subtree proportions remain stable across fold/unfold.
    pub fn unfold_towards(&mut self, target: TabId) -> bool {
        match self {
            LayoutNode::Leaf(_) => false,
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
                ..
            } => {
                let in_first = first.contains(target);
                let in_second = second.contains(target);
                if !(in_first || in_second) {
                    return false;
                }

                // Check if the child containing the target is a same-direction
                // split — if so, the fold might be internal and we should try
                // recursing first to avoid changing our ratio unnecessarily.
                let child_is_same_dir_split = {
                    let child: &LayoutNode = if in_first { first } else { second };
                    matches!(child, LayoutNode::Split { direction: d, .. } if *d == *direction)
                };
                if child_is_same_dir_split {
                    let child = if in_first {
                        first.as_mut()
                    } else {
                        second.as_mut()
                    };
                    if child.unfold_towards(target) {
                        return true;
                    }
                }

                // The fold is at this level (target is a direct child, in a
                // cross-direction subtree, or same-direction recursion didn't
                // find a deeper match). Skew ratio to give space to the target.
                *ratio = if in_first { 0.9 } else { 0.1 };
                true
            }
        }
    }

    /// Split a target pane into two, placing the new pane in the second position.
    pub fn split_pane(
        &mut self,
        target: TabId,
        direction: SplitDirection,
        new_id: TabId,
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
                first.close_pane(target).or_else(|| second.close_pane(target))
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

    // --- Fold tests ---

    #[test]
    fn test_fold_both_fit() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        // Plenty of space — no folding (each side gets 150, well above MIN_PANE_WIDTH=100)
        let area = Rect::new(0, 0, 300, 50);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, .. } if id == id2));
    }

    #[test]
    fn test_fold_second_too_narrow() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // Ratio 0.9 on 120 wide: second gets ~12 cols, which is < MIN_PANE_WIDTH(100)
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 120, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(
            matches!(resolved[1], ResolvedPane::Folded { id, rect, direction: SplitDirection::Horizontal } if id == id2 && rect.width == LayoutParams::default().fold_bar_size)
        );
    }

    #[test]
    fn test_fold_first_too_narrow() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // Ratio 0.1 on 120 wide: first gets ~12 cols, which is < MIN_PANE_WIDTH(100)
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.1,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 120, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 2);
        assert!(
            matches!(resolved[0], ResolvedPane::Folded { id, rect, direction: SplitDirection::Horizontal } if id == id1 && rect.width == LayoutParams::default().fold_bar_size)
        );
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, .. } if id == id2));
    }

    #[test]
    fn test_fold_both_too_small_keeps_one_visible() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // Total width 80 with 50/50 split → each gets ~40, both < MIN_PANE_WIDTH(80)
        // Neither fits alone with fold bar (80 < 81). Fallback: fold second, keep first.
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 80, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 2);
        // (false,false) case: fold_subtree pushes second first, then first resolves
        let second_folded = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Folded { id, .. } if *id == id2));
        let first_visible = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Visible { id, .. } if *id == id1));
        assert!(first_visible, "first (left) should be visible");
        assert!(second_folded, "second (right) should be folded");
    }

    #[test]
    fn test_fold_nested_subtree() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        // Right subtree is a split of id2/id3. With ratio 0.9 on 150 wide,
        // right gets ~15 cols which is < MIN_PANE_WIDTH(80), so entire subtree folds.
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(id2)),
                second: Box::new(LayoutNode::Leaf(id3)),
            }),
        };
        let area = Rect::new(0, 0, 150, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 3);
        // First visible (gets 148 cols, 2 taken by fold bars)
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        // Each folded leaf gets its own 1-cell bar
        assert!(
            matches!(resolved[1], ResolvedPane::Folded { id, rect, .. } if id == id2 && rect.width == 1)
        );
        assert!(
            matches!(resolved[2], ResolvedPane::Folded { id, rect, .. } if id == id3 && rect.width == 1)
        );
    }

    #[test]
    fn test_single_leaf_never_folds() {
        let id = TabId::new_v4();
        let node = LayoutNode::Leaf(id);
        // Even with very small area, a lone leaf is always Visible
        let area = Rect::new(0, 0, 3, 2);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 1);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id: rid, .. } if rid == id));
    }

    #[test]
    fn test_unfold_resets_ratio() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert!(node.unfold(id2));
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((*ratio - 0.5).abs() < f64::EPSILON);
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_unfold_towards_skews_ratio() {
        // Two panes in horizontal split, ratio=0.9 means id1 is big, id2 is folded.
        // Clicking id2 should skew ratio to 0.1 so id2 gets the space.
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert!(node.unfold_towards(id2));
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((*ratio - 0.1).abs() < f64::EPSILON);
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_unfold_towards_first_child() {
        // id1 is folded (ratio=0.1), clicking it should set ratio to 0.9
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.1,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert!(node.unfold_towards(id1));
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((*ratio - 0.9).abs() < f64::EPSILON);
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_unfold_towards_same_direction_recurses() {
        // [id1 | [id2 | id3]] both horizontal. Inner ratio 0.9 causes id3 to fold.
        // unfold_towards(id3) recurses into the same-direction child and skews
        // the inner ratio to 0.1 (give space to id3). Outer ratio is preserved.
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.9,
                first: Box::new(LayoutNode::Leaf(id2)),
                second: Box::new(LayoutNode::Leaf(id3)),
            }),
        };
        assert!(node.unfold_towards(id3));

        if let LayoutNode::Split { ratio, second, .. } = &node {
            // Outer ratio preserved — the inner split handled the unfold
            assert!((*ratio - 0.5).abs() < f64::EPSILON);
            // Inner ratio skewed to give id3 space
            if let LayoutNode::Split { ratio: inner_ratio, .. } = second.as_ref() {
                assert!((*inner_ratio - 0.1).abs() < f64::EPSILON);
            }
        }
    }

    #[test]
    fn test_unfold_towards_cross_direction_preserves_inner() {
        // [id1 | Split(V) → [id2, id3]] with outer ratio 0.9.
        // The vertical subtree is cross-direction, folds as a unit at outer level.
        // Clicking id3 should skew outer ratio to 0.1 (give space to second),
        // preserving inner vertical ratio at 0.3.
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.3,
                first: Box::new(LayoutNode::Leaf(id2)),
                second: Box::new(LayoutNode::Leaf(id3)),
            }),
        };
        assert!(node.unfold_towards(id3));

        if let LayoutNode::Split { ratio, second, .. } = &node {
            // Outer ratio skewed to give space to right subtree
            assert!((*ratio - 0.1).abs() < f64::EPSILON);
            // Inner vertical ratio preserved at 0.3
            if let LayoutNode::Split { ratio: inner_ratio, .. } = second.as_ref() {
                assert!((*inner_ratio - 0.3).abs() < f64::EPSILON);
            }
        }
    }

    #[test]
    fn test_fold_vertical_split() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // Ratio 0.9 vertical on 30 rows: first=27, second=3.
        // 3 < min_pane_height(20), total 30 < 20+21=41 → case 3.
        // first_fits_alone: 30 >= 20+1 → true. second_fits_alone: 30 >= 20+1 → true.
        // ratio 0.9 >= 0.5 → fold second.
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 30);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(
            matches!(resolved[1], ResolvedPane::Folded { id, rect, direction: SplitDirection::Vertical } if id == id2 && rect.height == LayoutParams::default().fold_bar_size)
        );
    }

    // --- Clamping & leaf_min tests ---

    #[test]
    fn test_leaf_min_prevents_fold() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // 140 wide, ratio 0.7: first=98, second=42
        // Without leaf_mins: total 140 < 80+80=160 → fold. ratio 0.7 → fold second.
        // With leaf_min(id2)=(50,4): total 140 >= 80+50=130 → clamp. Both visible!
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.7,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 140, 10);

        // Without leaf_mins: second folds
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        let second_folded = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Folded { id, .. } if *id == id2));
        assert!(second_folded, "second should fold without leaf_mins");

        // With leaf_min for id2: no fold (clamping works)
        let mut leaf_mins = HashMap::new();
        leaf_mins.insert(id2, (50u16, 4u16));
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &leaf_mins);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, .. } if id == id2));
    }

    #[test]
    fn test_clamping_respects_minimums() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // 200 wide, ratio 0.8 → first=160, second=40
        // leaf_min for id2=60: total 200 >= 80+60=140, case 2 (clamp)
        // second should get at least 60
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.8,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 10);
        let mut leaf_mins = HashMap::new();
        leaf_mins.insert(id2, (60u16, 4u16));
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &leaf_mins);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, rect, .. } if id == id2 && rect.width >= 60));
    }

    #[test]
    fn test_fold_prefers_second_at_equal_ratio() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // 120 wide, ratio 0.5 → each gets 60, both < 80
        // Both could fit alone (119 >= 80). ratio == 0.5 → fold second (right)
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 120, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        // First visible, second folded (right/bottom folds at equal ratio)
        let first_visible = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Visible { id, .. } if *id == id1));
        let second_folded = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Folded { id, .. } if *id == id2));
        assert!(first_visible, "first (left) should be visible");
        assert!(second_folded, "second (right) should be folded");
    }

    #[test]
    fn test_fold_subtree_cross_direction_preserves_structure() {
        // Layout: horizontal split, right child is a vertical split with 2 leaves.
        // When the right subtree folds horizontally, the cross-direction vertical split
        // is preserved: both leaves share a single column, split vertically by ratio.
        let id_left = TabId::new_v4();
        let id_top_right = TabId::new_v4();
        let id_bot_right = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id_left)),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(id_top_right)),
                second: Box::new(LayoutNode::Leaf(id_bot_right)),
            }),
        };
        // 120 wide, right subtree needs 80+1=81 (cross-direction: fold_cell_count=1)
        let area = Rect::new(0, 0, 120, 40);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());

        // Left should be visible
        let left_visible = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Visible { id, .. } if *id == id_left));
        assert!(left_visible, "left pane should be visible");

        // Both right panes should be folded
        let right_folded: Vec<_> = resolved.iter().filter(|rp| matches!(rp, ResolvedPane::Folded { .. })).collect();
        assert_eq!(right_folded.len(), 2, "both right panes should be folded");

        // Both fold bars should be 1 cell wide (same column) and fold direction = Horizontal
        for bar in &right_folded {
            if let ResolvedPane::Folded { rect, direction, .. } = bar {
                assert_eq!(rect.width, 1, "fold bar should be 1 cell wide");
                assert_eq!(*direction, SplitDirection::Horizontal);
            }
        }

        // Bars should share the same x (single column), split vertically
        if let (
            ResolvedPane::Folded { rect: r1, .. },
            ResolvedPane::Folded { rect: r2, .. },
        ) = (right_folded[0], right_folded[1])
        {
            assert_eq!(r1.x, r2.x, "bars should share the same column");
            assert_eq!(r1.y + r1.height, r2.y, "bars should be stacked vertically");
            assert_eq!(
                r1.height + r2.height,
                40,
                "bars should span full height together"
            );
        }
    }

    #[test]
    fn test_subtree_min_size_leaf() {
        let id = TabId::new_v4();
        let node = LayoutNode::Leaf(id);
        let params = LayoutParams::default();

        // Without overrides: use global defaults
        let (w, h) = node.subtree_min_size(params, &HashMap::new());
        assert_eq!(w, 80);
        assert_eq!(h, 20);

        // With override
        let mut leaf_mins = HashMap::new();
        leaf_mins.insert(id, (30u16, 2u16));
        let (w, h) = node.subtree_min_size(params, &leaf_mins);
        assert_eq!(w, 30);
        assert_eq!(h, 2);
    }

    #[test]
    fn test_subtree_min_size_split() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let params = LayoutParams::default();
        let (w, h) = node.subtree_min_size(params, &HashMap::new());
        // Horizontal split: min(80+1, 80+1) = 81, height = max(50, 50) = 50
        // (each folded leaf takes 1 cell)
        assert_eq!(w, 81);
        assert_eq!(h, 20);
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

    #[test]
    fn test_resolve_with_fold_tiny_1x1() {
        let id = TabId::new_v4();
        let node = LayoutNode::Leaf(id);
        let area = Rect::new(0, 0, 1, 1);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 1);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id: rid, rect } if rid == id && rect.width == 1 && rect.height == 1));
    }

    #[test]
    fn test_resolve_with_fold_split_in_tiny_area() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        // 3 wide: both panes < min width. One must fold.
        let area = Rect::new(0, 0, 3, 3);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 2);
        let visible_count = resolved.iter().filter(|rp| matches!(rp, ResolvedPane::Visible { .. })).count();
        let folded_count = resolved.iter().filter(|rp| matches!(rp, ResolvedPane::Folded { .. })).count();
        assert_eq!(visible_count, 1);
        assert_eq!(folded_count, 1);
    }

    // --- Fold case 2 asymmetry tests ---

    #[test]
    fn test_fold_case2_asymmetric_leaf_mins_first_undersized() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // 100 wide, ratio 0.3 → first=30, second=70
        // leaf_mins: id1=(50,4), id2=(40,4) → total needed=90 <= 100 → case 2 (clamp)
        // first_size(30) < first_min(50) → clamp first to 50, second gets 50
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.3,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 100, 10);
        let mut leaf_mins = HashMap::new();
        leaf_mins.insert(id1, (50u16, 4u16));
        leaf_mins.insert(id2, (40u16, 4u16));
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &leaf_mins);
        assert_eq!(resolved.len(), 2);
        // Both should be visible (case 2 clamping)
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, rect, .. } if id == id1 && rect.width >= 50));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, rect, .. } if id == id2 && rect.width >= 40));
    }

    #[test]
    fn test_fold_case2_asymmetric_leaf_mins_second_undersized() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        // 100 wide, ratio 0.8 → first=80, second=20
        // leaf_mins: id1=(40,4), id2=(50,4) → total needed=90 <= 100 → case 2
        // second_size(20) < second_min(50) → clamp second to 50, first gets 50
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.8,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 100, 10);
        let mut leaf_mins = HashMap::new();
        leaf_mins.insert(id1, (40u16, 4u16));
        leaf_mins.insert(id2, (50u16, 4u16));
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &leaf_mins);
        assert_eq!(resolved.len(), 2);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, rect, .. } if id == id1 && rect.width >= 40));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, rect, .. } if id == id2 && rect.width >= 50));
    }

    // --- Large fold_cell_count tests ---

    #[test]
    fn test_large_fold_cell_count_nested() {
        // Build a deeply nested same-direction split tree (4 leaves in horizontal chain)
        // fold_cell_count = 4 for this subtree
        let ids: Vec<TabId> = (0..4).map(|_| TabId::new_v4()).collect();
        let right_subtree = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(ids[1])),
                second: Box::new(LayoutNode::Leaf(ids[2])),
            }),
            second: Box::new(LayoutNode::Leaf(ids[3])),
        };
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(ids[0])),
            second: Box::new(right_subtree),
        };
        // fold_cell_count of right subtree = 3 (two leaves in H-split + one leaf)
        // With 90 wide area: first gets ~81, second gets ~9
        // second subtree min = min(81+3, 81+1) = min(84, 82) = 82 (with default params)
        // So with small enough area, the right subtree folds
        let area = Rect::new(0, 0, 90, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        // First should be visible, all 3 right leaves should be folded
        let visible: Vec<_> = resolved.iter().filter(|rp| matches!(rp, ResolvedPane::Visible { .. })).collect();
        let folded: Vec<_> = resolved.iter().filter(|rp| matches!(rp, ResolvedPane::Folded { .. })).collect();
        assert_eq!(visible.len(), 1);
        assert_eq!(folded.len(), 3);
    }

    #[test]
    fn test_fold_cell_count_exceeds_available_space() {
        // 8 leaves in same-direction split chain → fold_cell_count = 8
        // Available width is only 5 → fold bar capped at available space
        fn chain(ids: &[TabId]) -> LayoutNode {
            if ids.len() == 1 {
                return LayoutNode::Leaf(ids[0]);
            }
            LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(ids[0])),
                second: Box::new(chain(&ids[1..])),
            }
        }
        let ids: Vec<TabId> = (0..8).map(|_| TabId::new_v4()).collect();
        let subtree = chain(&ids);
        assert_eq!(subtree.fold_cell_count(SplitDirection::Horizontal), 8);

        // Put it as second child of a split with tiny total area
        let main_id = TabId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(main_id)),
            second: Box::new(subtree),
        };
        let area = Rect::new(0, 0, 12, 5);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        // Should not panic; all rects should fit within the area
        for rp in &resolved {
            let rect = match rp {
                ResolvedPane::Visible { rect, .. } | ResolvedPane::Folded { rect, .. } => rect,
            };
            assert!(rect.x + rect.width <= area.x + area.width);
            assert!(rect.y + rect.height <= area.y + area.height);
        }
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
            assert!((*ratio - 0.5).abs() < f64::EPSILON, "root ratio should be unchanged");
            if let LayoutNode::Split { ratio: mid_ratio, second: deep, .. } = second.as_ref() {
                assert!((*mid_ratio - 0.5).abs() < f64::EPSILON, "middle ratio should be unchanged");
                if let LayoutNode::Split { ratio: deep_ratio, .. } = deep.as_ref() {
                    assert!((*deep_ratio - 0.6).abs() < f64::EPSILON, "deep ratio should be 0.6");
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
        assert!((r - 0.3).abs() < f64::EPSILON, "deepest ratio should be 0.3 (0.5 - 0.2)");
    }

    // --- Unfold edge case tests ---

    #[test]
    fn test_unfold_towards_nonexistent_pane() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let nonexistent = TabId::new_v4();
        assert!(!node.unfold_towards(nonexistent));
        // Ratio should be unchanged
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((*ratio - 0.9).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_unfold_towards_on_leaf() {
        let id = TabId::new_v4();
        let mut node = LayoutNode::Leaf(id);
        assert!(!node.unfold_towards(id));
    }

    #[test]
    fn test_unfold_towards_already_unfolded() {
        // ratio is 0.5, pane is not folded, but unfold_towards still sets ratio
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert!(node.unfold_towards(id2));
        if let LayoutNode::Split { ratio, .. } = &node {
            // Skews to 0.1 to give space to second
            assert!((*ratio - 0.1).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_unfold_nonexistent_pane() {
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        assert!(!node.unfold(TabId::new_v4()));
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((*ratio - 0.9).abs() < f64::EPSILON);
        }
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
        assert_eq!(node.find_neighbor(id1, SplitDirection::Horizontal, Side::Second), Some(id2));
        assert_eq!(node.find_neighbor(id2, SplitDirection::Horizontal, Side::Second), Some(id3));
        assert_eq!(node.find_neighbor(id3, SplitDirection::Horizontal, Side::Second), Some(id4));
        assert_eq!(node.find_neighbor(id4, SplitDirection::Horizontal, Side::Second), None);

        // Left neighbors
        assert_eq!(node.find_neighbor(id1, SplitDirection::Horizontal, Side::First), None);
        assert_eq!(node.find_neighbor(id2, SplitDirection::Horizontal, Side::First), Some(id1));
        assert_eq!(node.find_neighbor(id3, SplitDirection::Horizontal, Side::First), Some(id2));
        assert_eq!(node.find_neighbor(id4, SplitDirection::Horizontal, Side::First), Some(id3));
    }

    #[test]
    fn test_neighbor_mixed_direction_deep() {
        // H[id1 | V[id2 | H[id3 | id4]]]
        let (node, id1, id2, id3, id4) = build_design_example();

        // id4 → left = id3 (same H split)
        assert_eq!(node.find_neighbor(id4, SplitDirection::Horizontal, Side::First), Some(id3));
        // id3 → left crosses into id1's territory
        assert_eq!(node.find_neighbor(id3, SplitDirection::Horizontal, Side::First), Some(id1));
        // id4 → up = id2 (vertical neighbor from bottom-right)
        assert_eq!(node.find_neighbor(id4, SplitDirection::Vertical, Side::First), Some(id2));
        // id1 → down = None (id1 spans full height, no vertical split at root)
        assert_eq!(node.find_neighbor(id1, SplitDirection::Vertical, Side::Second), None);
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
            if let LayoutNode::Split { ratio, first, second, .. } = n {
                assert!((*ratio - 0.5).abs() < f64::EPSILON, "ratio was {} instead of 0.5", ratio);
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
            if let LayoutNode::Split { ratio, first, second, .. } = n {
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
    fn test_split_rects_clamped_horizontal() {
        let area = Rect::new(5, 10, 100, 30);
        let (first, second) = LayoutNode::split_rects_clamped(
            &SplitDirection::Horizontal, area, 30, 70,
        );
        assert_eq!(first, Rect::new(5, 10, 30, 30));
        assert_eq!(second, Rect::new(35, 10, 70, 30));
    }

    #[test]
    fn test_split_rects_clamped_vertical() {
        let area = Rect::new(5, 10, 100, 40);
        let (first, second) = LayoutNode::split_rects_clamped(
            &SplitDirection::Vertical, area, 15, 25,
        );
        assert_eq!(first, Rect::new(5, 10, 100, 15));
        assert_eq!(second, Rect::new(5, 25, 100, 25));
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
