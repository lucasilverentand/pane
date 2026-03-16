use std::path::PathBuf;

use pane_protocol::config::HubWidget;
use pane_protocol::layout::{LayoutNode, TabId};
use pane_protocol::window_types::WindowId;
use serde::{Deserialize, Serialize};

use crate::window::{Tab, Window};
use crate::workspace::Workspace;

/// Serializable state of the home workspace layout.
#[derive(Serialize, Deserialize)]
struct HomeLayoutState {
    layout: LayoutNode,
    windows: Vec<HomeWindowState>,
}

/// Serializable state of a single window in the home workspace.
#[derive(Serialize, Deserialize)]
struct HomeWindowState {
    id: WindowId,
    widgets: Vec<HubWidget>,
    active_tab: usize,
}

fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("pane"))
}

fn layout_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("home_layout.json"))
}

/// Save the home workspace layout to disk.
pub fn save_home_layout(workspace: &Workspace) {
    if !workspace.is_home {
        return;
    }
    let windows: Vec<HomeWindowState> = workspace
        .groups
        .iter()
        .map(|(id, window)| {
            let widgets: Vec<HubWidget> = window
                .tabs
                .iter()
                .filter_map(|tab| {
                    if let pane_protocol::window_types::TabKind::Widget(ref w) = tab.kind {
                        Some(w.clone())
                    } else {
                        None
                    }
                })
                .collect();
            HomeWindowState {
                id: *id,
                widgets,
                active_tab: window.active_tab,
            }
        })
        .collect();

    let state = HomeLayoutState {
        layout: workspace.layout.clone(),
        windows,
    };

    let Some(path) = layout_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    // Write atomically via temp file
    let tmp = path.with_extension("json.tmp");
    match serde_json::to_string_pretty(&state) {
        Ok(json) => {
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
        Err(_) => {}
    }
}

/// Load a saved home workspace layout from disk.
pub fn load_home_layout() -> Option<Workspace> {
    let path = layout_path()?;
    let json = std::fs::read_to_string(&path).ok()?;
    let state: HomeLayoutState = serde_json::from_str(&json).ok()?;

    let home_cwd = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"));

    let mut groups = std::collections::HashMap::new();
    let mut first_group_id = None;

    for ws in &state.windows {
        if ws.widgets.is_empty() {
            continue;
        }
        let tabs: Vec<Tab> = ws
            .widgets
            .iter()
            .map(|w| Tab::new_widget(TabId::new_v4(), w.clone()))
            .collect();
        if tabs.is_empty() {
            continue;
        }
        let active_tab = ws.active_tab.min(tabs.len() - 1);
        let window = Window {
            id: ws.id,
            tabs,
            active_tab,
            name: None,
        };
        groups.insert(ws.id, window);
        if first_group_id.is_none() {
            first_group_id = Some(ws.id);
        }
    }

    if groups.is_empty() {
        return None;
    }

    // Validate layout references — all leaf IDs must exist in groups
    let leaf_ids = state.layout.pane_ids();
    for id in &leaf_ids {
        if !groups.contains_key(id) {
            return None;
        }
    }

    let active_group = first_group_id.unwrap();
    Some(Workspace {
        name: "Home".to_string(),
        cwd: home_cwd,
        layout: state.layout,
        groups,
        active_group,
        folded_windows: std::collections::HashSet::new(),
        sync_panes: false,
        zoomed_window: None,
        saved_ratios: None,
        floating_windows: Vec::new(),
        is_home: true,
    })
}
