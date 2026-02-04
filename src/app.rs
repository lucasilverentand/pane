use std::collections::HashMap;

use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::event::{self, AppEvent};
use crate::layout::{PaneId, Side, SplitDirection};
use crate::pane::{Pane, PaneGroup, PaneGroupId, PaneKind};
use crate::session::store::SessionSummary;
use crate::session::{self, Session};
use crate::tui::Tui;
use crate::ui;
use crate::workspace::Workspace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    SessionPicker,
    NewPane,
    Help,
    DevServerInput,
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
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

impl App {
    pub fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active_workspace]
    }

    pub fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_workspace]
    }

    /// Find a pane mutably across all workspaces/groups/tabs.
    /// Returns (workspace_idx, group_id, tab_idx, &mut Pane)
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

    pub fn run_with_args(args: CliArgs) -> anyhow::Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { Self::run_async(args).await })
    }

    async fn run_async(args: CliArgs) -> anyhow::Result<()> {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        event::start_event_loop(event_tx.clone());

        let mut tui = Tui::new()?;
        tui.enter()?;

        let mut app = match args {
            CliArgs::Default => {
                let sessions = session::store::list().unwrap_or_default();
                if sessions.is_empty() {
                    App::new_session("default".to_string(), &event_tx, &tui)?
                } else {
                    App::with_session_picker(sessions, event_tx.clone())
                }
            }
            CliArgs::New(name) => App::new_session(name, &event_tx, &tui)?,
            CliArgs::Attach(name) => {
                let sessions = session::store::list().unwrap_or_default();
                if let Some(summary) = sessions.iter().find(|s| s.name == name) {
                    let session = session::store::load(&summary.id)?;
                    App::restore_session(session, event_tx.clone(), &tui)?
                } else {
                    App::new_session(name, &event_tx, &tui)?
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
    ) -> anyhow::Result<Self> {
        let pane_id = PaneId::new_v4();
        let group_id = PaneGroupId::new_v4();
        let size = tui.size()?;
        // Account for footer (1) + borders (2)
        let cols = size.width.saturating_sub(2);
        let rows = size.height.saturating_sub(3);

        let pane = match Pane::spawn(pane_id, PaneKind::Shell, cols, rows, event_tx.clone(), None) {
            Ok(p) => p,
            Err(e) => Pane::spawn_error(pane_id, PaneKind::Shell, &e.to_string()),
        };

        let group = PaneGroup::new(group_id, pane);
        let workspace = Workspace::new("1".to_string(), group_id, group);

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
            event_tx: event_tx.clone(),
        })
    }

    fn with_session_picker(
        sessions: Vec<SessionSummary>,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        // Create a dummy workspace for session picker mode
        let group_id = PaneGroupId::new_v4();
        let pane_id = PaneId::new_v4();
        // Dummy pane with error display (no PTY)
        let pane = Pane::spawn_error(pane_id, PaneKind::Shell, "");
        let group = PaneGroup::new(group_id, pane);
        let workspace = Workspace::new("1".to_string(), group_id, group);

        Self {
            should_quit: false,
            mode: Mode::SessionPicker,
            workspaces: vec![workspace],
            active_workspace: 0,
            session_name: String::new(),
            session_id: Uuid::new_v4(),
            session_created_at: Utc::now(),
            session_list: sessions,
            session_selected: 0,
            dev_server_input: String::new(),
            event_tx,
        }
    }

    fn restore_session(
        session: Session,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        tui: &Tui,
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
                // Find this group's resolved rect for sizing
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
                            p.title = pane_config.title.clone();
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
            });
        }

        if workspaces.is_empty() {
            // Fallback: create a fresh workspace
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
            event_tx,
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
            AppEvent::Resize(w, h) => {
                self.resize_all_panes(w, h);
            }
            AppEvent::MouseDown { x, y } => {
                if self.mode == Mode::Normal {
                    self.handle_mouse_click(x, y, tui)?;
                }
            }
            AppEvent::Tick => {}
        }
        Ok(())
    }

    fn handle_pty_exited(&mut self, pane_id: PaneId) {
        // Mark the pane as exited
        if let Some(pane) = self.find_pane_mut(pane_id) {
            pane.exited = true;
        }

        // Find which group/workspace this pane is in
        let location = self.find_pane_location(pane_id);
        if let Some((ws_idx, group_id)) = location {
            let ws = &self.workspaces[ws_idx];
            if let Some(group) = ws.groups.get(&group_id) {
                if group.tab_count() <= 1 {
                    // Last tab in group — check if last group
                    let group_ids = ws.layout.group_ids();
                    if group_ids.len() <= 1 && self.workspaces.len() <= 1 {
                        // Last group in last workspace — quit
                        self.should_quit = true;
                        return;
                    }
                    // Close the group from workspace
                    let ws = &mut self.workspaces[ws_idx];
                    if let Some(new_focus) = ws.layout.close_pane(group_id) {
                        ws.groups.remove(&group_id);
                        ws.active_group = new_focus;
                    } else if self.workspaces.len() > 1 {
                        // Only group in this workspace, close the workspace
                        self.workspaces.remove(ws_idx);
                        if self.active_workspace >= self.workspaces.len() {
                            self.active_workspace = self.workspaces.len() - 1;
                        }
                    } else {
                        self.should_quit = true;
                    }
                } else {
                    // Close just this tab
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

    fn handle_mouse_click(&mut self, x: u16, y: u16, tui: &Tui) -> anyhow::Result<()> {
        let size = tui.size()?;
        let body = ratatui::layout::Rect::new(0, 0, size.width, size.height.saturating_sub(1));
        let ws = self.active_workspace();
        let resolved = ws.layout.resolve(body);
        for (group_id, rect) in resolved {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                self.active_workspace_mut().active_group = group_id;
                break;
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        match &self.mode {
            Mode::SessionPicker => return self.handle_session_picker_key(key, tui),
            Mode::NewPane => return self.handle_new_pane_key(key, tui),
            Mode::Help => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::Normal;
                }
                return Ok(());
            }
            Mode::DevServerInput => return self.handle_dev_server_input_key(key, tui),
            Mode::Normal => {}
        }

        let mods = key.modifiers;

        match key.code {
            // Ctrl+q → quit
            KeyCode::Char('q') if mods.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            // Ctrl+n → new tab menu
            KeyCode::Char('n') if mods.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::NewPane;
            }
            // Ctrl+s → session picker
            KeyCode::Char('s') if mods.contains(KeyModifiers::CONTROL) => {
                self.session_list = session::store::list().unwrap_or_default();
                self.session_selected = 0;
                self.mode = Mode::SessionPicker;
            }
            // Ctrl+h → help
            KeyCode::Char('h') if mods.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::Help;
            }
            KeyCode::Char('/') if mods.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::Help;
            }
            KeyCode::Char('?') if mods.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::Help;
            }

            // --- Workspace keys ---
            // Ctrl+t → new workspace
            KeyCode::Char('t') if mods.contains(KeyModifiers::CONTROL) => {
                self.new_workspace(tui)?;
            }
            // Ctrl+Shift+W → close workspace
            KeyCode::Char('W') if mods.contains(KeyModifiers::CONTROL) => {
                self.close_workspace();
            }
            // Ctrl+1..9 → switch workspace
            KeyCode::Char(c @ '1'..='9') if mods.contains(KeyModifiers::CONTROL) => {
                let idx = (c as usize) - ('1' as usize);
                if idx < self.workspaces.len() {
                    self.active_workspace = idx;
                }
            }

            // --- Tab keys ---
            // Ctrl+Tab or Alt+] → next tab
            KeyCode::BackTab if mods.contains(KeyModifiers::CONTROL) => {
                // Ctrl+Shift+Tab → prev tab
                self.active_workspace_mut().active_group_mut().prev_tab();
            }
            KeyCode::Tab if mods.contains(KeyModifiers::CONTROL) => {
                self.active_workspace_mut().active_group_mut().next_tab();
            }
            KeyCode::Char(']') if mods.contains(KeyModifiers::ALT) => {
                self.active_workspace_mut().active_group_mut().next_tab();
            }
            KeyCode::Char('[') if mods.contains(KeyModifiers::ALT) => {
                self.active_workspace_mut().active_group_mut().prev_tab();
            }

            // Ctrl+w → close tab (close group if last tab)
            KeyCode::Char('w') if mods.contains(KeyModifiers::CONTROL) => {
                self.close_active_tab();
            }

            // Ctrl+d → split horizontal (new group)
            KeyCode::Char('d')
                if mods.contains(KeyModifiers::CONTROL)
                    && !mods.contains(KeyModifiers::SHIFT) =>
            {
                self.split_active_group(SplitDirection::Horizontal, PaneKind::Shell, tui)?;
            }
            // Ctrl+Shift+D → split vertical (new group)
            KeyCode::Char('D') if mods.contains(KeyModifiers::CONTROL) => {
                self.split_active_group(SplitDirection::Vertical, PaneKind::Shell, tui)?;
            }
            KeyCode::Char('d')
                if mods.contains(KeyModifiers::CONTROL)
                    && mods.contains(KeyModifiers::SHIFT) =>
            {
                self.split_active_group(SplitDirection::Vertical, PaneKind::Shell, tui)?;
            }

            // Ctrl+r → restart dead pane
            KeyCode::Char('r') if mods.contains(KeyModifiers::CONTROL) => {
                self.restart_active_pane(tui)?;
            }

            // Alt+1..9 → focus group by index
            KeyCode::Char(c @ '1'..='9') if mods.contains(KeyModifiers::ALT) => {
                let idx = (c as usize) - ('1' as usize);
                let ws = self.active_workspace();
                let ids = ws.layout.group_ids();
                if let Some(&id) = ids.get(idx) {
                    self.active_workspace_mut().active_group = id;
                }
            }

            // Alt+h/j/k/l → directional group navigation
            KeyCode::Char('h')
                if mods.contains(KeyModifiers::ALT)
                    && !mods.contains(KeyModifiers::CONTROL) =>
            {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Horizontal,
                    Side::First,
                ) {
                    self.active_workspace_mut().active_group = id;
                }
            }
            KeyCode::Char('l')
                if mods.contains(KeyModifiers::ALT)
                    && !mods.contains(KeyModifiers::CONTROL) =>
            {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Horizontal,
                    Side::Second,
                ) {
                    self.active_workspace_mut().active_group = id;
                }
            }
            KeyCode::Char('k')
                if mods.contains(KeyModifiers::ALT)
                    && !mods.contains(KeyModifiers::CONTROL) =>
            {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::First,
                ) {
                    self.active_workspace_mut().active_group = id;
                }
            }
            KeyCode::Char('j')
                if mods.contains(KeyModifiers::ALT)
                    && !mods.contains(KeyModifiers::CONTROL) =>
            {
                let ws = self.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::Second,
                ) {
                    self.active_workspace_mut().active_group = id;
                }
            }

            // Ctrl+Alt+h/l → resize horizontal
            KeyCode::Char('h')
                if mods.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, -0.05);
            }
            KeyCode::Char('l')
                if mods.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, 0.05);
            }
            // Ctrl+Alt+j/k → resize vertical
            KeyCode::Char('j')
                if mods.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, 0.05);
            }
            KeyCode::Char('k')
                if mods.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let active = self.active_workspace().active_group;
                self.active_workspace_mut().layout.resize(active, -0.05);
            }
            // Ctrl+Alt+= → equalize
            KeyCode::Char('=')
                if mods.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.active_workspace_mut().layout.equalize();
            }

            // Forward everything else to active pane's PTY
            _ => {
                let ws = self.active_workspace_mut();
                let group = ws.groups.get_mut(&ws.active_group);
                if let Some(group) = group {
                    let pane = group.active_pane_mut();
                    let bytes = key_to_bytes(key);
                    if !bytes.is_empty() {
                        pane.write_input(&bytes);
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_session_picker_key(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {
                // Check if we have a real workspace (not the dummy one)
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
                        let restored =
                            App::restore_session(session, self.event_tx.clone(), tui)?;
                        self.workspaces = restored.workspaces;
                        self.active_workspace = restored.active_workspace;
                        self.session_name = restored.session_name;
                        self.session_id = restored.session_id;
                        self.session_created_at = restored.session_created_at;
                        self.mode = Mode::Normal;
                    }
                }
            }
            KeyCode::Char('n') => {
                let name = format!("session-{}", self.session_list.len() + 1);
                let new = App::new_session(name, &self.event_tx, tui)?;
                self.workspaces = new.workspaces;
                self.active_workspace = new.active_workspace;
                self.session_name = new.session_name;
                self.session_id = new.session_id;
                self.session_created_at = new.session_created_at;
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

    fn handle_new_pane_key(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char('a') => {
                self.add_tab_to_active_group(PaneKind::Agent, None, tui)?;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') => {
                self.add_tab_to_active_group(PaneKind::Nvim, None, tui)?;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('s') => {
                self.add_tab_to_active_group(PaneKind::Shell, None, tui)?;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('d') => {
                self.dev_server_input.clear();
                self.mode = Mode::DevServerInput;
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
                // Close the active tab
                group.close_tab(group.active_tab);
                return;
            }
        }

        // Last tab in group — close the group
        let group_ids = ws.layout.group_ids();
        if group_ids.len() <= 1 {
            // Don't close the last group
            return;
        }

        if let Some(new_focus) = ws.layout.close_pane(active_group_id) {
            ws.groups.remove(&active_group_id);
            ws.active_group = new_focus;
        }
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
        let name = format!("{}", self.workspaces.len() + 1);
        let workspace = Workspace::new(name, group_id, group);
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

    fn resize_all_panes(&mut self, w: u16, h: u16) {
        let body_height = h.saturating_sub(1); // footer only
        let size = ratatui::layout::Rect::new(0, 0, w, body_height);

        for ws in &mut self.workspaces {
            let resolved = ws.layout.resolve(size);
            for (group_id, rect) in resolved {
                if let Some(group) = ws.groups.get_mut(&group_id) {
                    let has_tab_bar = group.tab_count() > 1;
                    let tab_bar_offset: u16 = if has_tab_bar { 1 } else { 0 };
                    let cols = rect.width.saturating_sub(4); // borders + padding
                    let rows = rect.height.saturating_sub(2 + tab_bar_offset);
                    if cols > 0 && rows > 0 {
                        for pane in &mut group.tabs {
                            pane.resize_pty(cols, rows);
                        }
                    }
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
        assert_ne!(Mode::SessionPicker, Mode::NewPane);
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
        // Active is now 2 (last added)
        assert_eq!(group.active_pane().id, p3_id);

        group.next_tab();
        assert_eq!(group.active_tab, 0); // wraps around
        assert_eq!(group.active_pane().id, p1_id);

        group.prev_tab();
        assert_eq!(group.active_tab, 2); // wraps backward
        assert_eq!(group.active_pane().id, p3_id);
    }
}
