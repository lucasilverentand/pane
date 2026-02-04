use std::collections::HashMap;

use crate::layout::LayoutNode;
use crate::pane::{PaneGroup, PaneGroupId};

pub struct Workspace {
    pub name: String,
    pub layout: LayoutNode,
    pub groups: HashMap<PaneGroupId, PaneGroup>,
    pub active_group: PaneGroupId,
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
