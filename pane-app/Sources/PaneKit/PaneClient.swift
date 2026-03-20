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
    public var onPaneOutput: (@Sendable (TabId, [UInt8]) -> Void)?

    /// Callback invoked when the session ends.
    public var onSessionEnded: (@Sendable () -> Void)?

    // MARK: - Private

    private var connection: PaneConnection?
    private var receiveTask: Task<Void, Never>?

    public init() {}

    // MARK: - Lifecycle

    /// Connect to the daemon and start receiving messages.
    /// Automatically starts the daemon if it isn't already running.
    public func connect(path: String? = nil) async throws {
        connectionState = .connecting

        let socketPath = path ?? PaneConnection.defaultSocketPath
        print("[PaneKit] Connecting to \(socketPath)")

        // Ensure daemon is running before connecting
        try Self.ensureDaemon(socketPath: socketPath)
        print("[PaneKit] Daemon is running")

        do {
            let conn = try PaneConnection.connect(path: socketPath)
            print("[PaneKit] Socket connected")
            connection = conn

            // Send attach request
            try await conn.send(ClientRequest.attach)

            // Start the receive loop
            receiveTask = Task { [weak self] in
                await self?.receiveLoop(conn)
            }
        } catch {
            connectionState = .error(error.localizedDescription)
            throw error
        }
    }

    /// Locate the `pane` binary.
    private static func findPaneBinary() -> String? {
        let candidates = [
            "\(NSHomeDirectory())/.cargo/bin/pane",
            "/usr/local/bin/pane",
            "/opt/homebrew/bin/pane",
        ]
        for path in candidates {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }
        return nil
    }

    /// Start the daemon if no socket is reachable. Mirrors the Rust `start_daemon()`.
    private static func ensureDaemon(socketPath: String) throws {
        // Check if daemon is already running
        if FileManager.default.fileExists(atPath: socketPath) {
            let fd = socket(AF_UNIX, SOCK_STREAM, 0)
            if fd >= 0 {
                var addr = sockaddr_un()
                addr.sun_family = sa_family_t(AF_UNIX)
                let pathBytes = socketPath.utf8CString
                withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
                    ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                        for (i, byte) in pathBytes.enumerated() { dest[i] = byte }
                    }
                }
                let connected = withUnsafePointer(to: &addr) { ptr in
                    ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                        Darwin.connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
                    }
                }
                Darwin.close(fd)
                if connected == 0 {
                    return // daemon already running
                }
            }
            // Stale socket — clean up
            try? FileManager.default.removeItem(atPath: socketPath)
        }

        guard let binary = findPaneBinary() else {
            throw ClientError.daemonNotFound
        }

        // Ensure socket directory exists
        let socketDir = (socketPath as NSString).deletingLastPathComponent
        try FileManager.default.createDirectory(atPath: socketDir, withIntermediateDirectories: true)

        // Spawn `pane daemon` as a background process
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = ["daemon"]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()

        // Wait for socket to appear (up to 5 seconds)
        for _ in 0..<100 {
            Thread.sleep(forTimeInterval: 0.05)
            if FileManager.default.fileExists(atPath: socketPath) {
                // Verify it's connectable
                let fd = socket(AF_UNIX, SOCK_STREAM, 0)
                guard fd >= 0 else { continue }
                var addr = sockaddr_un()
                addr.sun_family = sa_family_t(AF_UNIX)
                let pathBytes = socketPath.utf8CString
                withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
                    ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                        for (i, byte) in pathBytes.enumerated() { dest[i] = byte }
                    }
                }
                let ok = withUnsafePointer(to: &addr) { ptr in
                    ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                        Darwin.connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
                    }
                }
                Darwin.close(fd)
                if ok == 0 { return }
            }
        }

        throw ClientError.daemonStartTimeout
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

    /// Convenience: send a command.
    public func sendCommand(_ command: String) async throws {
        try await send(.command(command))
    }

    /// Convenience: paste text.
    public func paste(_ text: String) async throws {
        try await send(.paste(text))
    }

    /// Convenience: focus a specific window.
    public func focusWindow(_ id: WindowId) async throws {
        try await send(.focusWindow(id: id))
    }

    /// Convenience: select a tab in a window.
    public func selectTab(windowId: WindowId, tabIndex: Int) async throws {
        try await send(.selectTab(windowId: windowId, tabIndex: tabIndex))
    }

    // MARK: - Receive loop

    private func receiveLoop(_ connection: PaneConnection) async {
        while !Task.isCancelled {
            do {
                guard let response = try await connection.receive(ServerResponse.self) else {
                    print("[PaneKit] Clean disconnect (EOF)")
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
                    print("[PaneKit] Receive error: \(error)")
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
            onSessionEnded?()

        case .fullScreenDump(let paneId, let data):
            onPaneOutput?(paneId, data)

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

public enum ClientError: Error, Sendable, CustomStringConvertible {
    case notConnected
    case daemonNotFound
    case daemonStartTimeout

    public var description: String {
        switch self {
        case .notConnected: "Not connected to daemon"
        case .daemonNotFound: "Could not find the 'pane' binary. Install it with: cargo install pane"
        case .daemonStartTimeout: "Timed out waiting for daemon to start"
        }
    }
}
