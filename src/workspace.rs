use std::collections::HashMap;

use crate::layout::LayoutNode;
use crate::pane::{PaneGroup, PaneGroupId};

pub struct Workspace {
    pub name: String,
    pub layout: LayoutNode,
    pub groups: HashMap<PaneGroupId, PaneGroup>,
    pub active_group: PaneGroupId,
    /// Per-leaf custom minimum sizes set by user drag/resize.
    /// Falls back to global config defaults when absent.
    pub leaf_min_sizes: HashMap<PaneGroupId, (u16, u16)>,
    /// When true, key input is broadcast to all panes in this workspace.
    pub sync_panes: bool,
}

impl Workspace {
    pub fn new(name: String, group_id: PaneGroupId, group: PaneGroup) -> Self {
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
    pub fn active_group(&self) -> &PaneGroup {
        self.groups.get(&self.active_group).unwrap()
    }

    pub fn active_group_mut(&mut self) -> &mut PaneGroup {
        self.groups.get_mut(&self.active_group).unwrap()
    }

    #[allow(dead_code)]
    pub fn group_ids(&self) -> Vec<PaneGroupId> {
        self.layout.pane_ids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PaneId;
    use crate::pane::{Pane, PaneKind};

    fn make_workspace() -> (Workspace, PaneGroupId, PaneId) {
        let gid = PaneGroupId::new_v4();
        let pid = PaneId::new_v4();
        let pane = Pane::spawn_error(pid, PaneKind::Shell, "test");
        let group = PaneGroup::new(gid, pane);
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
        assert_eq!(group.active_pane().id, pid);
    }

    #[test]
    fn test_active_group_mut_allows_modification() {
        let (mut ws, _, _) = make_workspace();
        let p2 = Pane::spawn_error(PaneId::new_v4(), PaneKind::Agent, "tab2");
        ws.active_group_mut().add_tab(p2);
        assert_eq!(ws.active_group().tab_count(), 2);
    }

    #[test]
    fn test_group_ids_matches_layout() {
        let (ws, gid, _) = make_workspace();
        let ids = ws.group_ids();
        assert_eq!(ids, vec![gid]);
    }
}
