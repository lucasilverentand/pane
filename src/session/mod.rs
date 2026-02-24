pub mod store;

use crate::layout::{LayoutNode, TabId};
use crate::window::{TabKind, WindowId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Serializable snapshot of all workspace state, saved to disk for persistence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedState {
    #[serde(default = "default_version")]
    pub version: u32,
    pub updated_at: DateTime<Utc>,
    pub workspaces: Vec<WorkspaceConfig>,
    pub active_workspace: usize,
}

fn default_version() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    pub layout: LayoutNode,
    pub groups: Vec<WindowConfig>,
    pub active_group: WindowId,
    #[serde(default)]
    pub sync_panes: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowConfig {
    pub id: WindowId,
    pub tabs: Vec<TabConfig>,
    pub active_tab: usize,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabConfig {
    pub id: TabId,
    pub kind: TabKind,
    pub title: String,
    pub command: Option<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub scrollback: Vec<String>,
}

impl SavedState {
    pub fn from_server(state: &crate::server::state::ServerState) -> Self {
        let mut workspaces = Vec::new();

        for ws in &state.workspaces {
            let mut groups = Vec::new();

            for (gid, group) in &ws.groups {
                let tabs: Vec<TabConfig> = group
                    .tabs
                    .iter()
                    .map(|pane| {
                        let screen = pane.screen();
                        let mut scrollback = Vec::new();
                        let rows = screen.size().0;
                        for row in 0..rows {
                            let line = screen.contents_between(row, 0, row + 1, screen.size().1);
                            scrollback.push(line);
                        }
                        while scrollback
                            .last()
                            .map(|l| l.trim().is_empty())
                            .unwrap_or(false)
                        {
                            scrollback.pop();
                        }

                        TabConfig {
                            id: pane.id,
                            kind: pane.kind.clone(),
                            title: pane.title.clone(),
                            command: pane.command.clone(),
                            cwd: pane.cwd.clone(),
                            env: HashMap::new(),
                            scrollback,
                        }
                    })
                    .collect();

                groups.push(WindowConfig {
                    id: *gid,
                    tabs,
                    active_tab: group.active_tab,
                    name: group.name.clone(),
                });
            }

            workspaces.push(WorkspaceConfig {
                name: ws.name.clone(),
                layout: ws.layout.clone(),
                groups,
                active_group: ws.active_group,
                sync_panes: ws.sync_panes,
            });
        }

        SavedState {
            version: 2,
            updated_at: Utc::now(),
            workspaces,
            active_workspace: state.active_workspace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SplitDirection;

    fn make_saved_state() -> SavedState {
        let group_id1 = WindowId::new_v4();
        let group_id2 = WindowId::new_v4();
        let pane_id1 = TabId::new_v4();
        let pane_id2 = TabId::new_v4();
        let pane_id3 = TabId::new_v4();

        SavedState {
            version: 2,
            updated_at: Utc::now(),
            workspaces: vec![WorkspaceConfig {
                name: "1".to_string(),
                layout: LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.6,
                    first: Box::new(LayoutNode::Leaf(group_id1)),
                    second: Box::new(LayoutNode::Leaf(group_id2)),
                },
                groups: vec![
                    WindowConfig {
                        id: group_id1,
                        tabs: vec![
                            TabConfig {
                                id: pane_id1,
                                kind: TabKind::Shell,
                                title: "shell".to_string(),
                                command: None,
                                cwd: PathBuf::from("/home/user"),
                                env: HashMap::from([(
                                    "TERM".to_string(),
                                    "xterm-256color".to_string(),
                                )]),
                                scrollback: vec![
                                    "$ cargo build".to_string(),
                                    "   Compiling pane v0.1.0".to_string(),
                                ],
                            },
                            TabConfig {
                                id: pane_id2,
                                kind: TabKind::Nvim,
                                title: "nvim".to_string(),
                                command: None,
                                cwd: PathBuf::from("/home/user"),
                                env: HashMap::new(),
                                scrollback: vec![],
                            },
                        ],
                        active_tab: 0,
                        name: None,
                    },
                    WindowConfig {
                        id: group_id2,
                        tabs: vec![TabConfig {
                            id: pane_id3,
                            kind: TabKind::DevServer,
                            title: "server".to_string(),
                            command: Some("npm run dev".to_string()),
                            cwd: PathBuf::from("/home/user/project"),
                            env: HashMap::new(),
                            scrollback: vec!["ready on localhost:3000".to_string()],
                        }],
                        active_tab: 0,
                        name: None,
                    },
                ],
                active_group: group_id1,
                sync_panes: false,
            }],
            active_workspace: 0,
        }
    }

    #[test]
    fn test_saved_state_serialization_roundtrip() {
        let state = make_saved_state();
        let json = serde_json::to_string_pretty(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.workspaces.len(), 1);
        assert_eq!(restored.workspaces[0].groups.len(), 2);
    }

    #[test]
    fn test_pane_config_preserves_fields() {
        let state = make_saved_state();
        let json = serde_json::to_string(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();

        let group0 = &restored.workspaces[0].groups[0];
        let shell = &group0.tabs[0];
        assert_eq!(shell.kind, TabKind::Shell);
        assert_eq!(shell.title, "shell");
        assert!(shell.command.is_none());
        assert_eq!(shell.cwd, PathBuf::from("/home/user"));
        assert_eq!(shell.scrollback.len(), 2);

        let group1 = &restored.workspaces[0].groups[1];
        let server = &group1.tabs[0];
        assert_eq!(server.kind, TabKind::DevServer);
        assert_eq!(server.command.as_deref(), Some("npm run dev"));
    }

    #[test]
    fn test_layout_preserved() {
        let state = make_saved_state();
        let json = serde_json::to_string(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();

        let layout = &restored.workspaces[0].layout;
        let ids = layout.pane_ids();
        assert_eq!(ids.len(), 2);

        if let LayoutNode::Split {
            direction, ratio, ..
        } = layout
        {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!((ratio - 0.6).abs() < f64::EPSILON);
        } else {
            panic!("Expected Split layout");
        }
    }

    #[test]
    fn test_empty_state() {
        let group_id = WindowId::new_v4();
        let state = SavedState {
            version: 2,
            updated_at: Utc::now(),
            workspaces: vec![WorkspaceConfig {
                name: "1".to_string(),
                layout: LayoutNode::Leaf(group_id),
                groups: vec![WindowConfig {
                    id: group_id,
                    tabs: vec![],
                    active_tab: 0,
                    name: None,
                }],
                active_group: group_id,
                sync_panes: false,
            }],
            active_workspace: 0,
        };

        let json = serde_json::to_string(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.workspaces[0].groups[0].tabs.len(), 0);
    }

    #[test]
    fn test_pane_kind_serialization() {
        let kinds = vec![
            TabKind::Shell,
            TabKind::Agent,
            TabKind::Nvim,
            TabKind::DevServer,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let restored: TabKind = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, kind);
        }
    }

    #[test]
    fn test_tab_groups_preserved() {
        let state = make_saved_state();
        let json = serde_json::to_string(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();

        // First group has 2 tabs
        assert_eq!(restored.workspaces[0].groups[0].tabs.len(), 2);
        // Second group has 1 tab
        assert_eq!(restored.workspaces[0].groups[1].tabs.len(), 1);
    }
}
