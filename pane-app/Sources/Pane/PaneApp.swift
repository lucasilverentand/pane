import SwiftUI
import PaneKit

@main
struct PaneApp: App {
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(appState)
                .environment(appState.client)
                .environment(appState.browser)
                .navigationTitle(appState.windowTitle)
                .onAppear {
                    appState.connect()
                }
                .onDisappear {
                    appState.disconnect()
                }
        }
        .commands {
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
