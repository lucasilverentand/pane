import Foundation
import PaneKit

/// Root application state, backed by the PaneClient.
@MainActor
@Observable
final class AppState {
    let client = PaneClient()
    let browser = BrowserManager()
    let mcpServer: BrowserMCPServer

    var selectedWorkspaceIndex: Int = 0
    var selectedWindowId: WindowId?

    init() {
        mcpServer = BrowserMCPServer(browser: browser)
    }

    var windowTitle: String {
        guard let state = client.renderState,
              state.workspaces.indices.contains(selectedWorkspaceIndex)
        else {
            return "Pane"
        }
        return "Pane - \(state.workspaces[selectedWorkspaceIndex].name)"
    }

    var currentWorkspace: WorkspaceSnapshot? {
        // Sync with server's active workspace when render state updates
        if let state = client.renderState,
           state.activeWorkspace != selectedWorkspaceIndex,
           state.workspaces.indices.contains(state.activeWorkspace)
        {
            selectedWorkspaceIndex = state.activeWorkspace
        }

        guard let state = client.renderState,
              state.workspaces.indices.contains(selectedWorkspaceIndex)
        else {
            return nil
        }
        return state.workspaces[selectedWorkspaceIndex]
    }

    var isConnected: Bool {
        if case .connected = client.connectionState { return true }
        return false
    }

    func connect() {
        guard case .disconnected = client.connectionState else { return }
        Task {
            do {
                try await client.connect()
                mcpServer.start()
            } catch {
                // Connection failed — try to start the daemon and retry
                if startDaemon() {
                    try? await Task.sleep(for: .milliseconds(500))
                    do {
                        try await client.connect()
                        mcpServer.start()
                    } catch {
                        // Still failed — surface error via client.connectionState
                    }
                }
            }
        }
    }

    /// Try to start the pane daemon process. Returns true if launched.
    @discardableResult
    private func startDaemon() -> Bool {
        // Look for pane binary: bundled in app, or in PATH
        let candidates = [
            Bundle.main.url(forResource: "pane", withExtension: nil, subdirectory: "bin")?.path,
            "/opt/homebrew/bin/pane",
            "/usr/local/bin/pane",
        ].compactMap { $0 }

        guard let panePath = candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else {
            print("[Pane] No pane binary found — cannot auto-start daemon")
            return false
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: panePath)
        process.arguments = ["daemon"]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()
            print("[Pane] Started daemon via \(panePath)")
            return true
        } catch {
            print("[Pane] Failed to start daemon: \(error)")
            return false
        }
    }

    /// Disconnect and immediately attempt to reconnect.
    func reconnect() {
        mcpServer.stop()
        client.disconnect()
        connect()
    }

    func disconnect() {
        mcpServer.stop()
        client.disconnect()
    }

    func selectWorkspace(_ index: Int) {
        selectedWorkspaceIndex = index
        Task {
            try? await client.setActiveWorkspace(index)
        }
    }
}
