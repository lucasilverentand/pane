import SwiftUI
import PaneKit

/// Main layout: sidebar with workspace/window navigation + terminal area.
struct ContentView: View {
    @Environment(AppState.self) private var appState
    @Environment(PaneClient.self) private var client

    var body: some View {
        NavigationSplitView {
            SidebarView()
        } detail: {
            if let workspace = appState.currentWorkspace {
                VStack(spacing: 0) {
                    TerminalContainer(workspace: workspace)
                    StatusBarView()
                }
            } else {
                connectionStatusView
            }
        }
    }

    @ViewBuilder
    private var connectionStatusView: some View {
        switch client.connectionState {
        case .disconnected:
            ContentUnavailableView {
                Label("Not Connected", systemImage: "network.slash")
            } description: {
                Text("Start the Pane daemon to connect.")
            } actions: {
                Button("Connect") { appState.connect() }
                    .buttonStyle(.borderedProminent)
            }
        case .connecting:
            ProgressView("Connecting...")
        case .connected:
            ContentUnavailableView(
                "No Workspaces",
                systemImage: "rectangle.stack",
                description: Text("Waiting for layout data from the daemon.")
            )
        case .error(let message):
            ContentUnavailableView {
                Label("Connection Error", systemImage: "exclamationmark.triangle")
            } description: {
                Text(message)
            } actions: {
                Button("Reconnect") { appState.reconnect() }
                    .buttonStyle(.borderedProminent)
            }
        }
    }
}
