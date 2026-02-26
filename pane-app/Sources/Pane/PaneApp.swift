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
                .navigationTitle(appState.windowTitle)
                .onAppear {
                    appState.connect()
                }
                .onDisappear {
                    appState.disconnect()
                }
        }
    }
}
