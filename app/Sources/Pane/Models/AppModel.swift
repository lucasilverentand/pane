import SwiftUI

@Observable
final class AppModel {
    var workspace: WorkspaceModel?
    var openTabs: [FileTab] = []
    var activeTabId: FileTab.ID?
    var agents: [AgentModel] = []
    var showAgentPanel: Bool = true

    var activeTab: FileTab? {
        openTabs.first { $0.id == activeTabId }
    }

    func openWorkspace(at url: URL) {
        let name = url.lastPathComponent
        workspace = WorkspaceModel(name: name, roots: [url])
    }

    func addProjectToWorkspace(at url: URL) {
        workspace?.roots.append(url)
    }

    func openFile(_ url: URL) {
        if let existing = openTabs.first(where: { $0.url == url }) {
            activeTabId = existing.id
            return
        }
        guard let content = try? String(contentsOf: url, encoding: .utf8) else { return }
        let tab = FileTab(url: url, content: content)
        openTabs.append(tab)
        activeTabId = tab.id
    }

    func closeTab(_ id: FileTab.ID) {
        openTabs.removeAll { $0.id == id }
        if activeTabId == id {
            activeTabId = openTabs.last?.id
        }
    }
}
