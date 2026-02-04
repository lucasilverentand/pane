pub mod store;

use crate::layout::{LayoutNode, PaneId};
use crate::pane::{PaneGroupId, PaneKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub workspaces: Vec<WorkspaceConfig>,
    pub active_workspace: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    pub layout: LayoutNode,
    pub groups: Vec<PaneGroupConfig>,
    pub active_group: PaneGroupId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaneGroupConfig {
    pub id: PaneGroupId,
    pub tabs: Vec<PaneConfig>,
    pub active_tab: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaneConfig {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
    pub command: Option<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub scrollback: Vec<String>,
}

impl Session {
    pub fn from_app(app: &crate::app::App) -> Self {
        let mut workspaces = Vec::new();

        for ws in &app.workspaces {
            let mut groups = Vec::new();

            for (gid, group) in &ws.groups {
                let tabs: Vec<PaneConfig> = group
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

                        PaneConfig {
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

                groups.push(PaneGroupConfig {
                    id: *gid,
                    tabs,
                    active_tab: group.active_tab,
                });
            }

            workspaces.push(WorkspaceConfig {
                name: ws.name.clone(),
                layout: ws.layout.clone(),
                groups,
                active_group: ws.active_group,
            });
        }

        Session {
            id: app.session_id,
            name: app.session_name.clone(),
            created_at: app.session_created_at,
            updated_at: Utc::now(),
            workspaces,
            active_workspace: app.active_workspace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SplitDirection;

    fn make_session() -> Session {
        let group_id1 = PaneGroupId::new_v4();
        let group_id2 = PaneGroupId::new_v4();
        let pane_id1 = PaneId::new_v4();
        let pane_id2 = PaneId::new_v4();
        let pane_id3 = PaneId::new_v4();

        Session {
            id: Uuid::new_v4(),
            name: "test-session".to_string(),
            created_at: Utc::now(),
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
                    PaneGroupConfig {
                        id: group_id1,
                        tabs: vec![
                            PaneConfig {
                                id: pane_id1,
                                kind: PaneKind::Shell,
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
                            PaneConfig {
                                id: pane_id2,
                                kind: PaneKind::Nvim,
                                title: "nvim".to_string(),
                                command: None,
                                cwd: PathBuf::from("/home/user"),
                                env: HashMap::new(),
                                scrollback: vec![],
                            },
                        ],
                        active_tab: 0,
                    },
                    PaneGroupConfig {
                        id: group_id2,
                        tabs: vec![PaneConfig {
                            id: pane_id3,
                            kind: PaneKind::DevServer,
                            title: "server".to_string(),
                            command: Some("npm run dev".to_string()),
                            cwd: PathBuf::from("/home/user/project"),
                            env: HashMap::new(),
                            scrollback: vec!["ready on localhost:3000".to_string()],
                        }],
                        active_tab: 0,
                    },
                ],
                active_group: group_id1,
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
        assert_eq!(shell.kind, PaneKind::Shell);
        assert_eq!(shell.title, "shell");
        assert!(shell.command.is_none());
        assert_eq!(shell.cwd, PathBuf::from("/home/user"));
        assert_eq!(shell.scrollback.len(), 2);

        let group1 = &restored.workspaces[0].groups[1];
        let server = &group1.tabs[0];
        assert_eq!(server.kind, PaneKind::DevServer);
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
        let group_id = PaneGroupId::new_v4();
        let session = Session {
            id: Uuid::new_v4(),
            name: "empty".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            workspaces: vec![WorkspaceConfig {
                name: "1".to_string(),
                layout: LayoutNode::Leaf(group_id),
                groups: vec![PaneGroupConfig {
                    id: group_id,
                    tabs: vec![],
                    active_tab: 0,
                }],
                active_group: group_id,
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
            PaneKind::Shell,
            PaneKind::Agent,
            PaneKind::Nvim,
            PaneKind::DevServer,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let restored: PaneKind = serde_json::from_str(&json).unwrap();
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
