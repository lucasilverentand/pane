#![allow(dead_code)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::config::Config;
use crate::event::AppEvent;
use crate::server::command_parser;
use crate::server::framing;
use crate::server::id_map::IdMap;
use crate::server::protocol::{
    ClientRequest, RenderState, SerializableSystemStats, ServerResponse,
};
use crate::server::state::ServerState;
use crate::session;
use crate::system_stats;

/// Global counter for assigning unique client IDs.
static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(0);

/// Per-client state tracked by the server.
#[derive(Clone, Debug)]
struct ClientInfo {
    width: u16,
    height: u16,
    active_workspace: usize,
}

/// Registry of connected clients with their terminal sizes and workspace views.
/// The server uses the smallest dimensions across all clients.
#[derive(Clone)]
struct ClientRegistry {
    inner: Arc<Mutex<HashMap<u64, ClientInfo>>>,
}

impl ClientRegistry {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn register(&self, id: u64, width: u16, height: u16, active_workspace: usize) {
        self.inner.lock().await.insert(
            id,
            ClientInfo {
                width,
                height,
                active_workspace,
            },
        );
    }

    async fn update_size(&self, id: u64, width: u16, height: u16) {
        if let Some(info) = self.inner.lock().await.get_mut(&id) {
            info.width = width;
            info.height = height;
        }
    }

    async fn set_active_workspace(&self, id: u64, ws: usize) {
        if let Some(info) = self.inner.lock().await.get_mut(&id) {
            info.active_workspace = ws;
        }
    }

    async fn get_active_workspace(&self, id: u64) -> Option<usize> {
        self.inner.lock().await.get(&id).map(|i| i.active_workspace)
    }

    async fn unregister(&self, id: u64) {
        self.inner.lock().await.remove(&id);
    }

    /// Return the minimum width and height across all connected clients.
    /// Returns None if no clients are connected.
    async fn min_size(&self) -> Option<(u16, u16)> {
        let clients = self.inner.lock().await;
        if clients.is_empty() {
            return None;
        }
        let min_w = clients.values().map(|i| i.width).min().unwrap_or(80);
        let min_h = clients.values().map(|i| i.height).min().unwrap_or(24);
        Some((min_w, min_h))
    }

    async fn count(&self) -> usize {
        self.inner.lock().await.len()
    }

    async fn list(&self) -> Vec<crate::server::protocol::ClientListEntry> {
        self.inner
            .lock()
            .await
            .iter()
            .map(|(&id, info)| crate::server::protocol::ClientListEntry {
                id,
                width: info.width,
                height: info.height,
                active_workspace: info.active_workspace,
            })
            .collect()
    }
}

/// Returns the socket directory: $TMPDIR/pane-{uid}/ or /tmp/pane-{uid}/
pub fn socket_dir() -> PathBuf {
    let uid = nix::unistd::getuid();
    let base = std::env::var("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    base.join(format!("pane-{}", uid))
}

/// Returns the fixed socket path for the single pane daemon.
pub fn socket_path() -> PathBuf {
    socket_dir().join("pane.sock")
}

/// Check if the daemon is currently running.
pub fn is_running() -> bool {
    let sock = socket_path();
    sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok()
}

/// List running sessions by scanning socket files and testing connectivity.
pub fn list_sessions() -> Vec<String> {
    let dir = socket_dir();
    let mut sessions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "sock").unwrap_or(false) {
                if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                    // Test if socket is alive by attempting connection
                    if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                        sessions.push(name.to_string());
                    } else {
                        // Stale socket, clean up
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }
    sessions.sort();
    sessions
}

/// Run the server daemon for a given session.
pub async fn run_server(config: Config) -> Result<()> {
    let sock_dir = socket_dir();
    std::fs::create_dir_all(&sock_dir)?;

    let sock_path = socket_path();
    cleanup_stale_socket(&sock_path);

    let listener = UnixListener::bind(&sock_path)?;

    // Channel for internal events (PTY output, stats, etc.)
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Start system stats collector
    system_stats::start_stats_collector(event_tx.clone(), config.status_bar.update_interval_secs);

    let auto_suspend_secs = config.behavior.auto_suspend_secs;

    // Try to restore from saved state, otherwise create a new one
    let state = if let Some(saved) = session::store::load() {
        ServerState::restore(saved, event_tx.clone(), 80, 24, config)?
    } else {
        ServerState::new(&event_tx, 80, 24, config)?
    };
    // Start plugin manager
    let plugin_configs = state.config.plugins.clone();
    let state = Arc::new(Mutex::new(state));

    if !plugin_configs.is_empty() {
        let (mut plugin_mgr, mut plugin_rx) = crate::plugin::PluginManager::new(plugin_configs);
        plugin_mgr.start_all();
        let plugin_mgr = Arc::new(Mutex::new(plugin_mgr));

        // Forward plugin events to state/broadcast
        let plugin_mgr_clone = Arc::clone(&plugin_mgr);
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(event) = plugin_rx.recv().await {
                let mut mgr = plugin_mgr_clone.lock().await;
                let commands = mgr.handle_event(event);

                // Broadcast updated segments to clients
                // (segments are stored in the plugin manager, not sent via broadcast for now)

                // Execute any commands returned by plugins
                for cmd_str in commands {
                    if cmd_str.is_empty() {
                        continue;
                    }
                    if let Ok(parsed) = crate::server::command_parser::parse(&cmd_str) {
                        let mut state = state_clone.lock().await;
                        // Plugin commands don't need id_map or broadcast; fire and forget
                        let _ = crate::server::command::execute(
                            &parsed,
                            &mut state,
                            &mut crate::server::id_map::IdMap::new(),
                            &tokio::sync::broadcast::channel(1).0,
                        );
                    }
                }
            }
        });
    }

    // ID map for tmux-compatible sequential IDs
    let id_map = Arc::new(Mutex::new(IdMap::new()));

    // Client registry for multi-client size tracking
    let clients = ClientRegistry::new();

    // Broadcast channel for sending responses to all connected clients
    let (broadcast_tx, _) = broadcast::channel::<ServerResponse>(256);

    // Spawn event processing loop
    let state_clone = Arc::clone(&state);
    let broadcast_tx_clone = broadcast_tx.clone();
    let event_loop = tokio::spawn(async move {
        process_events(&mut event_rx, &state_clone, &broadcast_tx_clone).await;
    });

    // Accept client connections
    let accept_loop = tokio::spawn({
        let state = Arc::clone(&state);
        let id_map = Arc::clone(&id_map);
        let broadcast_tx = broadcast_tx.clone();
        let clients = clients.clone();
        async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        let state = Arc::clone(&state);
                        let id_map = Arc::clone(&id_map);
                        let broadcast_tx = broadcast_tx.clone();
                        let broadcast_rx = broadcast_tx.subscribe();
                        let clients = clients.clone();
                        let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(
                                stream,
                                state,
                                id_map,
                                broadcast_tx,
                                broadcast_rx,
                                clients,
                                client_id,
                            )
                            .await
                            {
                                eprintln!("pane: client error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("pane: accept error: {}", e);
                    }
                }
            }
        }
    });

    // Set up signal handler for graceful shutdown (SIGTERM + Ctrl-C)
    let state_clone = Arc::clone(&state);
    let broadcast_tx_term = broadcast_tx.clone();
    let sock_path_clone = sock_path.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        // Save session and notify clients before exiting
        let state = state_clone.lock().await;
        let saved = session::SavedState::from_server(&state);
        let _ = session::store::save(&saved);
        let _ = broadcast_tx_term.send(ServerResponse::SessionEnded);
        let _ = std::fs::remove_file(&sock_path_clone);
        std::process::exit(0);
    });

    // Set up SIGHUP handler for config reload
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("failed to register SIGHUP handler");
        loop {
            sighup.recv().await;
            let new_config = Config::load();
            let mut state = state_clone.lock().await;
            state.config = new_config;
        }
    });

    // Auto-suspend: save and exit after N seconds of no connected clients
    if auto_suspend_secs > 0 {
        let state_clone = Arc::clone(&state);
        let clients_clone = clients.clone();
        let broadcast_tx_suspend = broadcast_tx.clone();
        let sock_path_suspend = sock_path.clone();
        tokio::spawn(async move {
            let mut last_empty: Option<tokio::time::Instant> = None;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let count = clients_clone.count().await;
                if count == 0 {
                    if last_empty.is_none() {
                        last_empty = Some(tokio::time::Instant::now());
                    }
                    if let Some(since) = last_empty {
                        if since.elapsed().as_secs() >= auto_suspend_secs {
                            // Save session and exit
                            let state = state_clone.lock().await;
                            let saved = session::SavedState::from_server(&state);
                            let _ = session::store::save(&saved);
                            let _ = broadcast_tx_suspend.send(ServerResponse::SessionEnded);
                            let _ = std::fs::remove_file(&sock_path_suspend);
                            std::process::exit(0);
                        }
                    }
                } else {
                    last_empty = None;
                }
            }
        });
    }

    // Wait for the event loop to finish (happens when all panes exit)
    event_loop.await?;

    // Clean up
    accept_loop.abort();
    let state = state.lock().await;
    let saved = session::SavedState::from_server(&state);
    let _ = session::store::save(&saved);
    let _ = std::fs::remove_file(&sock_path);

    Ok(())
}

/// Process internal AppEvents and broadcast relevant updates to clients.
async fn process_events(
    event_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    state: &Arc<Mutex<ServerState>>,
    broadcast_tx: &broadcast::Sender<ServerResponse>,
) {
    while let Some(event) = event_rx.recv().await {
        match event {
            AppEvent::PtyOutput { pane_id, bytes } => {
                {
                    let mut state = state.lock().await;
                    if let Some(pane) = state.find_tab_mut(pane_id) {
                        pane.process_output(&bytes);
                    }
                }
                let _ = broadcast_tx.send(ServerResponse::PaneOutput {
                    pane_id,
                    data: bytes,
                });
            }
            AppEvent::PtyExited { pane_id } => {
                let should_quit = {
                    let mut state = state.lock().await;
                    state.handle_pty_exited(pane_id)
                };
                let _ = broadcast_tx.send(ServerResponse::PaneExited { pane_id });
                if should_quit {
                    let _ = broadcast_tx.send(ServerResponse::SessionEnded);
                    break;
                }
                let state = state.lock().await;
                let render_state = RenderState::from_server_state(&state);
                let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
            }
            AppEvent::SystemStats(stats) => {
                {
                    let mut state = state.lock().await;
                    state.system_stats = stats.clone();
                }
                let serializable = SerializableSystemStats::from(&stats);
                let _ = broadcast_tx.send(ServerResponse::StatsUpdate(serializable));
            }
            AppEvent::Tick => {
                // No-op for daemon mode — ticks are client-side
            }
            // These events come from clients, not the internal event loop
            AppEvent::Key(_)
            | AppEvent::MouseDown { .. }
            | AppEvent::MouseRightDown
            | AppEvent::MouseDrag { .. }
            | AppEvent::MouseMove { .. }
            | AppEvent::MouseUp
            | AppEvent::MouseScroll { .. }
            | AppEvent::Resize(_, _) => {}
        }
    }
}

/// Handle a single client connection.
async fn handle_client(
    mut stream: UnixStream,
    state: Arc<Mutex<ServerState>>,
    id_map: Arc<Mutex<IdMap>>,
    broadcast_tx: broadcast::Sender<ServerResponse>,
    mut broadcast_rx: broadcast::Receiver<ServerResponse>,
    clients: ClientRegistry,
    client_id: u64,
) -> Result<()> {
    // Read the initial request
    let first_msg: ClientRequest = framing::recv_required(&mut stream).await?;

    // Handle CommandSync: execute command, send result, disconnect immediately.
    if let ClientRequest::CommandSync(cmd_str) = &first_msg {
        let result = {
            let parsed = command_parser::parse(cmd_str);
            match parsed {
                Ok(parsed_cmd) => {
                    let mut state_guard = state.lock().await;
                    let mut id_map_guard = id_map.lock().await;
                    match crate::server::command::execute(
                        &parsed_cmd,
                        &mut state_guard,
                        &mut id_map_guard,
                        &broadcast_tx,
                    ) {
                        Ok(crate::server::command::CommandResult::Ok(output)) => {
                            ServerResponse::CommandOutput {
                                output,
                                pane_id: None,
                                window_id: None,
                                success: true,
                            }
                        }
                        Ok(crate::server::command::CommandResult::OkWithId {
                            output,
                            pane_id,
                            window_id,
                        }) => ServerResponse::CommandOutput {
                            output,
                            pane_id,
                            window_id,
                            success: true,
                        },
                        Ok(crate::server::command::CommandResult::LayoutChanged) => {
                            // Also broadcast the layout change to connected TUI clients
                            let render_state = RenderState::from_server_state(&state_guard);
                            let _ =
                                broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
                            ServerResponse::CommandOutput {
                                output: String::new(),
                                pane_id: None,
                                window_id: None,
                                success: true,
                            }
                        }
                        Ok(crate::server::command::CommandResult::SessionEnded) => {
                            ServerResponse::CommandOutput {
                                output: String::new(),
                                pane_id: None,
                                window_id: None,
                                success: true,
                            }
                        }
                        Ok(crate::server::command::CommandResult::DetachRequested) => {
                            ServerResponse::CommandOutput {
                                output: String::new(),
                                pane_id: None,
                                window_id: None,
                                success: true,
                            }
                        }
                        Err(e) => ServerResponse::CommandOutput {
                            output: e.to_string(),
                            pane_id: None,
                            window_id: None,
                            success: false,
                        },
                    }
                }
                Err(e) => ServerResponse::CommandOutput {
                    output: format!("parse error: {}", e),
                    pane_id: None,
                    window_id: None,
                    success: false,
                },
            }
        };
        framing::send(&mut stream, &result).await?;
        return Ok(());
    }

    match &first_msg {
        ClientRequest::Attach => {}
        _ => {
            framing::send(
                &mut stream,
                &ServerResponse::Error("expected Attach as first message".to_string()),
            )
            .await?;
            return Ok(());
        }
    };

    // Send attached confirmation with this client's ID
    framing::send(&mut stream, &ServerResponse::Attached { client_id }).await?;

    // Register client with default size and current active workspace
    {
        let state_guard = state.lock().await;
        let (w, h) = state_guard.last_size;
        clients
            .register(client_id, w, h, state_guard.active_workspace)
            .await;
        let client_list = clients.list().await;
        let _ = broadcast_tx.send(ServerResponse::ClientListChanged(client_list));
    }

    // Send initial layout state using this client's active workspace
    {
        let state = state.lock().await;
        let client_ws = clients.get_active_workspace(client_id).await.unwrap_or(0);
        let render_state = RenderState::for_client(&state, client_ws);
        framing::send(&mut stream, &ServerResponse::LayoutChanged { render_state }).await?;
    }

    // Split the stream for bidirectional communication
    let (read_half, write_half) = stream.into_split();
    let mut read_stream = read_half;
    let write_stream = Arc::new(Mutex::new(write_half));

    // Spawn a task to forward broadcasts to this client
    let write_clone = Arc::clone(&write_stream);
    let forward_task = tokio::spawn(async move {
        while let Ok(response) = broadcast_rx.recv().await {
            let mut writer = write_clone.lock().await;
            // Reassemble a UnixStream-like writer for framing
            let json = match serde_json::to_vec(&response) {
                Ok(j) => j,
                Err(_) => continue,
            };
            let len = json.len() as u32;
            use tokio::io::AsyncWriteExt;
            if writer.write_all(&len.to_be_bytes()).await.is_err() {
                break;
            }
            if writer.write_all(&json).await.is_err() {
                break;
            }
            let _ = writer.flush().await;
        }
    });

    // Read client requests
    loop {
        let mut len_buf = [0u8; 4];
        use tokio::io::AsyncReadExt;
        match read_stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(_) => break, // Client disconnected
        }
        let len = u32::from_be_bytes(len_buf);
        if len > 16 * 1024 * 1024 {
            break;
        }
        let mut buf = vec![0u8; len as usize];
        if read_stream.read_exact(&mut buf).await.is_err() {
            break;
        }
        let request: ClientRequest = match serde_json::from_slice(&buf) {
            Ok(r) => r,
            Err(_) => continue,
        };

        match request {
            ClientRequest::Detach => break,
            ClientRequest::Resize { width, height } => {
                clients.update_size(client_id, width, height).await;
                // Use the smallest terminal size across all connected clients
                let (eff_w, eff_h) = clients.min_size().await.unwrap_or((width, height));
                let mut state = state.lock().await;
                state.last_size = (eff_w, eff_h);
                state.resize_all_tabs(eff_w, eff_h);
                // Broadcast uses server's active_workspace; each client re-renders with their own
                let render_state = RenderState::from_server_state(&state);
                let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
            }
            ClientRequest::Key(sk) => {
                let key_event = sk.into();
                let mut state = state.lock().await;
                // Set state to this client's active workspace
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                // Forward key to the active pane as raw bytes
                let ws = state.active_workspace_mut();
                let group = ws.groups.get_mut(&ws.active_group);
                if let Some(group) = group {
                    let pane = group.active_tab_mut();
                    let bytes = crate::keys::key_to_bytes(key_event);
                    if !bytes.is_empty() {
                        pane.write_input(&bytes);
                    }
                }
            }
            ClientRequest::MouseDown { x, y } => {
                let mut state = state.lock().await;
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                if handle_mouse_down_server(&mut state, x, y) {
                    let render_state = RenderState::from_server_state(&state);
                    let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
                }
            }
            ClientRequest::MouseDrag { x, y } => {
                let mut state = state.lock().await;
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                let had_drag = state.drag_state.is_some();
                handle_mouse_drag_server(&mut state, x, y);
                if had_drag {
                    let render_state = RenderState::from_server_state(&state);
                    let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
                }
            }
            ClientRequest::MouseMove { .. } => {
                // Mouse move is client-side only (hover effects)
            }
            ClientRequest::MouseUp => {
                let had_drag = state.lock().await.drag_state.is_some();
                if had_drag {
                    let mut state = state.lock().await;
                    if let Some(cws) = clients.get_active_workspace(client_id).await {
                        state.active_workspace = cws;
                    }
                    state.drag_state = None;
                    let (w, h) = state.last_size;
                    state.resize_all_tabs(w, h);
                    let render_state = RenderState::from_server_state(&state);
                    let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
                }
            }
            ClientRequest::MouseScroll { up } => {
                let mut state = state.lock().await;
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                if up {
                    state.scroll_active_tab(|p| p.scroll_up(3));
                } else {
                    state.scroll_active_tab(|p| p.scroll_down(3));
                }
            }
            ClientRequest::Command(cmd) => {
                if handle_command(&cmd, &state, &id_map, &broadcast_tx, &clients, client_id).await {
                    break;
                }
            }
            ClientRequest::KickClient(target_id) => {
                let _ = broadcast_tx.send(ServerResponse::Kicked(target_id));
            }
            ClientRequest::SetActiveWorkspace(ws) => {
                let ws_count = state.lock().await.workspaces.len();
                if ws < ws_count {
                    clients.set_active_workspace(client_id, ws).await;
                    let client_list = clients.list().await;
                    let _ = broadcast_tx.send(ServerResponse::ClientListChanged(client_list));
                }
            }
            ClientRequest::Attach => {
                // Already attached, ignore
            }
            ClientRequest::CommandSync(_) => {
                // CommandSync is handled before attach; ignore if received mid-session
            }
        }
    }

    forward_task.abort();

    // Client disconnected: unregister and recalculate effective size
    clients.unregister(client_id).await;
    {
        let client_list = clients.list().await;
        let _ = broadcast_tx.send(ServerResponse::ClientListChanged(client_list));
    }
    if let Some((eff_w, eff_h)) = clients.min_size().await {
        let mut state = state.lock().await;
        state.last_size = (eff_w, eff_h);
        state.resize_all_tabs(eff_w, eff_h);
        let render_state = RenderState::from_server_state(&state);
        let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
    }

    Ok(())
}

/// Handle mouse down events server-side (pane focus changes).
fn handle_mouse_down_server(state: &mut ServerState, x: u16, y: u16) -> bool {
    state.drag_state = None;

    let bar_h = state.workspace_bar_height();
    let (w, h) = state.last_size;
    let body_height = h.saturating_sub(1 + bar_h);
    let body = ratatui::layout::Rect::new(0, bar_h, w, body_height);

    // Check for split border hit (drag initiation)
    {
        let ws = state.active_workspace();
        if let Some((path, direction)) = ws.layout.hit_test_split_border(body, x, y) {
            state.drag_state = Some(crate::server::state::DragState {
                split_path: path,
                direction,
                body,
            });
            return false;
        }
    }

    let ws = state.active_workspace();
    let resolved = ws
        .layout
        .resolve_with_folds(body, &ws.folded_windows);

    // Check fold bar clicks — unfold the clicked window
    for rp in &resolved {
        if let crate::layout::ResolvedPane::Folded {
            id: group_id, rect, ..
        } = rp
        {
            if rect.width == 0 || rect.height == 0 {
                continue;
            }
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                let group_id = *group_id;
                state.focus_group(group_id);
                return true;
            }
        }
    }

    // Check visible pane clicks for focus (with tab bar hit testing)
    for rp in &resolved {
        if let crate::layout::ResolvedPane::Visible {
            id: group_id, rect, ..
        } = rp
        {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                // Check tab bar click before focusing the group
                let tab_click = if let Some(group) = state.active_workspace().groups.get(group_id) {
                    if let Some(tab_area) = crate::ui::window_view::tab_bar_area(group, *rect) {
                        let layout = crate::ui::window_view::tab_bar_layout(
                            group,
                            &state.config.theme,
                            tab_area,
                        );
                        crate::ui::window_view::tab_bar_hit_test(&layout, x, y)
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(click) = tab_click {
                    state.focus_group(*group_id);
                    match click {
                        crate::ui::window_view::TabBarClick::Tab(i) => {
                            state.active_workspace_mut().active_group_mut().active_tab = i;
                        }
                        crate::ui::window_view::TabBarClick::NewTab => {
                            let cols = w.saturating_sub(4);
                            let rows = h.saturating_sub(2 + bar_h + 1);
                            let _ = state.add_tab_to_active_group(
                                crate::window::TabKind::Shell,
                                None,
                                cols,
                                rows,
                            );
                        }
                    }
                    return true;
                }

                state.focus_group(*group_id);
                return true;
            }
        }
    }

    false
}

/// Handle mouse drag events server-side (split resizing).
fn handle_mouse_drag_server(state: &mut ServerState, x: u16, y: u16) {
    let drag = match &state.drag_state {
        Some(d) => d.clone(),
        None => return,
    };

    let body = drag.body;
    let new_ratio = match drag.direction {
        crate::layout::SplitDirection::Horizontal => {
            if body.width == 0 {
                return;
            }
            ((x.saturating_sub(body.x)) as f64) / (body.width as f64)
        }
        crate::layout::SplitDirection::Vertical => {
            if body.height == 0 {
                return;
            }
            ((y.saturating_sub(body.y)) as f64) / (body.height as f64)
        }
    };

    let new_ratio = new_ratio.clamp(0.05, 0.95);
    let ws = state.active_workspace_mut();
    ws.layout.set_ratio_at_path(&drag.split_path, new_ratio);
}

/// Handle string commands from the command protocol.
/// Returns `true` if the client should detach (break the read loop).
async fn handle_command(
    cmd: &str,
    state: &Arc<Mutex<ServerState>>,
    id_map: &Arc<Mutex<IdMap>>,
    broadcast_tx: &broadcast::Sender<ServerResponse>,
    clients: &ClientRegistry,
    client_id: u64,
) -> bool {
    match command_parser::parse(cmd) {
        Ok(parsed_cmd) => {
            let mut state = state.lock().await;
            // Set active workspace to this client's view
            if let Some(cws) = clients.get_active_workspace(client_id).await {
                state.active_workspace = cws;
            }
            let mut id_map = id_map.lock().await;
            let result =
                crate::server::command::execute(&parsed_cmd, &mut state, &mut id_map, broadcast_tx);
            // Sync back: command may have changed active_workspace (e.g. new-workspace, next-workspace)
            clients
                .set_active_workspace(client_id, state.active_workspace)
                .await;
            match result {
                Ok(crate::server::command::CommandResult::Ok(output)) => {
                    if !output.is_empty() {
                        // Send output as a display message response
                        let _ =
                            broadcast_tx.send(ServerResponse::Error(format!("[cmd] {}", output)));
                    }
                }
                Ok(crate::server::command::CommandResult::OkWithId { output, .. }) => {
                    if !output.is_empty() {
                        let _ =
                            broadcast_tx.send(ServerResponse::Error(format!("[cmd] {}", output)));
                    }
                }
                Ok(crate::server::command::CommandResult::LayoutChanged) => {
                    // Layout update already broadcast by execute()
                }
                Ok(crate::server::command::CommandResult::SessionEnded) => {
                    // SessionEnded already broadcast by execute()
                }
                Ok(crate::server::command::CommandResult::DetachRequested) => {
                    return true;
                }
                Err(e) => {
                    let _ = broadcast_tx.send(ServerResponse::Error(e.to_string()));
                }
            }
        }
        Err(e) => {
            let _ = broadcast_tx.send(ServerResponse::Error(format!("parse error: {}", e)));
        }
    }
    false
}

/// Start a daemon in the background for the given session.
/// Forks the current exe with `daemon` arg, detaches stdio,
/// and waits for the socket to appear (up to 5 seconds).
pub fn start_daemon() -> Result<()> {
    let exe = std::env::current_exe()?;
    let sock = socket_path();

    // If socket already exists and daemon is alive, nothing to do
    if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
        return Ok(());
    }

    // Clean up stale socket
    if sock.exists() {
        let _ = std::fs::remove_file(&sock);
    }

    // Ensure socket directory exists
    let sock_dir = socket_dir();
    std::fs::create_dir_all(&sock_dir)?;

    // Fork a background daemon process
    use std::process::{Command, Stdio};
    Command::new(exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Wait for socket to appear (100 retries × 50ms = 5s max)
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            return Ok(());
        }
    }

    anyhow::bail!("timed out waiting for daemon to start")
}

fn cleanup_stale_socket(path: &Path) {
    if path.exists() {
        // Try to connect — if it fails, the socket is stale
        if std::os::unix::net::UnixStream::connect(path).is_err() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Kill the running daemon by connecting and sending a kill-server command.
pub async fn kill_daemon() -> Result<()> {
    let path = socket_path();
    if !path.exists() {
        anyhow::bail!("no running pane daemon");
    }
    let mut stream = UnixStream::connect(&path).await?;
    framing::send(&mut stream, &ClientRequest::Attach).await?;
    // Wait for Attached response
    let _: ServerResponse = framing::recv_required(&mut stream).await?;
    // Send kill command
    framing::send(
        &mut stream,
        &ClientRequest::Command("kill-server".to_string()),
    )
    .await?;
    Ok(())
}

/// Send keys to the active pane.
pub async fn send_keys(keys: &str) -> Result<()> {
    let path = socket_path();
    if !path.exists() {
        anyhow::bail!("no running pane daemon");
    }
    let mut stream = UnixStream::connect(&path).await?;
    framing::send(&mut stream, &ClientRequest::Attach).await?;
    let _: ServerResponse = framing::recv_required(&mut stream).await?;

    // Parse keys: simple text for now, each character as a key event
    for ch in keys.chars() {
        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(ch),
            crossterm::event::KeyModifiers::NONE,
        );
        framing::send(
            &mut stream,
            &ClientRequest::Key(crate::server::protocol::SerializableKeyEvent::from(key)),
        )
        .await?;
    }
    framing::send(&mut stream, &ClientRequest::Detach).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// Start a mini server on a Unix socket pair and return the client stream
    /// and a handle to drive the server side.
    async fn setup_test_server() -> (
        UnixStream,
        tokio::task::JoinHandle<()>,
        Arc<Mutex<ServerState>>,
    ) {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let config = Config::default();

        let state = ServerState::new(&event_tx, 80, 24, config).unwrap();
        let state = Arc::new(Mutex::new(state));
        let id_map = Arc::new(Mutex::new(IdMap::new()));
        let clients = ClientRegistry::new();
        let client_id = 0u64;

        let (broadcast_tx, _) = broadcast::channel::<ServerResponse>(256);
        let broadcast_rx = broadcast_tx.subscribe();

        // Process events in background
        let state_clone = Arc::clone(&state);
        let btx_clone = broadcast_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    AppEvent::PtyOutput { pane_id, bytes } => {
                        {
                            let mut s = state_clone.lock().await;
                            if let Some(pane) = s.find_tab_mut(pane_id) {
                                pane.process_output(&bytes);
                            }
                        }
                        let _ = btx_clone.send(ServerResponse::PaneOutput {
                            pane_id,
                            data: bytes,
                        });
                    }
                    AppEvent::PtyExited { pane_id } => {
                        let _ = btx_clone.send(ServerResponse::PaneExited { pane_id });
                    }
                    _ => {}
                }
            }
        });

        let state_clone = Arc::clone(&state);
        let handle = tokio::spawn(async move {
            let _ = handle_client(
                server_stream,
                state_clone,
                id_map,
                broadcast_tx,
                broadcast_rx,
                clients,
                client_id,
            )
            .await;
        });

        (client_stream, handle, state)
    }

    /// Helper: attach to the server and consume the initial Attached + LayoutChanged + ClientListChanged messages.
    async fn attach_and_consume_initial(stream: &mut UnixStream) {
        framing::send(stream, &ClientRequest::Attach).await.unwrap();

        // Read Attached
        let resp: ServerResponse = framing::recv_required(stream).await.unwrap();
        assert!(matches!(resp, ServerResponse::Attached { .. }));

        // Consume LayoutChanged and ClientListChanged in any order
        for _ in 0..2 {
            let resp: ServerResponse = framing::recv_required(stream).await.unwrap();
            match resp {
                ServerResponse::LayoutChanged { .. } | ServerResponse::ClientListChanged(_) => {}
                other => panic!(
                    "expected LayoutChanged or ClientListChanged, got {:?}",
                    other
                ),
            }
        }
    }

    #[tokio::test]
    async fn test_connect_attach_detach() {
        let (mut client, handle, _state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        // Detach
        framing::send(&mut client, &ClientRequest::Detach)
            .await
            .unwrap();

        // Server handle should finish
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_resize_triggers_layout_update() {
        let (mut client, handle, _state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        // Send Resize
        framing::send(
            &mut client,
            &ClientRequest::Resize {
                width: 120,
                height: 40,
            },
        )
        .await
        .unwrap();

        // Should receive a LayoutChanged
        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            matches!(resp, ServerResponse::LayoutChanged { .. }),
            "expected LayoutChanged, got {:?}",
            resp
        );

        framing::send(&mut client, &ClientRequest::Detach)
            .await
            .unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_mouse_down_triggers_layout_update() {
        let (mut client, handle, _state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        framing::send(&mut client, &ClientRequest::MouseDown { x: 1, y: 4 })
            .await
            .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            matches!(resp, ServerResponse::LayoutChanged { .. }),
            "expected LayoutChanged after mouse click, got {:?}",
            resp
        );

        framing::send(&mut client, &ClientRequest::Detach)
            .await
            .unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_list_windows() {
        let (mut client, handle, _state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        // Send list-windows command
        framing::send(
            &mut client,
            &ClientRequest::Command("list-windows".to_string()),
        )
        .await
        .unwrap();

        // list-windows returns Ok(output), which daemon sends as Error("[cmd] ...")
        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::Error(msg) => {
                assert!(
                    msg.contains("@0:"),
                    "expected window listing with @0, got: {}",
                    msg
                );
            }
            other => panic!("expected Error with command output, got {:?}", other),
        }

        framing::send(&mut client, &ClientRequest::Detach)
            .await
            .unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_list_panes() {
        let (mut client, handle, _state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        framing::send(
            &mut client,
            &ClientRequest::Command("list-panes".to_string()),
        )
        .await
        .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::Error(msg) => {
                assert!(
                    msg.contains("%0:"),
                    "expected pane listing with %0, got: {}",
                    msg
                );
            }
            other => panic!("expected Error with command output, got {:?}", other),
        }

        framing::send(&mut client, &ClientRequest::Detach)
            .await
            .unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_rename_session() {
        let (mut client, handle, state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        framing::send(
            &mut client,
            &ClientRequest::Command("rename-session new-name".to_string()),
        )
        .await
        .unwrap();

        // rename-session returns Ok(""), so no response broadcast
        // Verify state changed directly
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let s = state.lock().await;
        assert_eq!(s.active_workspace().name, "new-name");
        drop(s);

        framing::send(&mut client, &ClientRequest::Detach)
            .await
            .unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_client_registry_min_size() {
        let registry = ClientRegistry::new();
        assert_eq!(registry.min_size().await, None);

        registry.register(1, 120, 40, 0).await;
        assert_eq!(registry.min_size().await, Some((120, 40)));

        registry.register(2, 80, 24, 0).await;
        assert_eq!(registry.min_size().await, Some((80, 24)));

        registry.register(3, 100, 30, 0).await;
        assert_eq!(registry.min_size().await, Some((80, 24)));

        registry.unregister(2).await;
        assert_eq!(registry.min_size().await, Some((100, 30)));

        registry.unregister(1).await;
        assert_eq!(registry.min_size().await, Some((100, 30)));

        registry.unregister(3).await;
        assert_eq!(registry.min_size().await, None);
    }

    #[tokio::test]
    async fn test_client_registry_update_size() {
        let registry = ClientRegistry::new();

        registry.register(1, 120, 40, 0).await;
        registry.register(2, 80, 24, 0).await;
        assert_eq!(registry.min_size().await, Some((80, 24)));

        // Client 2 resizes larger
        registry.update_size(2, 200, 50).await;
        assert_eq!(registry.min_size().await, Some((120, 40)));
    }

    // --- CommandSync protocol tests ---

    #[tokio::test]
    async fn test_command_sync_list_windows() {
        let (mut client, handle, _state) = setup_test_server().await;

        // Send CommandSync as the first message (no Attach needed)
        framing::send(
            &mut client,
            &ClientRequest::CommandSync("list-windows".to_string()),
        )
        .await
        .unwrap();

        // Should receive a CommandOutput response
        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::CommandOutput {
                output, success, ..
            } => {
                assert!(success, "expected success, got output: {}", output);
                assert!(
                    output.contains("@0"),
                    "expected window @0 in output: {}",
                    output
                );
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        // Server should disconnect after CommandSync
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_sync_rename_session() {
        let (mut client, handle, state) = setup_test_server().await;

        framing::send(
            &mut client,
            &ClientRequest::CommandSync("rename-session sync-name".to_string()),
        )
        .await
        .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::CommandOutput { success, .. } => {
                assert!(success);
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        // Verify the state was actually changed
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let s = state.lock().await;
        assert_eq!(s.active_workspace().name, "sync-name");
        drop(s);

        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_sync_parse_error() {
        let (mut client, handle, _state) = setup_test_server().await;

        framing::send(
            &mut client,
            &ClientRequest::CommandSync("nonexistent-command --foo".to_string()),
        )
        .await
        .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::CommandOutput {
                success, output, ..
            } => {
                assert!(!success, "expected failure for unknown command");
                assert!(
                    output.contains("parse error"),
                    "expected parse error, got: {}",
                    output
                );
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    // --- OkWithId tests (SplitWindow, NewWindow, NewSession) ---

    #[tokio::test]
    async fn test_command_sync_split_window_returns_id() {
        let (mut client, handle, _state) = setup_test_server().await;

        framing::send(
            &mut client,
            &ClientRequest::CommandSync("split-window -h".to_string()),
        )
        .await
        .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::CommandOutput {
                success,
                pane_id,
                window_id,
                ..
            } => {
                assert!(success, "split-window should succeed");
                assert!(pane_id.is_some(), "split-window should return a pane_id");
                assert!(
                    window_id.is_some(),
                    "split-window should return a window_id"
                );
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_sync_new_window_returns_id() {
        let (mut client, handle, _state) = setup_test_server().await;

        framing::send(
            &mut client,
            &ClientRequest::CommandSync("new-window".to_string()),
        )
        .await
        .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::CommandOutput {
                success,
                pane_id,
                window_id,
                ..
            } => {
                assert!(success, "new-window should succeed");
                assert!(pane_id.is_some(), "new-window should return a pane_id");
                assert!(window_id.is_some(), "new-window should return a window_id");
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_sync_new_session_returns_id() {
        let (mut client, handle, _state) = setup_test_server().await;

        framing::send(
            &mut client,
            &ClientRequest::CommandSync("new-session -d -s inner".to_string()),
        )
        .await
        .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::CommandOutput {
                success,
                pane_id,
                window_id,
                ..
            } => {
                assert!(success, "new-session should succeed");
                assert!(pane_id.is_some(), "new-session should return a pane_id");
                assert!(window_id.is_some(), "new-session should return a window_id");
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_sync_display_message() {
        let (mut client, handle, _state) = setup_test_server().await;

        framing::send(
            &mut client,
            &ClientRequest::CommandSync("display-message -p #{session_name}".to_string()),
        )
        .await
        .unwrap();

        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        match resp {
            ServerResponse::CommandOutput {
                success, output, ..
            } => {
                assert!(success);
                // session_name now resolves to the active workspace name
                assert!(!output.is_empty(), "should expand session name to workspace name");
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_select_pane_with_title() {
        let (mut client, handle, state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        // First list-panes to register %0 in the IdMap
        framing::send(
            &mut client,
            &ClientRequest::Command("list-panes".to_string()),
        )
        .await
        .unwrap();
        let _resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();

        // Now select-pane with -T to set a title
        framing::send(
            &mut client,
            &ClientRequest::Command("select-pane -t %0 -T my-title".to_string()),
        )
        .await
        .unwrap();

        // select-pane returns LayoutChanged (broadcast)
        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            matches!(resp, ServerResponse::LayoutChanged { .. }),
            "expected LayoutChanged, got {:?}",
            resp
        );

        // Verify the title was set on the pane
        let s = state.lock().await;
        let ws = s.active_workspace();
        let group = ws.groups.get(&ws.active_group).unwrap();
        assert_eq!(group.active_tab().title, "my-title");
        drop(s);

        framing::send(&mut client, &ClientRequest::Detach)
            .await
            .unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }
}
