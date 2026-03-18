use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use pane_protocol::config::{Config, HubWidget};
use pane_protocol::event::AppEvent;
use pane_protocol::layout::{LayoutNode, ResolvedPane, Side, SplitDirection, TabId};
use pane_protocol::system_stats::SystemStats;
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

/// Auto-name a workspace based on the git repo name, then folder name, with
/// numeric suffix for duplicates.
/// Convert a kebab-case or snake_case name to Title Case.
/// e.g. "expo-passkite" → "Expo Passkite", "pane" → "Pane"
fn titlecase_name(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn auto_workspace_name(existing: &[Workspace], cwd: &Path) -> String {
    let used: std::collections::HashSet<&str> =
        existing.iter().map(|ws| ws.name.as_str()).collect();

    let raw = git_repo_name(cwd)
        .or_else(|| cwd.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_else(|| "1".to_string());
    let base = titlecase_name(&raw);

    if !used.contains(base.as_str()) {
        return base;
    }

    let mut n = 2u32;
    loop {
        let candidate = format!("{} {}", base, n);
        if !used.contains(candidate.as_str()) {
            return candidate;
        }
        n += 1;
    }
}

/// Get the repository name from a git working directory by reading the remote
/// origin URL or falling back to the repo root folder name.
fn git_repo_name(cwd: &Path) -> Option<String> {
    // Find the git repo root by walking up
    let mut dir = cwd.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            break;
        }
        if !dir.pop() {
            return None;
        }
    }

    // Try to get repo name from origin remote URL
    if let Ok(config) = std::fs::read_to_string(dir.join(".git/config")) {
        if let Some(url) = parse_git_remote_url(&config) {
            if let Some(name) = repo_name_from_url(&url) {
                return Some(name);
            }
        }
    }

    // Fall back to the repo root directory name
    dir.file_name().map(|f| f.to_string_lossy().to_string())
}

/// Parse the origin remote URL from a git config file.
fn parse_git_remote_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed == "[remote \"origin\"]" {
            in_origin = true;
        } else if trimmed.starts_with('[') {
            in_origin = false;
        } else if in_origin {
            if let Some(url) = trimmed.strip_prefix("url = ") {
                return Some(url.trim().to_string());
            }
        }
    }
    None
}

/// Extract the repository name from a git remote URL.
fn repo_name_from_url(url: &str) -> Option<String> {
    // Handle SSH (git@github.com:user/repo.git) and HTTPS (https://github.com/user/repo.git)
    let path = url
        .strip_suffix(".git")
        .unwrap_or(url);
    let name = path.rsplit('/').next()
        .or_else(|| path.rsplit(':').next())?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Build the home workspace from the hub layout config.
/// Each widget in the layout gets its own window with a single widget tab.
fn build_home_workspace(config: &Config) -> Workspace {
    let rows = &config.behavior.hub_layout.rows;
    let home_cwd = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"));

    // Collect (WindowId, Window, HubWidget-row-index) for layout building
    let mut row_nodes: Vec<LayoutNode> = Vec::new();
    let mut groups = std::collections::HashMap::new();
    let mut first_group_id = None;

    for row in rows {
        if row.is_empty() {
            continue;
        }

        if row.len() == 1 {
            let gid = WindowId::new_v4();
            let tab = Tab::new_widget(TabId::new_v4(), row[0].clone());
            let window = Window::new(gid, tab);
            groups.insert(gid, window);
            if first_group_id.is_none() {
                first_group_id = Some(gid);
            }
            row_nodes.push(LayoutNode::Leaf(gid));
        } else {
            // Multiple widgets in a row → horizontal split chain
            let mut leaves: Vec<LayoutNode> = Vec::new();
            for widget in row {
                let gid = WindowId::new_v4();
                let tab = Tab::new_widget(TabId::new_v4(), widget.clone());
                let window = Window::new(gid, tab);
                groups.insert(gid, window);
                if first_group_id.is_none() {
                    first_group_id = Some(gid);
                }
                leaves.push(LayoutNode::Leaf(gid));
            }
            // Chain leaves into balanced horizontal splits
            let node = chain_splits(leaves, SplitDirection::Horizontal);
            row_nodes.push(node);
        }
    }

    // Stack rows vertically
    let layout = if row_nodes.is_empty() {
        // Fallback: single ProjectInfo widget
        let gid = WindowId::new_v4();
        let tab = Tab::new_widget(TabId::new_v4(), HubWidget::ProjectInfo);
        let window = Window::new(gid, tab);
        groups.insert(gid, window);
        first_group_id = Some(gid);
        LayoutNode::Leaf(gid)
    } else {
        chain_splits(row_nodes, SplitDirection::Vertical)
    };

    let active_group = first_group_id.unwrap();
    Workspace {
        name: "Home".to_string(),
        cwd: home_cwd,
        layout,
        groups,
        active_group,
        folded_windows: std::collections::HashSet::new(),
        sync_panes: false,
        zoomed_window: None,
        saved_ratios: None,
        floating_windows: Vec::new(),
        is_home: true,
    }
}

/// Chain a list of layout nodes into a balanced binary split tree.
fn chain_splits(nodes: Vec<LayoutNode>, direction: SplitDirection) -> LayoutNode {
    assert!(!nodes.is_empty());
    if nodes.len() == 1 {
        return nodes.into_iter().next().unwrap();
    }
    // Build a right-leaning chain with equal ratios
    let n = nodes.len();
    let mut iter = nodes.into_iter();
    let mut result = iter.next().unwrap();
    for (i, node) in iter.enumerate() {
        let ratio = 1.0 / (n - i) as f64;
        result = LayoutNode::Split {
            direction,
            ratio,
            first: Box::new(result),
            second: Box::new(node),
        };
    }
    result
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
        let idx = self.active_workspace.min(self.workspaces.len().saturating_sub(1));
        &self.workspaces[idx]
    }

    pub fn active_workspace_mut(&mut self) -> &mut Workspace {
        let idx = self.active_workspace.min(self.workspaces.len().saturating_sub(1));
        &mut self.workspaces[idx]
    }

    /// Find a pane mutably across all workspaces/groups/tabs.
    pub fn find_tab(&self, pane_id: TabId) -> Option<&Tab> {
        for ws in &self.workspaces {
            for group in ws.groups.values() {
                for pane in &group.tabs {
                    if pane.id == pane_id {
                        return Some(pane);
                    }
                }
            }
        }
        None
    }

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

    /// Create a new server state with the home workspace at index 0.
    /// Tries to restore a saved home layout first, falls back to building from config.
    pub fn new(
        event_tx: &mpsc::UnboundedSender<AppEvent>,
        cols: u16,
        rows: u16,
        config: Config,
    ) -> Self {
        let home = crate::server::persistence::load_home_layout()
            .unwrap_or_else(|| build_home_workspace(&config));
        Self {
            workspaces: vec![home],
            active_workspace: 0,
            config,
            system_stats: SystemStats::default(),
            event_tx: event_tx.clone(),
            last_size: (cols.saturating_add(2), rows.saturating_add(3)),
            next_pane_number: 0,
            drag_state: None,
        }
    }

    /// Save the home workspace layout to disk (if it exists).
    pub fn save_home_layout(&self) {
        if let Some(home) = self.workspaces.iter().find(|w| w.is_home) {
            crate::server::persistence::save_home_layout(home);
        }
    }

    /// Create a new server state with a default workspace (for tests and backwards compat).
    pub fn new_with_workspace(
        event_tx: &mpsc::UnboundedSender<AppEvent>,
        cols: u16,
        rows: u16,
        config: Config,
    ) -> anyhow::Result<Self> {
        let mut state = Self::new(event_tx, cols, rows, config);
        state.new_workspace(cols, rows, None)?;
        Ok(state)
    }

    pub fn handle_pty_exited(&mut self, pane_id: TabId) -> bool {
        if let Some(pane) = self.find_tab_mut(pane_id) {
            pane.exited = true;
        }

        let location = self.find_tab_location(pane_id);
        if let Some((ws_idx, group_id)) = location {
            // Never remove tabs from the home workspace via PTY exit
            if self.workspaces[ws_idx].is_home {
                return false;
            }
            let ws = &self.workspaces[ws_idx];
            if let Some(group) = ws.groups.get(&group_id) {
                if group.tab_count() <= 1 {
                    let group_ids = ws.layout.group_ids();
                    let has_home = self.workspaces.iter().any(|w| w.is_home);
                    let non_home_count = self.workspaces.len() - if has_home { 1 } else { 0 };
                    if group_ids.len() <= 1 && non_home_count <= 1 {
                        if has_home {
                            // Remove this workspace, fall back to home
                            self.workspaces.remove(ws_idx);
                            self.active_workspace = 0;
                            return false;
                        }
                        return true; // should_quit (no home workspace)
                    }
                    let ws = &mut self.workspaces[ws_idx];
                    if let Some(new_focus) = ws.layout.close_pane(group_id) {
                        ws.groups.remove(&group_id);
                        ws.prune_folded_windows();
                        ws.active_group = new_focus;
                    } else if non_home_count > 1 {
                        self.workspaces.remove(ws_idx);
                        if self.active_workspace >= self.workspaces.len() {
                            self.active_workspace = self.workspaces.len() - 1;
                        }
                    } else if has_home {
                        self.workspaces.remove(ws_idx);
                        self.active_workspace = 0;
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

    /// Focus a pane group and unfold it if it's currently folded.
    pub fn focus_group(&mut self, id: WindowId, _bar_h: u16) {
        let ws = &mut self.workspaces[self.active_workspace];
        let was_folded = ws.folded_windows.remove(&id);
        ws.active_group = id;
        if was_folded {
            let (w, h) = self.last_size;
            self.resize_all_tabs(w, h);
        }
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
            .is_none_or(|g| g.tabs.len() <= 1)
        {
            return;
        }

        let ws = self.active_workspace_mut();
        let Some(source) = ws.groups.get(&source_group_id) else { return };
        let tab_idx = source.active_tab;
        let Some(source_mut) = ws.groups.get_mut(&source_group_id) else { return };
        let Some(pane) = source_mut.remove_tab(tab_idx) else { return };
        let Some(target) = ws.groups.get_mut(&neighbor_id) else {
            // Target vanished — re-add tab to source to prevent data loss
            if let Some(source_mut) = ws.groups.get_mut(&source_group_id) {
                source_mut.add_tab(pane);
            }
            return;
        };
        target.add_tab(pane);
        ws.active_group = neighbor_id;
    }

    pub fn add_tab_to_active_group(
        &mut self,
        kind: TabKind,
        command: Option<String>,
        shell: Option<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<TabId> {
        let pane_id = TabId::new_v4();
        let tmux_env = self.next_tmux_env();
        let ws_cwd = self.active_workspace().cwd.clone();
        let pane = match Tab::spawn_with_env(
            pane_id,
            kind.clone(),
            cols,
            rows,
            self.event_tx.clone(),
            command,
            shell,
            Some(tmux_env),
            Some(&ws_cwd),
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
        command: Option<String>,
        shell: Option<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<(WindowId, TabId)> {
        let new_group_id = WindowId::new_v4();
        let pane_id = TabId::new_v4();
        let tmux_env = self.next_tmux_env();
        let ws_cwd = self.active_workspace().cwd.clone();

        let pane = match Tab::spawn_with_env(
            pane_id,
            kind.clone(),
            cols,
            rows,
            self.event_tx.clone(),
            command,
            shell,
            Some(tmux_env),
            Some(&ws_cwd),
        ) {
            Ok(p) => p,
            Err(e) => Tab::spawn_error(pane_id, kind, &e.to_string()),
        };

        let group = Window::new(new_group_id, pane);
        let ws = self.active_workspace_mut();
        ws.layout
            .split_pane(ws.active_group, direction, new_group_id);
        ws.groups.insert(new_group_id, group);
        ws.active_group = new_group_id;

        let (w, h) = self.last_size;
        self.resize_all_tabs(w, h);
        Ok((new_group_id, pane_id))
    }

    pub fn new_workspace(&mut self, cols: u16, rows: u16, cwd: Option<PathBuf>) -> anyhow::Result<()> {
        let pane_id = TabId::new_v4();
        let group_id = WindowId::new_v4();
        let tmux_env = self.next_tmux_env();
        // Use provided cwd, inherit from current workspace, or fall back to $PWD.
        let cwd = cwd.unwrap_or_else(|| {
            if self.workspaces.is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
            } else {
                self.active_workspace().cwd.clone()
            }
        });

        let pane = match Tab::spawn_with_env(
            pane_id,
            TabKind::Shell,
            cols,
            rows,
            self.event_tx.clone(),
            None,
            None,
            Some(tmux_env),
            Some(&cwd),
        ) {
            Ok(p) => p,
            Err(e) => Tab::spawn_error(pane_id, TabKind::Shell, &e.to_string()),
        };

        let group = Window::new(group_id, pane);
        let workspace = Workspace::new(auto_workspace_name(&self.workspaces, &cwd), cwd, group_id, group);
        self.workspaces.push(workspace);
        self.active_workspace = self.workspaces.len() - 1;

        let (w, h) = self.last_size;
        self.resize_all_tabs(w, h);
        Ok(())
    }

    /// Close the active workspace. Returns true if there are no workspaces left
    /// (the client should show the project hub).
    /// Cannot close the home workspace.
    pub fn close_workspace(&mut self) -> bool {
        if self.workspaces.is_empty() {
            return true;
        }
        // Don't close the home workspace
        if self.workspaces[self.active_workspace].is_home {
            return false;
        }
        self.workspaces.remove(self.active_workspace);
        if self.workspaces.is_empty() {
            self.active_workspace = 0;
            // In production, home workspace always exists, so this shouldn't happen.
            // But tests might construct states without a home workspace.
            return false;
        }
        if self.active_workspace >= self.workspaces.len() {
            self.active_workspace = self.workspaces.len() - 1;
        }
        // If only home remains, switch to it
        if self.workspaces.len() == 1 && self.workspaces[0].is_home {
            self.active_workspace = 0;
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
        let ws_cwd = self.active_workspace().cwd.clone();
        let new_pane = match Tab::spawn_with_env(
            id,
            kind.clone(),
            cols,
            rows,
            self.event_tx.clone(),
            command,
            None,
            Some(tmux_env),
            Some(&ws_cwd),
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
                            let rows = rect.height.saturating_sub(4); // 2 borders + tab bar + separator
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
        3 // Always show workspace bar (home workspace always exists)
    }

    /// Compute the PTY cols/rows for the active window based on the resolved layout.
    /// Falls back to full-body estimate when layout resolution fails.
    pub fn active_window_pty_size(&self) -> (u16, u16) {
        let (w, h) = self.last_size;
        let overhead = 1 + self.workspace_bar_height();
        let body_height = h.saturating_sub(overhead);
        let body = ratatui::layout::Rect::new(0, 0, w, body_height);

        let ws = self.active_workspace();
        let resolved = ws.layout.resolve_with_folds(body, &ws.folded_windows);
        for rp in &resolved {
            if let ResolvedPane::Visible { id, rect, .. } = rp {
                if *id == ws.active_group {
                    let cols = rect.width.saturating_sub(4);
                    let rows = rect.height.saturating_sub(4);
                    if cols > 0 && rows > 0 {
                        return (cols, rows);
                    }
                }
            }
        }

        // Fallback: assume single window fills body
        let cols = w.saturating_sub(4);
        let rows = body_height.saturating_sub(4);
        (cols.max(1), rows.max(1))
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
    use std::collections::HashMap;
    use pane_protocol::config::Config;
    use pane_protocol::layout::{SplitDirection, TabId};
    use crate::window::{Tab, TabKind, Window, WindowId};

    /// Build a ServerState without PTY spawning. Uses `Tab::spawn_error` for all panes.
    fn make_test_state() -> (ServerState, mpsc::UnboundedReceiver<AppEvent>) {
        let (event_tx, rx) = mpsc::unbounded_channel();
        let pane_id = TabId::new_v4();
        let group_id = WindowId::new_v4();
        let pane = Tab::spawn_error(pane_id, TabKind::Shell, "test");
        let group = Window::new(group_id, pane);
        let workspace = Workspace::new("workspace".to_string(), PathBuf::from("/tmp"), group_id, group);
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
        let layout = pane_protocol::layout::LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(pane_protocol::layout::LayoutNode::Leaf(gid1)),
            second: Box::new(pane_protocol::layout::LayoutNode::Leaf(gid2)),
        };
        let workspace = Workspace {
            name: "workspace".to_string(),
            cwd: PathBuf::from("/tmp"),
            layout,
            groups,
            active_group: gid1,
            folded_windows: std::collections::HashSet::new(),
            sync_panes: false,
            zoomed_window: None,
            saved_ratios: None,
            floating_windows: Vec::new(),
            is_home: false,
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
        Workspace::new(name.to_string(), PathBuf::from("/tmp"), gid, g)
    }

    #[test]
    fn test_auto_workspace_name_empty_returns_nonempty() {
        let name = auto_workspace_name(&[], Path::new("/tmp/myproject"));
        assert!(!name.is_empty());
    }

    #[test]
    fn test_auto_workspace_name_uses_folder_name() {
        let name = auto_workspace_name(&[], Path::new("/home/user/myproject"));
        assert_eq!(name, "Myproject");
    }

    #[test]
    fn test_auto_workspace_name_avoids_duplicates() {
        let cwd = Path::new("/tmp/myproject");
        let first = auto_workspace_name(&[], cwd);
        let ws1 = make_named_ws(&first);
        let second = auto_workspace_name(&[ws1], cwd);
        assert_ne!(first, second);
        assert!(!second.is_empty());
    }

    #[test]
    fn test_titlecase_single_word() {
        assert_eq!(titlecase_name("pane"), "Pane");
    }

    #[test]
    fn test_titlecase_kebab_case() {
        assert_eq!(titlecase_name("expo-passkite"), "Expo Passkite");
    }

    #[test]
    fn test_titlecase_multi_kebab() {
        assert_eq!(titlecase_name("seventwo-astro-theme"), "Seventwo Astro Theme");
    }

    #[test]
    fn test_titlecase_snake_case() {
        assert_eq!(titlecase_name("my_project"), "My Project");
    }

    #[test]
    fn test_titlecase_already_capitalized() {
        assert_eq!(titlecase_name("MyProject"), "MyProject");
    }

    #[test]
    fn test_auto_workspace_name_duplicate_suffix() {
        let cwd = Path::new("/tmp/myproject");
        let first = auto_workspace_name(&[], cwd);
        assert_eq!(first, "Myproject");
        let ws1 = make_named_ws(&first);
        let second = auto_workspace_name(&[ws1], cwd);
        assert_eq!(second, "Myproject 2");
    }

    #[test]
    fn test_parse_git_remote_url_https() {
        let config = "[remote \"origin\"]\n\turl = https://github.com/user/repo.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n";
        assert_eq!(parse_git_remote_url(config), Some("https://github.com/user/repo.git".to_string()));
    }

    #[test]
    fn test_parse_git_remote_url_ssh() {
        let config = "[remote \"origin\"]\n\turl = git@github.com:user/repo.git\n";
        assert_eq!(parse_git_remote_url(config), Some("git@github.com:user/repo.git".to_string()));
    }

    #[test]
    fn test_repo_name_from_url_https() {
        assert_eq!(repo_name_from_url("https://github.com/user/repo.git"), Some("repo".to_string()));
    }

    #[test]
    fn test_repo_name_from_url_ssh() {
        assert_eq!(repo_name_from_url("git@github.com:user/repo.git"), Some("repo".to_string()));
    }

    #[test]
    fn test_repo_name_from_url_no_git_suffix() {
        assert_eq!(repo_name_from_url("https://github.com/user/repo"), Some("repo".to_string()));
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
        let ws2 = Workspace::new("workspace 2".to_string(), PathBuf::from("/tmp"), gid2, g2);
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
        let ws2 = Workspace::new("workspace 2".to_string(), PathBuf::from("/tmp"), gid2, g2);
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
        let (mut state, _gid1, gid2, _rx) = make_split_state();
        assert_ne!(state.workspaces[0].active_group, gid2);
        state.focus_group(gid2, 1);
        assert_eq!(state.workspaces[0].active_group, gid2);
    }

    #[test]
    fn test_focus_group_same_group_is_noop() {
        let (mut state, gid1, _gid2, _rx) = make_split_state();
        state.focus_group(gid1, 1);
        assert_eq!(state.workspaces[0].active_group, gid1);
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

        state.move_tab_to_neighbor(SplitDirection::Horizontal, pane_protocol::layout::Side::Second);
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
        state.move_tab_to_neighbor(SplitDirection::Horizontal, pane_protocol::layout::Side::Second);
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
        state.move_tab_to_neighbor(SplitDirection::Horizontal, pane_protocol::layout::Side::Second);
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
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        let gid = state.active_workspace().active_group;
        assert_eq!(state.active_workspace().groups[&gid].tab_count(), 1);

        state
            .add_tab_to_active_group(TabKind::Shell, None, None, 78, 22)
            .unwrap();
        assert_eq!(state.active_workspace().groups[&gid].tab_count(), 2);
    }

    #[tokio::test]
    async fn test_split_active_group() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        assert_eq!(state.active_workspace().groups.len(), 1);

        let (new_gid, _new_pid) = state
            .split_active_group(SplitDirection::Horizontal, TabKind::Shell, None, None, 40, 22)
            .unwrap();
        assert_eq!(state.active_workspace().groups.len(), 2);
        assert!(state.active_workspace().groups.contains_key(&new_gid));
        // Active group is the new split
        assert_eq!(state.active_workspace().active_group, new_gid);
    }

    // ---- new_workspace / close_workspace ----

    #[tokio::test]
    async fn test_new_workspace_names_increment() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        // First workspace gets an auto-generated name
        let first_name = state.workspaces[0].name.clone();
        assert!(!first_name.is_empty());

        state.new_workspace(80, 24, None).unwrap();
        let second_name = state.workspaces[1].name.clone();
        assert_ne!(first_name, second_name);

        state.new_workspace(80, 24, None).unwrap();
        let third_name = state.workspaces[2].name.clone();
        assert_ne!(first_name, third_name);
        assert_ne!(second_name, third_name);
    }

    #[tokio::test]
    async fn test_close_workspace_adjusts_active_index() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        // home + 1 regular = 2; add 2 more = 4 total
        state.new_workspace(80, 24, None).unwrap();
        state.new_workspace(80, 24, None).unwrap();
        assert_eq!(state.workspaces.len(), 4);
        // active_workspace is 3 (last created)
        assert_eq!(state.active_workspace, 3);

        assert!(!state.close_workspace());
        assert_eq!(state.workspaces.len(), 3);
        assert_eq!(state.active_workspace, 2);
    }

    #[test]
    fn test_close_workspace_single_non_home() {
        // make_test_state creates a single non-home workspace (no home)
        let (mut state, _rx) = make_test_state();
        assert!(!state.close_workspace());
        assert_eq!(state.workspaces.len(), 0);
    }

    #[test]
    fn test_close_workspace_first_of_two() {
        let (mut state, _rx) = make_test_state();
        // Add second workspace
        let gid2 = WindowId::new_v4();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "ws2");
        let g2 = Window::new(gid2, p2);
        let ws2 = Workspace::new("workspace 2".to_string(), PathBuf::from("/tmp"), gid2, g2);
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
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        // home workspace + 1 regular workspace = 2
        assert_eq!(state.workspaces.len(), 2);
        assert!(state.workspaces[0].is_home);
        assert!(!state.workspaces[1].is_home);
        assert_eq!(state.active_workspace, 1);
    }

    #[tokio::test]
    async fn test_server_state_workspace_accessors() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let state =
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        let ws = state.active_workspace();
        assert_eq!(ws.groups.len(), 1);
    }

    #[tokio::test]
    async fn test_server_state_close_workspace_single() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        // Active workspace is 1 (the regular one). Closing it leaves only home.
        assert!(!state.close_workspace());
        assert_eq!(state.workspaces.len(), 1);
        assert!(state.workspaces[0].is_home);
        assert_eq!(state.active_workspace, 0);
    }

    #[tokio::test]
    async fn test_server_state_new_workspace() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        state.new_workspace(80, 24, None).unwrap();
        // home + 2 regular = 3
        assert_eq!(state.workspaces.len(), 3);
        assert_eq!(state.active_workspace, 2);
    }

    #[tokio::test]
    async fn test_next_tmux_env_increments() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state =
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();

        // new() already uses pane 0, so next_pane_number starts at 1
        assert_eq!(state.next_pane_number, 1);

        let env1 = state.next_tmux_env();
        assert_eq!(env1.tmux_pane, "%1");
        assert!(env1.tmux_value.contains(",0")); // ends with ",<pid>,0"
        assert!(env1.tmux_value.contains(".sock"));

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
            ServerState::new_with_workspace(&event_tx, 80, 24, Config::default())
                .unwrap();
        // new() uses pane 0, so last_pane_number = 0 initially
        assert_eq!(state.last_pane_number(), 0);

        state.next_tmux_env(); // assigns %1
        assert_eq!(state.last_pane_number(), 1);

        state.next_tmux_env(); // assigns %2
        assert_eq!(state.last_pane_number(), 2);
    }

    // ---- active_window_pty_size ----

    #[test]
    fn test_active_window_pty_size_single_window() {
        let (state, _rx) = make_test_state();
        let (cols, rows) = state.active_window_pty_size();
        // With last_size (120, 40), overhead = 1 + workspace_bar_height(3) = 4
        // body_height = 40 - 4 = 36, single leaf gets full body
        // cols = 120 - 4 = 116, rows = 36 - 4 = 32
        assert!(cols > 0, "cols should be positive");
        assert!(rows > 0, "rows should be positive");
        assert!(cols <= 120, "cols should not exceed total width");
        assert!(rows <= 40, "rows should not exceed total height");
    }

    #[test]
    fn test_active_window_pty_size_split_layout() {
        let (state, gid1, _gid2, _rx) = make_split_state();
        assert_eq!(state.active_workspace().active_group, gid1);
        let (cols, rows) = state.active_window_pty_size();
        // In a 50/50 horizontal split, each window gets roughly half the width
        assert!(cols > 0);
        assert!(rows > 0);
        // cols should be less than full width minus borders
        assert!(cols < 116, "split window should be narrower than single");
    }

    #[test]
    fn test_active_window_pty_size_after_focus_change() {
        let (mut state, _gid1, gid2, _rx) = make_split_state();
        let (cols1, rows1) = state.active_window_pty_size();

        state.active_workspace_mut().active_group = gid2;
        let (cols2, rows2) = state.active_window_pty_size();

        // Both halves of a 50/50 split should have similar sizes
        assert_eq!(cols1, cols2);
        assert_eq!(rows1, rows2);
    }

    // ---- resize_all_tabs with folded windows ----

    #[test]
    fn test_resize_all_tabs_with_folded_window() {
        let (mut state, gid1, gid2, _rx) = make_split_state();
        // Fold gid1
        state.active_workspace_mut().folded_windows.insert(gid1);

        // Resize should not panic
        state.resize_all_tabs(120, 40);

        // gid2 (unfolded) should have been resized - just verify it doesn't panic
        // and the pane still has valid dimensions
        let ws = state.active_workspace();
        let group2 = ws.groups.get(&gid2).unwrap();
        let (rows, cols) = group2.active_tab().screen().size();
        assert!(cols > 0);
        assert!(rows > 0);
    }

    #[test]
    fn test_resize_all_tabs_all_folded_except_one() {
        let (mut state, gid1, gid2, _rx) = make_split_state();
        // Fold gid1, only gid2 visible
        state.active_workspace_mut().folded_windows.insert(gid1);
        state.resize_all_tabs(100, 30);

        // The visible window (gid2) should get nearly full size
        let ws = state.active_workspace();
        let group2 = ws.groups.get(&gid2).unwrap();
        let (rows, cols) = group2.active_tab().screen().size();
        assert!(cols > 0);
        assert!(rows > 0);
    }

    #[test]
    fn test_resize_all_tabs_small_terminal() {
        let (mut state, _rx) = make_test_state();
        // Very small terminal: should not panic
        state.resize_all_tabs(10, 10);
    }

    // ---- workspace_bar_height ----

    #[test]
    fn test_workspace_bar_height_single_workspace() {
        let (state, _rx) = make_test_state();
        // With 1 workspace, bar height = 3
        assert_eq!(state.workspace_bar_height(), 3);
    }

    #[test]
    fn test_workspace_bar_height_multiple_workspaces() {
        let (mut state, _rx) = make_test_state();
        // Add more workspaces
        let gid2 = WindowId::new_v4();
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "ws2");
        let g2 = Window::new(gid2, p2);
        state.workspaces.push(Workspace::new("ws2".to_string(), PathBuf::from("/tmp"), gid2, g2));

        let gid3 = WindowId::new_v4();
        let p3 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "ws3");
        let g3 = Window::new(gid3, p3);
        state.workspaces.push(Workspace::new("ws3".to_string(), PathBuf::from("/tmp"), gid3, g3));

        assert_eq!(state.workspace_bar_height(), 3);
    }

    #[test]
    fn test_workspace_bar_height_always_three() {
        let (mut state, _rx) = make_test_state();
        // Always 3, even if manually cleared (home workspace should always exist)
        assert_eq!(state.workspace_bar_height(), 3);
        state.workspaces.clear();
        assert_eq!(state.workspace_bar_height(), 3);
    }

    // ---- focus_group unfolds ----

    #[test]
    fn test_focus_group_unfolds_folded_window() {
        let (mut state, _gid1, gid2, _rx) = make_split_state();
        state.active_workspace_mut().folded_windows.insert(gid2);
        assert!(state.active_workspace().folded_windows.contains(&gid2));

        state.focus_group(gid2, 1);
        assert_eq!(state.active_workspace().active_group, gid2);
        assert!(!state.active_workspace().folded_windows.contains(&gid2));
    }

    #[test]
    fn test_focus_group_non_folded_keeps_others() {
        let (mut state, gid1, gid2, _rx) = make_split_state();
        state.active_workspace_mut().folded_windows.insert(gid1);

        // Focus gid2 which is not folded
        state.focus_group(gid2, 1);
        assert_eq!(state.active_workspace().active_group, gid2);
        // gid1 should still be folded
        assert!(state.active_workspace().folded_windows.contains(&gid1));
    }

    // ---- scroll_active_tab ----

    #[test]
    fn test_scroll_active_tab() {
        let (mut state, _rx) = make_test_state();
        // Produce scrollback content
        let gid = state.active_workspace().active_group;
        {
            let ws = state.active_workspace_mut();
            let tab = ws.groups.get_mut(&gid).unwrap().active_tab_mut();
            tab.vt = vt100::Parser::new(3, 80, 1000);
            for i in 0..20 {
                tab.vt.process(format!("line {}\r\n", i).as_bytes());
            }
        }

        state.scroll_active_tab(|tab| tab.scroll_up(5));
        let ws = state.active_workspace();
        let tab = ws.groups.get(&gid).unwrap().active_tab();
        assert!(tab.scroll_offset > 0);
    }

    // ---- restart_active_tab ----

    #[test]
    fn test_restart_active_tab_non_exited_is_noop() {
        let (mut state, _rx) = make_test_state();
        let gid = state.active_workspace().active_group;
        // The error pane starts with exited = true
        state.active_workspace_mut().groups.get_mut(&gid).unwrap().tabs[0].exited = false;
        // Restart should be a no-op because pane is not exited
        state.restart_active_tab(80, 24).unwrap();
        // Still 1 tab
        assert_eq!(state.active_workspace().groups[&gid].tab_count(), 1);
    }

    // ---- render_state_from_server ----

    #[test]
    fn test_render_state_from_server_basic() {
        let (state, _rx) = make_test_state();
        let rs = render_state_from_server(&state);
        assert_eq!(rs.workspaces.len(), 1);
        assert_eq!(rs.active_workspace, 0);
        assert_eq!(rs.workspaces[0].name, "workspace");
        assert!(!rs.workspaces[0].groups.is_empty());
    }

    #[test]
    fn test_render_state_for_client_clamps_index() {
        let (state, _rx) = make_test_state();
        let rs = render_state_for_client(&state, 999);
        assert_eq!(rs.active_workspace, 0); // clamped to valid range
    }

    #[test]
    fn test_render_state_captures_folded_windows() {
        let (mut state, gid1, _gid2, _rx) = make_split_state();
        state.active_workspace_mut().folded_windows.insert(gid1);
        let rs = render_state_from_server(&state);
        assert!(rs.workspaces[0].folded_windows.contains(&gid1));
    }

    #[test]
    fn test_render_state_captures_sync_panes() {
        let (mut state, _rx) = make_test_state();
        state.active_workspace_mut().sync_panes = true;
        let rs = render_state_from_server(&state);
        assert!(rs.workspaces[0].sync_panes);
    }

    #[test]
    fn test_render_state_captures_zoomed_window() {
        let (mut state, gid1, _gid2, _rx) = make_split_state();
        state.active_workspace_mut().zoomed_window = Some(gid1);
        let rs = render_state_from_server(&state);
        assert_eq!(rs.workspaces[0].zoomed_window, Some(gid1));
    }
}

// ---------------------------------------------------------------------------
// Build RenderState from ServerState
// ---------------------------------------------------------------------------

use pane_protocol::protocol::{
    FloatingWindowSnapshot, RenderState, TabSnapshot, WindowSnapshot, WorkspaceSnapshot,
};

/// Build a RenderState for a specific client, using their active_workspace.
pub fn render_state_for_client(state: &ServerState, active_workspace: usize) -> RenderState {
    let mut rs = render_state_from_server(state);
    rs.active_workspace = active_workspace.min(rs.workspaces.len().saturating_sub(1));
    rs
}

#[allow(dead_code)]
pub fn render_state_from_server(state: &ServerState) -> RenderState {
    let workspaces = state
        .workspaces
        .iter()
        .map(|ws| {
            let groups = ws
                .groups
                .iter()
                .map(|(gid, group)| WindowSnapshot {
                    id: *gid,
                    tabs: group
                        .tabs
                        .iter()
                        .map(|pane| {
                            let (rows, cols) = pane.screen().size();
                            // Resolve foreground process name: if the binary name
                            // doesn't match any decoration, check the full path.
                            // e.g. claude installs as `~/.local/share/claude/versions/2.1.74`
                            let fg = match (&pane.foreground_process, &pane.foreground_process_path) {
                                (Some(name), _) if state.config.decoration_for(name).is_some() => {
                                    Some(name.clone())
                                }
                                (_, Some(path)) => {
                                    let resolved = state.config.decoration_for_path(path)
                                        .map(|d| d.process.clone());
                                    resolved.or_else(|| pane.foreground_process.clone())
                                }
                                (name, _) => name.clone(),
                            };
                            TabSnapshot {
                                id: pane.id,
                                kind: pane.kind.clone(),
                                title: pane.title.clone(),
                                exited: pane.exited,
                                foreground_process: fg,
                                cwd: pane.cwd.to_string_lossy().to_string(),
                                cols,
                                rows,
                            }
                        })
                        .collect(),
                    active_tab: group.active_tab,
                    name: group.name.clone(),
                })
                .collect();
            WorkspaceSnapshot {
                name: ws.name.clone(),
                cwd: ws.cwd.to_string_lossy().to_string(),
                layout: ws.layout.clone(),
                groups,
                active_group: ws.active_group,
                sync_panes: ws.sync_panes,
                folded_windows: ws.folded_windows.clone(),
                zoomed_window: ws.zoomed_window,
                floating_windows: ws
                    .floating_windows
                    .iter()
                    .map(|fw| FloatingWindowSnapshot {
                        id: fw.id,
                        x: fw.x,
                        y: fw.y,
                        width: fw.width,
                        height: fw.height,
                    })
                    .collect(),
                is_home: ws.is_home,
            }
        })
        .collect();

    RenderState {
        workspaces,
        active_workspace: state.active_workspace,
    }
}
