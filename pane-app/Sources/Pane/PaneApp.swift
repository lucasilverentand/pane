import SwiftUI
import PaneKit

@main
struct PaneApp: App {
    @State private var appState = AppState()

    #if canImport(AppKit)
    private var ghosttyManager: GhosttyAppManager { GhosttyAppManager.shared }
    #endif

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(appState)
                .environment(appState.client)
                .environment(appState.browser)
                .navigationTitle(appState.windowTitle)
                #if canImport(AppKit)
                .preferredColorScheme(ghosttyManager.isLightTheme ? .light : .dark)
                #endif
                .onChange(of: appState.client.renderState?.activeWorkspace) {
                    appState.syncWorkspaceFromServer()
                }
                .onAppear {
                    appState.connect()
                }
                .onDisappear {
                    appState.disconnect()
                }
        }
        .windowStyle(.automatic)
        .commands {
            PaneCommands(appState: appState)

            CommandGroup(after: .appInfo) {
                Divider()
                Button("Reconnect to Daemon") {
                    appState.reconnect()
                }
                .keyboardShortcut("r", modifiers: [.command, .shift])
            }
        }
    }
}
