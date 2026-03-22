import SwiftUI
import PaneKit

/// macOS menu bar commands for all Pane terminal-multiplexer operations.
///
/// Adds a top-level "Pane" menu with keyboard shortcuts for splits, tabs,
/// windows, and workspaces — mirroring the TUI's leader-key operations.
struct PaneCommands: Commands {
    let appState: AppState

    private var client: PaneClient { appState.client }

    var body: some Commands {
        CommandMenu("Pane") {
            // MARK: Splits
            Button("Split Right") {
                Task { try? await client.splitHorizontal() }
            }
            .keyboardShortcut("d", modifiers: .command)

            Button("Split Down") {
                Task { try? await client.splitVertical() }
            }
            .keyboardShortcut("D", modifiers: [.command, .shift])

            Button("Equalize Layout") {
                Task { try? await client.equalizeLayout() }
            }
            .keyboardShortcut("e", modifiers: [.command, .option])

            Divider()

            // MARK: Tabs
            Button("Next Tab") {
                Task { try? await client.nextTab() }
            }
            .keyboardShortcut("]", modifiers: .command)

            Button("Previous Tab") {
                Task { try? await client.prevTab() }
            }
            .keyboardShortcut("[", modifiers: .command)

            Button("Restart Pane") {
                Task { try? await client.restartPane() }
            }
            .keyboardShortcut("r", modifiers: [.command, .option])

            Divider()

            // MARK: Window
            Button("New Window") {
                Task { try? await client.newWindow() }
            }
            .keyboardShortcut("n", modifiers: [.command, .option])

            Button("Close Window") {
                Task { try? await client.closeWindow() }
            }
            .keyboardShortcut("k", modifiers: [.command, .option])

            Button("Toggle Zoom") {
                Task { try? await client.toggleZoom() }
            }
            .keyboardShortcut("z", modifiers: [.command, .shift])

            Button("Toggle Fold") {
                Task { try? await client.toggleFold() }
            }
            .keyboardShortcut("f", modifiers: [.command, .option])

            Button("Toggle Sync Panes") {
                Task { try? await client.toggleSync() }
            }
            .keyboardShortcut("s", modifiers: [.command, .option])

            Divider()

            // MARK: Workspace
            Button("New Workspace") {
                Task { try? await client.newWorkspace() }
            }
            .keyboardShortcut("t", modifiers: [.command, .shift])

            Button("Close Workspace") {
                Task { try? await client.closeWorkspace() }
            }
            .keyboardShortcut("w", modifiers: [.command, .option])

            Button("Previous Workspace") {
                Task { try? await client.setActiveWorkspace(
                    max(0, (appState.selectedWorkspaceIndex) - 1)
                ) }
            }
            .keyboardShortcut("{", modifiers: [.command, .shift])

            Button("Next Workspace") {
                let count = client.renderState?.workspaces.count ?? 0
                let next = min(count - 1, appState.selectedWorkspaceIndex + 1)
                Task { try? await client.setActiveWorkspace(next) }
            }
            .keyboardShortcut("}", modifiers: [.command, .shift])
        }
    }
}
