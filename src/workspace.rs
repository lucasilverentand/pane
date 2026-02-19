use std::collections::HashMap;

use crate::layout::LayoutNode;
use crate::window::{Window, WindowId};

pub struct Workspace {
    pub name: String,
    pub layout: LayoutNode,
    pub groups: HashMap<WindowId, Window>,
    pub active_group: WindowId,
    /// Per-leaf custom minimum sizes set by user drag/resize.
    /// Falls back to global config defaults when absent.
    pub leaf_min_sizes: HashMap<WindowId, (u16, u16)>,
    /// When true, key input is broadcast to all panes in this workspace.
    pub sync_panes: bool,
}

impl Workspace {
    pub fn new(name: String, group_id: WindowId, group: Window) -> Self {
        let layout = LayoutNode::Leaf(group_id);
        let mut groups = HashMap::new();
        let active_group = group_id;
        groups.insert(group_id, group);
        Self {
            name,
            layout,
            groups,
            active_group,
            leaf_min_sizes: HashMap::new(),
            sync_panes: false,
        }
    }

    #[allow(dead_code)]
    pub fn active_group(&self) -> &Window {
        self.groups.get(&self.active_group).unwrap()
    }

    pub fn active_group_mut(&mut self) -> &mut Window {
        self.groups.get_mut(&self.active_group).unwrap()
    }

    #[allow(dead_code)]
    pub fn group_ids(&self) -> Vec<WindowId> {
        self.layout.pane_ids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::TabId;
    use crate::window::{Tab, TabKind};

    fn make_workspace() -> (Workspace, WindowId, TabId) {
        let gid = WindowId::new_v4();
        let pid = TabId::new_v4();
        let pane = Tab::spawn_error(pid, TabKind::Shell, "test");
        let group = Window::new(gid, pane);
        let ws = Workspace::new("ws1".to_string(), gid, group);
        (ws, gid, pid)
    }

    #[test]
    fn test_new_workspace_has_correct_name() {
        let (ws, _, _) = make_workspace();
        assert_eq!(ws.name, "ws1");
    }

    #[test]
    fn test_new_workspace_has_single_group() {
        let (ws, gid, _) = make_workspace();
        assert_eq!(ws.groups.len(), 1);
        assert!(ws.groups.contains_key(&gid));
    }

    #[test]
    fn test_new_workspace_active_group_is_initial() {
        let (ws, gid, _) = make_workspace();
        assert_eq!(ws.active_group, gid);
    }

    #[test]
    fn test_new_workspace_layout_is_leaf() {
        let (ws, gid, _) = make_workspace();
        assert_eq!(ws.layout, LayoutNode::Leaf(gid));
    }

    #[test]
    fn test_active_group_returns_correct_group() {
        let (ws, gid, pid) = make_workspace();
        let group = ws.active_group();
        assert_eq!(group.id, gid);
        assert_eq!(group.active_tab().id, pid);
    }

    #[test]
    fn test_active_group_mut_allows_modification() {
        let (mut ws, _, _) = make_workspace();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Agent, "tab2");
        ws.active_group_mut().add_tab(p2);
        assert_eq!(ws.active_group().tab_count(), 2);
    }

    #[test]
    fn test_group_ids_matches_layout() {
        let (ws, gid, _) = make_workspace();
        let ids = ws.group_ids();
        assert_eq!(ids, vec![gid]);
    }

    // --- Integration tests ---

    #[test]
    fn test_create_split_close_cycle() {
        let (mut ws, gid1, _pid1) = make_workspace();

        // Split the initial group horizontally
        let gid2 = WindowId::new_v4();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "tab");
        let group2 = Window::new(gid2, p2);
        assert!(ws.layout.split_pane(gid1, crate::layout::SplitDirection::Horizontal, gid2));
        ws.groups.insert(gid2, group2);
        ws.active_group = gid2;

        // Verify state after split
        assert_eq!(ws.groups.len(), 2);
        assert_eq!(ws.group_ids().len(), 2);
        assert!(ws.groups.contains_key(&gid1));
        assert!(ws.groups.contains_key(&gid2));
        assert_eq!(ws.active_group, gid2);

        // Close gid2 — layout collapses back to single leaf
        let focus = ws.layout.close_pane(gid2);
        assert_eq!(focus, Some(gid1));
        ws.groups.remove(&gid2);
        ws.active_group = gid1;

        // Verify state after close
        assert_eq!(ws.groups.len(), 1);
        assert_eq!(ws.group_ids(), vec![gid1]);
        assert_eq!(ws.active_group, gid1);
        assert_eq!(ws.layout, LayoutNode::Leaf(gid1));
    }

    #[test]
    fn test_active_group_after_split() {
        let (mut ws, gid1, _) = make_workspace();

        let gid2 = WindowId::new_v4();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2");
        ws.layout.split_pane(gid1, crate::layout::SplitDirection::Horizontal, gid2);
        ws.groups.insert(gid2, Window::new(gid2, p2));
        ws.active_group = gid2;

        // Active group should be gid2
        assert_eq!(ws.active_group_mut().id, gid2);

        // Switch to gid1
        ws.active_group = gid1;
        assert_eq!(ws.active_group().id, gid1);
    }

    #[test]
    fn test_active_group_after_close_focused() {
        let (mut ws, gid1, _) = make_workspace();

        // Add gid2
        let gid2 = WindowId::new_v4();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2");
        ws.layout.split_pane(gid1, crate::layout::SplitDirection::Vertical, gid2);
        ws.groups.insert(gid2, Window::new(gid2, p2));
        ws.active_group = gid2;

        // Close the active group (gid2)
        let focus = ws.layout.close_pane(gid2);
        ws.groups.remove(&gid2);
        if let Some(new_focus) = focus {
            ws.active_group = new_focus;
        }

        assert_eq!(ws.active_group, gid1);
        assert_eq!(ws.groups.len(), 1);
    }

    #[test]
    fn test_leaf_min_sizes_stale_entries() {
        let (mut ws, gid1, _) = make_workspace();

        // Add a second group and set leaf_min_sizes for both
        let gid2 = WindowId::new_v4();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2");
        ws.layout.split_pane(gid1, crate::layout::SplitDirection::Horizontal, gid2);
        ws.groups.insert(gid2, Window::new(gid2, p2));
        ws.leaf_min_sizes.insert(gid1, (50, 10));
        ws.leaf_min_sizes.insert(gid2, (60, 12));

        // Close gid2 — leaf_min_sizes still has the entry for gid2 (stale)
        ws.layout.close_pane(gid2);
        ws.groups.remove(&gid2);

        // Verify stale entry exists
        assert!(ws.leaf_min_sizes.contains_key(&gid2));
        assert!(!ws.layout.contains(gid2));

        // Clean up stale entries (the pattern the codebase should follow)
        let live_ids: std::collections::HashSet<_> = ws.layout.pane_ids().into_iter().collect();
        ws.leaf_min_sizes.retain(|id, _| live_ids.contains(id));

        // After cleanup, only gid1's entry remains
        assert_eq!(ws.leaf_min_sizes.len(), 1);
        assert!(ws.leaf_min_sizes.contains_key(&gid1));
    }

    #[test]
    fn test_multiple_groups_in_workspace() {
        let (mut ws, gid1, _) = make_workspace();

        // Add 3 more groups via splits
        let gid2 = WindowId::new_v4();
        let gid3 = WindowId::new_v4();
        let gid4 = WindowId::new_v4();

        ws.layout.split_pane(gid1, crate::layout::SplitDirection::Horizontal, gid2);
        ws.groups.insert(gid2, Window::new(gid2, Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "g2")));

        ws.layout.split_pane(gid2, crate::layout::SplitDirection::Vertical, gid3);
        ws.groups.insert(gid3, Window::new(gid3, Tab::spawn_error(TabId::new_v4(), TabKind::Agent, "g3")));

        ws.layout.split_pane(gid1, crate::layout::SplitDirection::Vertical, gid4);
        ws.groups.insert(gid4, Window::new(gid4, Tab::spawn_error(TabId::new_v4(), TabKind::Nvim, "g4")));

        // 4 groups total
        assert_eq!(ws.groups.len(), 4);
        assert_eq!(ws.group_ids().len(), 4);

        // All group IDs in the layout
        let layout_ids = ws.group_ids();
        assert!(layout_ids.contains(&gid1));
        assert!(layout_ids.contains(&gid2));
        assert!(layout_ids.contains(&gid3));
        assert!(layout_ids.contains(&gid4));

        // Can switch active group to any
        for &gid in &[gid1, gid2, gid3, gid4] {
            ws.active_group = gid;
            assert_eq!(ws.active_group().id, gid);
        }
    }

    #[test]
    fn test_workspace_sync_panes_default() {
        let (ws, _, _) = make_workspace();
        assert!(!ws.sync_panes);
    }

    #[test]
    fn test_multiple_splits_then_equalize() {
        let (mut ws, gid1, _) = make_workspace();

        let gid2 = WindowId::new_v4();
        let gid3 = WindowId::new_v4();

        ws.layout.split_pane(gid1, crate::layout::SplitDirection::Horizontal, gid2);
        ws.groups.insert(gid2, Window::new(gid2, Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "g2")));

        ws.layout.split_pane(gid2, crate::layout::SplitDirection::Vertical, gid3);
        ws.groups.insert(gid3, Window::new(gid3, Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "g3")));

        // Resize to skew ratios
        ws.layout.resize(gid1, 0.2);

        // Equalize
        ws.layout.equalize();
        ws.leaf_min_sizes.clear();

        fn check_ratios(n: &LayoutNode) {
            if let LayoutNode::Split { ratio, first, second, .. } = n {
                assert!((*ratio - 0.5).abs() < f64::EPSILON);
                check_ratios(first);
                check_ratios(second);
            }
        }
        check_ratios(&ws.layout);
        assert!(ws.leaf_min_sizes.is_empty());
    }
}
