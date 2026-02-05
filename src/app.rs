use std::collections::HashMap;

use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use uuid::Uuid;

use std::path::Path;
use std::process::Command;

use crate::config::{self, Action, Config};
use crate::event::{self, AppEvent};
use crate::layout::{PaneId, ResolvedPane, Side, SplitDirection};
use crate::pane::{Pane, PaneGroup, PaneGroupId, PaneKind};
use crate::session::store::SessionSummary;
use crate::session::{self, Session};
use crate::system_stats::{self, SystemStats};
use crate::tui::Tui;
use crate::ui;
use crate::workspace::Workspace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Scroll,
    SessionPicker,
    Help,
    DevServerInput,
}

struct DragState {
    pane_id: PaneId,
    direction: SplitDirection,
    start_pos: u16,
    total_size: u16,
}

pub struct App {
    pub should_quit: bool,
    pub mode: Mode,
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub session_name: String,
    pub session_id: Uuid,
    pub session_created_at: DateTime<Utc>,
    pub session_list: Vec<SessionSummary>,
    pub session_selected: usize,
    pub dev_server_input: String,
    pub config: Config,
    pub system_stats: SystemStats,
    pub hovered_workspace_tab: Option<usize>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    drag_state: Option<DragState>,
    last_size: (u16, u16),
}

fn auto_workspace_name() -> String {
    // Try git repo name
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(name) = Path::new(&path).file_name() {
                return name.to_string_lossy().into_owned();
            }
        }
    }
    // Fallback: current directory basename
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = cwd.file_name() {
            return name.to_string_lossy().into_owned();
        }
    }
    "1".to_string()
}

impl App {
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
    fn find_pane_location(&self, pane_id: PaneId) -> Option<(usize, PaneGroupId)> {
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

    pub fn run_with_args(args: CliArgs, config: Config) -> anyhow::Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { Self::run_async(args, config).await })
    }

    async fn run_async(args: CliArgs, config: Config) -> anyhow::Result<()> {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        event::start_event_loop(event_tx.clone());

        // Start system stats collector
        system_stats::start_stats_collector(
            event_tx.clone(),
            config.status_bar.update_interval_secs,
        );

        let mut tui = Tui::new()?;
        tui.enter()?;

        let mut app = match args {
            CliArgs::Default => {
                let sessions = session::store::list().unwrap_or_default();
                if sessions.is_empty() {
                    App::new_session("default".to_string(), &event_tx, &tui, config)?
                } else {
                    App::with_session_picker(sessions, event_tx.clone(), config)
                }
            }
            CliArgs::New(name) => App::new_session(name, &event_tx, &tui, config)?,
            CliArgs::Attach(name) => {
                let sessions = session::store::list().unwrap_or_default();
                if let Some(summary) = sessions.iter().find(|s| s.name == name) {
                    let session = session::store::load(&summary.id)?;
                    App::restore_session(session, event_tx.clone(), &tui, config)?
                } else {
                    App::new_session(name, &event_tx, &tui, config)?
                }
            }
        };

        loop {
            tui.draw(|frame| ui::render(&app, frame))?;

            if let Some(event) = event_rx.recv().await {
                app.handle_event(event, &tui)?;
            }

            if app.should_quit {
                if !app.workspaces.is_empty() {
                    let session = Session::from_app(&app);
                    let _ = session::store::save(&session);
                }
                break;
            }
        }

        Ok(())
    }

    fn new_session(
        name: String,
        event_tx: &mpsc::UnboundedSender<AppEvent>,
        tui: &Tui,
        config: Config,
    ) -> anyhow::Result<Self> {
        let pane_id = PaneId::new_v4();
        let group_id = PaneGroupId::new_v4();
        let size = tui.size()?;
        let cols = size.width.saturating_sub(2);
        let rows = size.height.saturating_sub(3);

        let pane = match Pane::spawn(pane_id, PaneKind::Shell, cols, rows, event_tx.clone(), None) {
            Ok(p) => p,
            Err(e) => Pane::spawn_error(pane_id, PaneKind::Shell, &e.to_string()),
        };

        let group = PaneGroup::new(group_id, pane);
        let workspace = Workspace::new(auto_workspace_name(), group_id, group);

        Ok(Self {
            should_quit: false,
            mode: Mode::Normal,
            workspaces: vec![workspace],
            active_workspace: 0,
            session_name: name,
            session_id: Uuid::new_v4(),
            session_created_at: Utc::now(),
            session_list: Vec::new(),
            session_selected: 0,
            dev_server_input: String::new(),
            config,
            system_stats: SystemStats::default(),
            hovered_workspace_tab: None,
            event_tx: event_tx.clone(),
            drag_state: None,
            last_size: (size.width, size.height),
        })
    }

    fn with_session_picker(
        sessions: Vec<SessionSummary>,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        config: Config,
    ) -> Self {
        Self {
            should_quit: false,
            mode: Mode::SessionPicker,
            workspaces: Vec::new(),
            active_workspace: 0,
            session_name: String::new(),
            session_id: Uuid::new_v4(),
            session_created_at: Utc::now(),
            session_list: sessions,
            session_selected: 0,
            dev_server_input: String::new(),
            config,
            system_stats: SystemStats::default(),
            hovered_workspace_tab: None,
            event_tx,
            drag_state: None,
            last_size: (0, 0),
        }
    }

    fn restore_session(
        session: Session,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        tui: &Tui,
        config: Config,
    ) -> anyhow::Result<Self> {
        let size = tui.size()?;
        let mut workspaces = Vec::new();

        for ws_config in &session.workspaces {
            let layout = ws_config.layout.clone();
            let resolved =
                layout.resolve(ratatui::layout::Rect::new(0, 0, size.width, size.height));
            let mut groups = HashMap::new();

            for group_config in &ws_config.groups {
                let mut tabs = Vec::new();
                let (cols, rows) = resolved
                    .iter()
                    .find(|(id, _)| *id == group_config.id)
                    .map(|(_, r)| (r.width.saturating_sub(2), r.height.saturating_sub(2)))
                    .unwrap_or((80, 24));

                for pane_config in &group_config.tabs {
                    let pane = match Pane::spawn(
                        pane_config.id,
                        pane_config.kind.clone(),
                        cols,
                        rows,
                        event_tx.clone(),
                        pane_config.command.clone(),
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
            });
        }

        if workspaces.is_empty() {
            let pane_id = PaneId::new_v4();
            let group_id = PaneGroupId::new_v4();
            let pane =
                match Pane::spawn(pane_id, PaneKind::Shell, 80, 24, event_tx.clone(), None) {
                    Ok(p) => p,
                    Err(e) => Pane::spawn_error(pane_id, PaneKind::Shell, &e.to_string()),
                };
            let group = PaneGroup::new(group_id, pane);
            workspaces.push(Workspace::new("1".to_string(), group_id, group));
        }

        Ok(Self {
            should_quit: false,
            mode: Mode::Normal,
            workspaces,
            active_workspace: session.active_workspace,
            session_name: session.name,
            session_id: session.id,
            session_created_at: session.created_at,
            session_list: Vec::new(),
            session_selected: 0,
            dev_server_input: String::new(),
            config,
            system_stats: SystemStats::default(),
            hovered_workspace_tab: None,
            event_tx,
            drag_state: None,
            last_size: (size.width, size.height),
        })
    }

    fn handle_event(&mut self, event: AppEvent, tui: &Tui) -> anyhow::Result<()> {
        match event {
            AppEvent::Key(key) => self.handle_key_event(key, tui)?,
            AppEvent::PtyOutput { pane_id, bytes } => {
                if let Some(pane) = self.find_pane_mut(pane_id) {
                    pane.process_output(&bytes);
                }
            }
            AppEvent::PtyExited { pane_id } => {
                self.handle_pty_exited(pane_id);
            }
            AppEvent::MouseScroll { up } => {
                if self.mode == Mode::Normal || self.mode == Mode::Scroll {
                    if up {
                        self.scroll_active_pane(|p| p.scroll_up(3));
                        if self.is_active_pane_scrolled() {
                            self.mode = Mode::Scroll;
                        }
                    } else {
                        self.scroll_active_pane(|p| p.scroll_down(3));
                        if !self.is_active_pane_scrolled() {
                            self.mode = Mode::Normal;
                        }
                    }
                }
            }
            AppEvent::Resize(w, h) => {
                self.last_size = (w, h);
                self.resize_all_panes(w, h);
                if self.mode == Mode::Scroll {
                    self.mode = Mode::Normal;
                }
            }
            AppEvent::MouseDown { x, y } => {
                if self.mode == Mode::Normal {
                    self.handle_mouse_down(x, y, tui)?;
                }
            }
            AppEvent::MouseDrag { x, y } => {
                self.handle_mouse_drag(x, y);
            }
            AppEvent::MouseMove { x, y } => {
                self.handle_mouse_move(x, y);
            }
            AppEvent::MouseUp => {
                self.drag_state = None;
            }
            AppEvent::SystemStats(stats) => {
                self.system_stats = stats;
            }
            AppEvent::Tick => {}
        }
        Ok(())
    }

    fn handle_pty_exited(&mut self, pane_id: PaneId) {
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
                        self.should_quit = true;
                        return;
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
                        self.should_quit = true;
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
    }

    fn handle_mouse_down(&mut self, x: u16, y: u16, tui: &Tui) -> anyhow::Result<()> {
        let size = tui.size()?;
        let bar_h = self.workspace_bar_height();

        // Check workspace bar clicks first
        if y < bar_h {
            let names: Vec<String> = self.workspaces.iter().map(|ws| ws.name.clone()).collect();
            let bar_area = ratatui::layout::Rect::new(0, 0, size.width, bar_h);
            if let Some(click) = crate::ui::workspace_bar::hit_test(
                &names,
                self.active_workspace,
                self.hovered_workspace_tab,
                bar_area,
                x,
                y,
            ) {
                match click {
                    crate::ui::workspace_bar::WorkspaceBarClick::Tab(i) => {
                        if i < self.workspaces.len() {
                            self.active_workspace = i;
                        }
                    }
                    crate::ui::workspace_bar::WorkspaceBarClick::CloseTab(i) => {
                        if self.workspaces.len() > 1 && i < self.workspaces.len() {
                            self.workspaces.remove(i);
                            if self.active_workspace >= self.workspaces.len() {
                                self.active_workspace = self.workspaces.len() - 1;
                            }
                            self.hovered_workspace_tab = None;
                        }
                    }
                    crate::ui::workspace_bar::WorkspaceBarClick::NewWorkspace => {
                        self.new_workspace(tui)?;
                    }
                }
            }
            return Ok(());
        }

        let body_height = size.height.saturating_sub(1 + bar_h);
        let body = ratatui::layout::Rect::new(0, bar_h, size.width, body_height);

        // Check fold bars FIRST — they take priority over split border drag
        let params = crate::layout::LayoutParams::from(&self.config.behavior);
        let ws = self.active_workspace();
        let resolved = ws.layout.resolve_with_fold(body, params, &ws.leaf_min_sizes);
        for rp in &resolved {
            if let ResolvedPane::Folded { id: group_id, rect, .. } = rp {
                if rect.width == 0 || rect.height == 0 {
                    continue;
                }
                if x >= rect.x
                    && x < rect.x + rect.width
                    && y >= rect.y
                    && y < rect.y + rect.height
                {
                    let group_id = *group_id;
                    // Clear stale leaf minimums so the new ratio takes full effect
                    self.active_workspace_mut().leaf_min_sizes.clear();
                    self.active_workspace_mut().layout.unfold_towards(group_id);
                    self.active_workspace_mut().active_group = group_id;
                    self.resize_all_panes(size.width, size.height);
                    return Ok(());
                }
            }
        }

        // Then check split borders for drag resize
        let ws = self.active_workspace();
        if let Some((pane_id, direction, border_pos, total_size)) =
            ws.layout.find_split_border(x, y, body)
        {
            self.drag_state = Some(DragState {
                pane_id,
                direction,
                start_pos: border_pos,
                total_size,
            });
            return Ok(());
        }

        // Finally, check visible panes for focus
        for rp in &resolved {
            if let ResolvedPane::Visible { id: group_id, rect } = rp {
                if x >= rect.x
                    && x < rect.x + rect.width
                    && y >= rect.y
                    && y < rect.y + rect.height
                {
                    self.active_workspace_mut().active_group = *group_id;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn handle_mouse_drag(&mut self, x: u16, y: u16) {
        if let Some(drag) = &self.drag_state {
            if drag.total_size == 0 {
                return;
            }
            let current_pos = match drag.direction {
                SplitDirection::Horizontal => x,
                SplitDirection::Vertical => y,
            };
            let delta_px = current_pos as f64 - drag.start_pos as f64;
            let delta_ratio = delta_px / drag.total_size as f64;
            let pane_id = drag.pane_id;

            self.active_workspace_mut().layout.resize(pane_id, delta_ratio);

            if let Some(drag) = &mut self.drag_state {
                drag.start_pos = current_pos;
            }

            self.update_leaf_mins();
        }
    }

    fn handle_mouse_move(&mut self, x: u16, y: u16) {
        if y == 0 {
            let names: Vec<String> = self.workspaces.iter().map(|ws| ws.name.clone()).collect();
            let area = ratatui::layout::Rect::new(0, 0, self.last_size.0, 1);
            // Find which tab the mouse is over
            if let Some(click) = crate::ui::workspace_bar::hit_test(
                &names,
                self.active_workspace,
                self.hovered_workspace_tab,
                area,
                x,
                y,
            ) {
                match click {
                    crate::ui::workspace_bar::WorkspaceBarClick::Tab(i)
                    | crate::ui::workspace_bar::WorkspaceBarClick::CloseTab(i) => {
                        self.hovered_workspace_tab = Some(i);
                    }
                    _ => {
                        self.hovered_workspace_tab = None;
                    }
                }
            } else {
                self.hovered_workspace_tab = None;
            }
        } else {
            self.hovered_workspace_tab = None;
        }
    }

    /// Compute proportional leaf sizes and store custom minimums for any pane
    /// whose size is below the global config default (set by user drag/resize).
    fn update_leaf_mins(&mut self) {
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
                ws.leaf_min_sizes.insert(id, (rect.width.max(1), rect.height.max(1)));
            } else {
                ws.leaf_min_sizes.remove(&id);
            }
        }
    }

    /// Focus a pane group and unfold it if it's currently folded.
    fn focus_group(&mut self, id: PaneGroupId) {
        let (w, h) = self.last_size;
        let bar_h = self.workspace_bar_height();
        let body_height = h.saturating_sub(1 + bar_h);
        let body = ratatui::layout::Rect::new(0, bar_h, w, body_height);
        let params = crate::layout::LayoutParams::from(&self.config.behavior);

        let ws = &self.workspaces[self.active_workspace];
        let resolved = ws.layout.resolve_with_fold(body, params, &ws.leaf_min_sizes);
        let is_folded = resolved.iter().any(|rp| matches!(rp, ResolvedPane::Folded { id: fid, .. } if *fid == id));

        let ws = &mut self.workspaces[self.active_workspace];
        ws.active_group = id;
        if is_folded {
            ws.leaf_min_sizes.clear();
            ws.layout.unfold_towards(id);
            self.resize_all_panes(w, h);
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        // Modal keys: these modes handle their own input
        match &self.mode {
            Mode::SessionPicker => return self.handle_session_picker_key(key, tui),
            Mode::Help => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::Normal;
                }
                return Ok(());
            }
            Mode::DevServerInput => return self.handle_dev_server_input_key(key, tui),
            Mode::Scroll => return self.handle_scroll_key(key),
            Mode::Normal => {}
        }

        // KeyMap dispatch
        let normalized = config::normalize_key(key);
        if let Some(action) = self.config.keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui);
        }

        // Forward to PTY
        let ws = self.active_workspace_mut();
        let group = ws.groups.get_mut(&ws.active_group);
        if let Some(group) = group {
            let pane = group.active_pane_mut();
            let bytes = key_to_bytes(key);
            if !bytes.is_empty() {
                pane.write_input(&bytes);
            }
        }

        Ok(())
    }

    fn execute_action(&mut self, action: Action, tui: &Tui) -> anyhow::Result<()> {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }
            Action::NewWorkspace => {
                self.new_workspace(tui)?;
            }
            Action::CloseWorkspace => {
                self.close_workspace();
            }
            Action::SwitchWorkspace(n) => {
                let idx = (n as usize) - 1;
                if idx < self.workspaces.len() {
                    self.active_workspace = idx;
                }
            }
            Action::NewTab => {
                self.add_tab_to_active_group(PaneKind::Shell, None, tui)?;
            }
            Action::DevServerInput => {
                self.dev_server_input.clear();
                self.mode = Mode::DevServerInput;
            }
            Action::NextTab => {
                self.active_workspace_mut().active_group_mut().next_tab();
            }
            Action::PrevTab => {
                self.active_workspace_mut().active_group_mut().prev_tab();
            }
            Action::CloseTab => {
                self.close_active_tab();
            }
            Action::SplitHorizontal => {
                self.split_active_group(SplitDirection::Horizontal, PaneKind::Shell, tui)?;
            }
            Action::SplitVertical => {
                self.split_active_group(SplitDirection::Vertical, PaneKind::Shell, tui)?;
            }
            Action::RestartPane => {
                self.restart_active_pane(tui)?;
            }
            Action::FocusLeft => {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Horizontal,
                    Side::First,
                ) {
                    self.focus_group(id);
                }
            }
            Action::FocusDown => {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::Second,
                ) {
                    self.focus_group(id);
                }
            }
            Action::FocusUp => {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::First,
                ) {
                    self.focus_group(id);
                }
            }
            Action::FocusRight => {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Horizontal,
                    Side::Second,
                ) {
                    self.focus_group(id);
                }
            }
            Action::FocusGroupN(n) => {
                let idx = (n as usize) - 1;
                let ws = self.active_workspace();
                let ids = ws.layout.group_ids();
                if let Some(&id) = ids.get(idx) {
                    self.focus_group(id);
                }
            }
            Action::MoveTabLeft => {
                self.move_tab_to_neighbor(SplitDirection::Horizontal, Side::First);
            }
            Action::MoveTabDown => {
                self.move_tab_to_neighbor(SplitDirection::Vertical, Side::Second);
            }
            Action::MoveTabUp => {
                self.move_tab_to_neighbor(SplitDirection::Vertical, Side::First);
            }
            Action::MoveTabRight => {
                self.move_tab_to_neighbor(SplitDirection::Horizontal, Side::Second);
            }
            Action::ResizeShrinkH => {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, -0.05);
                self.update_leaf_mins();
            }
            Action::ResizeGrowH => {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, 0.05);
                self.update_leaf_mins();
            }
            Action::ResizeGrowV => {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, 0.05);
                self.update_leaf_mins();
            }
            Action::ResizeShrinkV => {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, -0.05);
                self.update_leaf_mins();
            }
            Action::Equalize => {
                self.active_workspace_mut().layout.equalize();
                self.active_workspace_mut().leaf_min_sizes.clear();
            }
            Action::SessionPicker => {
                self.session_list = session::store::list().unwrap_or_default();
                self.session_selected = 0;
                self.mode = Mode::SessionPicker;
            }
            Action::Help => {
                self.mode = Mode::Help;
            }
            Action::ScrollMode => {
                let rows = self.active_pane_screen_rows();
                self.scroll_active_pane(|p| p.scroll_up(rows / 2));
                if self.is_active_pane_scrolled() {
                    self.mode = Mode::Scroll;
                }
            }
        }
        Ok(())
    }

    fn handle_scroll_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let mods = key.modifiers;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if mods.is_empty() => {
                self.scroll_active_pane(|p| p.scroll_up(1));
            }
            KeyCode::Down | KeyCode::Char('j') if mods.is_empty() => {
                self.scroll_active_pane(|p| p.scroll_down(1));
            }
            KeyCode::PageUp => {
                let rows = self.active_pane_screen_rows();
                self.scroll_active_pane(|p| p.scroll_up(rows / 2));
            }
            KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                let rows = self.active_pane_screen_rows();
                self.scroll_active_pane(|p| p.scroll_up(rows / 2));
            }
            KeyCode::PageDown => {
                let rows = self.active_pane_screen_rows();
                self.scroll_active_pane(|p| p.scroll_down(rows / 2));
            }
            KeyCode::Char('d') if mods.contains(KeyModifiers::CONTROL) => {
                let rows = self.active_pane_screen_rows();
                self.scroll_active_pane(|p| p.scroll_down(rows / 2));
            }
            KeyCode::Char('g') if mods.is_empty() => {
                self.scroll_active_pane(|p| p.scroll_to_top());
            }
            KeyCode::Home => {
                self.scroll_active_pane(|p| p.scroll_to_top());
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.exit_scroll_mode();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.exit_scroll_mode();
            }
            _ => {
                self.exit_scroll_mode();
                let ws = self.active_workspace_mut();
                let group = ws.groups.get_mut(&ws.active_group);
                if let Some(group) = group {
                    let pane = group.active_pane_mut();
                    let bytes = key_to_bytes(key);
                    if !bytes.is_empty() {
                        pane.write_input(&bytes);
                    }
                }
                return Ok(());
            }
        }
        if !self.is_active_pane_scrolled() {
            self.mode = Mode::Normal;
        }
        Ok(())
    }

    fn scroll_active_pane(&mut self, f: impl FnOnce(&mut Pane)) {
        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
            f(group.active_pane_mut());
        }
    }

    fn is_active_pane_scrolled(&self) -> bool {
        let ws = self.active_workspace();
        ws.groups
            .get(&ws.active_group)
            .map(|g| g.active_pane().is_scrolled())
            .unwrap_or(false)
    }

    fn active_pane_screen_rows(&self) -> usize {
        let ws = self.active_workspace();
        ws.groups
            .get(&ws.active_group)
            .map(|g| g.active_pane().screen().size().0 as usize)
            .unwrap_or(24)
    }

    fn exit_scroll_mode(&mut self) {
        self.scroll_active_pane(|p| p.scroll_to_bottom());
        self.mode = Mode::Normal;
    }

    fn handle_session_picker_key(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {
                let has_real_panes = self.workspaces.iter().any(|ws| {
                    ws.groups
                        .values()
                        .any(|g| g.tabs.iter().any(|p| !p.exited))
                });
                if !has_real_panes {
                    self.should_quit = true;
                } else {
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.session_list.is_empty() {
                    self.session_selected =
                        (self.session_selected + 1) % self.session_list.len();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.session_list.is_empty() {
                    self.session_selected = self
                        .session_selected
                        .checked_sub(1)
                        .unwrap_or(self.session_list.len() - 1);
                }
            }
            KeyCode::Enter => {
                if let Some(summary) = self.session_list.get(self.session_selected) {
                    if let Ok(session) = session::store::load(&summary.id) {
                        let config = self.config.clone();
                        let restored =
                            App::restore_session(session, self.event_tx.clone(), tui, config)?;
                        self.workspaces = restored.workspaces;
                        self.active_workspace = restored.active_workspace;
                        self.session_name = restored.session_name;
                        self.session_id = restored.session_id;
                        self.session_created_at = restored.session_created_at;
                        self.last_size = restored.last_size;
                        self.mode = Mode::Normal;
                    }
                }
            }
            KeyCode::Char('n') => {
                let name = format!("session-{}", self.session_list.len() + 1);
                let config = self.config.clone();
                let new = App::new_session(name, &self.event_tx, tui, config)?;
                self.workspaces = new.workspaces;
                self.active_workspace = new.active_workspace;
                self.session_name = new.session_name;
                self.session_id = new.session_id;
                self.session_created_at = new.session_created_at;
                self.last_size = new.last_size;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('d') => {
                if let Some(summary) = self.session_list.get(self.session_selected) {
                    let _ = session::store::delete(&summary.id);
                    self.session_list = session::store::list().unwrap_or_default();
                    if self.session_selected >= self.session_list.len()
                        && !self.session_list.is_empty()
                    {
                        self.session_selected = self.session_list.len() - 1;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_dev_server_input_key(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let cmd = self.dev_server_input.clone();
                if !cmd.is_empty() {
                    self.add_tab_to_active_group(PaneKind::DevServer, Some(cmd), tui)?;
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.dev_server_input.pop();
            }
            KeyCode::Char(c) => {
                self.dev_server_input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn add_tab_to_active_group(
        &mut self,
        kind: PaneKind,
        command: Option<String>,
        tui: &Tui,
    ) -> anyhow::Result<()> {
        let pane_id = PaneId::new_v4();
        let size = tui.size()?;
        let cols = size.width.saturating_sub(4);
        let rows = size.height.saturating_sub(3);

        let pane = match Pane::spawn(pane_id, kind.clone(), cols, rows, self.event_tx.clone(), command)
        {
            Ok(p) => p,
            Err(e) => Pane::spawn_error(pane_id, kind, &e.to_string()),
        };

        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
            group.add_tab(pane);
        }
        Ok(())
    }

    fn split_active_group(
        &mut self,
        direction: SplitDirection,
        kind: PaneKind,
        tui: &Tui,
    ) -> anyhow::Result<()> {
        let new_group_id = PaneGroupId::new_v4();
        let pane_id = PaneId::new_v4();
        let size = tui.size()?;
        let cols = size.width.saturating_sub(4) / 2;
        let rows = size.height.saturating_sub(3);

        let pane = match Pane::spawn(pane_id, kind.clone(), cols, rows, self.event_tx.clone(), None)
        {
            Ok(p) => p,
            Err(e) => Pane::spawn_error(pane_id, kind, &e.to_string()),
        };

        let group = PaneGroup::new(new_group_id, pane);
        let ws = self.active_workspace_mut();
        ws.layout
            .split_pane(ws.active_group, direction, new_group_id);
        ws.groups.insert(new_group_id, group);
        ws.active_group = new_group_id;

        self.resize_all_panes(size.width, size.height);
        Ok(())
    }

    fn close_active_tab(&mut self) {
        let ws = self.active_workspace_mut();
        let active_group_id = ws.active_group;

        if let Some(group) = ws.groups.get_mut(&active_group_id) {
            if group.tab_count() > 1 {
                group.close_tab(group.active_tab);
                return;
            }
        }

        let group_ids = ws.layout.group_ids();
        if group_ids.len() <= 1 {
            return;
        }

        if let Some(new_focus) = ws.layout.close_pane(active_group_id) {
            ws.groups.remove(&active_group_id);
            ws.active_group = new_focus;
        }
    }

    fn move_tab_to_neighbor(&mut self, direction: SplitDirection, side: Side) {
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

    fn new_workspace(&mut self, tui: &Tui) -> anyhow::Result<()> {
        let pane_id = PaneId::new_v4();
        let group_id = PaneGroupId::new_v4();
        let size = tui.size()?;
        let cols = size.width.saturating_sub(2);
        let rows = size.height.saturating_sub(3);

        let pane = match Pane::spawn(pane_id, PaneKind::Shell, cols, rows, self.event_tx.clone(), None)
        {
            Ok(p) => p,
            Err(e) => Pane::spawn_error(pane_id, PaneKind::Shell, &e.to_string()),
        };

        let group = PaneGroup::new(group_id, pane);
        let workspace = Workspace::new(auto_workspace_name(), group_id, group);
        self.workspaces.push(workspace);
        self.active_workspace = self.workspaces.len() - 1;
        Ok(())
    }

    fn close_workspace(&mut self) {
        if self.workspaces.len() <= 1 {
            return;
        }
        self.workspaces.remove(self.active_workspace);
        if self.active_workspace >= self.workspaces.len() {
            self.active_workspace = self.workspaces.len() - 1;
        }
    }

    fn restart_active_pane(&mut self, tui: &Tui) -> anyhow::Result<()> {
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

        let size = tui.size()?;
        let cols = size.width.saturating_sub(4);
        let rows = size.height.saturating_sub(3);

        let new_pane =
            match Pane::spawn(id, kind.clone(), cols, rows, self.event_tx.clone(), command) {
                Ok(p) => p,
                Err(e) => Pane::spawn_error(id, kind, &e.to_string()),
            };

        let ws = self.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&active_group_id) {
            *group.active_pane_mut() = new_pane;
        }
        Ok(())
    }

    fn workspace_bar_height(&self) -> u16 {
        1
    }

    fn resize_all_panes(&mut self, w: u16, h: u16) {
        let overhead = 1 + self.workspace_bar_height(); // status bar + optional workspace bar
        let body_height = h.saturating_sub(overhead);
        let size = ratatui::layout::Rect::new(0, 0, w, body_height);

        let params = crate::layout::LayoutParams::from(&self.config.behavior);
        for ws in &mut self.workspaces {
            let resolved = ws.layout.resolve_with_fold(size, params, &ws.leaf_min_sizes);
            for rp in resolved {
                match rp {
                    ResolvedPane::Visible { id: group_id, rect } => {
                        if let Some(group) = ws.groups.get_mut(&group_id) {
                            let has_tab_bar = group.tab_count() > 1;
                            let tab_bar_offset: u16 = if has_tab_bar { 1 } else { 0 };
                            let cols = rect.width.saturating_sub(4);
                            let rows = rect.height.saturating_sub(2 + tab_bar_offset);
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
}

pub enum CliArgs {
    Default,
    New(String),
    Attach(String),
}

impl CliArgs {
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        match args.get(1).map(|s| s.as_str()) {
            Some("new") => {
                let name = args
                    .get(2)
                    .cloned()
                    .unwrap_or_else(|| format!("session-{}", chrono::Utc::now().timestamp()));
                CliArgs::New(name)
            }
            Some("attach") => {
                let name = args.get(2).cloned().unwrap_or_else(|| "default".to_string());
                CliArgs::Attach(name)
            }
            _ => CliArgs::Default,
        }
    }
}

/// Convert a crossterm KeyEvent to bytes suitable for writing to a PTY.
pub(crate) fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    let mods = key.modifiers;

    match key.code {
        KeyCode::Char(c) => {
            if mods.contains(KeyModifiers::CONTROL) {
                if c.is_ascii_lowercase() {
                    return vec![c as u8 - b'a' + 1];
                }
                if c.is_ascii_uppercase() {
                    return vec![c.to_ascii_lowercase() as u8 - b'a' + 1];
                }
            }
            if mods.contains(KeyModifiers::ALT) {
                let mut bytes = vec![0x1b];
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                return bytes;
            }
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    // --- key_to_bytes tests ---

    #[test]
    fn test_key_plain_char() {
        let bytes = key_to_bytes(make_key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(bytes, b"a");
    }

    #[test]
    fn test_key_uppercase_char() {
        let bytes = key_to_bytes(make_key(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(bytes, b"A");
    }

    #[test]
    fn test_key_ctrl_a() {
        let bytes = key_to_bytes(make_key(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(bytes, vec![0x01]);
    }

    #[test]
    fn test_key_ctrl_c() {
        let bytes = key_to_bytes(make_key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(bytes, vec![0x03]);
    }

    #[test]
    fn test_key_ctrl_z() {
        let bytes = key_to_bytes(make_key(KeyCode::Char('z'), KeyModifiers::CONTROL));
        assert_eq!(bytes, vec![0x1a]);
    }

    #[test]
    fn test_key_ctrl_uppercase() {
        let bytes = key_to_bytes(make_key(
            KeyCode::Char('D'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert_eq!(bytes, vec![0x04]);
    }

    #[test]
    fn test_key_alt_char() {
        let bytes = key_to_bytes(make_key(KeyCode::Char('x'), KeyModifiers::ALT));
        assert_eq!(bytes, vec![0x1b, b'x']);
    }

    #[test]
    fn test_key_enter() {
        let bytes = key_to_bytes(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(bytes, vec![b'\r']);
    }

    #[test]
    fn test_key_backspace() {
        let bytes = key_to_bytes(make_key(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(bytes, vec![0x7f]);
    }

    #[test]
    fn test_key_tab() {
        let bytes = key_to_bytes(make_key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(bytes, vec![b'\t']);
    }

    #[test]
    fn test_key_escape() {
        let bytes = key_to_bytes(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(bytes, vec![0x1b]);
    }

    #[test]
    fn test_key_arrow_keys() {
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Up, KeyModifiers::NONE)),
            b"\x1b[A"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Down, KeyModifiers::NONE)),
            b"\x1b[B"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Right, KeyModifiers::NONE)),
            b"\x1b[C"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Left, KeyModifiers::NONE)),
            b"\x1b[D"
        );
    }

    #[test]
    fn test_key_home_end() {
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Home, KeyModifiers::NONE)),
            b"\x1b[H"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::End, KeyModifiers::NONE)),
            b"\x1b[F"
        );
    }

    #[test]
    fn test_key_page_up_down() {
        assert_eq!(
            key_to_bytes(make_key(KeyCode::PageUp, KeyModifiers::NONE)),
            b"\x1b[5~"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::PageDown, KeyModifiers::NONE)),
            b"\x1b[6~"
        );
    }

    #[test]
    fn test_key_delete() {
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Delete, KeyModifiers::NONE)),
            b"\x1b[3~"
        );
    }

    #[test]
    fn test_key_function_keys() {
        assert_eq!(
            key_to_bytes(make_key(KeyCode::F(1), KeyModifiers::NONE)),
            b"\x1bOP"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::F(4), KeyModifiers::NONE)),
            b"\x1bOS"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::F(12), KeyModifiers::NONE)),
            b"\x1b[24~"
        );
    }

    #[test]
    fn test_key_unicode_char() {
        let bytes = key_to_bytes(make_key(KeyCode::Char('é'), KeyModifiers::NONE));
        assert_eq!(bytes, "é".as_bytes());
    }

    // --- Mode tests ---

    #[test]
    fn test_mode_equality() {
        assert_eq!(Mode::Normal, Mode::Normal);
        assert_ne!(Mode::Normal, Mode::Help);
        assert_ne!(Mode::SessionPicker, Mode::DevServerInput);
    }

    #[test]
    fn test_pane_kind_labels() {
        use crate::pane::PaneKind;
        assert_eq!(PaneKind::Shell.label(), "shell");
        assert_eq!(PaneKind::Agent.label(), "claude");
        assert_eq!(PaneKind::Nvim.label(), "nvim");
        assert_eq!(PaneKind::DevServer.label(), "server");
    }

    // --- PaneGroup tests ---

    #[test]
    fn test_pane_group_new() {
        let gid = PaneGroupId::new_v4();
        let pid = PaneId::new_v4();
        let pane = Pane::spawn_error(pid, PaneKind::Shell, "test");
        let group = PaneGroup::new(gid, pane);
        assert_eq!(group.tab_count(), 1);
        assert_eq!(group.active_tab, 0);
        assert_eq!(group.active_pane().id, pid);
    }

    #[test]
    fn test_pane_group_add_tab() {
        let gid = PaneGroupId::new_v4();
        let p1 = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "t1");
        let p2_id = PaneId::new_v4();
        let p2 = Pane::spawn_error(p2_id, PaneKind::Shell, "t2");
        let mut group = PaneGroup::new(gid, p1);
        group.add_tab(p2);
        assert_eq!(group.tab_count(), 2);
        assert_eq!(group.active_tab, 1);
        assert_eq!(group.active_pane().id, p2_id);
    }

    #[test]
    fn test_pane_group_close_tab() {
        let gid = PaneGroupId::new_v4();
        let p1 = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "t1");
        let p2 = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "t2");
        let mut group = PaneGroup::new(gid, p1);
        group.add_tab(p2);
        assert!(group.close_tab(1));
        assert_eq!(group.tab_count(), 1);
        assert_eq!(group.active_tab, 0);
    }

    #[test]
    fn test_pane_group_close_last_tab_fails() {
        let gid = PaneGroupId::new_v4();
        let p1 = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "t1");
        let mut group = PaneGroup::new(gid, p1);
        assert!(!group.close_tab(0));
        assert_eq!(group.tab_count(), 1);
    }

    #[test]
    fn test_pane_group_tab_cycling() {
        let gid = PaneGroupId::new_v4();
        let p1_id = PaneId::new_v4();
        let p2_id = PaneId::new_v4();
        let p3_id = PaneId::new_v4();
        let p1 = Pane::spawn_error(p1_id, PaneKind::Shell, "t1");
        let p2 = Pane::spawn_error(p2_id, PaneKind::Shell, "t2");
        let p3 = Pane::spawn_error(p3_id, PaneKind::Shell, "t3");
        let mut group = PaneGroup::new(gid, p1);
        group.add_tab(p2);
        group.add_tab(p3);
        assert_eq!(group.active_pane().id, p3_id);

        group.next_tab();
        assert_eq!(group.active_tab, 0);
        assert_eq!(group.active_pane().id, p1_id);

        group.prev_tab();
        assert_eq!(group.active_tab, 2);
        assert_eq!(group.active_pane().id, p3_id);
    }
}
