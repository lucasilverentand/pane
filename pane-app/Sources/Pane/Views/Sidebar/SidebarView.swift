import SwiftUI
import PaneKit

/// Combined sidebar: workspace list at the top, windows/tabs for selected workspace below.
struct SidebarView: View {
    @Environment(AppState.self) private var appState
    @Environment(PaneClient.self) private var client

    var body: some View {
        List {
            workspacesSection
            if let workspace = appState.currentWorkspace {
                windowsSection(workspace)
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("Pane")
    }

    @ViewBuilder
    private var workspacesSection: some View {
        if let state = client.renderState, !state.workspaces.isEmpty {
            Section("Workspaces") {
                ForEach(Array(state.workspaces.enumerated()), id: \.offset) { index, workspace in
                    Button {
                        appState.selectWorkspace(index)
                    } label: {
                        Label(workspace.name, systemImage: "rectangle.stack")
                    }
                    .buttonStyle(.plain)
                    .fontWeight(index == appState.selectedWorkspaceIndex ? .semibold : .regular)
                }
            }
        }
    }

    @ViewBuilder
    private func windowsSection(_ workspace: WorkspaceSnapshot) -> some View {
        Section("Windows") {
            ForEach(workspace.groups, id: \.id) { window in
                windowRow(window, isActive: window.id == workspace.activeGroup)
            }
        }
    }

    @ViewBuilder
    private func windowRow(_ window: WindowSnapshot, isActive: Bool) -> some View {
        DisclosureGroup {
            ForEach(Array(window.tabs.enumerated()), id: \.element.id) { index, tab in
                HStack {
                    Image(systemName: iconForTabKind(tab.kind))
                        .foregroundStyle(index == window.activeTab ? .primary : .secondary)
                    Text(tab.title)
                        .foregroundStyle(tab.exited ? .secondary : .primary)
                    Spacer()
                    if let process = tab.foregroundProcess {
                        Text(process)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        } label: {
            HStack {
                Image(systemName: "rectangle.split.3x1")
                Text("Window")
                    .fontWeight(isActive ? .semibold : .regular)
                Spacer()
                Text("\(window.tabs.count) tabs")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func iconForTabKind(_ kind: TabKind) -> String {
        switch kind {
        case .shell: "terminal"
        case .agent: "sparkles"
        case .nvim: "doc.text"
        case .devServer: "server.rack"
        }
    }
}
