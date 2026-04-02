import Foundation

/// High-level client for communicating with the Pane daemon.
///
/// Manages the connection lifecycle, sends requests, and processes responses
/// into observable state for the UI layer.
@Observable
public final class PaneClient: @unchecked Sendable {
    // MARK: - Connection state

    public enum ConnectionState: Sendable {
        case disconnected
        case connecting
        case connected
        case error(String)
    }

    public private(set) var connectionState: ConnectionState = .disconnected
    public private(set) var renderState: RenderState?
    public private(set) var systemStats: SerializableSystemStats?
    public private(set) var pluginSegments: [[PluginSegment]] = []
    public private(set) var clientCount: UInt32 = 0

    /// Callback invoked on the main actor when pane output arrives.
    /// The app layer should feed these bytes into SwiftTerm.
    ///
    /// Prefer `subscribePaneOutput(_:)` when multiple consumers need output (e.g. multi-window layouts).
    public var onPaneOutput: (@Sendable (TabId, [UInt8]) -> Void)?

    @ObservationIgnored
    private var _paneOutputSubscribers: [UUID: @Sendable (TabId, [UInt8]) -> Void] = [:]

    /// Register a handler to receive pane output events. Returns an opaque token.
    /// Call `unsubscribePaneOutput(_:)` with the token to stop receiving events.
    /// Unlike `onPaneOutput`, multiple subscribers can coexist — last one doesn't win.
    @MainActor
    public func subscribePaneOutput(_ handler: @Sendable @escaping (TabId, [UInt8]) -> Void) -> UUID {
        let id = UUID()
        _paneOutputSubscribers[id] = handler
        return id
    }

    /// Remove a previously registered pane output handler.
    @MainActor
    public func unsubscribePaneOutput(_ id: UUID) {
        _paneOutputSubscribers.removeValue(forKey: id)
    }

    /// Callback invoked when the session ends or all workspaces close.
    public var onSessionEvent: (@Sendable (SessionEvent) -> Void)?

    public enum SessionEvent: Sendable {
        case sessionEnded
    }

    // MARK: - Private

    private var connection: PaneConnection?
    private var receiveTask: Task<Void, Never>?

    public init() {}

    // MARK: - Lifecycle

    /// Connect to the daemon and start receiving messages.
    public func connect(path: String? = nil) async throws {
        connectionState = .connecting

        do {
            let conn = try PaneConnection.connect(path: path)
            connection = conn

            // Send attach request — identify as a native macOS app client
            try await conn.send(ClientRequest.attachV2(clientType: .nativeApp))

            // Start the receive loop
            receiveTask = Task { [weak self] in
                await self?.receiveLoop(conn)
            }
        } catch {
            connectionState = .error(error.localizedDescription)
            throw error
        }
    }

    /// Disconnect from the daemon.
    public func disconnect() {
        receiveTask?.cancel()
        receiveTask = nil
        connection?.disconnect()
        connection = nil
        connectionState = .disconnected
        renderState = nil
        systemStats = nil
        clientCount = 0
        _paneOutputSubscribers.removeAll()
    }

    // MARK: - Sending

    /// Send a client request to the daemon.
    public func send(_ request: ClientRequest) async throws {
        guard let connection else {
            throw ClientError.notConnected
        }
        try await connection.send(request)
    }

    /// Convenience: send a resize request.
    public func resize(width: UInt16, height: UInt16) async throws {
        try await send(.resize(width: width, height: height))
    }

    /// Convenience: send a key event.
    public func sendKey(code: SerializableKeyCode, modifiers: UInt8 = 0) async throws {
        try await send(.key(SerializableKeyEvent(code: code, modifiers: modifiers)))
    }

    /// Convenience: select a workspace by index (0-based).
    ///
    /// Sends `select-workspace -t N` command to the daemon (1-indexed target).
    public func setActiveWorkspace(_ index: Int) async throws {
        try await send(.command("select-workspace -t \(index + 1)"))
    }

    /// Convenience: paste text to the active PTY.
    public func paste(_ text: String) async throws {
        try await send(.paste(text))
    }

    /// Write raw bytes directly to the active PTY.
    /// Used by the native terminal emulator — bytes are already encoded by ghostty.
    public func rawInput(_ data: Data) async throws {
        try await send(.rawInput(data))
    }

    /// Resize a specific pane's PTY (no TUI layout overhead applied).
    public func setPaneSize(tabId: TabId, cols: UInt16, rows: UInt16, pixelWidth: UInt16 = 0, pixelHeight: UInt16 = 0) async throws {
        try await send(.setPaneSize(tabId: tabId, cols: cols, rows: rows, pixelWidth: pixelWidth, pixelHeight: pixelHeight))
    }

    /// Convenience: focus a window by ID.
    public func focusWindow(id: WindowId) async throws {
        try await send(.focusWindow(id: id))
    }

    /// Convenience: select a tab in a window.
    public func selectTab(windowId: WindowId, tabIndex: Int) async throws {
        try await send(.selectTab(windowId: windowId, tabIndex: tabIndex))
    }

    /// Convenience: send a command string to the daemon.
    public func sendCommand(_ command: String) async throws {
        try await send(.command(command))
    }

    // MARK: - Layout commands

    /// Split the active window horizontally (creates a left/right split).
    public func splitHorizontal() async throws {
        try await send(.command("split-window -h"))
    }

    /// Split the active window vertically (creates a top/bottom split).
    public func splitVertical() async throws {
        try await send(.command("split-window"))
    }

    /// Close the active pane (tab).
    public func closePane() async throws {
        try await send(.command("kill-pane"))
    }

    /// Close the active window (and all its tabs).
    public func closeWindow() async throws {
        try await send(.command("kill-window"))
    }

    /// Create a new window in the current workspace.
    public func newWindow(kind: String? = nil) async throws {
        if let kind {
            try await send(.command("new-window -k \(kind)"))
        } else {
            try await send(.command("new-window"))
        }
    }

    /// Add a new tab to the active window with the specified kind.
    public func newTab(kind: String) async throws {
        try await send(.command("new-window -k \(kind)"))
    }

    /// Toggle zoom on the active window (full-screen the focused window).
    public func toggleZoom() async throws {
        try await send(.command("toggle-zoom"))
    }

    /// Toggle fold on the active window (collapse it in the layout).
    public func toggleFold() async throws {
        try await send(.command("toggle-fold"))
    }

    /// Toggle sync-panes — send all keystrokes to every pane in the workspace.
    public func toggleSync() async throws {
        try await send(.command("toggle-sync"))
    }

    // MARK: - Tab navigation commands

    /// Switch to the next tab in the active window.
    public func nextTab() async throws {
        try await send(.command("next-tab"))
    }

    /// Switch to the previous tab in the active window.
    public func prevTab() async throws {
        try await send(.command("prev-tab"))
    }

    /// Restart the active pane (re-launch its process).
    public func restartPane() async throws {
        try await send(.command("restart-pane"))
    }

    /// Equalize the sizes of all panes in the current workspace.
    public func equalizeLayout() async throws {
        try await send(.command("equalize-layout"))
    }

    // MARK: - Focus navigation commands

    /// Focus the pane to the left of the active pane.
    public func focusLeft() async throws {
        try await send(.command("select-pane -L"))
    }

    /// Focus the pane to the right of the active pane.
    public func focusRight() async throws {
        try await send(.command("select-pane -R"))
    }

    /// Focus the pane above the active pane.
    public func focusUp() async throws {
        try await send(.command("select-pane -U"))
    }

    /// Focus the pane below the active pane.
    public func focusDown() async throws {
        try await send(.command("select-pane -D"))
    }

    // MARK: - Pane resize commands

    /// Shrink the active pane horizontally (move right edge left).
    public func resizeShrinkH() async throws {
        try await send(.command("resize-pane -L"))
    }

    /// Grow the active pane horizontally (move right edge right).
    public func resizeGrowH() async throws {
        try await send(.command("resize-pane -R"))
    }

    /// Grow the active pane vertically (move bottom edge down).
    public func resizeGrowV() async throws {
        try await send(.command("resize-pane -D"))
    }

    /// Shrink the active pane vertically (move bottom edge up).
    public func resizeShrinkV() async throws {
        try await send(.command("resize-pane -U"))
    }

    // MARK: - Tab move commands

    /// Move the active tab to the window on the left.
    public func moveTabLeft() async throws {
        try await send(.command("move-tab -L"))
    }

    /// Move the active tab to the window on the right.
    public func moveTabRight() async throws {
        try await send(.command("move-tab -R"))
    }

    /// Move the active tab to the window above.
    public func moveTabUp() async throws {
        try await send(.command("move-tab -U"))
    }

    /// Move the active tab to the window below.
    public func moveTabDown() async throws {
        try await send(.command("move-tab -D"))
    }

    // MARK: - Floating window commands

    /// Toggle floating mode for the active window.
    public func toggleFloat() async throws {
        try await send(.command("toggle-float"))
    }

    /// Create a new floating window.
    public func newFloat() async throws {
        try await send(.command("new-float"))
    }

    // MARK: - Window rename command

    /// Rename the active window.
    public func renameWindow(_ name: String) async throws {
        try await send(.command("rename-window \(name)"))
    }

    // MARK: - Workspace commands

    /// Create a new workspace with an optional name and working directory.
    public func newWorkspace(name: String? = nil, cwd: String? = nil) async throws {
        var cmd = "new-workspace"
        if let name { cmd += " -n \(name)" }
        if let cwd { cmd += " -c \(cwd)" }
        try await send(.command(cmd))
    }

    /// Rename the current workspace.
    public func renameWorkspace(_ name: String) async throws {
        try await send(.command("rename-workspace \(name)"))
    }

    /// Close the current workspace.
    public func closeWorkspace() async throws {
        try await send(.command("close-workspace"))
    }

    // MARK: - Receive loop

    private func receiveLoop(_ connection: PaneConnection) async {
        while !Task.isCancelled {
            do {
                guard let response = try await connection.receive(ServerResponse.self) else {
                    // Clean disconnect
                    await MainActor.run {
                        self.connectionState = .disconnected
                    }
                    break
                }

                await MainActor.run {
                    self.handleResponse(response)
                }
            } catch {
                if !Task.isCancelled {
                    await MainActor.run {
                        self.connectionState = .error(error.localizedDescription)
                    }
                }
                break
            }
        }
    }

    @MainActor
    private func handleResponse(_ response: ServerResponse) {
        switch response {
        case .attached:
            connectionState = .connected

        case .paneOutput(let paneId, let data):
            onPaneOutput?(paneId, data)
            for handler in _paneOutputSubscribers.values { handler(paneId, data) }

        case .paneExited:
            // Layout update will follow
            break

        case .layoutChanged(let state):
            renderState = state

        case .statsUpdate(let stats):
            systemStats = stats

        case .pluginSegments(let segments):
            pluginSegments = segments

        case .sessionEnded:
            onSessionEvent?(.sessionEnded)

        case .fullScreenDump(let paneId, let data):
            onPaneOutput?(paneId, data)
            for handler in _paneOutputSubscribers.values { handler(paneId, data) }

        case .clientCountChanged(let count):
            clientCount = count

        case .error(let msg):
            connectionState = .error(msg)

        case .commandOutput:
            // Handled by sync command callers, not the general receive loop
            break
        }
    }
}

// MARK: - ClientError

public enum ClientError: Error, Sendable {
    case notConnected
}
