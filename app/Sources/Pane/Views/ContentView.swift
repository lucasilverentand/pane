import SwiftUI

struct ContentView: View {
    @Environment(AppModel.self) private var appModel

    var body: some View {
        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 400)
        } detail: {
            HSplitView {
                EditorArea()

                if appModel.showAgentPanel {
                    AgentPanel()
                        .frame(minWidth: 320, idealWidth: 400, maxWidth: 600)
                }
            }
        }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button {
                    withAnimation {
                        appModel.showAgentPanel.toggle()
                    }
                } label: {
                    Image(systemName: "sparkles")
                }
                .help("Toggle Agent Panel")
            }
        }
        .frame(minWidth: 800, minHeight: 500)
    }
}
