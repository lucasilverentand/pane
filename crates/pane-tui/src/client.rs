use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use pane_protocol::app::{LeaderState, Mode};
use crate::clipboard;
use pane_protocol::config::{self, Action, Config};
use crate::copy_mode::{CopyModeAction, CopyModeState};
use pane_protocol::layout::{Side, SplitDirection, TabId};
use pane_daemon::server::daemon;
use pane_protocol::framing;
use pane_protocol::protocol::{
    ClientRequest, RenderState, SerializableKeyEvent, ServerResponse, WorkspaceSnapshot,
};
use pane_protocol::system_stats::SystemStats;
use crate::tui::Tui;
use crate::ui;
use crate::ui::context_menu::ContextMenuState;
use crate::ui::palette::UnifiedPaletteState;
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
    pub plugin_segments: Vec<Vec<pane_protocol::plugin::PluginSegment>>,

    // Client-only UI state
    pub leader_state: Option<LeaderState>,
    pub palette_state: Option<UnifiedPaletteState>,
    pub copy_mode_state: Option<CopyModeState>,
    pub tab_picker_state: Option<TabPickerState>,
    pub context_menu_state: Option<ContextMenuState>,
    pub pending_confirm_action: Option<Action>,
    pub confirm_message: Option<String>,
    pub workspace_bar_focused: bool,
    pub should_quit: bool,
    pub rename_input: String,
    pub rename_target: RenameTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameTarget {
    Window,
    Workspace,
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
            palette_state: None,
            copy_mode_state: None,
            tab_picker_state: None,
            context_menu_state: None,
            pending_confirm_action: None,
            confirm_message: None,
            workspace_bar_focused: false,
            should_quit: false,
            rename_input: String::new(),
            rename_target: RenameTarget::Window,
        }
    }

    /// Connect to a daemon and run the TUI event loop.
    pub async fn run(session_name: &str, config: Config) -> Result<()> {
        let sock = daemon::socket_path(session_name);
        if !sock.exists() {
            anyhow::bail!(
                "no running session '{}'. Start one with: pane daemon {}",
                session_name,
                session_name
            );
        }

        let mut stream = UnixStream::connect(&sock).await?;

        // Attach with timeout — if the daemon is stuck, don't hang forever
        let handshake = async {
            framing::send(
                &mut stream,
                &ClientRequest::Attach {
                    session_name: session_name.to_string(),
                },
            )
            .await?;

            let resp: ServerResponse = framing::recv_required(&mut stream).await?;
            match resp {
                ServerResponse::Attached { .. } => {}
                ServerResponse::Error(e) => anyhow::bail!("server error: {}", e),
                _ => anyhow::bail!("unexpected response: {:?}", resp),
            };

            let resp: ServerResponse = framing::recv_required(&mut stream).await?;
            Ok::<_, anyhow::Error>(resp)
        };

        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), handshake)
            .await
            .map_err(|_| anyhow::anyhow!("daemon handshake timed out — is the daemon healthy?"))?
            ?;

        let mut client = Client::new(config, session_name);

        // Apply initial LayoutChanged
        if let ServerResponse::LayoutChanged { render_state } = resp {
            client.apply_layout(render_state);
        }

        // Set up TUI
        let mut tui = Tui::new()?;
        tui.enter()?;

        // Send initial resize
        let size = tui.size()?;
        framing::send(
            &mut stream,
            &ClientRequest::Resize {
                width: size.width,
                height: size.height,
            },
        )
        .await?;

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
                    self.screens
                        .entry(pane.id)
                        .or_insert_with(|| vt100::Parser::new(24, 80, 1000));
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
            ServerResponse::Error(_)
            | ServerResponse::Attached { .. }
            | ServerResponse::CommandOutput { .. } => {}
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
                let _ = send_request(
                    &mut *w_guard,
                    &ClientRequest::Resize {
                        width: w,
                        height: h,
                    },
                )
                .await;
            }
            AppEvent::MouseDown { x, y } => {
                if self.mode == Mode::ContextMenu {
                    let size = tui.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    if let Some(ref cm) = self.context_menu_state {
                        if let Some(idx) = crate::ui::context_menu::hit_test(cm, area, x, y) {
                            let action = cm.items[idx].action.clone();
                            self.context_menu_state = None;
                            self.mode = Mode::Normal;
                            self.execute_action(action, tui, writer).await?;
                        } else {
                            // Click outside menu — dismiss
                            self.context_menu_state = None;
                            self.mode = Mode::Normal;
                        }
                    }
                } else if self.mode == Mode::Confirm {
                    let size = tui.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    if let Some(click) = ui::confirm_dialog_hit_test(area, x, y) {
                        match click {
                            ui::ConfirmDialogClick::Confirm => {
                                if let Some(action) = self.pending_confirm_action.take() {
                                    if let Some(cmd) = action_to_command(&action) {
                                        let mut w = writer.lock().await;
                                        let _ = send_request(&mut *w, &ClientRequest::Command(cmd)).await;
                                    }
                                }
                                self.confirm_message = None;
                                self.mode = Mode::Normal;
                            }
                            ui::ConfirmDialogClick::Cancel => {
                                self.pending_confirm_action = None;
                                self.confirm_message = None;
                                self.mode = Mode::Normal;
                            }
                        }
                    }
                } else if self.mode == Mode::Normal
                    || self.mode == Mode::Interact
                {
                    // Check workspace bar clicks (client-side)
                    let show_workspace_bar = !self.render_state.workspaces.is_empty();
                    if show_workspace_bar && y < crate::ui::workspace_bar::HEIGHT {
                        let names: Vec<&str> = self
                            .render_state
                            .workspaces
                            .iter()
                            .map(|ws| ws.name.as_str())
                            .collect();
                        let bar_area =
                            Rect::new(0, 0, tui.size()?.width, crate::ui::workspace_bar::HEIGHT);
                        if let Some(click) = crate::ui::workspace_bar::hit_test(
                            &names,
                            self.render_state.active_workspace,
                            bar_area,
                            x,
                            y,
                        ) {
                            match click {
                                crate::ui::workspace_bar::WorkspaceBarClick::Tab(i) => {
                                    let mut w = writer.lock().await;
                                    let _ = send_request(
                                        &mut *w,
                                        &ClientRequest::Command(format!(
                                            "select-workspace -t {}",
                                            i
                                        )),
                                    )
                                    .await;
                                }
                                crate::ui::workspace_bar::WorkspaceBarClick::NewWorkspace => {
                                    let mut w = writer.lock().await;
                                    let _ = send_request(
                                        &mut *w,
                                        &ClientRequest::Command(
                                            "new-session -d -s workspace".to_string(),
                                        ),
                                    )
                                    .await;
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
            AppEvent::MouseRightDown { x, y } => {
                if self.mode == Mode::Normal || self.mode == Mode::Interact {
                    let show_workspace_bar = !self.render_state.workspaces.is_empty();

                    if show_workspace_bar && y < crate::ui::workspace_bar::HEIGHT {
                        // Right-click on workspace bar
                        self.context_menu_state =
                            Some(crate::ui::context_menu::workspace_bar_menu(x, y));
                        self.mode = Mode::ContextMenu;
                    } else {
                        // Right-click on pane body (default)
                        self.context_menu_state =
                            Some(crate::ui::context_menu::pane_body_menu(x, y));
                        self.mode = Mode::ContextMenu;
                    }
                }
            }
            AppEvent::Tick => {
                if let Some(ref mut ls) = self.leader_state {
                    if !ls.popup_visible {
                        let elapsed = ls.entered_at.elapsed().as_millis() as u64;
                        if elapsed >= self.config.leader.timeout_ms {
                            if ls.path.is_empty() {
                                // Root level timeout → open command palette
                                self.leader_state = None;
                                self.palette_state =
                                    Some(UnifiedPaletteState::new_full_search(&self.config.keys, &self.config.leader));
                                self.mode = Mode::Palette;
                            } else {
                                // Sub-group timeout → show which-key as before
                                ls.popup_visible = true;
                            }
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
            Mode::Scroll => return self.handle_scroll_key(key, writer).await,
            Mode::Copy => return self.handle_copy_mode_key(key),
            Mode::Palette => return self.handle_palette_key(key, tui, writer).await,
            Mode::TabPicker => return self.handle_tab_picker_key(key, writer).await,
            Mode::Confirm => return self.handle_confirm_key(key, writer).await,
            Mode::Leader => return self.handle_leader_key(key, tui, writer).await,
            Mode::Rename => return self.handle_rename_key(key, writer).await,
            Mode::ContextMenu => return self.handle_context_menu_key(key, tui, writer).await,
            Mode::Normal => return self.handle_normal_key(key, tui, writer).await,
            Mode::Interact => return self.handle_interact_key(key, tui, writer).await,
        }
    }

    /// Interact mode: forward ALL keys to PTY except Escape (-> Normal).
    /// The leader key (space) is NOT intercepted here — use Escape to enter
    /// Normal mode first, then press space for the leader popup.
    async fn handle_interact_key(
        &mut self,
        key: KeyEvent,
        _tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let normalized = config::normalize_key(key);

        // Escape -> Normal mode
        if normalized.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            return Ok(());
        }

        // Check global keymap (ctrl+q etc.)
        if let Some(action) = self.config.keys.lookup(&normalized).cloned() {
            return self.execute_action(action, _tui, writer).await;
        }

        // Forward to PTY
        let mut w = writer.lock().await;
        let _ = send_request(
            &mut *w,
            &ClientRequest::Key(SerializableKeyEvent::from(key)),
        )
        .await;
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

        // Esc clears transient state but stays in Normal mode
        if normalized.code == KeyCode::Esc {
            self.workspace_bar_focused = false;
            return Ok(());
        }

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

        // Normal mode data-driven keymap
        if let Some(action) = self.config.normal_keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui, writer).await;
        }

        // 1-9 → FocusGroupN (number keys are dynamic, keep outside the keymap)
        if let KeyCode::Char(c) = normalized.code {
            if c.is_ascii_digit()
                && c != '0'
                && normalized.modifiers == KeyModifiers::NONE
            {
                let n = c as u8 - b'0';
                return self
                    .execute_action(Action::FocusGroupN(n), tui, writer)
                    .await;
            }
        }

        // No PTY fallback in Normal mode — keys are consumed
        Ok(())
    }

    fn enter_leader_mode(&mut self) {
        self.workspace_bar_focused = false;
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
        // Workspace bar focus mode
        if self.workspace_bar_focused {
            match &action {
                Action::FocusLeft => {
                    let idx = self.render_state.active_workspace;
                    if idx > 0 {
                        let mut w = writer.lock().await;
                        let _ = send_request(
                            &mut *w,
                            &ClientRequest::Command(format!("select-workspace -t {}", idx - 1)),
                        )
                        .await;
                    }
                    return Ok(());
                }
                Action::FocusRight => {
                    let idx = self.render_state.active_workspace;
                    if idx + 1 < self.render_state.workspaces.len() {
                        let mut w = writer.lock().await;
                        let _ = send_request(
                            &mut *w,
                            &ClientRequest::Command(format!("select-workspace -t {}", idx + 1)),
                        )
                        .await;
                    }
                    return Ok(());
                }
                Action::FocusDown | Action::FocusUp => {
                    self.workspace_bar_focused = false;
                    return Ok(());
                }
                Action::CloseTab => {
                    // Remap to close workspace when bar is focused
                    self.pending_confirm_action = Some(Action::CloseWorkspace);
                    self.confirm_message = Some("Close this workspace?".into());
                    self.workspace_bar_focused = false;
                    self.mode = Mode::Confirm;
                    return Ok(());
                }
                Action::EnterInteract => {
                    self.workspace_bar_focused = false;
                    self.mode = Mode::Interact;
                    return Ok(());
                }
                _ => {
                    self.workspace_bar_focused = false;
                    // Fall through to normal action handling
                }
            }
        }

        // Entering workspace bar focus: FocusUp at the top of the layout
        if action == Action::FocusUp && !self.render_state.workspaces.is_empty() {
            if let Some(ws) = self.active_workspace() {
                let at_top = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::First,
                ).is_none();
                if at_top {
                    self.workspace_bar_focused = true;
                    return Ok(());
                }
            }
        }

        // Client-only actions
        match &action {
            Action::Quit => {
                self.should_quit = true;
                return Ok(());
            }
            Action::Help => {
                // Help opens the command palette (unified palette)
                self.palette_state = Some(UnifiedPaletteState::new_full_search(&self.config.keys, &self.config.leader));
                self.mode = Mode::Palette;
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
                self.palette_state = Some(UnifiedPaletteState::new_full_search(&self.config.keys, &self.config.leader));
                self.mode = Mode::Palette;
                return Ok(());
            }
            Action::EnterInteract => {
                self.workspace_bar_focused = false;
                self.mode = Mode::Interact;
                return Ok(());
            }
            Action::EnterNormal => {
                self.workspace_bar_focused = false;
                self.mode = Mode::Normal;
                return Ok(());
            }
            Action::Detach => {
                self.should_quit = true;
                return Ok(());
            }
            Action::NewTab => {
                self.tab_picker_state = Some(TabPickerState::new(&self.config.tab_picker_entries));
                self.mode = Mode::TabPicker;
                return Ok(());
            }
            Action::RenameWindow => {
                self.rename_input.clear();
                self.rename_target = RenameTarget::Window;
                self.mode = Mode::Rename;
                return Ok(());
            }
            Action::RenameWorkspace => {
                self.rename_input.clear();
                self.rename_target = RenameTarget::Workspace;
                self.mode = Mode::Rename;
                return Ok(());
            }
            Action::PasteClipboard => {
                if let Ok(text) = clipboard::paste_from_clipboard() {
                    if !text.is_empty() {
                        let mut w = writer.lock().await;
                        let _ = send_request(
                            &mut *w,
                            &ClientRequest::Command(format!("paste-buffer {}", text)),
                        )
                        .await;
                    }
                }
                return Ok(());
            }
            _ => {}
        }

        // Destructive actions — smart confirm: only prompt if foreground process
        match &action {
            Action::CloseTab => {
                let has_fg = self
                    .active_workspace()
                    .and_then(|ws| ws.groups.iter().find(|g| g.id == ws.active_group))
                    .and_then(|g| g.tabs.get(g.active_tab))
                    .and_then(|tab| tab.foreground_process.as_ref())
                    .is_some();

                if has_fg {
                    self.pending_confirm_action = Some(Action::CloseTab);
                    self.confirm_message = Some("Close this tab? (process running)".into());
                    self.workspace_bar_focused = false;
                    self.mode = Mode::Confirm;
                } else {
                    // Idle shell — close immediately
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut *w, &ClientRequest::Command("kill-pane".to_string())).await;
                }
                return Ok(());
            }
            Action::CloseWorkspace => {
                let has_any_fg = self
                    .active_workspace()
                    .map(|ws| {
                        ws.groups.iter().any(|g| {
                            g.tabs.iter().any(|tab| tab.foreground_process.is_some())
                        })
                    })
                    .unwrap_or(false);

                if has_any_fg {
                    self.pending_confirm_action = Some(Action::CloseWorkspace);
                    self.confirm_message = Some("Close workspace? (processes running)".into());
                    self.workspace_bar_focused = false;
                    self.mode = Mode::Confirm;
                } else {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut *w, &ClientRequest::Command("close-workspace".to_string())).await;
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

    async fn handle_confirm_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                if let Some(action) = self.pending_confirm_action.take() {
                    if let Some(cmd) = action_to_command(&action) {
                        let mut w = writer.lock().await;
                        let _ = send_request(&mut *w, &ClientRequest::Command(cmd)).await;
                    }
                }
                self.confirm_message = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Esc | KeyCode::Char('n') => {
                self.pending_confirm_action = None;
                self.confirm_message = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_rename_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.rename_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let name = self.rename_input.clone();
                self.rename_input.clear();
                self.mode = Mode::Normal;
                if !name.is_empty() {
                    let cmd = match self.rename_target {
                        RenameTarget::Window => format!("rename-window {}", name),
                        RenameTarget::Workspace => format!("rename-session {}", name),
                    };
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut *w, &ClientRequest::Command(cmd)).await;
                }
            }
            KeyCode::Backspace => {
                self.rename_input.pop();
            }
            KeyCode::Char(c) => {
                self.rename_input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_context_menu_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.context_menu_state = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut cm) = self.context_menu_state {
                    cm.move_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut cm) = self.context_menu_state {
                    cm.move_down();
                }
            }
            KeyCode::Enter => {
                if let Some(cm) = self.context_menu_state.take() {
                    self.mode = Mode::Normal;
                    if let Some(action) = cm.selected_action().cloned() {
                        self.execute_action(action, tui, writer).await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_copy_mode_key(&mut self, key: KeyEvent) -> Result<()> {
        // Get the pane_id for the active pane so we can borrow screen and cms separately
        let pane_id = self
            .active_workspace()
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
                let _ = send_request(
                    &mut *w,
                    &ClientRequest::Key(SerializableKeyEvent::from(key)),
                )
                .await;
            }
        }
        Ok(())
    }

    async fn handle_palette_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        if let Some(ref mut cp) = self.palette_state {
            match key.code {
                KeyCode::Esc => {
                    self.palette_state = None;
                    self.mode = Mode::Normal;
                }
                KeyCode::Enter => {
                    if let Some(action) = cp.selected_action() {
                        self.palette_state = None;
                        self.mode = Mode::Normal;
                        return self.execute_action(action, tui, writer).await;
                    }
                }
                KeyCode::Up => cp.move_up(),
                KeyCode::Down => cp.move_down(),
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
        use pane_protocol::config::LeaderNode;

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
                let _ = send_request(
                    &mut *w,
                    &ClientRequest::Key(SerializableKeyEvent::from(leader_key)),
                )
                .await;
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
        self.render_state
            .workspaces
            .get(self.render_state.active_workspace)
    }

    pub fn pane_screen(&self, pane_id: TabId) -> Option<&vt100::Screen> {
        self.screens.get(&pane_id).map(|p| p.screen())
    }

    fn update_terminal_title(&self) {
        if let Some(ref fmt) = self.config.behavior.terminal_title_format {
            let session = &self.render_state.session_name;
            let workspace = self
                .active_workspace()
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
        Action::ToggleFold => Some("toggle-fold".to_string()),
        Action::RenameWindow | Action::RenameWorkspace | Action::RenamePane => None,
        // Client-only actions handled before this function is called
        Action::Quit
        | Action::Help
        | Action::ScrollMode
        | Action::CopyMode
        | Action::CommandPalette
        | Action::PasteClipboard
        | Action::EnterInteract
        | Action::EnterNormal
        | Action::Detach
        | Action::SessionPicker
        | Action::NewTab // NewTab opens picker client-side
        | Action::NewPane
        | Action::ClientPicker => None,
    }
}
