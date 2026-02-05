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
use crate::session::{self, Session};
use crate::system_stats;

/// Global counter for assigning unique client IDs.
static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(0);

/// Registry of connected clients with their terminal sizes.
/// The server uses the smallest dimensions across all clients.
#[derive(Clone)]
struct ClientRegistry {
    inner: Arc<Mutex<HashMap<u64, (u16, u16)>>>,
}

impl ClientRegistry {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn register(&self, id: u64, width: u16, height: u16) {
        self.inner.lock().await.insert(id, (width, height));
    }

    async fn update_size(&self, id: u64, width: u16, height: u16) {
        self.inner.lock().await.insert(id, (width, height));
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
        let min_w = clients.values().map(|(w, _)| *w).min().unwrap_or(80);
        let min_h = clients.values().map(|(_, h)| *h).min().unwrap_or(24);
        Some((min_w, min_h))
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

/// Returns the socket path for a given session name.
pub fn socket_path(session_name: &str) -> PathBuf {
    socket_dir().join(format!("{}.sock", session_name))
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
pub async fn run_server(session_name: String, config: Config) -> Result<()> {
    let sock_dir = socket_dir();
    std::fs::create_dir_all(&sock_dir)?;

    let sock_path = socket_path(&session_name);
    cleanup_stale_socket(&sock_path);

    let listener = UnixListener::bind(&sock_path)?;

    // Channel for internal events (PTY output, stats, etc.)
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Start system stats collector
    system_stats::start_stats_collector(event_tx.clone(), config.status_bar.update_interval_secs);

    // Create initial server state
    let state = ServerState::new_session(session_name.clone(), &event_tx, 80, 24, config)?;
    let state = Arc::new(Mutex::new(state));

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
                            if let Err(e) =
                                handle_client(stream, state, id_map, broadcast_tx, broadcast_rx, clients, client_id)
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
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        // Save session and notify clients before exiting
        let state = state_clone.lock().await;
        let session = Session::from_state(&state);
        let _ = session::store::save(&session);
        let _ = broadcast_tx_term.send(ServerResponse::SessionEnded);
        let _ = std::fs::remove_file(&sock_path_clone);
        std::process::exit(0);
    });

    // Set up SIGHUP handler for config reload
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let mut sighup =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                .expect("failed to register SIGHUP handler");
        loop {
            sighup.recv().await;
            let new_config = Config::load();
            let mut state = state_clone.lock().await;
            state.config = new_config;
        }
    });

    // Wait for the event loop to finish (happens when all panes exit)
    event_loop.await?;

    // Clean up
    accept_loop.abort();
    let state = state.lock().await;
    let session = Session::from_state(&state);
    let _ = session::store::save(&session);
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
                    if let Some(pane) = state.find_pane_mut(pane_id) {
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
    // Read the initial Attach request
    let first_msg: ClientRequest = framing::recv_required(&mut stream).await?;
    let session_name = match &first_msg {
        ClientRequest::Attach { session_name } => session_name.clone(),
        _ => {
            framing::send(
                &mut stream,
                &ServerResponse::Error("expected Attach as first message".to_string()),
            )
            .await?;
            return Ok(());
        }
    };

    // Send attached confirmation
    framing::send(&mut stream, &ServerResponse::Attached { session_name }).await?;

    // Register client with default size; will be updated on first Resize
    {
        let state_guard = state.lock().await;
        let (w, h) = state_guard.last_size;
        clients.register(client_id, w, h).await;
    }

    // Send initial layout state
    {
        let state = state.lock().await;
        let render_state = RenderState::from_server_state(&state);
        framing::send(
            &mut stream,
            &ServerResponse::LayoutChanged { render_state },
        )
        .await?;
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
                state.resize_all_panes(eff_w, eff_h);
                let render_state = RenderState::from_server_state(&state);
                let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
            }
            ClientRequest::Key(sk) => {
                let key_event = sk.into();
                let mut state = state.lock().await;
                // Forward key to the active pane as raw bytes
                let ws = state.active_workspace_mut();
                let group = ws.groups.get_mut(&ws.active_group);
                if let Some(group) = group {
                    let pane = group.active_pane_mut();
                    let bytes = crate::app::key_to_bytes(key_event);
                    if !bytes.is_empty() {
                        pane.write_input(&bytes);
                    }
                }
            }
            ClientRequest::MouseDown { x, y } => {
                let mut state = state.lock().await;
                handle_mouse_down_server(&mut state, x, y);
            }
            ClientRequest::MouseDrag { x, y } => {
                let mut state = state.lock().await;
                handle_mouse_drag_server(&mut state, x, y);
            }
            ClientRequest::MouseMove { .. } => {
                // Mouse move is client-side only (hover effects)
            }
            ClientRequest::MouseUp => {
                // Drag state is client-side; no-op for now
            }
            ClientRequest::MouseScroll { up } => {
                let mut state = state.lock().await;
                if up {
                    state.scroll_active_pane(|p| p.scroll_up(3));
                } else {
                    state.scroll_active_pane(|p| p.scroll_down(3));
                }
            }
            ClientRequest::Command(cmd) => {
                handle_command(&cmd, &state, &id_map, &broadcast_tx).await;
            }
            ClientRequest::Attach { .. } => {
                // Already attached, ignore
            }
        }
    }

    forward_task.abort();

    // Client disconnected: unregister and recalculate effective size
    clients.unregister(client_id).await;
    if let Some((eff_w, eff_h)) = clients.min_size().await {
        let mut state = state.lock().await;
        state.last_size = (eff_w, eff_h);
        state.resize_all_panes(eff_w, eff_h);
        let render_state = RenderState::from_server_state(&state);
        let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
    }

    Ok(())
}

/// Handle mouse down events server-side (pane focus changes).
fn handle_mouse_down_server(state: &mut ServerState, x: u16, y: u16) {
    let bar_h = state.workspace_bar_height();
    let (w, h) = state.last_size;
    let body_height = h.saturating_sub(1 + bar_h);
    let body = ratatui::layout::Rect::new(0, bar_h, w, body_height);

    let params = crate::layout::LayoutParams::from(&state.config.behavior);
    let ws = state.active_workspace();
    let resolved = ws
        .layout
        .resolve_with_fold(body, params, &ws.leaf_min_sizes);

    // Check fold bar clicks
    for rp in &resolved {
        if let crate::layout::ResolvedPane::Folded {
            id: group_id,
            rect,
            ..
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

    // Check visible pane clicks for focus
    for rp in &resolved {
        if let crate::layout::ResolvedPane::Visible {
            id: group_id, rect, ..
        } = rp
        {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                state.active_workspace_mut().active_group = *group_id;
                return;
            }
        }
    }
}

/// Handle mouse drag events server-side (split resizing).
fn handle_mouse_drag_server(state: &mut ServerState, _x: u16, _y: u16) {
    // Drag state is maintained client-side for now.
    // A full implementation would track drag on the server.
    let _ = state;
}

/// Handle string commands from the command protocol.
async fn handle_command(
    cmd: &str,
    state: &Arc<Mutex<ServerState>>,
    id_map: &Arc<Mutex<IdMap>>,
    broadcast_tx: &broadcast::Sender<ServerResponse>,
) {
    match command_parser::parse(cmd) {
        Ok(parsed_cmd) => {
            let mut state = state.lock().await;
            let mut id_map = id_map.lock().await;
            match crate::server::command::execute(&parsed_cmd, &mut state, &mut id_map, broadcast_tx) {
                Ok(crate::server::command::CommandResult::Ok(output)) => {
                    if !output.is_empty() {
                        // Send output as a display message response
                        let _ = broadcast_tx.send(ServerResponse::Error(format!("[cmd] {}", output)));
                    }
                }
                Ok(crate::server::command::CommandResult::LayoutChanged) => {
                    // Layout update already broadcast by execute()
                }
                Ok(crate::server::command::CommandResult::SessionEnded) => {
                    // SessionEnded already broadcast by execute()
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
}

fn cleanup_stale_socket(path: &Path) {
    if path.exists() {
        // Try to connect — if it fails, the socket is stale
        if std::os::unix::net::UnixStream::connect(path).is_err() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Kill a running session by connecting and sending a kill-server command.
pub async fn kill_session(session_name: &str) -> Result<()> {
    let path = socket_path(session_name);
    if !path.exists() {
        anyhow::bail!("no session named '{}'", session_name);
    }
    let mut stream = UnixStream::connect(&path).await?;
    framing::send(
        &mut stream,
        &ClientRequest::Attach {
            session_name: session_name.to_string(),
        },
    )
    .await?;
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

/// Send keys to a specific pane target (session:window.pane format).
pub async fn send_keys(session_name: &str, keys: &str) -> Result<()> {
    let path = socket_path(session_name);
    if !path.exists() {
        anyhow::bail!("no session named '{}'", session_name);
    }
    let mut stream = UnixStream::connect(&path).await?;
    framing::send(
        &mut stream,
        &ClientRequest::Attach {
            session_name: session_name.to_string(),
        },
    )
    .await?;
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

        let state =
            ServerState::new_session("test-session".to_string(), &event_tx, 80, 24, config)
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
                            if let Some(pane) = s.find_pane_mut(pane_id) {
                                pane.process_output(&bytes);
                            }
                        }
                        let _ = btx_clone.send(ServerResponse::PaneOutput { pane_id, data: bytes });
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

    /// Helper: attach to the server and consume the initial Attached + LayoutChanged messages.
    async fn attach_and_consume_initial(stream: &mut UnixStream) {
        framing::send(
            stream,
            &ClientRequest::Attach {
                session_name: "test-session".to_string(),
            },
        )
        .await
        .unwrap();

        // Read Attached
        let resp: ServerResponse = framing::recv_required(stream).await.unwrap();
        assert!(matches!(resp, ServerResponse::Attached { .. }));

        // Read initial LayoutChanged
        let resp: ServerResponse = framing::recv_required(stream).await.unwrap();
        assert!(matches!(resp, ServerResponse::LayoutChanged { .. }));
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
        assert_eq!(s.session_name, "new-name");
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

        registry.register(1, 120, 40).await;
        assert_eq!(registry.min_size().await, Some((120, 40)));

        registry.register(2, 80, 24).await;
        assert_eq!(registry.min_size().await, Some((80, 24)));

        registry.register(3, 100, 30).await;
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

        registry.register(1, 120, 40).await;
        registry.register(2, 80, 24).await;
        assert_eq!(registry.min_size().await, Some((80, 24)));

        // Client 2 resizes larger
        registry.update_size(2, 200, 50).await;
        assert_eq!(registry.min_size().await, Some((120, 40)));
    }
}
