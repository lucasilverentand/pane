use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::clipboard;
use crate::config::{self, Action, Config};
use crate::copy_mode::{CopyModeAction, CopyModeState};
use crate::event::{self, AppEvent};
use crate::layout::{PaneId, ResolvedPane, Side, SplitDirection};
use crate::layout_presets::LayoutPreset;
use crate::pane::{PaneGroupId, PaneKind};
use crate::server::state::ServerState;
use crate::session::store::SessionSummary;
use crate::session::{self, Session};
use crate::system_stats;
use crate::tui::Tui;
use crate::ui;
use crate::ui::command_palette::CommandPaletteState;
use crate::ui::help::HelpState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Select,
    Scroll,
    SessionPicker,
    Help,
    DevServerInput,
    Copy,
    CommandPalette,
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
    pub state: ServerState,
    pub session_list: Vec<SessionSummary>,
    pub session_selected: usize,
    pub dev_server_input: String,
    pub command_palette_state: Option<CommandPaletteState>,
    pub help_state: HelpState,
    pub copy_mode_state: Option<CopyModeState>,
    drag_state: Option<DragState>,
}

impl App {
    // --- Delegation accessors for UI compatibility ---

    pub fn active_workspace(&self) -> &crate::workspace::Workspace {
        self.state.active_workspace()
    }

    pub fn run_with_args(args: CliArgs, config: Config) -> anyhow::Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { Self::run_async(args, config).await })
    }

    async fn run_async(args: CliArgs, config: Config) -> anyhow::Result<()> {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        event::start_event_loop(event_tx.clone());

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
                if !app.state.workspaces.is_empty() {
                    let session = Session::from_state(&app.state);
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
        let size = tui.size()?;
        let cols = size.width.saturating_sub(2);
        let rows = size.height.saturating_sub(3);
        let state = ServerState::new_session(name, event_tx, cols, rows, config)?;

        Ok(Self {
            should_quit: false,
            mode: Mode::Normal,
            state,
            session_list: Vec::new(),
            session_selected: 0,
            dev_server_input: String::new(),
            command_palette_state: None,
            help_state: HelpState::default(),
            copy_mode_state: None,
            drag_state: None,
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
            state: ServerState {
                workspaces: Vec::new(),
                active_workspace: 0,
                session_name: String::new(),
                session_id: uuid::Uuid::new_v4(),
                session_created_at: chrono::Utc::now(),
                config,
                system_stats: crate::system_stats::SystemStats::default(),
                event_tx,
                last_size: (0, 0),
                next_pane_number: 0,
            },
            session_list: sessions,
            session_selected: 0,
            dev_server_input: String::new(),
            command_palette_state: None,
            help_state: HelpState::default(),
            copy_mode_state: None,
            drag_state: None,
        }
    }

    fn restore_session(
        session: Session,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        tui: &Tui,
        config: Config,
    ) -> anyhow::Result<Self> {
        let size = tui.size()?;
        let state =
            ServerState::restore_session(session, event_tx, size.width, size.height, config)?;

        Ok(Self {
            should_quit: false,
            mode: Mode::Normal,
            state,
            session_list: Vec::new(),
            session_selected: 0,
            dev_server_input: String::new(),
            command_palette_state: None,
            copy_mode_state: None,
            help_state: HelpState::default(),
            drag_state: None,
        })
    }

    fn handle_event(&mut self, event: AppEvent, tui: &Tui) -> anyhow::Result<()> {
        match event {
            AppEvent::Key(key) => self.handle_key_event(key, tui)?,
            AppEvent::PtyOutput { pane_id, bytes } => {
                if let Some(pane) = self.state.find_pane_mut(pane_id) {
                    pane.process_output(&bytes);
                }
            }
            AppEvent::PtyExited { pane_id } => {
                if self.state.handle_pty_exited(pane_id) {
                    self.should_quit = true;
                }
            }
            AppEvent::MouseScroll { up } => {
                if self.mode == Mode::Normal || self.mode == Mode::Select || self.mode == Mode::Scroll {
                    if up {
                        self.state.scroll_active_pane(|p| p.scroll_up(3));
                        if self.state.is_active_pane_scrolled() {
                            self.mode = Mode::Scroll;
                        }
                    } else {
                        self.state.scroll_active_pane(|p| p.scroll_down(3));
                        if !self.state.is_active_pane_scrolled() {
                            self.mode = Mode::Normal;
                        }
                    }
                }
            }
            AppEvent::Resize(w, h) => {
                self.state.last_size = (w, h);
                self.state.resize_all_panes(w, h);
                if self.mode == Mode::Scroll {
                    self.mode = Mode::Normal;
                }
            }
            AppEvent::MouseDown { x, y } => {
                if self.mode == Mode::Normal || self.mode == Mode::Select {
                    self.handle_mouse_down(x, y, tui)?;
                }
            }
            AppEvent::MouseRightDown { x, y } => {
                if self.mode == Mode::Normal || self.mode == Mode::Select {
                    self.handle_mouse_right_down(x, y, tui)?;
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
                self.state.system_stats = stats;
            }
            AppEvent::Tick => {}
        }
        Ok(())
    }

    fn handle_mouse_down(&mut self, x: u16, y: u16, tui: &Tui) -> anyhow::Result<()> {
        let size = tui.size()?;
        let bar_h = self.state.workspace_bar_height();

        if y < bar_h {
            let names: Vec<String> = self
                .state
                .workspaces
                .iter()
                .map(|ws| ws.name.clone())
                .collect();
            let bar_area = ratatui::layout::Rect::new(0, 0, size.width, bar_h);
            if let Some(click) = crate::ui::workspace_bar::hit_test(
                &names,
                self.state.active_workspace,
                bar_area,
                x,
                y,
            ) {
                match click {
                    crate::ui::workspace_bar::WorkspaceBarClick::Tab(i) => {
                        if i < self.state.workspaces.len() {
                            self.state.active_workspace = i;
                        }
                    }
                    crate::ui::workspace_bar::WorkspaceBarClick::NewWorkspace => {
                        let cols = size.width.saturating_sub(2);
                        let rows = size.height.saturating_sub(3);
                        self.state.new_workspace(cols, rows)?;
                    }
                }
            }
            return Ok(());
        }

        let body_height = size.height.saturating_sub(1 + bar_h);
        let body = ratatui::layout::Rect::new(0, bar_h, size.width, body_height);

        let params = crate::layout::LayoutParams::from(&self.state.config.behavior);
        let ws = self.state.active_workspace();
        let resolved = ws
            .layout
            .resolve_with_fold(body, params, &ws.leaf_min_sizes);
        for rp in &resolved {
            if let ResolvedPane::Folded {
                id: group_id,
                rect,
                ..
            } = rp
            {
                if rect.width == 0 || rect.height == 0 {
                    continue;
                }
                if x >= rect.x
                    && x < rect.x + rect.width
                    && y >= rect.y
                    && y < rect.y + rect.height
                {
                    let group_id = *group_id;
                    self.state.active_workspace_mut().leaf_min_sizes.clear();
                    self.state
                        .active_workspace_mut()
                        .layout
                        .unfold_towards(group_id);
                    self.state.active_workspace_mut().active_group = group_id;
                    self.state.resize_all_panes(size.width, size.height);
                    return Ok(());
                }
            }
        }

        let ws = self.state.active_workspace();
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

        for rp in &resolved {
            if let ResolvedPane::Visible {
                id: group_id, rect, ..
            } = rp
            {
                if x >= rect.x
                    && x < rect.x + rect.width
                    && y >= rect.y
                    && y < rect.y + rect.height
                {
                    // Check tab bar click before focusing the group
                    let ws = self.state.active_workspace();
                    if let Some(group) = ws.groups.get(group_id) {
                        if let Some(tab_area) =
                            crate::ui::pane_view::tab_bar_area(group, *rect)
                        {
                            let layout = crate::ui::pane_view::tab_bar_layout(
                                group,
                                &self.state.config.theme,
                                tab_area,
                            );
                            if let Some(click) =
                                crate::ui::pane_view::tab_bar_hit_test(&layout, x, y)
                            {
                                self.state.active_workspace_mut().active_group = *group_id;
                                match click {
                                    crate::ui::pane_view::TabBarClick::Tab(i) => {
                                        self.state
                                            .active_workspace_mut()
                                            .active_group_mut()
                                            .active_tab = i;
                                    }
                                    crate::ui::pane_view::TabBarClick::NewTab => {
                                        let (w, h) = (size.width, size.height);
                                        let cols = w.saturating_sub(4);
                                        let rows = h.saturating_sub(3);
                                        self.state.add_tab_to_active_group(
                                            PaneKind::Shell,
                                            None,
                                            cols,
                                            rows,
                                        )?;
                                    }
                                }
                                return Ok(());
                            }
                        }
                    }
                    self.state.active_workspace_mut().active_group = *group_id;
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

            self.state
                .active_workspace_mut()
                .layout
                .resize(pane_id, delta_ratio);

            if let Some(drag) = &mut self.drag_state {
                drag.start_pos = current_pos;
            }

            self.state.update_leaf_mins();
        }
    }

    fn handle_mouse_move(&mut self, _x: u16, _y: u16) {}

    fn handle_mouse_right_down(&mut self, x: u16, y: u16, tui: &Tui) -> anyhow::Result<()> {
        let size = tui.size()?;
        let bar_h = self.state.workspace_bar_height();

        if y < bar_h {
            let names: Vec<String> = self
                .state
                .workspaces
                .iter()
                .map(|ws| ws.name.clone())
                .collect();
            let bar_area = ratatui::layout::Rect::new(0, 0, size.width, bar_h);
            if let Some(crate::ui::workspace_bar::WorkspaceBarClick::Tab(i)) =
                crate::ui::workspace_bar::hit_test(
                    &names,
                    self.state.active_workspace,
                    bar_area,
                    x,
                    y,
                )
            {
                if self.state.workspaces.len() > 1 && i < self.state.workspaces.len() {
                    self.state.workspaces.remove(i);
                    if self.state.active_workspace >= self.state.workspaces.len() {
                        self.state.active_workspace = self.state.workspaces.len() - 1;
                    }
                }
            }
        }
        Ok(())
    }

    fn focus_group(&mut self, id: PaneGroupId) {
        let bar_h = self.state.workspace_bar_height();
        self.state.focus_group(id, bar_h);
    }

    fn handle_key_event(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        match &self.mode {
            Mode::SessionPicker => return self.handle_session_picker_key(key, tui),
            Mode::Help => return self.handle_help_key(key),
            Mode::DevServerInput => return self.handle_dev_server_input_key(key, tui),
            Mode::Scroll => return self.handle_scroll_key(key),
            Mode::Copy => {
                return self.handle_copy_mode_key(key);
            }
            Mode::CommandPalette => return self.handle_command_palette_key(key, tui),
            Mode::Select => return self.handle_select_key(key, tui),
            Mode::Normal => {}
        }

        let normalized = config::normalize_key(key);
        if let Some(action) = self.state.config.keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui);
        }

        let bytes = key_to_bytes(key);
        if !bytes.is_empty() {
            let ws = self.state.active_workspace_mut();
            if ws.sync_panes {
                // Broadcast to all panes in workspace
                for group in ws.groups.values_mut() {
                    for pane in &mut group.tabs {
                        pane.write_input(&bytes);
                    }
                }
            } else if let Some(group) = ws.groups.get_mut(&ws.active_group) {
                group.active_pane_mut().write_input(&bytes);
            }
        }

        Ok(())
    }

    fn execute_action(&mut self, action: Action, tui: &Tui) -> anyhow::Result<()> {
        let size = || -> anyhow::Result<(u16, u16)> {
            let s = tui.size()?;
            Ok((s.width, s.height))
        };

        match action {
            Action::Quit => {
                self.should_quit = true;
            }
            Action::NewWorkspace => {
                let (w, h) = size()?;
                let cols = w.saturating_sub(2);
                let rows = h.saturating_sub(3);
                self.state.new_workspace(cols, rows)?;
            }
            Action::CloseWorkspace => {
                self.state.close_workspace();
            }
            Action::SwitchWorkspace(n) => {
                let idx = (n as usize) - 1;
                if idx < self.state.workspaces.len() {
                    self.state.active_workspace = idx;
                }
            }
            Action::NewTab => {
                let (w, h) = size()?;
                let cols = w.saturating_sub(4);
                let rows = h.saturating_sub(3);
                self.state
                    .add_tab_to_active_group(PaneKind::Shell, None, cols, rows)?;
            }
            Action::DevServerInput => {
                self.dev_server_input.clear();
                self.mode = Mode::DevServerInput;
            }
            Action::NextTab => {
                self.state.active_workspace_mut().active_group_mut().next_tab();
            }
            Action::PrevTab => {
                self.state.active_workspace_mut().active_group_mut().prev_tab();
            }
            Action::CloseTab => {
                self.state.close_active_tab();
            }
            Action::SplitHorizontal => {
                let (w, h) = size()?;
                let cols = w.saturating_sub(4) / 2;
                let rows = h.saturating_sub(3);
                self.state
                    .split_active_group(SplitDirection::Horizontal, PaneKind::Shell, cols, rows)?;
            }
            Action::SplitVertical => {
                let (w, h) = size()?;
                let cols = w.saturating_sub(4) / 2;
                let rows = h.saturating_sub(3);
                self.state
                    .split_active_group(SplitDirection::Vertical, PaneKind::Shell, cols, rows)?;
            }
            Action::RestartPane => {
                let (w, h) = size()?;
                let cols = w.saturating_sub(4);
                let rows = h.saturating_sub(3);
                self.state.restart_active_pane(cols, rows)?;
            }
            Action::FocusLeft => {
                if self.mode != Mode::Select && self.vim_forward_if_active(KeyCode::Char('h'), KeyModifiers::ALT) {
                    return Ok(());
                }
                let ws = self.state.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Horizontal,
                    Side::First,
                ) {
                    self.focus_group(id);
                }
            }
            Action::FocusDown => {
                if self.mode != Mode::Select && self.vim_forward_if_active(KeyCode::Char('j'), KeyModifiers::ALT) {
                    return Ok(());
                }
                let ws = self.state.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::Second,
                ) {
                    self.focus_group(id);
                }
            }
            Action::FocusUp => {
                if self.mode != Mode::Select && self.vim_forward_if_active(KeyCode::Char('k'), KeyModifiers::ALT) {
                    return Ok(());
                }
                let ws = self.state.active_workspace();
                if let Some(id) = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::First,
                ) {
                    self.focus_group(id);
                }
            }
            Action::FocusRight => {
                if self.mode != Mode::Select && self.vim_forward_if_active(KeyCode::Char('l'), KeyModifiers::ALT) {
                    return Ok(());
                }
                let ws = self.state.active_workspace();
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
                let ws = self.state.active_workspace();
                let ids = ws.layout.group_ids();
                if let Some(&id) = ids.get(idx) {
                    self.focus_group(id);
                }
            }
            Action::MoveTabLeft => {
                self.state
                    .move_tab_to_neighbor(SplitDirection::Horizontal, Side::First);
            }
            Action::MoveTabDown => {
                self.state
                    .move_tab_to_neighbor(SplitDirection::Vertical, Side::Second);
            }
            Action::MoveTabUp => {
                self.state
                    .move_tab_to_neighbor(SplitDirection::Vertical, Side::First);
            }
            Action::MoveTabRight => {
                self.state
                    .move_tab_to_neighbor(SplitDirection::Horizontal, Side::Second);
            }
            Action::ResizeShrinkH => {
                let active = self.state.active_workspace().active_group;
                self.state.active_workspace_mut().layout.resize(active, -0.05);
                self.state.update_leaf_mins();
            }
            Action::ResizeGrowH => {
                let active = self.state.active_workspace().active_group;
                self.state.active_workspace_mut().layout.resize(active, 0.05);
                self.state.update_leaf_mins();
            }
            Action::ResizeGrowV => {
                let active = self.state.active_workspace().active_group;
                self.state.active_workspace_mut().layout.resize(active, 0.05);
                self.state.update_leaf_mins();
            }
            Action::ResizeShrinkV => {
                let active = self.state.active_workspace().active_group;
                self.state
                    .active_workspace_mut()
                    .layout
                    .resize(active, -0.05);
                self.state.update_leaf_mins();
            }
            Action::Equalize => {
                self.state.active_workspace_mut().layout.equalize();
                self.state.active_workspace_mut().leaf_min_sizes.clear();
            }
            Action::SessionPicker => {
                self.session_list = session::store::list().unwrap_or_default();
                self.session_selected = 0;
                self.mode = Mode::SessionPicker;
            }
            Action::Help => {
                self.help_state = HelpState::default();
                self.mode = Mode::Help;
            }
            Action::ScrollMode => {
                let rows = self.state.active_pane_screen_rows();
                self.state.scroll_active_pane(|p| p.scroll_up(rows / 2));
                if self.state.is_active_pane_scrolled() {
                    self.mode = Mode::Scroll;
                }
            }
            Action::CopyMode => {
                let ws = self.state.active_workspace();
                if let Some(group) = ws.groups.get(&ws.active_group) {
                    let pane = group.active_pane();
                    let (cursor_row, cursor_col) = pane.screen().cursor_position();
                    let (rows, cols) = pane.screen().size();
                    self.copy_mode_state = Some(CopyModeState::new(
                        rows as usize,
                        cols as usize,
                        cursor_row as usize,
                        cursor_col as usize,
                    ));
                    self.mode = Mode::Copy;
                }
            }
            Action::CommandPalette => {
                self.command_palette_state =
                    Some(CommandPaletteState::new(&self.state.config.keys));
                self.mode = Mode::CommandPalette;
            }
            Action::PasteClipboard => {
                if let Ok(text) = clipboard::paste_from_clipboard() {
                    let bytes = text.into_bytes();
                    if !bytes.is_empty() {
                        let ws = self.state.active_workspace_mut();
                        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
                            group.active_pane_mut().write_input(&bytes);
                        }
                    }
                }
            }
            Action::SelectLayout(name) => {
                if let Some(preset) = LayoutPreset::from_name(&name) {
                    let ws = self.state.active_workspace_mut();
                    let group_ids = ws.layout.group_ids();
                    if !group_ids.is_empty() {
                        ws.layout = preset.build(&group_ids);
                        let (w, h) = self.state.last_size;
                        self.state.resize_all_panes(w, h);
                    }
                }
            }
            Action::ToggleSyncPanes => {
                let ws = self.state.active_workspace_mut();
                ws.sync_panes = !ws.sync_panes;
            }
            Action::RenameWindow => {
                // TODO: needs an input mode to collect the new name
            }
            Action::SelectMode => {
                self.mode = if self.mode == Mode::Select {
                    Mode::Normal
                } else {
                    Mode::Select
                };
            }
            Action::RenamePane | Action::Detach => {
                // Will be implemented in later phases
            }
        }
        Ok(())
    }

    fn handle_copy_mode_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let screen = {
            let ws = self.state.active_workspace();
            ws.groups
                .get(&ws.active_group)
                .map(|g| g.active_pane().screen().clone())
        };
        let screen = match screen {
            Some(s) => s,
            None => {
                self.mode = Mode::Normal;
                self.copy_mode_state = None;
                return Ok(());
            }
        };

        if let Some(ref mut cms) = self.copy_mode_state {
            match cms.handle_key(key, &screen) {
                CopyModeAction::None => {}
                CopyModeAction::YankSelection(text) => {
                    let _ = clipboard::copy_to_clipboard(&text);
                    self.copy_mode_state = None;
                    self.mode = Mode::Normal;
                }
                CopyModeAction::Exit => {
                    self.copy_mode_state = None;
                    self.mode = Mode::Normal;
                }
            }
        } else {
            self.mode = Mode::Normal;
        }
        Ok(())
    }

    fn handle_scroll_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let mods = key.modifiers;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if mods.is_empty() => {
                self.state.scroll_active_pane(|p| p.scroll_up(1));
            }
            KeyCode::Down | KeyCode::Char('j') if mods.is_empty() => {
                self.state.scroll_active_pane(|p| p.scroll_down(1));
            }
            KeyCode::PageUp => {
                let rows = self.state.active_pane_screen_rows();
                self.state.scroll_active_pane(|p| p.scroll_up(rows / 2));
            }
            KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                let rows = self.state.active_pane_screen_rows();
                self.state.scroll_active_pane(|p| p.scroll_up(rows / 2));
            }
            KeyCode::PageDown => {
                let rows = self.state.active_pane_screen_rows();
                self.state.scroll_active_pane(|p| p.scroll_down(rows / 2));
            }
            KeyCode::Char('d') if mods.contains(KeyModifiers::CONTROL) => {
                let rows = self.state.active_pane_screen_rows();
                self.state.scroll_active_pane(|p| p.scroll_down(rows / 2));
            }
            KeyCode::Char('g') if mods.is_empty() => {
                self.state.scroll_active_pane(|p| p.scroll_to_top());
            }
            KeyCode::Home => {
                self.state.scroll_active_pane(|p| p.scroll_to_top());
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.exit_scroll_mode();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.exit_scroll_mode();
            }
            _ => {
                self.exit_scroll_mode();
                let ws = self.state.active_workspace_mut();
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
        if !self.state.is_active_pane_scrolled() {
            self.mode = Mode::Normal;
        }
        Ok(())
    }

    fn exit_scroll_mode(&mut self) {
        self.state.scroll_active_pane(|p| p.scroll_to_bottom());
        self.mode = Mode::Normal;
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        if let Some(ref mut search) = self.help_state.search_input {
            // In search mode within help
            match key.code {
                KeyCode::Esc => {
                    self.help_state.search_input = None;
                }
                KeyCode::Backspace => {
                    search.pop();
                    if search.is_empty() {
                        self.help_state.search_input = None;
                    }
                }
                KeyCode::Char(c) => {
                    search.push(c);
                }
                KeyCode::Enter => {
                    // Stay in search mode, just close the search input
                    // (results remain filtered)
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.help_state.scroll_offset += 1;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.help_state.scroll_offset =
                        self.help_state.scroll_offset.saturating_sub(1);
                }
                KeyCode::Char('/') => {
                    self.help_state.search_input = Some(String::new());
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_command_palette_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
    ) -> anyhow::Result<()> {
        if let Some(ref mut cp) = self.command_palette_state {
            match key.code {
                KeyCode::Esc => {
                    self.command_palette_state = None;
                    self.mode = Mode::Normal;
                }
                KeyCode::Enter => {
                    if let Some(action) = cp.selected_action() {
                        self.command_palette_state = None;
                        self.mode = Mode::Normal;
                        return self.execute_action(action, tui);
                    }
                }
                KeyCode::Up => {
                    cp.move_up();
                }
                KeyCode::Down => {
                    cp.move_down();
                }
                KeyCode::Backspace => {
                    cp.input.pop();
                    cp.update_filter();
                }
                KeyCode::Char(c) => {
                    cp.input.push(c);
                    cp.update_filter();
                }
                _ => {}
            }
        } else {
            // No command palette state, cancel
            self.mode = Mode::Normal;
        }
        Ok(())
    }

    fn handle_select_key(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        let normalized = config::normalize_key(key);

        // Check normal keymap first (for Ctrl+Space toggle back)
        if let Some(action) = self.state.config.keys.lookup(&normalized).cloned() {
            if action == Action::SelectMode {
                self.mode = Mode::Normal;
                return Ok(());
            }
        }

        // Check select keymap
        if let Some(action) = self.state.config.select_keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui);
        }

        // Unbound keys silently ignored
        Ok(())
    }

    /// If vim_navigator is enabled and the active pane is running vim/nvim,
    /// forward the key press to the PTY and return true.
    fn vim_forward_if_active(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if !self.state.config.behavior.vim_navigator {
            return false;
        }
        let ws = self.state.active_workspace();
        let title = ws
            .groups
            .get(&ws.active_group)
            .map(|g| g.active_pane().title.to_lowercase())
            .unwrap_or_default();

        if !title.contains("vim") && !title.contains("nvim") {
            return false;
        }

        let key = KeyEvent::new(code, mods);
        let bytes = key_to_bytes(key);
        if bytes.is_empty() {
            return false;
        }

        let ws = self.state.active_workspace_mut();
        if let Some(group) = ws.groups.get_mut(&ws.active_group) {
            group.active_pane_mut().write_input(&bytes);
        }
        true
    }

    fn handle_session_picker_key(&mut self, key: KeyEvent, tui: &Tui) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {
                let has_real_panes = self.state.workspaces.iter().any(|ws| {
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
                        let config = self.state.config.clone();
                        let restored =
                            App::restore_session(session, self.state.event_tx.clone(), tui, config)?;
                        self.state = restored.state;
                        self.mode = Mode::Normal;
                    }
                }
            }
            KeyCode::Char('n') => {
                let name = format!("session-{}", self.session_list.len() + 1);
                let config = self.state.config.clone();
                let new = App::new_session(name, &self.state.event_tx, tui, config)?;
                self.state = new.state;
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
                    let size = tui.size()?;
                    let cols = size.width.saturating_sub(4);
                    let rows = size.height.saturating_sub(3);
                    self.state
                        .add_tab_to_active_group(PaneKind::DevServer, Some(cmd), cols, rows)?;
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
}

pub enum CliArgs {
    Default,
    New(String),
    Attach(String),
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
    use crate::pane::Pane;
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
    fn test_new_modes_exist() {
        assert_ne!(Mode::Copy, Mode::Normal);
        assert_ne!(Mode::CommandPalette, Mode::Normal);
        assert_ne!(Mode::Copy, Mode::CommandPalette);
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
        let group = crate::pane::PaneGroup::new(gid, pane);
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
        let mut group = crate::pane::PaneGroup::new(gid, p1);
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
        let mut group = crate::pane::PaneGroup::new(gid, p1);
        group.add_tab(p2);
        assert!(group.close_tab(1));
        assert_eq!(group.tab_count(), 1);
        assert_eq!(group.active_tab, 0);
    }

    #[test]
    fn test_pane_group_close_last_tab_fails() {
        let gid = PaneGroupId::new_v4();
        let p1 = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "t1");
        let mut group = crate::pane::PaneGroup::new(gid, p1);
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
        let mut group = crate::pane::PaneGroup::new(gid, p1);
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
