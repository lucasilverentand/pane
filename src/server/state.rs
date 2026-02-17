use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Config;
use crate::event::AppEvent;
use crate::layout::{LayoutParams, PaneId, ResolvedPane, Side, SplitDirection};
use crate::pane::{Pane, PaneGroup, PaneGroupId, PaneKind};
use crate::system_stats::SystemStats;
use crate::workspace::Workspace;


pub struct ServerState {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub session_name: String,
    pub session_id: Uuid,
    pub session_created_at: DateTime<Utc>,
    pub config: Config,
    pub system_stats: SystemStats,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub last_size: (u16, u16),
    /// Counter for assigning tmux-compatible %N pane IDs.
    pub next_pane_number: u32,
}

fn next_workspace_name(existing: &[Workspace]) -> String {
    let base = "workspace";
    // "workspace" is equivalent to number 1
    let mut used: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for ws in existing {
        if ws.name == base {
            used.insert(1);
        } else if let Some(suffix) = ws.name.strip_prefix("workspace ") {
            if let Ok(n) = suffix.parse::<u32>() {
                used.insert(n);
            }
        }
    }
    let mut n = 1u32;
    while used.contains(&n) {
        n += 1;
    }
    if n == 1 {
        base.to_string()
    } else {
        format!("workspace {}", n)
    }
}

impl ServerState {
    /// Build tmux env vars for a new pane, incrementing the pane counter.
    pub fn next_tmux_env(&mut self) -> crate::pane::pty::TmuxEnv {
        let n = self.next_pane_number;
        self.next_pane_number += 1;
        let socket_path = crate::server::daemon::socket_path(&self.session_name);
        crate::pane::pty::TmuxEnv {
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
    pub fn find_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut Pane> {
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
    pub fn find_pane_location(&self, pane_id: PaneId) -> Option<(usize, PaneGroupId)> {
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

    pub fn new_session(
        name: String,
        event_tx: &mpsc::UnboundedSender<AppEvent>,
        cols: u16,
        rows: u16,
        config: Config,
    ) -> anyhow::Result<Self> {
        let pane_id = PaneId::new_v4();
        let group_id = PaneGroupId::new_v4();

        // Build tmux env for the first pane
        let socket_path = crate::server::daemon::socket_path(&name);
        let tmux_env = crate::pane::pty::TmuxEnv {
            tmux_value: format!("{},{},0", socket_path.display(), std::process::id()),
            tmux_pane: "%0".to_string(),
        };

        let pane = match Pane::spawn_with_env(pane_id, PaneKind::Shell, cols, rows, event_tx.clone(), None, Some(tmux_env)) {
            Ok(p) => p,
            Err(e) => Pane::spawn_error(pane_id, PaneKind::Shell, &e.to_string()),
        };

        let group = PaneGroup::new(group_id, pane);
        let workspace = Workspace::new(next_workspace_name(&[]), group_id, group);

        Ok(Self {
            workspaces: vec![workspace],
            active_workspace: 0,
            session_name: name,
            session_id: Uuid::new_v4(),
            session_created_at: Utc::now(),
            config,
            system_stats: SystemStats::default(),
            event_tx: event_tx.clone(),
            last_size: (cols.saturating_add(2), rows.saturating_add(3)),
            next_pane_number: 1, // 0 was already used
        })
    }

    pub fn restore_session(
        session: crate::session::Session,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        width: u16,
        height: u16,
        config: Config,
    ) -> anyhow::Result<Self> {
        let size = ratatui::layout::Rect::new(0, 0, width, height);
        let mut workspaces = Vec::new();
        let mut pane_counter: u32 = 0;

        let socket_path = crate::server::daemon::socket_path(&session.name);
        let pid = std::process::id();

        for ws_config in &session.workspaces {
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
                    let tmux_env = crate::pane::pty::TmuxEnv {
                        tmux_value: format!("{},{},0", socket_path.display(), pid),
                        tmux_pane: format!("%{}", pane_counter),
                    };
                    pane_counter += 1;

                    let pane = match Pane::spawn_with_env(
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
                        Err(e) => Pane::spawn_error(
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
                        PaneGroup {
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
                leaf_min_sizes: HashMap::new(),
                sync_panes: false,
            });
        }

        if workspaces.is_empty() {
            let pane_id = PaneId::new_v4();
            let group_id = PaneGroupId::new_v4();
            let tmux_env = crate::pane::pty::TmuxEnv {
                tmux_value: format!("{},{},0", socket_path.display(), pid),
                tmux_pane: format!("%{}", pane_counter),
            };
            pane_counter += 1;
            let pane =
                match Pane::spawn_with_env(pane_id, PaneKind::Shell, 80, 24, event_tx.clone(), None, Some(tmux_env)) {
                    Ok(p) => p,
                    Err(e) => Pane::spawn_error(pane_id, PaneKind::Shell, &e.to_string()),
                };
            let group = PaneGroup::new(group_id, pane);
            workspaces.push(Workspace::new("1".to_string(), group_id, group));
        }

        Ok(Self {
            workspaces,
            active_workspace: session.active_workspace,
            session_name: session.name,
            session_id: session.id,
            session_created_at: session.created_at,
            config,
            system_stats: SystemStats::default(),
            event_tx,
            last_size: (width, height),
            next_pane_number: pane_counter,
        })
    }

    pub fn handle_pty_exited(&mut self, pane_id: PaneId) -> bool {
        if let Some(pane) = self.find_pane_mut(pane_id) {
            pane.exited = true;
        }

        let location = self.find_pane_location(pane_id);
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

    /// Compute proportional leaf sizes and store custom minimums for any pane
    /// whose size is below the global config default (set by user drag/resize).
    pub fn update_leaf_mins(&mut self) {
        let (w, h) = self.last_size;
        if w == 0 || h == 0 {
            return;
        }
        let bar_h = 1u16;
        let body_height = h.saturating_sub(1 + bar_h);
        let body = ratatui::layout::Rect::new(0, bar_h, w, body_height);
        let min_pw = self.config.behavior.min_pane_width;
        let min_ph = self.config.behavior.min_pane_height;

        let ws = &mut self.workspaces[self.active_workspace];
        let resolved = ws.layout.resolve(body);
        for (id, rect) in resolved {
            if rect.width < min_pw || rect.height < min_ph {
                ws.leaf_min_sizes
                    .insert(id, (rect.width.max(1), rect.height.max(1)));
            } else {
                ws.leaf_min_sizes.remove(&id);
            }
        }
    }

    /// Focus a pane group and unfold it if it's currently folded.
    pub fn focus_group(&mut self, id: PaneGroupId, bar_h: u16) {
        let (w, h) = self.last_size;
        let body_height = h.saturating_sub(1 + bar_h);
        let body = ratatui::layout::Rect::new(0, bar_h, w, body_height);
        let params = LayoutParams::from(&self.config.behavior);

        let ws = &self.workspaces[self.active_workspace];
        let resolved = ws.layout.resolve_with_fold(body, params, &ws.leaf_min_sizes);
        let is_folded = resolved.iter().any(|rp| {
            matches!(rp, ResolvedPane::Folded { id: fid, .. } if *fid == id)
        });

        let ws = &mut self.workspaces[self.active_workspace];
        ws.active_group = id;
        if is_folded {
            ws.leaf_min_sizes.clear();
            ws.layout.unfold_towards(id);
            self.resize_all_panes(w, h);
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
        kind: PaneKind,
        command: Option<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<PaneId> {
        let pane_id = PaneId::new_v4();
        let tmux_env = self.next_tmux_env();
        let pane =
            match Pane::spawn_with_env(pane_id, kind.clone(), cols, rows, self.event_tx.clone(), command, Some(tmux_env)) {
                Ok(p) => p,
                Err(e) => Pane::spawn_error(pane_id, kind, &e.to_string()),
            };

        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
            group.add_tab(pane);
        }

        let (w, h) = self.last_size;
        self.resize_all_panes(w, h);
        Ok(pane_id)
    }

    /// Split the active group and return (new_group_id, new_pane_id) for the created pane.
    pub fn split_active_group(
        &mut self,
        direction: SplitDirection,
        kind: PaneKind,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<(PaneGroupId, PaneId)> {
        let new_group_id = PaneGroupId::new_v4();
        let pane_id = PaneId::new_v4();
        let tmux_env = self.next_tmux_env();

        let pane =
            match Pane::spawn_with_env(pane_id, kind.clone(), cols, rows, self.event_tx.clone(), None, Some(tmux_env)) {
                Ok(p) => p,
                Err(e) => Pane::spawn_error(pane_id, kind, &e.to_string()),
            };

        let group = PaneGroup::new(new_group_id, pane);
        let ws = self.active_workspace_mut();
        ws.layout
            .split_pane(ws.active_group, direction, new_group_id);
        ws.groups.insert(new_group_id, group);
        ws.active_group = new_group_id;

        let (w, h) = self.last_size;
        self.resize_all_panes(w, h);
        Ok((new_group_id, pane_id))
    }

    pub fn new_workspace(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let pane_id = PaneId::new_v4();
        let group_id = PaneGroupId::new_v4();
        let tmux_env = self.next_tmux_env();

        let pane = match Pane::spawn_with_env(
            pane_id,
            PaneKind::Shell,
            cols,
            rows,
            self.event_tx.clone(),
            None,
            Some(tmux_env),
        ) {
            Ok(p) => p,
            Err(e) => Pane::spawn_error(pane_id, PaneKind::Shell, &e.to_string()),
        };

        let group = PaneGroup::new(group_id, pane);
        let workspace = Workspace::new(next_workspace_name(&self.workspaces), group_id, group);
        self.workspaces.push(workspace);
        self.active_workspace = self.workspaces.len() - 1;

        let (w, h) = self.last_size;
        self.resize_all_panes(w, h);
        Ok(())
    }

    pub fn close_workspace(&mut self) {
        if self.workspaces.len() <= 1 {
            return;
        }
        self.workspaces.remove(self.active_workspace);
        if self.active_workspace >= self.workspaces.len() {
            self.active_workspace = self.workspaces.len() - 1;
        }
    }

    pub fn restart_active_pane(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let active_group_id = self.active_workspace().active_group;
        let (exited, kind, command, id) = {
            let ws = self.active_workspace();
            if let Some(group) = ws.groups.get(&active_group_id) {
                let pane = group.active_pane();
                (pane.exited, pane.kind.clone(), pane.command.clone(), pane.id)
            } else {
                return Ok(());
            }
        };

        if !exited {
            return Ok(());
        }

        let tmux_env = self.next_tmux_env();
        let new_pane =
            match Pane::spawn_with_env(id, kind.clone(), cols, rows, self.event_tx.clone(), command, Some(tmux_env)) {
                Ok(p) => p,
                Err(e) => Pane::spawn_error(id, kind, &e.to_string()),
            };

        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&active_group_id) {
            *group.active_pane_mut() = new_pane;
        }

        let (w, h) = self.last_size;
        self.resize_all_panes(w, h);
        Ok(())
    }

    pub fn resize_all_panes(&mut self, w: u16, h: u16) {
        let overhead = 1 + self.workspace_bar_height();
        let body_height = h.saturating_sub(overhead);
        let size = ratatui::layout::Rect::new(0, 0, w, body_height);

        let params = LayoutParams::from(&self.config.behavior);
        for ws in &mut self.workspaces {
            let resolved = ws
                .layout
                .resolve_with_fold(size, params, &ws.leaf_min_sizes);
            for rp in resolved {
                match rp {
                    ResolvedPane::Visible {
                        id: group_id, rect, ..
                    } => {
                        if let Some(group) = ws.groups.get_mut(&group_id) {
                            let cols = rect.width.saturating_sub(4);
                            let rows = rect.height.saturating_sub(3); // 2 borders + 1 tab bar (always rendered)
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
        1
    }

    pub fn scroll_active_pane(&mut self, f: impl FnOnce(&mut Pane)) {
        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
            f(group.active_pane_mut());
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_server_state_new_session() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let state = ServerState::new_session(
            "test".to_string(),
            &event_tx,
            80,
            24,
            Config::default(),
        )
        .unwrap();
        assert_eq!(state.session_name, "test");
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.active_workspace, 0);
    }

    #[tokio::test]
    async fn test_server_state_workspace_accessors() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let state = ServerState::new_session(
            "test".to_string(),
            &event_tx,
            80,
            24,
            Config::default(),
        )
        .unwrap();
        let ws = state.active_workspace();
        assert_eq!(ws.groups.len(), 1);
    }

    #[tokio::test]
    async fn test_server_state_close_workspace_single() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state = ServerState::new_session(
            "test".to_string(),
            &event_tx,
            80,
            24,
            Config::default(),
        )
        .unwrap();
        // Should not close the last workspace
        state.close_workspace();
        assert_eq!(state.workspaces.len(), 1);
    }

    #[tokio::test]
    async fn test_server_state_new_workspace() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state = ServerState::new_session(
            "test".to_string(),
            &event_tx,
            80,
            24,
            Config::default(),
        )
        .unwrap();
        state.new_workspace(80, 24).unwrap();
        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.active_workspace, 1);
    }

    #[tokio::test]
    async fn test_next_tmux_env_increments() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state = ServerState::new_session(
            "tmux-test".to_string(),
            &event_tx,
            80,
            24,
            Config::default(),
        )
        .unwrap();

        // new_session already uses pane 0, so next_pane_number starts at 1
        assert_eq!(state.next_pane_number, 1);

        let env1 = state.next_tmux_env();
        assert_eq!(env1.tmux_pane, "%1");
        assert!(env1.tmux_value.contains(",0")); // ends with ",<pid>,0"
        assert!(env1.tmux_value.contains("tmux-test.sock"));

        let env2 = state.next_tmux_env();
        assert_eq!(env2.tmux_pane, "%2");

        let env3 = state.next_tmux_env();
        assert_eq!(env3.tmux_pane, "%3");

        assert_eq!(state.next_pane_number, 4);
    }

    #[tokio::test]
    async fn test_last_pane_number() {
        let (event_tx, _rx) = mpsc::unbounded_channel();
        let mut state = ServerState::new_session(
            "test".to_string(),
            &event_tx,
            80,
            24,
            Config::default(),
        )
        .unwrap();
        // new_session uses pane 0, so last_pane_number = 0 initially
        assert_eq!(state.last_pane_number(), 0);

        state.next_tmux_env(); // assigns %1
        assert_eq!(state.last_pane_number(), 1);

        state.next_tmux_env(); // assigns %2
        assert_eq!(state.last_pane_number(), 2);
    }
}
