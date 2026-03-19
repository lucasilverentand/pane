#![allow(dead_code)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, Mutex};

use pane_protocol::config::Config;
use pane_protocol::event::AppEvent;
use crate::server::command_parser;
use pane_protocol::framing;
use crate::server::id_map::IdMap;
use pane_protocol::protocol::{
    ClientRequest, SerializableSystemStats, ServerResponse,
};
use crate::server::state::{ServerState, render_state_from_server, render_state_for_client};
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
}

/// Returns the socket directory: $TMPDIR/pane-{uid}/ or /tmp/pane-{uid}/
pub fn socket_dir() -> PathBuf {
    let uid = nix::unistd::getuid();
    let base = std::env::var("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    base.join(format!("pane-{}", uid))
}

/// Returns the socket path for the pane daemon.
/// Debug builds automatically use `pane-dev.sock` to avoid colliding with release installs.
/// Set `PANE_SOCKET` to override (e.g. `PANE_SOCKET=test pane` → `pane-test.sock`).
pub fn socket_path() -> PathBuf {
    socket_dir().join(format!("{}.sock", instance_prefix()))
}

/// Returns the version file path.
fn instance_prefix() -> String {
    match std::env::var("PANE_SOCKET") {
        Ok(name) if !name.is_empty() => format!("pane-{}", name),
        _ if cfg!(debug_assertions) => "pane-dev".to_string(),
        _ => "pane".to_string(),
    }
}

fn version_path() -> PathBuf {
    socket_dir().join(format!("{}.version", instance_prefix()))
}

/// Returns the PID file path.
fn pid_path() -> PathBuf {
    socket_dir().join(format!("{}.pid", instance_prefix()))
}

/// Returns the log file path.
fn log_path() -> PathBuf {
    socket_dir().join("pane.log")
}

/// Run the server daemon.
pub async fn run_server(config: Config) -> Result<()> {
    let sock_dir = socket_dir();
    std::fs::create_dir_all(&sock_dir)?;

    let sock_path = socket_path();
    cleanup_stale_socket(&sock_path);

    let listener = UnixListener::bind(&sock_path)?;

    // Write version and PID files so clients can detect version mismatches
    std::fs::write(version_path(), env!("CARGO_PKG_VERSION"))?;
    std::fs::write(pid_path(), std::process::id().to_string())?;

    // Channel for internal events (PTY output, stats, etc.)
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Start system stats collector
    system_stats::start_stats_collector(event_tx.clone(), config.status_bar.update_interval_secs);

    // Periodic foreground process polling (every 2s)
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                if tx.send(AppEvent::ForegroundPoll).is_err() {
                    break;
                }
            }
        });
    }

    let auto_suspend_secs = config.behavior.auto_suspend_secs;

    let state = ServerState::new(&event_tx, 80, 24, config);
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
    let broadcast_tx_term = broadcast_tx.clone();
    let sock_path_clone = sock_path.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        let _ = broadcast_tx_term.send(ServerResponse::SessionEnded);
        let _ = std::fs::remove_file(&sock_path_clone);
        let _ = std::fs::remove_file(version_path());
        let _ = std::fs::remove_file(pid_path());
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
                            let _ = broadcast_tx_suspend.send(ServerResponse::SessionEnded);
                            let _ = std::fs::remove_file(&sock_path_suspend);
                            let _ = std::fs::remove_file(version_path());
                            let _ = std::fs::remove_file(pid_path());
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
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(version_path());
    let _ = std::fs::remove_file(pid_path());

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
                let fg_changed = {
                    let mut state = state.lock().await;
                    if let Some(pane) = state.find_tab_mut(pane_id) {
                        // Catch panics in vt100 processing so a single pane
                        // can't take down the entire daemon.
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            pane.process_output(&bytes)
                        }));
                        match result {
                            Ok(changed) => changed,
                            Err(_) => {
                                eprintln!(
                                    "pane: caught panic in process_output for pane {:?}",
                                    pane_id
                                );
                                false
                            }
                        }
                    } else {
                        false
                    }
                };
                let _ = broadcast_tx.send(ServerResponse::PaneOutput {
                    pane_id,
                    data: bytes,
                });
                if fg_changed {
                    let state = state.lock().await;
                    let render_state = render_state_from_server(&state);
                    let _ = broadcast_tx.send(ServerResponse::LayoutChanged {
                        render_state,
                    });
                }
            }
            AppEvent::PtyExited { pane_id } => {
                let should_quit = {
                    let mut state = state.lock().await;
                    let quit = state.handle_pty_exited(pane_id);
                    if !quit {
                        let (w, h) = state.last_size;
                        state.resize_all_tabs(w, h);
                        let render_state = render_state_from_server(&state);
                        let _ = broadcast_tx.send(ServerResponse::LayoutChanged {
                            render_state,
                        });
                    }
                    quit
                };
                let _ = broadcast_tx.send(ServerResponse::PaneExited { pane_id });
                if should_quit {
                    let _ = broadcast_tx.send(ServerResponse::SessionEnded);
                    break;
                }
            }
            AppEvent::SystemStats(stats) => {
                {
                    let mut state = state.lock().await;
                    state.system_stats = stats.clone();
                }
                let serializable = SerializableSystemStats::from(&stats);
                let _ = broadcast_tx.send(ServerResponse::StatsUpdate(serializable));
            }
            AppEvent::ForegroundPoll => {
                let any_changed = {
                    let mut state = state.lock().await;
                    let mut changed = false;
                    for ws in &mut state.workspaces {
                        for group in ws.groups.values_mut() {
                            for tab in &mut group.tabs {
                                if tab.update_foreground_process() {
                                    changed = true;
                                }
                            }
                        }
                    }
                    changed
                };
                if any_changed {
                    let state = state.lock().await;
                    let render_state = render_state_from_server(&state);
                    let _ = broadcast_tx.send(ServerResponse::LayoutChanged {
                        render_state,
                    });
                }
            }
            AppEvent::Tick => {
                // No-op for daemon mode — ticks are client-side
            }
            // These events come from clients, not the internal event loop
            AppEvent::Key(_)
            | AppEvent::MouseDown { .. }
            | AppEvent::MouseRightDown { .. }
            | AppEvent::MouseDrag { .. }
            | AppEvent::MouseMove { .. }
            | AppEvent::MouseUp { .. }
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
                            let render_state = render_state_from_server(&state_guard);
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

    if !matches!(&first_msg, ClientRequest::Attach) {
        framing::send(
            &mut stream,
            &ServerResponse::Error("expected Attach as first message".to_string()),
        )
        .await?;
        return Ok(());
    }

    // Send attached confirmation
    framing::send(&mut stream, &ServerResponse::Attached).await?;

    // Register client with default size and current active workspace
    {
        let state_guard = state.lock().await;
        let (w, h) = state_guard.last_size;
        clients
            .register(client_id, w, h, state_guard.active_workspace)
            .await;
        let count = clients.count().await as u32;
        let _ = broadcast_tx.send(ServerResponse::ClientCountChanged(count));
    }

    // Send initial layout state using this client's active workspace
    {
        let state = state.lock().await;
        let client_ws = clients.get_active_workspace(client_id).await.unwrap_or(0);
        let render_state = render_state_for_client(&state, client_ws);
        framing::send(&mut stream, &ServerResponse::LayoutChanged { render_state }).await?;

        // Collect current screen content for all panes so the client can render
        // output that arrived before it connected.
        let mut screen_dumps: Vec<(pane_protocol::layout::TabId, Vec<u8>)> = Vec::new();
        for ws in &state.workspaces {
            for group in ws.groups.values() {
                for pane in &group.tabs {
                    let data = pane.screen().state_formatted();
                    if !data.is_empty() {
                        screen_dumps.push((pane.id, data));
                    }
                }
            }
        }
        drop(state); // release lock before sending
        for (pane_id, data) in screen_dumps {
            framing::send(
                &mut stream,
                &ServerResponse::FullScreenDump { pane_id, data },
            )
            .await?;
        }
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
                let render_state = render_state_from_server(&state);
                let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
            }
            ClientRequest::Key(sk) => {
                let key_event = sk.into();
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                // Set state to this client's active workspace
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                // Forward key to the active pane as raw bytes
                let ws = state.active_workspace_mut();
                let group = ws.groups.get_mut(&ws.active_group);
                if let Some(group) = group {
                    let pane = group.active_tab_mut();
                    let app_cursor = pane.screen().application_cursor();
                    let bytes = pane_protocol::keys::key_to_bytes(key_event, app_cursor);
                    if !bytes.is_empty() {
                        pane.write_input(&bytes);
                    }
                }
            }
            ClientRequest::MouseDown { x, y } => {
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                handle_mouse_down_server(&mut state, x, y);
                let cws = state.active_workspace;
                let render_state = render_state_for_client(&state, cws);
                let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
            }
            ClientRequest::MouseDrag { x, y } => {
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                let had_drag = state.drag_state.is_some();
                handle_mouse_drag_server(&mut state, x, y);
                if had_drag {
                    let cws = state.active_workspace;
                    let render_state = render_state_for_client(&state, cws);
                    let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
                } else {
                    // Forward drag to PTY if the process wants button-motion events
                    forward_mouse_to_pty(&mut state, 32, x, y, true);
                }
            }
            ClientRequest::MouseMove { x, y } => {
                // Forward motion to PTY if the process wants any-motion events
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                forward_mouse_to_pty(&mut state, 35, x, y, true);
            }
            ClientRequest::MouseUp { x, y } => {
                let had_drag = state.lock().await.drag_state.is_some();
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                if had_drag {
                    state.drag_state = None;
                    let (w, h) = state.last_size;
                    state.resize_all_tabs(w, h);
                    let cws = state.active_workspace;
                    let render_state = render_state_for_client(&state, cws);
                    let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
                } else {
                    forward_mouse_to_pty(&mut state, 0, x, y, false);
                }
            }
            ClientRequest::MouseScroll { up } => {
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                // Forward scroll to PTY if the process wants mouse events
                let mouse_mode = state
                    .active_workspace()
                    .groups
                    .get(&state.active_workspace().active_group)
                    .map(|g| g.active_tab().screen().mouse_protocol_mode())
                    .unwrap_or(vt100::MouseProtocolMode::None);
                if mouse_mode != vt100::MouseProtocolMode::None {
                    let encoding = state
                        .active_workspace()
                        .groups
                        .get(&state.active_workspace().active_group)
                        .map(|g| g.active_tab().screen().mouse_protocol_encoding())
                        .unwrap_or(vt100::MouseProtocolEncoding::Default);
                    let button = if up { 64u8 } else { 65u8 };
                    let bytes = encode_mouse_sgr(button, 0, 0, true, encoding);
                    state.active_workspace_mut().active_group_mut().active_tab_mut().write_input(&bytes);
                } else if up {
                    state.scroll_active_tab(|p| p.scroll_up(3));
                } else {
                    state.scroll_active_tab(|p| p.scroll_down(3));
                }
            }
            ClientRequest::Paste(text) => {
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                let bytes = text.into_bytes();
                if !bytes.is_empty() {
                    let ws = state.active_workspace_mut();
                    if let Some(group) = ws.groups.get_mut(&ws.active_group) {
                        group.active_tab_mut().write_input(&bytes);
                    }
                }
            }
            ClientRequest::Command(cmd) => {
                if handle_command(&cmd, &state, &id_map, &broadcast_tx, &clients, client_id).await {
                    break;
                }
            }
            ClientRequest::FocusWindow { id } => {
                let mut state = state.lock().await;
                if state.workspaces.is_empty() { continue; }
                if let Some(cws) = clients.get_active_workspace(client_id).await {
                    state.active_workspace = cws;
                }
                let bar_h = state.workspace_bar_height();
                state.focus_group(id, bar_h);
                let cws = state.active_workspace;
                let render_state = render_state_for_client(&state, cws);
                let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
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
        let count = clients.count().await as u32;
        let _ = broadcast_tx.send(ServerResponse::ClientCountChanged(count));
    }
    if let Some((eff_w, eff_h)) = clients.min_size().await {
        let mut state = state.lock().await;
        state.last_size = (eff_w, eff_h);
        state.resize_all_tabs(eff_w, eff_h);
        let render_state = render_state_from_server(&state);
        let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
    }

    Ok(())
}

/// Handle mouse down events server-side (pane focus changes).
fn handle_mouse_down_server(state: &mut ServerState, x: u16, y: u16) {
    state.drag_state = None;

    let bar_h = state.workspace_bar_height();
    let (w, h) = state.last_size;
    let body_height = h.saturating_sub(1 + bar_h);
    let body = ratatui::layout::Rect::new(0, bar_h, w, body_height);

    // Check for split border hit (drag initiation)
    {
        let ws = state.active_workspace();
        if let Some((path, direction, split_rect)) = ws.layout.hit_test_split_border(body, x, y) {
            state.drag_state = Some(crate::server::state::DragState {
                split_path: path,
                direction,
                body: split_rect,
            });
            return;
        }
    }

    let ws = state.active_workspace();
    let resolved = ws
        .layout
        .resolve_with_folds(body, &ws.folded_windows);

    // Check fold bar clicks
    for rp in &resolved {
        if let pane_protocol::layout::ResolvedPane::Folded {
            id: group_id, rect, ..
        } = rp
        {
            if rect.width == 0 || rect.height == 0 {
                continue;
            }
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                let group_id = *group_id;
                state.focus_group(group_id, bar_h);
                return;
            }
        }
    }

    // Check visible pane clicks for focus (with tab bar hit testing)
    for rp in &resolved {
        if let pane_protocol::layout::ResolvedPane::Visible {
            id: group_id, rect, ..
        } = rp
        {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                // Check tab bar click before focusing the group
                if let Some(group) = state.active_workspace().groups.get(group_id) {
                    if let Some(tab_area) = crate::tab_bar::tab_bar_area(group, *rect) {
                        let layout = crate::tab_bar::tab_bar_layout(
                            group,
                            &state.config.theme,
                            tab_area,
                        );
                        if let Some(click) = crate::tab_bar::tab_bar_hit_test(&layout, x, y)
                        {
                            state.active_workspace_mut().active_group = *group_id;
                            match click {
                                crate::tab_bar::TabBarClick::Tab(i) => {
                                    state.active_workspace_mut().active_group_mut().active_tab = i;
                                }
                                crate::tab_bar::TabBarClick::NewTab => {
                                    let cols = w.saturating_sub(4);
                                    let rows = h.saturating_sub(1 + bar_h + 2 + 2);
                                    let _ = state.add_tab_to_active_group(
                                        crate::window::TabKind::Shell,
                                        None,
                                        None,
                                        cols,
                                        rows,
                                    );
                                }
                            }
                            return;
                        }
                    }
                }
                let group_id = *group_id;
                state.active_workspace_mut().active_group = group_id;

                // Forward mouse press to PTY if the process wants mouse events
                if let Some(content_rect) = window_content_rect(*rect) {
                    if let Some(group) = state.active_workspace().groups.get(&group_id) {
                        let tab = group.active_tab();
                        let mode = tab.screen().mouse_protocol_mode();
                        if mode != vt100::MouseProtocolMode::None {
                            let encoding = tab.screen().mouse_protocol_encoding();
                            let local_x = x.saturating_sub(content_rect.x);
                            let local_y = y.saturating_sub(content_rect.y);
                            let bytes = encode_mouse_sgr(0, local_x, local_y, true, encoding);
                            state.active_workspace_mut().active_group_mut().active_tab_mut().write_input(&bytes);
                        }
                    }
                }

                return;
            }
        }
    }
}

/// Forward a mouse event to the active tab's PTY, translating coordinates to the content area.
/// `button`: SGR button code (0=left, 1=mid, 2=right, 32=motion+left, 35=motion, 64/65=scroll)
fn forward_mouse_to_pty(state: &mut ServerState, button: u8, x: u16, y: u16, pressed: bool) {
    let bar_h = state.workspace_bar_height();
    let (w, h) = state.last_size;
    let body_height = h.saturating_sub(1 + bar_h);
    let body = ratatui::layout::Rect::new(0, bar_h, w, body_height);

    let active_group_id = state.active_workspace().active_group;
    let ws = state.active_workspace();
    let resolved = ws.layout.resolve_with_folds(body, &ws.folded_windows);
    for rp in &resolved {
        if let pane_protocol::layout::ResolvedPane::Visible { id, rect, .. } = rp {
            if *id == active_group_id {
                if let Some(content_rect) = window_content_rect(*rect) {
                    if let Some(group) = state.active_workspace().groups.get(id) {
                        let tab = group.active_tab();
                        let mode = tab.screen().mouse_protocol_mode();
                        if mode == vt100::MouseProtocolMode::None {
                            return;
                        }
                        let encoding = tab.screen().mouse_protocol_encoding();
                        let local_x = x.saturating_sub(content_rect.x);
                        let local_y = y.saturating_sub(content_rect.y);
                        let bytes = encode_mouse_sgr(button, local_x, local_y, pressed, encoding);
                        state.active_workspace_mut().active_group_mut().active_tab_mut().write_input(&bytes);
                    }
                }
                return;
            }
        }
    }
}

/// Compute the content area (where terminal output is rendered) within a window rect.
/// This accounts for the border, padding, tab bar, and separator.
fn window_content_rect(rect: ratatui::layout::Rect) -> Option<ratatui::layout::Rect> {
    use ratatui::widgets::{Block, Borders, BorderType};
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(rect);
    if inner.width <= 2 || inner.height <= 2 {
        return None;
    }
    // Padded area: 1 char padding on each side, then tab bar (1 row) + separator (1 row)
    Some(ratatui::layout::Rect::new(
        inner.x + 1,
        inner.y + 2,
        inner.width - 2,
        inner.height.saturating_sub(2),
    ))
}

/// Encode a mouse event for forwarding to the PTY.
/// `button`: 0=left, 1=middle, 2=right, 64=scroll-up, 65=scroll-down
/// `pressed`: true for press, false for release
fn encode_mouse_sgr(
    button: u8,
    x: u16,
    y: u16,
    pressed: bool,
    encoding: vt100::MouseProtocolEncoding,
) -> Vec<u8> {
    match encoding {
        vt100::MouseProtocolEncoding::Sgr => {
            // SGR format: \x1b[<Btn;X;YM (press) or \x1b[<Btn;X;Ym (release)
            let suffix = if pressed { 'M' } else { 'm' };
            format!("\x1b[<{};{};{}{}", button, x + 1, y + 1, suffix).into_bytes()
        }
        _ => {
            // Default/UTF-8 encoding: \x1b[M CbCxCy (all +32, release=3)
            let cb = if pressed { button + 32 } else { 3 + 32 };
            let cx = (x as u8).saturating_add(33);
            let cy = (y as u8).saturating_add(33);
            vec![0x1b, b'[', b'M', cb, cx, cy]
        }
    }
}

/// Handle mouse drag events server-side (split resizing).
fn handle_mouse_drag_server(state: &mut ServerState, x: u16, y: u16) {
    let drag = match &state.drag_state {
        Some(d) => d.clone(),
        None => return,
    };

    let body = drag.body;
    let new_ratio = match drag.direction {
        pane_protocol::layout::SplitDirection::Horizontal => {
            if body.width == 0 {
                return;
            }
            ((x.saturating_sub(body.x)) as f64) / (body.width as f64)
        }
        pane_protocol::layout::SplitDirection::Vertical => {
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

/// Kill the daemon by PID (SIGTERM) and wait for it to exit.
/// Also cleans up socket, version, and PID files.
pub fn kill_daemon() {
    let pid_file = pid_path();
    if let Ok(contents) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = contents.trim().parse::<i32>() {
            kill_pid(pid);
        }
    }
    kill_orphan_daemons();
    let _ = std::fs::remove_file(socket_path());
    let _ = std::fs::remove_file(pid_file);
    let _ = std::fs::remove_file(version_path());
}

/// Send SIGTERM to a PID and wait up to 1 second for it to exit.
fn kill_pid(pid: i32) {
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGTERM,
    );
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err() {
            return;
        }
    }
}

/// Find and kill any `pane daemon` processes that aren't tracked by
/// the PID file.
fn kill_orphan_daemons() {
    let my_pid = std::process::id() as i32;
    let Ok(output) = std::process::Command::new("pgrep")
        .args(["-f", "pane daemon"])
        .output()
    else {
        return;
    };
    let Ok(stdout) = std::str::from_utf8(&output.stdout) else {
        return;
    };
    for line in stdout.lines() {
        if let Ok(pid) = line.trim().parse::<i32>() {
            if pid != my_pid {
                kill_pid(pid);
            }
        }
    }
}

/// Start the daemon in the background.
/// Forks the current exe with `daemon` arg, detaches stdio,
/// and waits for the socket to appear (up to 5 seconds).
/// If a daemon is already running with a different version, it is restarted.
pub fn start_daemon() -> Result<()> {
    let exe = std::env::current_exe()?;
    let sock = socket_path();

    // If socket already exists and daemon is alive, check if restart is needed
    if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
        let version_mismatch = match std::fs::read_to_string(version_path()) {
            Ok(v) => v.trim() != env!("CARGO_PKG_VERSION"),
            Err(_) => true,
        };

        // In debug builds, also restart if the binary is newer than the daemon
        let binary_changed = cfg!(debug_assertions) && {
            let pid_file = pid_path();
            match (exe.metadata(), pid_file.metadata()) {
                (Ok(exe_meta), Ok(pid_meta)) => {
                    match (exe_meta.modified(), pid_meta.modified()) {
                        (Ok(exe_time), Ok(pid_time)) => exe_time > pid_time,
                        _ => false,
                    }
                }
                _ => false,
            }
        };

        if version_mismatch || binary_changed {
            let reason = if version_mismatch { "version mismatch" } else { "binary changed" };
            eprintln!("pane: restarting daemon ({})", reason);
            kill_daemon();
        } else {
            return Ok(());
        }
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
    let log = log_path();
    let log_file = std::fs::File::create(&log).unwrap_or_else(|_| {
        std::fs::File::open("/dev/null").unwrap()
    });
    Command::new(exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file))
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
pub async fn kill_session() -> Result<()> {
    let path = socket_path();
    if !path.exists() {
        anyhow::bail!("no running daemon");
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

/// Send keys to the running daemon.
pub async fn send_keys(keys: &str) -> Result<()> {
    let path = socket_path();
    if !path.exists() {
        anyhow::bail!("no running daemon");
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
            &ClientRequest::Key(pane_protocol::protocol::SerializableKeyEvent::from(key)),
        )
        .await?;
    }
    framing::send(&mut stream, &ClientRequest::Detach).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pane_protocol::config::Config;

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

        let state = ServerState::new_with_workspace(&event_tx, 80, 24, config)
            .unwrap();
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
                                let _ = pane.process_output(&bytes);
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

    /// Helper: attach to the server and consume the initial Attached + LayoutChanged + ClientCountChanged messages.
    async fn attach_and_consume_initial(stream: &mut UnixStream) {
        framing::send(stream, &ClientRequest::Attach).await.unwrap();

        // Read Attached
        let resp: ServerResponse = framing::recv_required(stream).await.unwrap();
        assert!(matches!(resp, ServerResponse::Attached));

        // Consume LayoutChanged, ClientCountChanged, and any FullScreenDump messages
        let mut seen_layout = false;
        let mut seen_count = false;
        loop {
            let resp: ServerResponse = framing::recv_required(stream).await.unwrap();
            match resp {
                ServerResponse::LayoutChanged { .. } => seen_layout = true,
                ServerResponse::ClientCountChanged(_) => seen_count = true,
                ServerResponse::FullScreenDump { .. } => {}
                other => panic!(
                    "expected LayoutChanged, ClientCountChanged, or FullScreenDump, got {:?}",
                    other
                ),
            }
            if seen_layout && seen_count {
                break;
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
    async fn test_command_rename_workspace() {
        let (mut client, handle, state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        framing::send(
            &mut client,
            &ClientRequest::Command("rename-workspace new-name".to_string()),
        )
        .await
        .unwrap();

        // rename-workspace broadcasts LayoutChanged
        let resp: ServerResponse = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            framing::recv_required(&mut client),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(resp, ServerResponse::LayoutChanged { .. }));

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
    async fn test_command_sync_rename_workspace() {
        let (mut client, handle, state) = setup_test_server().await;

        framing::send(
            &mut client,
            &ClientRequest::CommandSync("rename-workspace sync-name".to_string()),
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
                // #{session_name} now expands to the active workspace name
                assert!(!output.is_empty(), "should expand to workspace name");
            }
            other => panic!("expected CommandOutput, got {:?}", other),
        }

        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    #[tokio::test]
    async fn test_command_select_pane_with_title() {
        let (mut client, handle, state) = setup_test_server().await;

        attach_and_consume_initial(&mut client).await;

        // First list-panes to register pane IDs in the IdMap
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
        // Find the shell pane number from list-panes output
        let shell_pane_id = match &resp {
            ServerResponse::CommandOutput { output, .. } => {
                output.lines()
                    .find(|l| l.contains("[shell]"))
                    .and_then(|l| l.split(':').next())
                    .unwrap_or("%0")
                    .to_string()
            }
            _ => "%0".to_string(),
        };

        // Now select-pane with -T to set a title on the shell pane
        framing::send(
            &mut client,
            &ClientRequest::Command(format!("select-pane -t {} -T my-title", shell_pane_id)),
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

    #[tokio::test]
    async fn test_full_screen_dump_alt_screen() {
        let (_client, _handle, state) = setup_test_server().await;

        // Feed btop-like alt-screen output into the first tab
        let mut s = state.lock().await;
        let ws = s.active_workspace_mut();
        let group = ws.groups.get_mut(&ws.active_group).unwrap();
        let tab = group.active_tab_mut();

        // Simulate btop: enter alt screen, cursor-addressed frame
        let mut output = Vec::new();
        output.extend_from_slice(b"\x1b[?1049h"); // enter alt screen
        output.extend_from_slice(b"\x1b[?25l"); // hide cursor
        for row in 0..24u16 {
            output.extend_from_slice(format!("\x1b[{};1H", row + 1).as_bytes());
            output.extend_from_slice(format!("btop row {:>2}", row).as_bytes());
        }
        tab.process_output(&output);

        // Verify alt screen is active
        assert!(tab.screen().alternate_screen());

        // Get state_formatted and round-trip through a fresh parser
        let formatted = tab.screen().state_formatted();
        drop(s);

        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(&formatted);

        assert!(
            parser.screen().alternate_screen(),
            "round-tripped parser should be in alt screen mode"
        );

        // Verify content matches cell-by-cell for first row
        let expected = "btop row  0";
        for (col, ch) in expected.chars().enumerate() {
            assert_eq!(
                parser.screen().cell(0, col as u16).unwrap().contents(),
                ch.to_string(),
                "mismatch at row 0, col {col}"
            );
        }
    }

    #[tokio::test]
    async fn test_full_screen_dump_main_screen() {
        let (_client, _handle, state) = setup_test_server().await;

        // Feed normal main-screen output
        let mut s = state.lock().await;
        let ws = s.active_workspace_mut();
        let group = ws.groups.get_mut(&ws.active_group).unwrap();
        let tab = group.active_tab_mut();

        tab.process_output(b"$ hello world\r\n$ ls -la\r\n");

        assert!(!tab.screen().alternate_screen());

        let formatted = tab.screen().state_formatted();
        drop(s);

        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(&formatted);

        assert!(
            !parser.screen().alternate_screen(),
            "round-tripped parser should be on main screen"
        );

        // Verify "$ hello world" on first row
        assert_eq!(parser.screen().cell(0, 0).unwrap().contents(), "$");
        assert_eq!(parser.screen().cell(0, 2).unwrap().contents(), "h");
    }
}
