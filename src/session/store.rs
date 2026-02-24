use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use super::SavedState;

fn state_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pane")
}

pub fn state_file_path() -> PathBuf {
    state_dir().join("state.json")
}

pub fn save(state: &SavedState) -> Result<()> {
    save_to(state, &state_file_path())
}

pub fn load() -> Option<SavedState> {
    load_from(&state_file_path())
}

// Path-parameterized variants for testability

pub fn save_to(state: &SavedState, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(state)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_from(path: &Path) -> Option<SavedState> {
    let json = fs::read_to_string(path).ok()?;
    let mut state: SavedState = serde_json::from_str(&json).ok()?;
    migrate(&mut state);
    Some(state)
}

/// Migrate saved state to the latest version (currently v2).
fn migrate(state: &mut SavedState) {
    if state.version < 2 {
        state.version = 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutNode, TabId};
    use crate::session::{TabConfig, WindowConfig, WorkspaceConfig};
    use crate::window::{TabKind, WindowId};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_test_state() -> SavedState {
        let group_id = WindowId::new_v4();
        let pane_id = TabId::new_v4();
        SavedState {
            version: 2,
            updated_at: Utc::now(),
            workspaces: vec![WorkspaceConfig {
                name: "1".to_string(),
                layout: LayoutNode::Leaf(group_id),
                groups: vec![WindowConfig {
                    id: group_id,
                    tabs: vec![TabConfig {
                        id: pane_id,
                        kind: TabKind::Shell,
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
        let path = dir.path().join("state.json");
        let state = make_test_state();

        save_to(&state, &path).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.workspaces.len(), 1);
        let tabs = &loaded.workspaces[0].groups[0].tabs;
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].title, "shell");
        assert_eq!(tabs[0].scrollback.len(), 2);
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        assert!(load_from(&path).is_none());
    }

    #[test]
    fn test_save_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        let mut state = make_test_state();
        save_to(&state, &path).unwrap();

        state.workspaces[0].name = "updated".to_string();
        save_to(&state, &path).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.workspaces[0].name, "updated");
    }

    #[test]
    fn test_load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, "{ invalid }").unwrap();
        assert!(load_from(&path).is_none());
    }
}
