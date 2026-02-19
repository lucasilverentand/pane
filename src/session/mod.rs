pub mod store;

use crate::layout::{LayoutNode, TabId};
use crate::window::{WindowId, TabKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

fn default_version() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_version")]
    pub version: u32,
    pub workspaces: Vec<WorkspaceConfig>,
    pub active_workspace: usize,
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

impl Session {
    pub fn from_state(state: &crate::server::state::ServerState) -> Self {
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
                            let line =
                                screen.contents_between(row, 0, row + 1, screen.size().1);
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

        Session {
            id: state.session_id,
            name: state.session_name.clone(),
            created_at: state.session_created_at,
            updated_at: Utc::now(),
            version: 2,
            workspaces,
            active_workspace: state.active_workspace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SplitDirection;

    fn make_session() -> Session {
        let group_id1 = WindowId::new_v4();
        let group_id2 = WindowId::new_v4();
        let pane_id1 = TabId::new_v4();
        let pane_id2 = TabId::new_v4();
        let pane_id3 = TabId::new_v4();

        Session {
            id: Uuid::new_v4(),
            name: "test-session".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 2,
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
    fn test_session_serialization_roundtrip() {
        let session = make_session();
        let json = serde_json::to_string_pretty(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.id, session.id);
        assert_eq!(restored.name, session.name);
        assert_eq!(restored.workspaces.len(), 1);
        assert_eq!(restored.workspaces[0].groups.len(), 2);
    }

    #[test]
    fn test_pane_config_preserves_fields() {
        let session = make_session();
        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();

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
    fn test_layout_preserved_in_session() {
        let session = make_session();
        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();

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
    fn test_session_timestamps() {
        let session = make_session();
        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(
            restored.created_at.timestamp(),
            session.created_at.timestamp()
        );
    }

    #[test]
    fn test_empty_session() {
        let group_id = WindowId::new_v4();
        let session = Session {
            id: Uuid::new_v4(),
            name: "empty".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 2,
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

        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.workspaces[0].groups[0].tabs.len(), 0);
        assert_eq!(restored.name, "empty");
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
        let session = make_session();
        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();

        // First group has 2 tabs
        assert_eq!(restored.workspaces[0].groups[0].tabs.len(), 2);
        // Second group has 1 tab
        assert_eq!(restored.workspaces[0].groups[1].tabs.len(), 1);
    }
}
