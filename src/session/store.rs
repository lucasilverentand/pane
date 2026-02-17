use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::Session;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: Uuid,
    pub name: String,
    pub pane_count: usize,
    pub updated_at: DateTime<Utc>,
}

pub fn sessions_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pane")
        .join("sessions");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn save(session: &Session) -> Result<()> {
    save_to_dir(session, &sessions_dir())
}

pub fn load(id: &Uuid) -> Result<Session> {
    load_from_dir(id, &sessions_dir())
}

pub fn list() -> Result<Vec<SessionSummary>> {
    list_from_dir(&sessions_dir())
}

pub fn delete(id: &Uuid) -> Result<()> {
    delete_from_dir(id, &sessions_dir())
}

/// Load the most recently updated session with the given name.
pub fn load_by_name(name: &str) -> Option<Session> {
    let summaries = list().ok()?;
    let summary = summaries.iter().find(|s| s.name == name)?;
    load(&summary.id).ok()
}

// Path-parameterized variants for testability

pub fn save_to_dir(session: &Session, dir: &Path) -> Result<()> {
    let _ = fs::create_dir_all(dir);
    let path = dir.join(format!("{}.json", session.id));
    let json = serde_json::to_string_pretty(session)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_from_dir(id: &Uuid, dir: &Path) -> Result<Session> {
    let path = dir.join(format!("{}.json", id));
    let json = fs::read_to_string(path)?;
    let mut session: Session = serde_json::from_str(&json)?;
    migrate_session(&mut session);
    Ok(session)
}

/// Migrate a session to the latest version (currently v2).
/// v1 (no explicit version field) -> v2: adds version, sync_panes, group name fields.
/// The `#[serde(default)]` annotations handle missing fields during deserialization,
/// so migration only needs to bump the version number for re-saving.
fn migrate_session(session: &mut Session) {
    if session.version < 2 {
        session.version = 2;
    }
}

pub fn list_from_dir(dir: &Path) -> Result<Vec<SessionSummary>> {
    let mut summaries = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(json) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<Session>(&json) {
                        let pane_count: usize = session
                            .workspaces
                            .iter()
                            .flat_map(|ws| &ws.groups)
                            .map(|g| g.tabs.len())
                            .sum();
                        summaries.push(SessionSummary {
                            id: session.id,
                            name: session.name,
                            pane_count,
                            updated_at: session.updated_at,
                        });
                    }
                }
            }
        }
    }

    summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(summaries)
}

pub fn delete_from_dir(id: &Uuid, dir: &Path) -> Result<()> {
    let path = dir.join(format!("{}.json", id));
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutNode, PaneId};
    use crate::pane::{PaneGroupId, PaneKind};
    use crate::session::{PaneConfig, PaneGroupConfig, WorkspaceConfig};
    use std::collections::HashMap;

    fn make_test_session(name: &str) -> Session {
        let group_id = PaneGroupId::new_v4();
        let pane_id = PaneId::new_v4();
        Session {
            id: Uuid::new_v4(),
            name: name.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 2,
            workspaces: vec![WorkspaceConfig {
                name: "1".to_string(),
                layout: LayoutNode::Leaf(group_id),
                groups: vec![PaneGroupConfig {
                    id: group_id,
                    tabs: vec![PaneConfig {
                        id: pane_id,
                        kind: PaneKind::Shell,
                        title: "shell".to_string(),
                        command: None,
                        cwd: PathBuf::from("/tmp"),
                        env: HashMap::new(),
                        scrollback: vec!["$ echo hello".to_string(), "hello".to_string()],
                    }],
                    active_tab: 0,
                    name: None,
                }],
                active_group: group_id,
                sync_panes: false,
            }],
            active_workspace: 0,
        }
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let session = make_test_session("test-session");
        let id = session.id;

        save_to_dir(&session, dir.path()).unwrap();
        let loaded = load_from_dir(&id, dir.path()).unwrap();

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.name, "test-session");
        let tabs = &loaded.workspaces[0].groups[0].tabs;
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].title, "shell");
        assert_eq!(tabs[0].scrollback.len(), 2);
    }

    #[test]
    fn test_list_sessions() {
        let dir = tempfile::tempdir().unwrap();

        let s1 = make_test_session("alpha");
        let s2 = make_test_session("beta");
        save_to_dir(&s1, dir.path()).unwrap();
        save_to_dir(&s2, dir.path()).unwrap();

        let summaries = list_from_dir(dir.path()).unwrap();
        assert_eq!(summaries.len(), 2);

        let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn test_delete_session() {
        let dir = tempfile::tempdir().unwrap();

        let session = make_test_session("doomed");
        let id = session.id;
        save_to_dir(&session, dir.path()).unwrap();

        assert_eq!(list_from_dir(dir.path()).unwrap().len(), 1);

        delete_from_dir(&id, dir.path()).unwrap();
        assert_eq!(list_from_dir(dir.path()).unwrap().len(), 0);
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_from_dir(&Uuid::new_v4(), dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_nonexistent_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let result = delete_from_dir(&Uuid::new_v4(), dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let summaries = list_from_dir(dir.path()).unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn test_list_ignores_non_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a session").unwrap();
        let summaries = list_from_dir(dir.path()).unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn test_list_ignores_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.json"), "{ invalid }").unwrap();
        let summaries = list_from_dir(dir.path()).unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn test_save_overwrites() {
        let dir = tempfile::tempdir().unwrap();

        let mut session = make_test_session("original");
        let id = session.id;
        save_to_dir(&session, dir.path()).unwrap();

        session.name = "updated".to_string();
        save_to_dir(&session, dir.path()).unwrap();

        let loaded = load_from_dir(&id, dir.path()).unwrap();
        assert_eq!(loaded.name, "updated");
        assert_eq!(list_from_dir(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn test_summary_pane_count() {
        let dir = tempfile::tempdir().unwrap();
        let group_id = PaneGroupId::new_v4();
        let pane1 = PaneId::new_v4();
        let pane2 = PaneId::new_v4();

        let session = Session {
            id: Uuid::new_v4(),
            name: "multi".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 2,
            workspaces: vec![WorkspaceConfig {
                name: "1".to_string(),
                layout: LayoutNode::Leaf(group_id),
                groups: vec![PaneGroupConfig {
                    id: group_id,
                    tabs: vec![
                        PaneConfig {
                            id: pane1,
                            kind: PaneKind::Shell,
                            title: "shell".to_string(),
                            command: None,
                            cwd: PathBuf::from("/"),
                            env: HashMap::new(),
                            scrollback: vec![],
                        },
                        PaneConfig {
                            id: pane2,
                            kind: PaneKind::Nvim,
                            title: "nvim".to_string(),
                            command: None,
                            cwd: PathBuf::from("/"),
                            env: HashMap::new(),
                            scrollback: vec![],
                        },
                    ],
                    active_tab: 0,
                    name: None,
                }],
                active_group: group_id,
                sync_panes: false,
            }],
            active_workspace: 0,
        };

        save_to_dir(&session, dir.path()).unwrap();
        let summaries = list_from_dir(dir.path()).unwrap();
        assert_eq!(summaries[0].pane_count, 2);
    }

    #[test]
    fn test_v1_session_migration() {
        let dir = tempfile::tempdir().unwrap();
        // Create a v1 session (no "version" field, no "sync_panes", no group "name")
        let id = Uuid::new_v4();
        let group_id = PaneGroupId::new_v4();
        let pane_id = PaneId::new_v4();
        let v1_json = serde_json::json!({
            "id": id.to_string(),
            "name": "old-session",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "workspaces": [{
                "name": "1",
                "layout": { "Leaf": group_id.to_string() },
                "groups": [{
                    "id": group_id.to_string(),
                    "tabs": [{
                        "id": pane_id.to_string(),
                        "kind": "Shell",
                        "title": "shell",
                        "command": null,
                        "cwd": "/tmp",
                        "env": {},
                        "scrollback": []
                    }],
                    "active_tab": 0
                }],
                "active_group": group_id.to_string()
            }],
            "active_workspace": 0
        });
        let path = dir.path().join(format!("{}.json", id));
        std::fs::write(&path, serde_json::to_string_pretty(&v1_json).unwrap()).unwrap();

        let loaded = load_from_dir(&id, dir.path()).unwrap();
        // Migration should bump version to 2
        assert_eq!(loaded.version, 2);
        // Default values should be present
        assert_eq!(loaded.workspaces[0].sync_panes, false);
        assert_eq!(loaded.workspaces[0].groups[0].name, None);
    }
}
