use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::app::{LeaderState, Mode};
use crate::clipboard;
use crate::config::{self, Action, Config};
use crate::copy_mode::{CopyModeAction, CopyModeState};
use crate::layout::TabId;
use crate::server::daemon;
use crate::server::framing;
use crate::server::protocol::{
    ClientRequest, RenderState, SerializableKeyEvent, ServerResponse, WorkspaceSnapshot,
};
use crate::system_stats::SystemStats;
use crate::tui::Tui;
use crate::ui;
use crate::ui::command_palette::CommandPaletteState;
use crate::ui::help::HelpState;
use crate::ui::tab_picker::TabPickerState;

/// TUI client that connects to a pane daemon via Unix socket.
pub struct Client {
    // Local rendering state (received from server)
    pub mode: Mode,
    pub render_state: RenderState,
    pub screens: HashMap<TabId, vt100::Parser>,
    pub system_stats: SystemStats,
    pub config: Config,
    pub client_count: u32,
    pub plugin_segments: Vec<Vec<crate::plugin::PluginSegment>>,

    // Client-only UI state
    pub leader_state: Option<LeaderState>,
    pub help_state: HelpState,
    pub command_palette_state: Option<CommandPaletteState>,
    pub copy_mode_state: Option<CopyModeState>,
    pub tab_picker_state: Option<TabPickerState>,
    pub should_quit: bool,
}

impl Client {
    pub fn new(config: Config, session_name: &str) -> Self {
        Self {
            mode: Mode::Interact,
            render_state: RenderState {
                workspaces: Vec::new(),
                active_workspace: 0,
                session_name: session_name.to_string(),
            },
            screens: HashMap::new(),
            system_stats: SystemStats::default(),
            config,
            client_count: 1,
            plugin_segments: Vec::new(),

            leader_state: None,
            help_state: HelpState::default(),
            command_palette_state: None,
            copy_mode_state: None,
            tab_picker_state: None,
            should_quit: false,
        }
    }

    /// Connect to a daemon and run the TUI event loop.
    pub async fn run(session_name: &str, config: Config) -> Result<()> {
        let sock = daemon::socket_path(session_name);
        if !sock.exists() {
            anyhow::bail!("no running session '{}'. Start one with: pane daemon {}", session_name, session_name);
        }

        let mut stream = UnixStream::connect(&sock).await?;

        // Attach
        framing::send(&mut stream, &ClientRequest::Attach {
            session_name: session_name.to_string(),
        }).await?;

        // Wait for Attached
        let resp: ServerResponse = framing::recv_required(&mut stream).await?;
        let _session_name = match resp {
            ServerResponse::Attached { session_name } => session_name,
            ServerResponse::Error(e) => anyhow::bail!("server error: {}", e),
            _ => anyhow::bail!("unexpected response: {:?}", resp),
        };

        let mut client = Client::new(config, session_name);

        // Read initial LayoutChanged
        let resp: ServerResponse = framing::recv_required(&mut stream).await?;
        if let ServerResponse::LayoutChanged { render_state } = resp {
            client.apply_layout(render_state);
        }

        // Set up TUI
        let mut tui = Tui::new()?;
        tui.enter()?;

        // Send initial resize
        let size = tui.size()?;
        framing::send(&mut stream, &ClientRequest::Resize {
            width: size.width,
            height: size.height,
        }).await?;

        // Split stream
        let (read_half, write_half) = stream.into_split();
        let writer = Arc::new(Mutex::new(write_half));

        // Event loop
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ServerEvent>();

        // Start terminal event reader — bridge AppEvent → ServerEvent
        let (app_tx, mut app_rx) = tokio::sync::mpsc::unbounded_channel();
        crate::event::start_event_loop(app_tx);
        let term_tx = event_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = app_rx.recv().await {
                if term_tx.send(ServerEvent::Terminal(event)).is_err() {
                    break;
                }
            }
        });

        // Spawn server message reader
        let server_tx = event_tx.clone();
        let server_reader = tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                let mut len_buf = [0u8; 4];
                match reader.read_exact(&mut len_buf).await {
                    Ok(_) => {}
                    Err(_) => break,
                }
                let len = u32::from_be_bytes(len_buf);
                if len > 16 * 1024 * 1024 {
                    break;
                }
                let mut buf = vec![0u8; len as usize];
                if reader.read_exact(&mut buf).await.is_err() {
                    break;
                }
                let response: ServerResponse = match serde_json::from_slice(&buf) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if server_tx.send(ServerEvent::Server(response)).is_err() {
                    break;
                }
            }
            // Server disconnected
            let _ = server_tx.send(ServerEvent::Disconnected);
        });

        loop {
            tui.draw(|frame| ui::render_client(&client, frame))?;

            if let Some(event) = event_rx.recv().await {
                client.handle_event(event, &tui, &writer).await?;
            }
            while let Ok(event) = event_rx.try_recv() {
                client.handle_event(event, &tui, &writer).await?;
                if client.should_quit {
                    break;
                }
            }

            if client.should_quit {
                break;
            }
        }

        // Clean up
        server_reader.abort();
        let mut w = writer.lock().await;
        let _ = send_request(&mut *w, &ClientRequest::Detach).await;

        Ok(())
    }

    fn apply_layout(&mut self, render_state: RenderState) {
        // Reconcile screen map: add new panes, remove dead ones
        let mut live_pane_ids: std::collections::HashSet<TabId> = std::collections::HashSet::new();
        for ws in &render_state.workspaces {
            for group in &ws.groups {
                for pane in &group.tabs {
                    live_pane_ids.insert(pane.id);
                    // Ensure a vt100 parser exists for each pane
                    self.screens.entry(pane.id).or_insert_with(|| {
                        vt100::Parser::new(24, 80, 1000)
                    });
                }
            }
        }
        // Remove screens for panes that no longer exist
        self.screens.retain(|id, _| live_pane_ids.contains(id));
        self.render_state = render_state;
    }

    async fn handle_event(
        &mut self,
        event: ServerEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match event {
            ServerEvent::Terminal(app_event) => {
                self.handle_terminal_event(app_event, tui, writer).await?;
            }
            ServerEvent::Server(response) => {
                self.handle_server_response(response);
            }
            ServerEvent::Disconnected => {
                self.should_quit = true;
            }
        }
        Ok(())
    }

    fn handle_server_response(&mut self, response: ServerResponse) {
        match response {
            ServerResponse::PaneOutput { pane_id, data } => {
                if let Some(parser) = self.screens.get_mut(&pane_id) {
                    parser.process(&data);
                }
            }
            ServerResponse::FullScreenDump { pane_id, data } => {
                if let Some(parser) = self.screens.get_mut(&pane_id) {
                    parser.process(&data);
                }
            }
            ServerResponse::LayoutChanged { render_state } => {
                self.apply_layout(render_state);
                self.update_terminal_title();
            }
            ServerResponse::PaneExited { pane_id } => {
                // Mark locally if needed — the server handles cleanup
                let _ = pane_id;
            }
            ServerResponse::StatsUpdate(stats) => {
                self.system_stats = stats.into();
            }
            ServerResponse::SessionEnded => {
                self.should_quit = true;
            }
            ServerResponse::ClientCountChanged(count) => {
                self.client_count = count;
            }
            ServerResponse::PluginSegments(segments) => {
                self.plugin_segments = segments;
            }
            ServerResponse::Error(_) | ServerResponse::Attached { .. } | ServerResponse::CommandOutput { .. } => {}
        }
    }

    async fn handle_terminal_event(
        &mut self,
        event: crate::event::AppEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        use crate::event::AppEvent;
        match event {
            AppEvent::Key(key) => {
                self.handle_key_event(key, tui, writer).await?;
            }
            AppEvent::Resize(w, h) => {
                let mut w_guard = writer.lock().await;
                let _ = send_request(&mut *w_guard, &ClientRequest::Resize { width: w, height: h }).await;
            }
            AppEvent::MouseDown { x, y } => {
                if self.mode == Mode::Confirm {
                    let size = tui.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    if let Some(click) = ui::confirm_dialog_hit_test(area, x, y) {
                        match click {
                            ui::ConfirmDialogClick::Confirm => {
                                self.mode = Mode::Normal;
                            }
                            ui::ConfirmDialogClick::Cancel => {
                                self.mode = Mode::Normal;
                            }
                        }
                    }
                } else if self.mode == Mode::Normal || self.mode == Mode::Interact || self.mode == Mode::Select {
                    // Check workspace bar clicks (client-side)
                    if y == 0 && !self.render_state.workspaces.is_empty() {
                        let names: Vec<&str> = self.render_state.workspaces.iter().map(|ws| ws.name.as_str()).collect();
                        let bar_area = Rect::new(0, 0, tui.size()?.width, 1);
                        if let Some(click) = crate::ui::workspace_bar::hit_test(
                            &names, self.render_state.active_workspace, bar_area, x, y,
                        ) {
                            match click {
                                crate::ui::workspace_bar::WorkspaceBarClick::Tab(i) => {
                                    let mut w = writer.lock().await;
                                    let _ = send_request(&mut *w, &ClientRequest::Command(
                                        format!("select-workspace -t {}", i),
                                    )).await;
                                }
                                crate::ui::workspace_bar::WorkspaceBarClick::NewWorkspace => {
                                    let mut w = writer.lock().await;
                                    let _ = send_request(&mut *w, &ClientRequest::Command(
                                        "new-session -d -s workspace".to_string(),
                                    )).await;
                                }
                            }
                            return Ok(());
                        }
                    }

                    // Forward mouse to server
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut *w, &ClientRequest::MouseDown { x, y }).await;
                }
            }
            AppEvent::MouseDrag { x, y } => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::MouseDrag { x, y }).await;
            }
            AppEvent::MouseUp => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::MouseUp).await;
            }
            AppEvent::MouseScroll { up } => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::MouseScroll { up }).await;
            }
            AppEvent::MouseMove { x, y } => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::MouseMove { x, y }).await;
            }
            AppEvent::MouseRightDown => {
                // Right-click handled client-side in future
            }
            AppEvent::Tick => {
                if let Some(ref mut ls) = self.leader_state {
                    if !ls.popup_visible {
                        let elapsed = ls.entered_at.elapsed().as_millis() as u64;
                        if elapsed >= self.config.leader.timeout_ms {
                            ls.popup_visible = true;
                        }
                    }
                }
            }
            AppEvent::PtyOutput { .. } | AppEvent::PtyExited { .. } | AppEvent::SystemStats(_) => {
                // These come from the server, not terminal
            }
        }
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        // Modal modes handled client-side
        match &self.mode {
            Mode::Help => return self.handle_help_key(key),
            Mode::Scroll => return self.handle_scroll_key(key, writer).await,
            Mode::Copy => return self.handle_copy_mode_key(key),
            Mode::CommandPalette => return self.handle_command_palette_key(key, tui, writer).await,
            Mode::TabPicker => return self.handle_tab_picker_key(key, writer).await,
            Mode::Confirm => return self.handle_confirm_key(key),
            Mode::Leader => return self.handle_leader_key(key, tui, writer).await,
            Mode::Select => {
                // Falls through to action handling below
            }
            Mode::Normal => return self.handle_normal_key(key, tui, writer).await,
            Mode::Interact => return self.handle_interact_key(key, tui, writer).await,
        }

        // Select mode: check leader key, then select keymap
        let normalized = config::normalize_key(key);
        let leader_key = config::normalize_key(self.config.leader.key);
        if normalized == leader_key {
            self.enter_leader_mode();
            return Ok(());
        }

        if let Some(action) = self.config.select_keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui, writer).await;
        }

        Ok(())
    }

    /// Interact mode: forward ALL keys to PTY except Escape (→ Normal).
    async fn handle_interact_key(
        &mut self,
        key: KeyEvent,
        _tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let normalized = config::normalize_key(key);

        // Escape → Normal mode
        if normalized.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            return Ok(());
        }

        // Check for leader key (allows accessing leader from Interact)
        let leader_key = config::normalize_key(self.config.leader.key);
        if normalized == leader_key {
            self.enter_leader_mode();
            return Ok(());
        }

        // Check global keymap (ctrl+q etc.)
        if let Some(action) = self.config.keys.lookup(&normalized).cloned() {
            return self.execute_action(action, _tui, writer).await;
        }

        // Forward to PTY
        let mut w = writer.lock().await;
        let _ = send_request(&mut *w, &ClientRequest::Key(SerializableKeyEvent::from(key))).await;
        Ok(())
    }

    /// Normal mode: strict vim-style. No PTY fallback.
    async fn handle_normal_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let normalized = config::normalize_key(key);

        // Leader key
        let leader_key = config::normalize_key(self.config.leader.key);
        if normalized == leader_key {
            self.enter_leader_mode();
            return Ok(());
        }

        // Global keymap (ctrl+q, shift+pageup, etc.)
        if let Some(action) = self.config.keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui, writer).await;
        }

        // Normal mode plain-key bindings
        match normalized.code {
            KeyCode::Char('i') if normalized.modifiers == KeyModifiers::NONE => {
                self.mode = Mode::Interact;
            }
            KeyCode::Char('h') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::FocusLeft, tui, writer).await;
            }
            KeyCode::Char('j') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::FocusDown, tui, writer).await;
            }
            KeyCode::Char('k') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::FocusUp, tui, writer).await;
            }
            KeyCode::Char('l') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::FocusRight, tui, writer).await;
            }
            KeyCode::Char('d') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::SplitHorizontal, tui, writer).await;
            }
            KeyCode::Char('D') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::SplitVertical, tui, writer).await;
            }
            KeyCode::Char('x') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::CloseTab, tui, writer).await;
            }
            KeyCode::Char('n') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::NewTab, tui, writer).await;
            }
            KeyCode::Char('m') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::MaximizeFocused, tui, writer).await;
            }
            KeyCode::Char('z') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::ToggleZoom, tui, writer).await;
            }
            KeyCode::Char('f') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::ToggleFloat, tui, writer).await;
            }
            KeyCode::Char('F') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::NewFloat, tui, writer).await;
            }
            KeyCode::Char('s') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::ScrollMode, tui, writer).await;
            }
            KeyCode::Char('c') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::CopyMode, tui, writer).await;
            }
            KeyCode::Char('p') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::PasteClipboard, tui, writer).await;
            }
            KeyCode::Char('/') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::CommandPalette, tui, writer).await;
            }
            KeyCode::Char('?') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::Help, tui, writer).await;
            }
            KeyCode::Char('=') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::Equalize, tui, writer).await;
            }
            KeyCode::Char('H') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::ResizeShrinkH, tui, writer).await;
            }
            KeyCode::Char('L') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::ResizeGrowH, tui, writer).await;
            }
            KeyCode::Char('J') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::ResizeGrowV, tui, writer).await;
            }
            KeyCode::Char('K') if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::ResizeShrinkV, tui, writer).await;
            }
            KeyCode::Tab if normalized.modifiers == KeyModifiers::NONE => {
                return self.execute_action(Action::NextTab, tui, writer).await;
            }
            KeyCode::BackTab => {
                return self.execute_action(Action::PrevTab, tui, writer).await;
            }
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' && normalized.modifiers == KeyModifiers::NONE => {
                let n = c as u8 - b'0';
                return self.execute_action(Action::FocusGroupN(n), tui, writer).await;
            }
            _ => {
                // No PTY fallback in Normal mode — keys are consumed
            }
        }
        Ok(())
    }

    fn enter_leader_mode(&mut self) {
        let root = self.config.leader.root.clone();
        self.leader_state = Some(LeaderState {
            path: Vec::new(),
            current_node: root,
            entered_at: std::time::Instant::now(),
            popup_visible: false,
        });
        self.mode = Mode::Leader;
    }

    async fn execute_action(
        &mut self,
        action: Action,
        _tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        // Client-only actions
        match &action {
            Action::Quit => {
                self.should_quit = true;
                return Ok(());
            }
            Action::Help => {
                self.help_state = HelpState::default();
                self.mode = Mode::Help;
                return Ok(());
            }
            Action::ScrollMode => {
                self.mode = Mode::Scroll;
                return Ok(());
            }
            Action::CopyMode => {
                // Set up copy mode from current screen
                if let Some(ws) = self.active_workspace() {
                    if let Some(group) = ws.groups.iter().find(|g| g.id == ws.active_group) {
                        if let Some(pane) = group.tabs.get(group.active_tab) {
                            if let Some(parser) = self.screens.get(&pane.id) {
                                let screen = parser.screen();
                                let (cursor_row, cursor_col) = screen.cursor_position();
                                let (rows, cols) = screen.size();
                                self.copy_mode_state = Some(CopyModeState::new(
                                    rows as usize,
                                    cols as usize,
                                    cursor_row as usize,
                                    cursor_col as usize,
                                ));
                                self.mode = Mode::Copy;
                            }
                        }
                    }
                }
                return Ok(());
            }
            Action::CommandPalette => {
                self.command_palette_state = Some(CommandPaletteState::new(&self.config.keys));
                self.mode = Mode::CommandPalette;
                return Ok(());
            }
            Action::SelectMode => {
                self.mode = if self.mode == Mode::Select {
                    Mode::Normal
                } else {
                    Mode::Select
                };
                return Ok(());
            }
            Action::EnterInteract => {
                self.mode = Mode::Interact;
                return Ok(());
            }
            Action::EnterNormal => {
                self.mode = Mode::Normal;
                return Ok(());
            }
            Action::Detach => {
                self.should_quit = true;
                return Ok(());
            }
            Action::NewTab => {
                self.tab_picker_state = Some(TabPickerState::new());
                self.mode = Mode::TabPicker;
                return Ok(());
            }
            Action::PasteClipboard => {
                if let Ok(text) = clipboard::paste_from_clipboard() {
                    if !text.is_empty() {
                        let mut w = writer.lock().await;
                        let _ = send_request(&mut *w, &ClientRequest::Command(
                            format!("paste-buffer {}", text),
                        )).await;
                    }
                }
                return Ok(());
            }
            _ => {}
        }

        // Server-mutating actions — translate to commands
        if let Some(cmd) = action_to_command(&action) {
            let mut w = writer.lock().await;
            let _ = send_request(&mut *w, &ClientRequest::Command(cmd)).await;
        }
        Ok(())
    }

    async fn handle_tab_picker_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let state = match self.tab_picker_state.as_mut() {
            Some(s) => s,
            None => {
                self.mode = Mode::Normal;
                return Ok(());
            }
        };

        match key.code {
            KeyCode::Esc => {
                self.tab_picker_state = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                if let Some(cmd) = state.selected_command() {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut *w, &ClientRequest::Command(cmd)).await;
                }
                self.tab_picker_state = None;
                self.mode = Mode::Interact;
            }
            KeyCode::Up => state.move_up(),
            KeyCode::Down => state.move_down(),
            KeyCode::Backspace => {
                state.input.pop();
                state.update_filter();
            }
            KeyCode::Char(c) => {
                state.input.push(c);
                state.update_filter();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                self.mode = Mode::Normal;
            }
            KeyCode::Esc | KeyCode::Char('n') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> Result<()> {
        if let Some(ref mut search) = self.help_state.search_input {
            match key.code {
                KeyCode::Esc => { self.help_state.search_input = None; }
                KeyCode::Backspace => {
                    search.pop();
                    if search.is_empty() {
                        self.help_state.search_input = None;
                    }
                }
                KeyCode::Char(c) => { search.push(c); }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Esc => { self.mode = Mode::Normal; }
                KeyCode::Char('j') | KeyCode::Down => { self.help_state.scroll_offset += 1; }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.help_state.scroll_offset = self.help_state.scroll_offset.saturating_sub(1);
                }
                KeyCode::Char('/') => { self.help_state.search_input = Some(String::new()); }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_copy_mode_key(&mut self, key: KeyEvent) -> Result<()> {
        // Get the pane_id for the active pane so we can borrow screen and cms separately
        let pane_id = self.active_workspace()
            .and_then(|ws| ws.groups.iter().find(|g| g.id == ws.active_group))
            .and_then(|g| g.tabs.get(g.active_tab))
            .map(|p| p.id);

        let pane_id = match pane_id {
            Some(id) => id,
            None => {
                self.mode = Mode::Normal;
                self.copy_mode_state = None;
                return Ok(());
            }
        };

        let screen = match self.screens.get(&pane_id) {
            Some(parser) => parser.screen(),
            None => {
                self.mode = Mode::Normal;
                self.copy_mode_state = None;
                return Ok(());
            }
        };

        if let Some(ref mut cms) = self.copy_mode_state {
            match cms.handle_key(key, screen) {
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
        }
        Ok(())
    }

    async fn handle_scroll_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('G') | KeyCode::End => {
                self.mode = Mode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::MouseScroll { up: true }).await;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::MouseScroll { up: false }).await;
            }
            KeyCode::PageUp | KeyCode::Char('u') => {
                for _ in 0..10 {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut *w, &ClientRequest::MouseScroll { up: true }).await;
                }
            }
            KeyCode::PageDown | KeyCode::Char('d') => {
                for _ in 0..10 {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut *w, &ClientRequest::MouseScroll { up: false }).await;
                }
            }
            _ => {
                self.mode = Mode::Normal;
                // Forward the key to PTY
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::Key(SerializableKeyEvent::from(key))).await;
            }
        }
        Ok(())
    }

    async fn handle_command_palette_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
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
                        return self.execute_action(action, tui, writer).await;
                    }
                }
                KeyCode::Up => cp.move_up(),
                KeyCode::Down => cp.move_down(),
                KeyCode::Backspace => { cp.input.pop(); cp.update_filter(); }
                KeyCode::Char(c) => { cp.input.push(c); cp.update_filter(); }
                _ => {}
            }
        } else {
            self.mode = Mode::Normal;
        }
        Ok(())
    }

    async fn handle_leader_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        use crate::config::LeaderNode;

        if key.code == KeyCode::Esc {
            self.leader_state = None;
            self.mode = Mode::Normal;
            return Ok(());
        }

        let normalized = config::normalize_key(key);
        let next = {
            let ls = self.leader_state.as_ref().unwrap();
            if let LeaderNode::Group { children, .. } = &ls.current_node {
                children.get(&normalized).cloned()
            } else {
                None
            }
        };

        match next {
            Some(LeaderNode::Leaf { action, .. }) => {
                self.leader_state = None;
                self.mode = Mode::Normal;
                return self.execute_action(action, tui, writer).await;
            }
            Some(LeaderNode::PassThrough) => {
                self.leader_state = None;
                self.mode = Mode::Normal;
                let leader_key = self.config.leader.key;
                let mut w = writer.lock().await;
                let _ = send_request(&mut *w, &ClientRequest::Key(SerializableKeyEvent::from(leader_key))).await;
            }
            Some(group @ LeaderNode::Group { .. }) => {
                let ls = self.leader_state.as_mut().unwrap();
                ls.path.push(normalized);
                ls.current_node = group;
                ls.entered_at = std::time::Instant::now();
                ls.popup_visible = false;
            }
            None => {
                self.leader_state = None;
                self.mode = Mode::Normal;
            }
        }
        Ok(())
    }

    // --- Accessors for UI rendering ---

    pub fn active_workspace(&self) -> Option<&WorkspaceSnapshot> {
        self.render_state.workspaces.get(self.render_state.active_workspace)
    }

    pub fn pane_screen(&self, pane_id: TabId) -> Option<&vt100::Screen> {
        self.screens.get(&pane_id).map(|p| p.screen())
    }

    fn update_terminal_title(&self) {
        if let Some(ref fmt) = self.config.behavior.terminal_title_format {
            let session = &self.render_state.session_name;
            let workspace = self.active_workspace()
                .map(|ws| ws.name.as_str())
                .unwrap_or("");
            let title = fmt
                .replace("{session}", session)
                .replace("{workspace}", workspace);
            // OSC 0 - set terminal title
            print!("\x1b]0;{}\x07", title);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }
}

/// Events fed into the client event loop.
enum ServerEvent {
    Terminal(crate::event::AppEvent),
    Server(ServerResponse),
    Disconnected,
}

// Implement From so the event loop channel works
impl From<crate::event::AppEvent> for ServerEvent {
    fn from(e: crate::event::AppEvent) -> Self {
        ServerEvent::Terminal(e)
    }
}

/// Send a client request using length-prefixed framing on the write half.
async fn send_request(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    request: &ClientRequest,
) -> Result<()> {
    let json = serde_json::to_vec(request)?;
    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    Ok(())
}

/// Translate an Action to a server command string.
fn action_to_command(action: &Action) -> Option<String> {
    match action {
        Action::NewWorkspace => Some("new-session -d -s workspace".to_string()),
        Action::CloseWorkspace => Some("close-workspace".to_string()),
        Action::SwitchWorkspace(n) => Some(format!("select-workspace -t {}", (*n as usize) - 1)),
        // Action::NewTab is handled client-side (opens picker)
        Action::NextTab => Some("next-window".to_string()),
        Action::PrevTab => Some("previous-window".to_string()),
        Action::CloseTab => Some("kill-pane".to_string()),
        Action::SplitHorizontal => Some("split-window -h".to_string()),
        Action::SplitVertical => Some("split-window -v".to_string()),
        Action::RestartPane => Some("restart-pane".to_string()),
        Action::FocusLeft => Some("select-pane -L".to_string()),
        Action::FocusDown => Some("select-pane -D".to_string()),
        Action::FocusUp => Some("select-pane -U".to_string()),
        Action::FocusRight => Some("select-pane -R".to_string()),
        Action::MoveTabLeft => Some("move-tab -L".to_string()),
        Action::MoveTabDown => Some("move-tab -D".to_string()),
        Action::MoveTabUp => Some("move-tab -U".to_string()),
        Action::MoveTabRight => Some("move-tab -R".to_string()),
        Action::ResizeShrinkH => Some("resize-pane -L".to_string()),
        Action::ResizeGrowH => Some("resize-pane -R".to_string()),
        Action::ResizeGrowV => Some("resize-pane -D".to_string()),
        Action::ResizeShrinkV => Some("resize-pane -U".to_string()),
        Action::Equalize => Some("equalize-layout".to_string()),
        Action::ToggleSyncPanes => Some("toggle-sync".to_string()),
        Action::SelectLayout(name) => Some(format!("select-layout {}", name)),
        Action::FocusGroupN(n) => {
            let ws_idx = (*n as usize) - 1;
            Some(format!("select-window -t {}", ws_idx))
        }
        Action::DevServerInput => Some("new-window".to_string()),
        Action::MaximizeFocused => Some("maximize-focused".to_string()),
        Action::ToggleZoom => Some("toggle-zoom".to_string()),
        Action::ToggleFloat => Some("toggle-float".to_string()),
        Action::NewFloat => Some("new-float".to_string()),
        Action::RenameWindow | Action::RenamePane => None,
        // Client-only actions handled before this function is called
        Action::Quit | Action::Help | Action::ScrollMode | Action::CopyMode
        | Action::CommandPalette | Action::PasteClipboard | Action::SelectMode
        | Action::EnterInteract | Action::EnterNormal
        | Action::Detach | Action::SessionPicker
        | Action::NewTab => None,  // NewTab opens picker client-side
    }
}
