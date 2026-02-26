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
        case connected(clientId: UInt64)
        case error(String)
    }

    public private(set) var connectionState: ConnectionState = .disconnected
    public private(set) var renderState: RenderState?
    public private(set) var systemStats: SerializableSystemStats?
    public private(set) var pluginSegments: [[PluginSegment]] = []
    public private(set) var clientList: [ClientListEntry] = []

    /// Callback invoked on the main actor when pane output arrives.
    /// The app layer should feed these bytes into SwiftTerm.
    public var onPaneOutput: (@Sendable (TabId, [UInt8]) -> Void)?

    /// Callback invoked when the session ends or all workspaces close.
    public var onSessionEvent: (@Sendable (SessionEvent) -> Void)?

    public enum SessionEvent: Sendable {
        case sessionEnded
        case allWorkspacesClosed
        case kicked(UInt64)
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

    /// Convenience: set the active workspace for this client.
    public func setActiveWorkspace(_ index: Int) async throws {
        try await send(.setActiveWorkspace(index))
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
        case .attached(let clientId):
            connectionState = .connected(clientId: clientId)

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
            onSessionEvent?(.sessionEnded)

        case .allWorkspacesClosed:
            onSessionEvent?(.allWorkspacesClosed)

        case .fullScreenDump(let paneId, let data):
            onPaneOutput?(paneId, data)

        case .clientListChanged(let entries):
            clientList = entries

        case .kicked(let id):
            onSessionEvent?(.kicked(id))
            disconnect()

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
