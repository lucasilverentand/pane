import Foundation
import PaneKit

/// Root application state, backed by the PaneClient.
@MainActor
@Observable
final class AppState {
    let client = PaneClient()

    var selectedWorkspaceIndex: Int = 0
    var selectedWindowId: WindowId?

    var windowTitle: String {
        guard let state = client.renderState,
              state.workspaces.indices.contains(selectedWorkspaceIndex)
        else {
            return "Pane"
        }
        return "Pane - \(state.workspaces[selectedWorkspaceIndex].name)"
    }

    var currentWorkspace: WorkspaceSnapshot? {
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
        Task {
            do {
                try await client.connect()
            } catch {
                // Connection error is surfaced via client.connectionState
            }
        }
    }

    func disconnect() {
        client.disconnect()
    }

    func selectWorkspace(_ index: Int) {
        selectedWorkspaceIndex = index
        Task {
            try? await client.setActiveWorkspace(index)
        }
    }
}
