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
                .background(.ultraThinMaterial)
            } else {
                connectionStatusView
            }
        }
        .toolbar {
            ToolbarItemGroup {
                toolbarItems
            }
        }
    }

    @ViewBuilder
    private var toolbarItems: some View {
        Button("Split Right", systemImage: "rectangle.split.1x2") {
            Task { try? await appState.client.splitHorizontal() }
        }
        Button("Split Down", systemImage: "rectangle.split.2x1") {
            Task { try? await appState.client.splitVertical() }
        }
        Button("Equalize", systemImage: "rectangle.split.3x3") {
            Task { try? await appState.client.equalizeLayout() }
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
                    .buttonStyle(.glass)
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
                    .buttonStyle(.glass)
            }
        }
    }
}
