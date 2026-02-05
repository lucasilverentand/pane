use std::collections::HashMap;

use ratatui::layout::{Constraint, Layout, Rect};
use serde::{Deserialize, Serialize};

use crate::config::Behavior;

pub type PaneId = uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub struct LayoutParams {
    pub min_pane_width: u16,
    pub min_pane_height: u16,
    pub fold_bar_size: u16,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            min_pane_width: 100,
            min_pane_height: 4,
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
    Visible { id: PaneId, rect: Rect },
    Folded { id: PaneId, rect: Rect, direction: SplitDirection },
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
        leaf_mins: &HashMap<PaneId, (u16, u16)>,
    ) -> Vec<ResolvedPane> {
        let mut result = Vec::new();
        self.resolve_fold_inner(area, params, leaf_mins, &mut result);
        result
    }

    fn resolve_fold_inner(
        &self,
        area: Rect,
        params: LayoutParams,
        leaf_mins: &HashMap<PaneId, (u16, u16)>,
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
                    let fold_bar = params.fold_bar_size;
                    let expanded_alone = total.saturating_sub(fold_bar);
                    let first_fits_alone = expanded_alone >= first_min;
                    let second_fits_alone = expanded_alone >= second_min;

                    match (first_fits_alone, second_fits_alone) {
                        (true, true) => {
                            // Both could fit alone. Use ratio to decide which to fold.
                            // ratio < 0.5 → first intended small → fold first
                            // ratio >= 0.5 → second intended small → fold second (right/bottom)
                            if *ratio < 0.5 {
                                let (bar, expanded) =
                                    Self::fold_redistribute(direction, area, false, params);
                                Self::fold_subtree(first, bar, *direction, result);
                                second.resolve_fold_inner(expanded, params, leaf_mins, result);
                            } else {
                                let (expanded, bar) =
                                    Self::fold_redistribute(direction, area, true, params);
                                first.resolve_fold_inner(expanded, params, leaf_mins, result);
                                Self::fold_subtree(second, bar, *direction, result);
                            }
                        }
                        (true, false) => {
                            // Only first fits alone — fold second
                            let (expanded, bar) =
                                Self::fold_redistribute(direction, area, true, params);
                            first.resolve_fold_inner(expanded, params, leaf_mins, result);
                            Self::fold_subtree(second, bar, *direction, result);
                        }
                        (false, true) => {
                            // Only second fits alone — fold first
                            let (bar, expanded) =
                                Self::fold_redistribute(direction, area, false, params);
                            Self::fold_subtree(first, bar, *direction, result);
                            second.resolve_fold_inner(expanded, params, leaf_mins, result);
                        }
                        (false, false) => {
                            // Neither fits alone — keep first (left/top), fold second
                            let (expanded, bar) =
                                Self::fold_redistribute(direction, area, true, params);
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
        leaf_mins: &HashMap<PaneId, (u16, u16)>,
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
                match direction {
                    SplitDirection::Horizontal => (
                        fw.min(sw) + params.fold_bar_size,
                        fh.max(sh),
                    ),
                    SplitDirection::Vertical => (
                        fw.max(sw),
                        fh.min(sh) + params.fold_bar_size,
                    ),
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
        params: LayoutParams,
    ) -> (Rect, Rect) {
        let fold_bar_size = params.fold_bar_size;
        match direction {
            SplitDirection::Horizontal => {
                let bar_w = fold_bar_size.min(area.width);
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
                let bar_h = fold_bar_size.min(area.height);
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

    /// Fold an entire subtree: the first leaf gets the visible fold bar rect,
    /// all other leaves get zero-size rects (hidden).
    fn fold_subtree(
        node: &LayoutNode,
        bar_rect: Rect,
        direction: SplitDirection,
        result: &mut Vec<ResolvedPane>,
    ) {
        let ids = node.pane_ids();
        for (i, id) in ids.into_iter().enumerate() {
            if i == 0 {
                result.push(ResolvedPane::Folded {
                    id,
                    rect: bar_rect,
                    direction,
                });
            } else {
                result.push(ResolvedPane::Folded {
                    id,
                    rect: Rect::new(0, 0, 0, 0),
                    direction,
                });
            }
        }
    }

    /// Unfold a pane by resetting its parent split ratio to 0.5.
    /// Returns true if the target was found and the parent ratio was reset.
    #[allow(dead_code)]
    pub fn unfold(&mut self, target: PaneId) -> bool {
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

    /// Unfold a pane by skewing the ratio so the target's side gets most of the space,
    /// causing the other side to auto-fold. Preserves positional order.
    pub fn unfold_towards(&mut self, target: PaneId) -> bool {
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
                    let is_direct =
                        matches!(first.as_ref(), LayoutNode::Leaf(id) if *id == target)
                            || matches!(second.as_ref(), LayoutNode::Leaf(id) if *id == target);
                    let is_first_leaf =
                        (in_first && first.first_leaf() == target)
                            || (in_second && second.first_leaf() == target);
                    if is_direct || is_first_leaf {
                        // Skew ratio to give space to the side containing the target
                        *ratio = if in_first { 0.9 } else { 0.1 };
                        // Also recurse into the target's subtree to unfold it internally
                        if in_first {
                            first.unfold_inner(target);
                        } else {
                            second.unfold_inner(target);
                        }
                        return true;
                    }
                    // Recurse: skew this split towards the side containing target
                    *ratio = if in_first { 0.9 } else { 0.1 };
                    if in_first {
                        return first.unfold_towards(target);
                    }
                    return second.unfold_towards(target);
                }
                false
            }
        }
    }

    /// Internal helper: unfold within a subtree by setting ratios to 0.5.
    fn unfold_inner(&mut self, target: PaneId) -> bool {
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
                    *ratio = 0.5;
                    if in_first {
                        first.unfold_inner(target);
                    } else {
                        second.unfold_inner(target);
                    }
                    return true;
                }
                false
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

    /// Find a split border at the given (x, y) position within `area`.
    /// Returns `(first_child_first_leaf, direction, border_pos, total_size)` where:
    /// - `first_child_first_leaf` is the ID to pass to `resize()` (the first child's first leaf)
    /// - `direction` is the split direction
    /// - `border_pos` is the coordinate of the border (x for Horizontal, y for Vertical)
    /// - `total_size` is the total extent in the split direction (for pixel→ratio conversion)
    pub fn find_split_border(
        &self,
        x: u16,
        y: u16,
        area: Rect,
    ) -> Option<(PaneId, SplitDirection, u16, u16)> {
        match self {
            LayoutNode::Leaf(_) => None,
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

                let first_rect = chunks[0];
                let hit = match direction {
                    SplitDirection::Horizontal => {
                        let border_x = first_rect.x + first_rect.width;
                        let in_y = y >= area.y && y < area.y + area.height;
                        let near = x >= border_x.saturating_sub(1) && x <= border_x;
                        in_y && near
                    }
                    SplitDirection::Vertical => {
                        let border_y = first_rect.y + first_rect.height;
                        let in_x = x >= area.x && x < area.x + area.width;
                        let near = y >= border_y.saturating_sub(1) && y <= border_y;
                        in_x && near
                    }
                };

                if hit {
                    let border_pos = match direction {
                        SplitDirection::Horizontal => first_rect.x + first_rect.width,
                        SplitDirection::Vertical => first_rect.y + first_rect.height,
                    };
                    let total_size = match direction {
                        SplitDirection::Horizontal => area.width,
                        SplitDirection::Vertical => area.height,
                    };
                    let leaf_id = first.first_leaf();
                    return Some((leaf_id, *direction, border_pos, total_size));
                }

                // Recurse into children
                first
                    .find_split_border(x, y, chunks[0])
                    .or_else(|| second.find_split_border(x, y, chunks[1]))
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

    // --- Fold tests ---

    #[test]
    fn test_fold_both_fit() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
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
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
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
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
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
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        // Total width 80 with 50/50 split → each gets ~40, both < MIN_PANE_WIDTH(100)
        // Second folds, first stays visible (cramped but never folded)
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 80, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 2);
        // First stays visible, second folds
        assert!(matches!(resolved[0], ResolvedPane::Folded { id, .. } if id == id2));
        assert!(matches!(resolved[1], ResolvedPane::Visible { id, .. } if id == id1));
    }

    #[test]
    fn test_fold_nested_subtree() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let id3 = PaneId::new_v4();
        // Right subtree is a split of id2/id3. With ratio 0.9 on 150 wide,
        // right gets ~15 cols which is < MIN_PANE_WIDTH(100), so entire subtree folds.
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
        // First visible (gets 148 cols after fold bar = 2)
        assert!(matches!(resolved[0], ResolvedPane::Visible { id, .. } if id == id1));
        // Second subtree folded: id2 has the bar rect, id3 has zero-size
        assert!(
            matches!(resolved[1], ResolvedPane::Folded { id, rect, .. } if id == id2 && rect.width > 0)
        );
        assert!(
            matches!(resolved[2], ResolvedPane::Folded { id, rect, .. } if id == id3 && rect.width == 0)
        );
    }

    #[test]
    fn test_single_leaf_never_folds() {
        let id = PaneId::new_v4();
        let node = LayoutNode::Leaf(id);
        // Even with very small area, a lone leaf is always Visible
        let area = Rect::new(0, 0, 3, 2);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        assert_eq!(resolved.len(), 1);
        assert!(matches!(resolved[0], ResolvedPane::Visible { id: rid, .. } if rid == id));
    }

    #[test]
    fn test_unfold_resets_ratio() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
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
        // Clicking id2 should skew ratio to 0.1 so id2 gets the space and id1 folds.
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
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
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
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
    fn test_unfold_towards_preserves_position() {
        // Three panes: [id1 | [id2 | id3]] with outer ratio 0.9 (id2/id3 folded on right).
        // Clicking id2 should: set outer ratio to 0.1 (push id1 left/fold).
        // The inner subtree now has enough space, so inner ratio stays 0.5 (both id2+id3 fit).
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let id3 = PaneId::new_v4();
        let mut node = LayoutNode::Split {
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
        // id2 is the first leaf of the second subtree, so it represents folded second subtree
        assert!(node.unfold_towards(id2));

        // Outer ratio should be 0.1 (give space to second subtree)
        if let LayoutNode::Split { ratio, second, .. } = &node {
            assert!((*ratio - 0.1).abs() < f64::EPSILON);
            // Inner ratio reset to 0.5 by unfold_inner — both children have room
            if let LayoutNode::Split { ratio: inner_ratio, .. } = second.as_ref() {
                assert!((*inner_ratio - 0.5).abs() < f64::EPSILON);
            }
        }

        // Verify positions: on a 150-wide area, total 150 < 100+101=201, fold triggers.
        // ratio 0.1 < 0.5 → fold first (id1), keep second subtree visible
        let area = Rect::new(0, 0, 150, 10);
        let resolved = node.resolve_with_fold(area, LayoutParams::default(), &HashMap::new());
        // id1 should be folded
        assert!(matches!(resolved[0], ResolvedPane::Folded { id, .. } if id == id1));
        // id2 should be visible
        let id2_visible = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Visible { id, .. } if *id == id2));
        assert!(id2_visible, "id2 should be visible after unfold_towards");
    }

    #[test]
    fn test_fold_vertical_split() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        // Ratio 0.9 vertical on 6 rows: total 6 < 4+4=8, so fold triggers.
        // ratio 0.9 >= 0.5 → fold second.
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.9,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 200, 6);
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
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        // 180 wide, ratio 0.7: first=126, second=54
        // Without leaf_mins: total 180 < 100+100=200 → fold. ratio 0.7 → fold second.
        // With leaf_min(id2)=(50,4): total 180 >= 100+50=150 → clamp. Both visible!
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.7,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let area = Rect::new(0, 0, 180, 10);

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
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        // 200 wide, ratio 0.8 → first=160, second=40
        // leaf_min for id2=60: total 200 >= 100+60=160, case 2 (clamp)
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
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        // 120 wide, ratio 0.5 → each gets 60, both < 100
        // Both could fit alone (119 >= 100). ratio == 0.5 → fold second (right)
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
    fn test_subtree_min_size_leaf() {
        let id = PaneId::new_v4();
        let node = LayoutNode::Leaf(id);
        let params = LayoutParams::default();

        // Without overrides: use global defaults
        let (w, h) = node.subtree_min_size(params, &HashMap::new());
        assert_eq!(w, 100);
        assert_eq!(h, 4);

        // With override
        let mut leaf_mins = HashMap::new();
        leaf_mins.insert(id, (30u16, 2u16));
        let (w, h) = node.subtree_min_size(params, &leaf_mins);
        assert_eq!(w, 30);
        assert_eq!(h, 2);
    }

    #[test]
    fn test_subtree_min_size_split() {
        let id1 = PaneId::new_v4();
        let id2 = PaneId::new_v4();
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(id1)),
            second: Box::new(LayoutNode::Leaf(id2)),
        };
        let params = LayoutParams::default();
        let (w, h) = node.subtree_min_size(params, &HashMap::new());
        // Horizontal split: width = min(100, 100) + 1 = 101, height = max(4, 4) = 4
        assert_eq!(w, 101);
        assert_eq!(h, 4);
    }
}
