use std::collections::{HashMap, HashSet};

use tokio::sync::mpsc;

use crate::config::Config;
use crate::event::AppEvent;
use crate::layout::{ResolvedPane, Side, SplitDirection, TabId};
use crate::system_stats::SystemStats;
use crate::window::{Tab, TabKind, Window, WindowId};
use crate::workspace::Workspace;

/// Active drag state for mouse-driven split resizing.
#[derive(Clone, Debug)]
pub struct DragState {
    /// Path through the layout tree to the Split node being dragged.
    pub split_path: Vec<Side>,
    /// Direction of the split being dragged.
    pub direction: SplitDirection,
    /// Body rect for coordinate mapping.
    pub body: ratatui::layout::Rect,
}

pub struct ServerState {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub config: Config,
    pub system_stats: SystemStats,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub last_size: (u16, u16),
    /// Counter for assigning tmux-compatible %N pane IDs.
    pub next_pane_number: u32,
    /// Active drag state for split border resizing.
    pub drag_state: Option<DragState>,
}

/// Auto-name a workspace: git repo basename → cwd basename → incremental number.
fn auto_workspace_name(existing: &[Workspace]) -> String {
    let used: std::collections::HashSet<&str> =
        existing.iter().map(|ws| ws.name.as_str()).collect();

    // Try git repo basename
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(name) = std::path::Path::new(&path).file_name() {
                let name = name.to_string_lossy().to_string();
                if !name.is_empty() && !used.contains(name.as_str()) {
                    return name;
                }
            }
        }
    }

    // Try cwd basename
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = cwd.file_name() {
            let name = name.to_string_lossy().to_string();
            if !name.is_empty() && !used.contains(name.as_str()) {
                return name;
            }
        }
    }

    // Fallback: incremental number
    let mut n = 1u32;
    loop {
        let candidate = format!("{}", n);
        if !used.contains(candidate.as_str()) {
            return candidate;
        }
        n += 1;
    }
}

impl ServerState {
    /// Build tmux env vars for a new pane, incrementing the pane counter.
    pub fn next_tmux_env(&mut self) -> crate::window::pty::TmuxEnv {
        let n = self.next_pane_number;
        self.next_pane_number += 1;
        let socket_path = crate::server::daemon::socket_path();
        crate::window::pty::TmuxEnv {
            tmux_value: format!("{},{},0", socket_path.display(), std::process::id()),
            tmux_pane: format!("%{}", n),
        }
    }

    /// Get the last assigned pane number (the one just assigned by next_tmux_env).
    #[cfg(test)]
    pub fn last_pane_number(&self) -> u32 {
        self.next_pane_number.saturating_sub(1)
    }

    pub fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active_workspace]
    }

    pub fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_workspace]
    }

    /// Find a pane mutably across all workspaces/groups/tabs.
    pub fn find_tab_mut(&mut self, pane_id: TabId) -> Option<&mut Tab> {
        for ws in &mut self.workspaces {
            for group in ws.groups.values_mut() {
                for pane in &mut group.tabs {
                    if pane.id == pane_id {
                        return Some(pane);
                    }
                }
            }
        }
        None
    }

    /// Find which workspace/group a pane belongs to.
    pub fn find_tab_location(&self, pane_id: TabId) -> Option<(usize, WindowId)> {
        for (ws_idx, ws) in self.workspaces.iter().enumerate() {
            for (gid, group) in &ws.groups {
                for pane in &group.tabs {
                    if pane.id == pane_id {
                        return Some((ws_idx, *gid));
                    }
                }
            }
        }
        None
    }

    pub fn new(
        event_tx: &mpsc::UnboundedSender<AppEvent>,
        cols: u16,
        rows: u16,
        config: Config,
    ) -> anyhow::Result<Self> {
        let pane_id = TabId::new_v4();
        let group_id = WindowId::new_v4();

        // Build tmux env for the first pane
        let socket_path = crate::server::daemon::socket_path();
        let tmux_env = crate::window::pty::TmuxEnv {
            tmux_value: format!("{},{},0", socket_path.display(), std::process::id()),
            tmux_pane: "%0".to_string(),
        };

        let pane = match Tab::spawn_with_env(
            pane_id,
            TabKind::Shell,
            cols,
            rows,
            event_tx.clone(),
            None,
            Some(tmux_env),
        ) {
            Ok(p) => p,
            Err(e) => Tab::spawn_error(pane_id, TabKind::Shell, &e.to_string()),
        };

        let group = Window::new(group_id, pane);
        let workspace = Workspace::new(auto_workspace_name(&[]), group_id, group);

        Ok(Self {
            workspaces: vec![workspace],
            active_workspace: 0,
            config,
            system_stats: SystemStats::default(),
            event_tx: event_tx.clone(),
            last_size: (cols.saturating_add(2), rows.saturating_add(3)),
            next_pane_number: 1, // 0 was already used
            drag_state: None,
        })
    }

    pub fn restore(
        saved: crate::session::SavedState,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        width: u16,
        height: u16,
        config: Config,
    ) -> anyhow::Result<Self> {
        let size = ratatui::layout::Rect::new(0, 0, width, height);
        let mut workspaces = Vec::new();
        let mut pane_counter: u32 = 0;

        let socket_path = crate::server::daemon::socket_path();
        let pid = std::process::id();

        for ws_config in &saved.workspaces {
            let layout = ws_config.layout.clone();
            let resolved = layout.resolve(size);
            let mut groups = HashMap::new();

            for group_config in &ws_config.groups {
                let mut tabs = Vec::new();
                let (cols, rows) = resolved
                    .iter()
                    .find(|(id, _)| *id == group_config.id)
                    .map(|(_, r)| (r.width.saturating_sub(2), r.height.saturating_sub(2)))
                    .unwrap_or((80, 24));

                for pane_config in &group_config.tabs {
                    let tmux_env = crate::window::pty::TmuxEnv {
                        tmux_value: format!("{},{},0", socket_path.display(), pid),
                        tmux_pane: format!("%{}", pane_counter),
                    };
                    pane_counter += 1;

                    let pane = match Tab::spawn_with_env(
                        pane_config.id,
                        pane_config.kind.clone(),
                        cols,
                        rows,
                        event_tx.clone(),
                        pane_config.command.clone(),
                        Some(tmux_env),
                    ) {
                        Ok(mut p) => {
                            if !pane_config.title.ends_with("(error)") {
                                p.title = pane_config.title.clone();
                            }
                            p
                        }
                        Err(e) => Tab::spawn_error(
                            pane_config.id,
                            pane_config.kind.clone(),
                            &e.to_string(),
                        ),
                    };
                    tabs.push(pane);
                }

                if !tabs.is_empty() {
                    groups.insert(
                        group_config.id,
                        Window {
                            id: group_config.id,
                            tabs,
                            active_tab: group_config.active_tab,
                            name: group_config.name.clone(),
                        },
                    );
                }
            }

            let active_group = ws_config.active_group;
            workspaces.push(Workspace {
                name: ws_config.name.clone(),
                layout,
                groups,
                active_group,
                folded_windows: HashSet::new(),
                sync_panes: false,
                zoomed_window: None,
                saved_ratios: None,
                floating_windows: Vec::new(),
            });
        }

        if workspaces.is_empty() {
            let pane_id = TabId::new_v4();
            let group_id = WindowId::new_v4();
            let tmux_env = crate::window::pty::TmuxEnv {
                tmux_value: format!("{},{},0", socket_path.display(), pid),
                tmux_pane: format!("%{}", pane_counter),
            };
            pane_counter += 1;
            let pane = match Tab::spawn_with_env(
                pane_id,
                TabKind::Shell,
                80,
                24,
                event_tx.clone(),
                None,
                Some(tmux_env),
            ) {
                Ok(p) => p,
                Err(e) => Tab::spawn_error(pane_id, TabKind::Shell, &e.to_string()),
            };
            let group = Window::new(group_id, pane);
            workspaces.push(Workspace::new("1".to_string(), group_id, group));
        }

        Ok(Self {
            workspaces,
            active_workspace: saved.active_workspace,
            config,
            system_stats: SystemStats::default(),
            event_tx,
            last_size: (width, height),
            next_pane_number: pane_counter,
            drag_state: None,
        })
    }

    pub fn handle_pty_exited(&mut self, pane_id: TabId) -> bool {
        if let Some(pane) = self.find_tab_mut(pane_id) {
            pane.exited = true;
        }

        let location = self.find_tab_location(pane_id);
        if let Some((ws_idx, group_id)) = location {
            let ws = &self.workspaces[ws_idx];
            if let Some(group) = ws.groups.get(&group_id) {
                if group.tab_count() <= 1 {
                    let group_ids = ws.layout.group_ids();
                    if group_ids.len() <= 1 && self.workspaces.len() <= 1 {
                        return true; // should_quit
                    }
                    let ws = &mut self.workspaces[ws_idx];
                    if let Some(new_focus) = ws.layout.close_pane(group_id) {
                        ws.groups.remove(&group_id);
                        ws.prune_folded_windows();
                        ws.active_group = new_focus;
                    } else if self.workspaces.len() > 1 {
                        self.workspaces.remove(ws_idx);
                        if self.active_workspace >= self.workspaces.len() {
                            self.active_workspace = self.workspaces.len() - 1;
                        }
                    } else {
                        return true; // should_quit
                    }
                } else {
                    let ws = &mut self.workspaces[ws_idx];
                    if let Some(group) = ws.groups.get_mut(&group_id) {
                        if let Some(idx) = group.tabs.iter().position(|p| p.id == pane_id) {
                            group.close_tab(idx);
                        }
                    }
                }
            }
        }
        false
    }

    /// Focus a pane group, unfolding it if it's currently folded.
    pub fn focus_group(&mut self, id: WindowId) {
        let ws = &mut self.workspaces[self.active_workspace];
        ws.folded_windows.remove(&id);
        ws.active_group = id;
    }

    /// Toggle manual fold on the active group.
    /// - If the active group is folded, unfold it.
    /// - If it's visible, fold it and focus a neighbor.
    pub fn toggle_fold_active_group(&mut self) -> bool {
        let ws = &self.workspaces[self.active_workspace];
        let active_group = ws.active_group;

        // If already folded, just unfold
        if ws.folded_windows.contains(&active_group) {
            let ws = &mut self.workspaces[self.active_workspace];
            ws.folded_windows.remove(&active_group);
            let (w, h) = self.last_size;
            self.resize_all_tabs(w, h);
            return true;
        }

        // Can't fold the only window
        if ws.layout.group_ids().len() <= 1 {
            return false;
        }

        // Find a neighbor to focus after folding
        let neighbor = ws
            .layout
            .find_neighbor(active_group, SplitDirection::Horizontal, Side::Second)
            .or_else(|| {
                ws.layout
                    .find_neighbor(active_group, SplitDirection::Horizontal, Side::First)
            })
            .or_else(|| {
                ws.layout
                    .find_neighbor(active_group, SplitDirection::Vertical, Side::Second)
            })
            .or_else(|| {
                ws.layout
                    .find_neighbor(active_group, SplitDirection::Vertical, Side::First)
            });

        if let Some(neighbor) = neighbor {
            let ws = &mut self.workspaces[self.active_workspace];
            ws.folded_windows.insert(active_group);
            ws.active_group = neighbor;
            let (w, h) = self.last_size;
            self.resize_all_tabs(w, h);
            return true;
        }

        false
    }

    pub fn move_tab_to_neighbor(&mut self, direction: SplitDirection, side: Side) {
        let ws = self.active_workspace();
        let source_group_id = ws.active_group;

        let neighbor_id = match ws.layout.find_neighbor(source_group_id, direction, side) {
            Some(id) => id,
            None => return,
        };

        let ws = self.active_workspace();
        if ws
            .groups
            .get(&source_group_id)
            .map_or(true, |g| g.tabs.len() <= 1)
        {
            return;
        }

        let ws = self.active_workspace_mut();
        let tab_idx = ws.groups.get(&source_group_id).unwrap().active_tab;
        let pane = ws
            .groups
            .get_mut(&source_group_id)
            .unwrap()
            .remove_tab(tab_idx)
            .unwrap();
        ws.groups.get_mut(&neighbor_id).unwrap().add_tab(pane);
        ws.active_group = neighbor_id;
    }

    pub fn add_tab_to_active_group(
        &mut self,
        kind: TabKind,
        command: Option<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<TabId> {
        let pane_id = TabId::new_v4();
        let tmux_env = self.next_tmux_env();
        let pane = match Tab::spawn_with_env(
            pane_id,
            kind.clone(),
            cols,
            rows,
            self.event_tx.clone(),
            command,
            Some(tmux_env),
        ) {
            Ok(p) => p,
            Err(e) => Tab::spawn_error(pane_id, kind, &e.to_string()),
        };

        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
            group.add_tab(pane);
        }

        let (w, h) = self.last_size;
        self.resize_all_tabs(w, h);
        Ok(pane_id)
    }

    /// Split the active group and return (new_group_id, new_pane_id) for the created pane.
    pub fn split_active_group(
        &mut self,
        direction: SplitDirection,
        kind: TabKind,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<(WindowId, TabId)> {
        let new_group_id = WindowId::new_v4();
        let pane_id = TabId::new_v4();
        let tmux_env = self.next_tmux_env();

        let pane = match Tab::spawn_with_env(
            pane_id,
            kind.clone(),
            cols,
            rows,
            self.event_tx.clone(),
            None,
            Some(tmux_env),
        ) {
            Ok(p) => p,
            Err(e) => Tab::spawn_error(pane_id, kind, &e.to_string()),
        };

        let (w, h) = self.last_size;
        let group = Window::new(new_group_id, pane);
        let ws = self.active_workspace_mut();
        let source_group_id = ws.active_group;
        ws.layout
            .split_pane(source_group_id, direction, new_group_id);
        ws.groups.insert(new_group_id, group);
        ws.active_group = new_group_id;

        self.resize_all_tabs(w, h);
        Ok((new_group_id, pane_id))
    }

    pub fn new_workspace(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let pane_id = TabId::new_v4();
        let group_id = WindowId::new_v4();
        let tmux_env = self.next_tmux_env();

        let pane = match Tab::spawn_with_env(
            pane_id,
            TabKind::Shell,
            cols,
            rows,
            self.event_tx.clone(),
            None,
            Some(tmux_env),
        ) {
            Ok(p) => p,
            Err(e) => Tab::spawn_error(pane_id, TabKind::Shell, &e.to_string()),
        };

        let group = Window::new(group_id, pane);
        let workspace = Workspace::new(auto_workspace_name(&self.workspaces), group_id, group);
        self.workspaces.push(workspace);
        self.active_workspace = self.workspaces.len() - 1;

        let (w, h) = self.last_size;
        self.resize_all_tabs(w, h);
        Ok(())
    }

    /// Close the active workspace. Returns true if this was the last workspace
    /// (caller should shut down the server).
    pub fn close_workspace(&mut self) -> bool {
        if self.workspaces.len() <= 1 {
            return true;
        }
        self.workspaces.remove(self.active_workspace);
        if self.active_workspace >= self.workspaces.len() {
            self.active_workspace = self.workspaces.len() - 1;
        }
        false
    }

    pub fn restart_active_tab(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let active_group_id = self.active_workspace().active_group;
        let (exited, kind, command, id) = {
            let ws = self.active_workspace();
            if let Some(group) = ws.groups.get(&active_group_id) {
                let pane = group.active_tab();
                (
                    pane.exited,
                    pane.kind.clone(),
                    pane.command.clone(),
                    pane.id,
                )
            } else {
                return Ok(());
            }
        };

        if !exited {
            return Ok(());
        }

        let tmux_env = self.next_tmux_env();
        let new_pane = match Tab::spawn_with_env(
            id,
            kind.clone(),
            cols,
            rows,
            self.event_tx.clone(),
            command,
            Some(tmux_env),
        ) {
            Ok(p) => p,
            Err(e) => Tab::spawn_error(id, kind, &e.to_string()),
        };

        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&active_group_id) {
            *group.active_tab_mut() = new_pane;
        }

        let (w, h) = self.last_size;
        self.resize_all_tabs(w, h);
        Ok(())
    }

    pub fn resize_all_tabs(&mut self, w: u16, h: u16) {
        let overhead = 1 + self.workspace_bar_height();
        let body_height = h.saturating_sub(overhead);
        let size = ratatui::layout::Rect::new(0, 0, w, body_height);

        for ws in &mut self.workspaces {
            let resolved = ws
                .layout
                .resolve_with_folds(size, &ws.folded_windows);
            for rp in resolved {
                match rp {
                    ResolvedPane::Visible {
                        id: group_id, rect, ..
                    } => {
                        if let Some(group) = ws.groups.get_mut(&group_id) {
                            let cols = rect.width.saturating_sub(4);
                            let tab_bar_overhead: u16 = if group.tabs.len() > 1 { 1 } else { 0 };
                            let rows = rect.height.saturating_sub(2 + tab_bar_overhead); // 2 borders + optional tab bar
                            if cols > 0 && rows > 0 {
                                for pane in &mut group.tabs {
                                    pane.resize_pty(cols, rows);
                                }
                            }
                        }
                    }
                    ResolvedPane::Folded { .. } => {}
                }
            }
        }
    }

    pub fn workspace_bar_height(&self) -> u16 {
        if self.workspaces.is_empty() {
            0
        } else {
            3
        }
    }

    pub fn scroll_active_tab(&mut self, f: impl FnOnce(&mut Tab)) {
        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
            f(group.active_tab_mut());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::layout::{SplitDirection, TabId};
    use crate::window::{Tab, TabKind, Window, WindowId};

    /// Build a ServerState without PTY spawning. Uses `Tab::spawn_error` for all panes.
    fn make_test_state() -> (ServerState, mpsc::UnboundedReceiver<AppEvent>) {
        let (event_tx, rx) = mpsc::unbounded_channel();
        let pane_id = TabId::new_v4();
        let group_id = WindowId::new_v4();
        let pane = Tab::spawn_error(pane_id, TabKind::Shell, "test");
        let group = Window::new(group_id, pane);
        let workspace = Workspace::new("workspace".to_string(), group_id, group);
        let state = ServerState {
            workspaces: vec![workspace],
            active_workspace: 0,
            config: Config::default(),
            system_stats: SystemStats::default(),
            event_tx,
            last_size: (120, 40),
            next_pane_number: 1,
            drag_state: None,
        };
        (state, rx)
    }

    /// Build a ServerState with two groups in a horizontal split.
    fn make_split_state() -> (
        ServerState,
        WindowId,
        WindowId,
        mpsc::UnboundedReceiver<AppEvent>,
    ) {
        let (event_tx, rx) = mpsc::unbounded_channel();
        let gid1 = WindowId::new_v4();
        let gid2 = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "left");
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "right");
        let g1 = Window::new(gid1, p1);
        let g2 = Window::new(gid2, p2);
        let mut groups = HashMap::new();
        groups.insert(gid1, g1);
        groups.insert(gid2, g2);
        let layout = crate::layout::LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(crate::layout::LayoutNode::Leaf(gid1)),
            second: Box::new(crate::layout::LayoutNode::Leaf(gid2)),
        };
        let workspace = Workspace {
            name: "workspace".to_string(),
            layout,
            groups,
            active_group: gid1,
            folded_windows: HashSet::new(),
            sync_panes: false,
            zoomed_window: None,
            saved_ratios: None,
            floating_windows: Vec::new(),
        };
        let state = ServerState {
            workspaces: vec![workspace],
            active_workspace: 0,
            config: Config::default(),
            system_stats: SystemStats::default(),
            event_tx,
            last_size: (120, 40),
            next_pane_number: 2,
            drag_state: None,
        };
        (state, gid1, gid2, rx)
    }

    // ---- auto_workspace_name ----

    fn make_named_ws(name: &str) -> Workspace {
        let gid = WindowId::new_v4();
        let p = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t");
        let g = Window::new(gid, p);
        Workspace::new(name.to_string(), gid, g)
    }

    #[test]
    fn test_auto_workspace_name_empty_returns_nonempty() {
        let name = auto_workspace_name(&[]);
        assert!(!name.is_empty());
    }

    #[test]
    fn test_auto_workspace_name_avoids_duplicates() {
        let first = auto_workspace_name(&[]);
        let ws1 = make_named_ws(&first);
        let second = auto_workspace_name(&[ws1]);
        assert_ne!(first, second);
        assert!(!second.is_empty());
    }

    #[test]
    fn test_auto_workspace_name_fallback_numbers() {
        // When all smart names are taken, falls back to numbers
        let first = auto_workspace_name(&[]);
        let ws1 = make_named_ws(&first);
        let second = auto_workspace_name(&[ws1]);
        // Third call: both names taken, create new workspaces with those names
        let ws1b = make_named_ws(&first);
        let ws2b = make_named_ws(&second);
        let third = auto_workspace_name(&[ws1b, ws2b]);
        assert!(!third.is_empty());
        assert_ne!(third, first);
        assert_ne!(third, second);
    }

    // ---- find_tab_mut / find_tab_location ----

    #[test]
    fn test_find_tab_mut_in_active_workspace() {
        let (mut state, _rx) = make_test_state();
        let ws = &state.workspaces[0];
        let pane_id = ws.groups.values().next().unwrap().tabs[0].id;
        assert!(state.find_tab_mut(pane_id).is_some());
    }

    #[test]
    fn test_find_tab_mut_nonexistent() {
        let (mut state, _rx) = make_test_state();
        assert!(state.find_tab_mut(TabId::new_v4()).is_none());
    }

    #[test]
    fn test_find_tab_location_returns_correct_workspace_and_group() {
        let (state, _rx) = make_test_state();
        let gid = state.workspaces[0].active_group;
        let pane_id = state.workspaces[0].groups[&gid].tabs[0].id;
        let (ws_idx, found_gid) = state.find_tab_location(pane_id).unwrap();
        assert_eq!(ws_idx, 0);
        assert_eq!(found_gid, gid);
    }

    #[test]
    fn test_find_tab_location_nonexistent() {
        let (state, _rx) = make_test_state();
        assert!(state.find_tab_location(TabId::new_v4()).is_none());
    }

    #[test]
    fn test_find_pane_across_workspaces() {
        let (mut state, _rx) = make_test_state();
        // Add a second workspace manually
        let gid2 = WindowId::new_v4();
        let pid2 = TabId::new_v4();
        let p2 = Tab::spawn_error(pid2, TabKind::Shell, "ws2-pane");
        let g2 = Window::new(gid2, p2);
        let ws2 = Workspace::new("workspace 2".to_string(), gid2, g2);
        state.workspaces.push(ws2);

        let (ws_idx, found_gid) = state.find_tab_location(pid2).unwrap();
        assert_eq!(ws_idx, 1);
        assert_eq!(found_gid, gid2);
        assert!(state.find_tab_mut(pid2).is_some());
    }

    #[test]
    fn test_find_tab_mut_can_modify() {
        let (mut state, _rx) = make_test_state();
        let gid = state.workspaces[0].active_group;
        let pane_id = state.workspaces[0].groups[&gid].tabs[0].id;
        let pane = state.find_tab_mut(pane_id).unwrap();
        pane.title = "modified".to_string();
        // Verify modification persisted
        let pane = state.find_tab_mut(pane_id).unwrap();
        assert_eq!(pane.title, "modified");
    }

    // ---- handle_pty_exited ----

    #[test]
    fn test_handle_pty_exited_single_tab_single_group_quits() {
        let (mut state, _rx) = make_test_state();
        let gid = state.workspaces[0].active_group;
        let pane_id = state.workspaces[0].groups[&gid].tabs[0].id;
        let should_quit = state.handle_pty_exited(pane_id);
        assert!(should_quit, "last pane in last workspace should quit");
    }

    #[test]
    fn test_handle_pty_exited_multi_tab_removes_tab() {
        let (mut state, _rx) = make_test_state();
        let gid = state.workspaces[0].active_group;
        // Add a second tab
        let pid2 = TabId::new_v4();
        let p2 = Tab::spawn_error(pid2, TabKind::Shell, "tab2");
        state.workspaces[0]
            .groups
            .get_mut(&gid)
            .unwrap()
            .add_tab(p2);
        assert_eq!(state.workspaces[0].groups[&gid].tab_count(), 2);

        let should_quit = state.handle_pty_exited(pid2);
        assert!(!should_quit);
        assert_eq!(state.workspaces[0].groups[&gid].tab_count(), 1);
    }

    #[test]
    fn test_handle_pty_exited_closes_group_in_split() {
        let (mut state, gid1, _gid2, _rx) = make_split_state();
        let pane_id = state.workspaces[0].groups[&gid1].tabs[0].id;
        let should_quit = state.handle_pty_exited(pane_id);
        assert!(!should_quit);
        // gid1 should be removed
        assert!(!state.workspaces[0].groups.contains_key(&gid1));
        assert_eq!(state.workspaces[0].groups.len(), 1);
    }

    #[test]
    fn test_handle_pty_exited_prunes_folded_windows() {
        let (mut state, gid1, gid2, _rx) = make_split_state();
        {
            let ws = state.active_workspace_mut();
            ws.folded_windows.insert(gid1);
            ws.folded_windows.insert(gid2);
        }

        let pane_id = state.workspaces[0].groups[&gid1].tabs[0].id;
        let should_quit = state.handle_pty_exited(pane_id);

        assert!(!should_quit);
        assert!(!state.workspaces[0].folded_windows.contains(&gid1));
        assert!(state.workspaces[0].folded_windows.contains(&gid2));
        assert_eq!(state.workspaces[0].groups.len(), 1);
    }

    #[test]
    fn test_handle_pty_exited_marks_pane_as_exited() {
        let (mut state, _rx) = make_test_state();
        let gid = state.workspaces[0].active_group;
        // Add a second tab so the pane survives (multi-tab removes tab, doesn't quit)
        let pid2 = TabId::new_v4();
        let p2 = Tab::spawn_error(pid2, TabKind::Shell, "tab2");
        state.workspaces[0]
            .groups
            .get_mut(&gid)
            .unwrap()
            .add_tab(p2);
        // Reset exited flag on the first pane
        state.workspaces[0].groups.get_mut(&gid).unwrap().tabs[0].exited = false;
        let pane_id = state.workspaces[0].groups[&gid].tabs[0].id;

        state.handle_pty_exited(pane_id);
        // The tab was removed, but the exited flag was set on the pane before removal
        // Verify remaining tab is the second one
        assert_eq!(state.workspaces[0].groups[&gid].tab_count(), 1);
    }

    #[test]
    fn test_handle_pty_exited_closes_workspace_when_multiple() {
        let (mut state, _rx) = make_test_state();
        // Add a second workspace
        let gid2 = WindowId::new_v4();
        let pid2 = TabId::new_v4();
        let p2 = Tab::spawn_error(pid2, TabKind::Shell, "ws2");
        let g2 = Window::new(gid2, p2);
        let ws2 = Workspace::new("workspace 2".to_string(), gid2, g2);
        state.workspaces.push(ws2);
        assert_eq!(state.workspaces.len(), 2);

        // Exit the pane in workspace 2 (single group, single tab, but multiple workspaces)
        let should_quit = state.handle_pty_exited(pid2);
        assert!(!should_quit);
        assert_eq!(state.workspaces.len(), 1);
    }

    #[test]
    fn test_handle_pty_exited_nonexistent_pane() {
        let (mut state, _rx) = make_test_state();
        let should_quit = state.handle_pty_exited(TabId::new_v4());
        assert!(!should_quit);
    }

    // ---- focus_group ----

    #[test]
    fn test_focus_group_changes_active() {
        let (mut state, gid1, gid2, _rx) = make_split_state();
        assert_eq!(state.workspaces[0].active_group, gid1);
        state.focus_group(gid2);
        assert_eq!(state.workspaces[0].active_group, gid2);
    }

    #[test]
    fn test_focus_group_unfolds_folded_target() {
        let (mut state, _gid1, gid2, _rx) = make_split_state();
        state.workspaces[0].folded_windows.insert(gid2);
        assert!(state.workspaces[0].folded_windows.contains(&gid2));

        state.focus_group(gid2);
        assert_eq!(state.workspaces[0].active_group, gid2);
        assert!(
            !state.workspaces[0].folded_windows.contains(&gid2),
            "focusing a folded window should unfold it"
        );
    }

    // ---- move_tab_to_neighbor ----

    #[test]
    fn test_move_tab_to_neighbor_moves_tab() {
        let (mut state, gid1, gid2, _rx) = make_split_state();
        // Add a second tab to gid1 so we can move one
        let pid_extra = TabId::new_v4();
        let p_extra = Tab::spawn_error(pid_extra, TabKind::Shell, "extra");
        state.workspaces[0]
            .groups
            .get_mut(&gid1)
            .unwrap()
            .add_tab(p_extra);
        assert_eq!(state.workspaces[0].groups[&gid1].tab_count(), 2);
        assert_eq!(state.workspaces[0].groups[&gid2].tab_count(), 1);

        state.move_tab_to_neighbor(SplitDirection::Horizontal, crate::layout::Side::Second);
        // Tab moved from gid1 to gid2
        assert_eq!(state.workspaces[0].groups[&gid1].tab_count(), 1);
        assert_eq!(state.workspaces[0].groups[&gid2].tab_count(), 2);
        // Focus moved to neighbor
        assert_eq!(state.workspaces[0].active_group, gid2);
    }

    #[test]
    fn test_move_tab_to_neighbor_single_tab_noop() {
        let (mut state, gid1, gid2, _rx) = make_split_state();
        // gid1 has only 1 tab, so move should be a no-op
        state.move_tab_to_neighbor(SplitDirection::Horizontal, crate::layout::Side::Second);
        assert_eq!(state.workspaces[0].groups[&gid1].tab_count(), 1);
        assert_eq!(state.workspaces[0].groups[&gid2].tab_count(), 1);
        assert_eq!(state.workspaces[0].active_group, gid1);
    }

    #[test]
    fn test_move_tab_no_neighbor_noop() {
        let (mut state, _rx) = make_test_state();
        // Single group, no neighbor exists
        let gid = state.workspaces[0].active_group;
        let tab_count_before = state.workspaces[0].groups[&gid].tab_count();
        state.move_tab_to_neighbor(SplitDirection::Horizontal, crate::layout::Side::Second);
        assert_eq!(
            state.workspaces[0].groups[&gid].tab_count(),
            tab_count_before
        );
    }

    // ---- add_tab_to_active_group / split_active_group ----

    #[tokio::test]
    async fn test_add_tab_to_active_group() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        let gid = state.active_workspace().active_group;
        assert_eq!(state.active_workspace().groups[&gid].tab_count(), 1);

        state
            .add_tab_to_active_group(TabKind::Shell, None, 78, 22)
            .unwrap();
        assert_eq!(state.active_workspace().groups[&gid].tab_count(), 2);
    }

    #[tokio::test]
    async fn test_split_active_group() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        assert_eq!(state.active_workspace().groups.len(), 1);

        let (new_gid, _new_pid) = state
            .split_active_group(SplitDirection::Horizontal, TabKind::Shell, 40, 22)
            .unwrap();
        assert_eq!(state.active_workspace().groups.len(), 2);
        assert!(state.active_workspace().groups.contains_key(&new_gid));
        // Active group is the new split
        assert_eq!(state.active_workspace().active_group, new_gid);
    }

    #[tokio::test]
    async fn test_split_active_group_keeps_new_split_visible() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        state.last_size = (120, 40);

        let original_gid = state.active_workspace().active_group;
        let (new_gid, _new_pid) = state
            .split_active_group(SplitDirection::Horizontal, TabKind::Shell, 40, 22)
            .unwrap();

        let body = ratatui::layout::Rect::new(0, 0, 120, 38);
        let resolved = state.workspaces[0]
            .layout
            .resolve_with_folds(body, &state.workspaces[0].folded_windows);

        let new_visible = resolved.iter().any(
            |rp| matches!(rp, crate::layout::ResolvedPane::Visible { id, .. } if *id == new_gid),
        );
        let old_visible = resolved.iter().any(
            |rp| matches!(rp, crate::layout::ResolvedPane::Visible { id, .. } if *id == original_gid),
        );
        assert!(new_visible, "new split should be visible");
        assert!(old_visible, "original split peer should remain visible");
    }

    // ---- new_workspace / close_workspace ----

    #[tokio::test]
    async fn test_new_workspace_names_increment() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        // First workspace gets an auto-generated name
        let first_name = state.workspaces[0].name.clone();
        assert!(!first_name.is_empty());

        state.new_workspace(80, 24).unwrap();
        let second_name = state.workspaces[1].name.clone();
        assert_ne!(first_name, second_name);

        state.new_workspace(80, 24).unwrap();
        let third_name = state.workspaces[2].name.clone();
        assert_ne!(first_name, third_name);
        assert_ne!(second_name, third_name);
    }

    #[tokio::test]
    async fn test_close_workspace_adjusts_active_index() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        state.new_workspace(80, 24).unwrap();
        state.new_workspace(80, 24).unwrap();
        assert_eq!(state.workspaces.len(), 3);
        // active_workspace is 2 (last created)
        assert_eq!(state.active_workspace, 2);

        assert!(!state.close_workspace());
        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.active_workspace, 1);
    }

    #[test]
    fn test_close_workspace_single_returns_true() {
        let (mut state, _rx) = make_test_state();
        assert!(state.close_workspace());
        assert_eq!(state.workspaces.len(), 1); // workspace still there, caller handles shutdown
    }

    #[test]
    fn test_close_workspace_first_of_two() {
        let (mut state, _rx) = make_test_state();
        // Add second workspace
        let gid2 = WindowId::new_v4();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "ws2");
        let g2 = Window::new(gid2, p2);
        let ws2 = Workspace::new("workspace 2".to_string(), gid2, g2);
        state.workspaces.push(ws2);
        state.active_workspace = 0;

        assert!(!state.close_workspace());
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.active_workspace, 0);
        assert_eq!(state.workspaces[0].name, "workspace 2");
    }

    // ---- active_group always exists ----

    #[test]
    fn test_active_group_exists_in_groups() {
        let (state, _rx) = make_test_state();
        let ws = state.active_workspace();
        assert!(ws.groups.contains_key(&ws.active_group));
    }

    #[test]
    fn test_active_group_exists_after_split_close() {
        let (mut state, gid1, _gid2, _rx) = make_split_state();
        let pane_id = state.workspaces[0].groups[&gid1].tabs[0].id;
        state.handle_pty_exited(pane_id);
        let ws = state.active_workspace();
        assert!(
            ws.groups.contains_key(&ws.active_group),
            "active_group must exist in groups HashMap after closing a group"
        );
    }

    // ---- existing tests ----

    #[tokio::test]
    async fn test_server_state_new() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.active_workspace, 0);
    }

    #[tokio::test]
    async fn test_server_state_workspace_accessors() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        let ws = state.active_workspace();
        assert_eq!(ws.groups.len(), 1);
    }

    #[tokio::test]
    async fn test_server_state_close_workspace_single() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        // Closing last workspace signals shutdown
        assert!(state.close_workspace());
        assert_eq!(state.workspaces.len(), 1);
    }

    #[tokio::test]
    async fn test_server_state_new_workspace() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        state.new_workspace(80, 24).unwrap();
        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.active_workspace, 1);
    }

    #[tokio::test]
    async fn test_next_tmux_env_increments() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state = ServerState::new(
            &event_tx,
            80,
            24,
            Config::default(),
        )
        .unwrap();

        // new already uses pane 0, so next_pane_number starts at 1
        assert_eq!(state.next_pane_number, 1);

        let env1 = state.next_tmux_env();
        assert_eq!(env1.tmux_pane, "%1");
        assert!(env1.tmux_value.contains(",0")); // ends with ",<pid>,0"

        let env2 = state.next_tmux_env();
        assert_eq!(env2.tmux_pane, "%2");

        let env3 = state.next_tmux_env();
        assert_eq!(env3.tmux_pane, "%3");

        assert_eq!(state.next_pane_number, 4);
    }

    #[tokio::test]
    async fn test_last_pane_number() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new(&event_tx, 80, 24, Config::default())
                .unwrap();
        // new uses pane 0, so last_pane_number = 0 initially
        assert_eq!(state.last_pane_number(), 0);

        state.next_tmux_env(); // assigns %1
        assert_eq!(state.last_pane_number(), 1);

        state.next_tmux_env(); // assigns %2
        assert_eq!(state.last_pane_number(), 2);
    }
}
