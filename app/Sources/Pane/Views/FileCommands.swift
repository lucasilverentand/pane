import SwiftUI

struct FileCommands: Commands {
    let appModel: AppModel

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Open Folder...") {
                openFolder()
            }
            .keyboardShortcut("o", modifiers: .command)

            Button("Add Project to Workspace...") {
                addProject()
            }
            .keyboardShortcut("o", modifiers: [.command, .shift])
            .disabled(appModel.workspace == nil)

            Divider()

            Button("Save") {
                save()
            }
            .keyboardShortcut("s", modifiers: .command)
            .disabled(appModel.activeTab?.isDirty != true)
        }
    }

    private func openFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false

        if panel.runModal() == .OK, let url = panel.url {
            appModel.openWorkspace(at: url)
        }
    }

    private func addProject() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true

        if panel.runModal() == .OK {
            for url in panel.urls {
                appModel.addProjectToWorkspace(at: url)
            }
        }
    }

    private func save() {
        guard let tab = appModel.activeTab else { return }
        try? tab.content.write(to: tab.url, atomically: true, encoding: .utf8)
        if let index = appModel.openTabs.firstIndex(where: { $0.id == tab.id }) {
            appModel.openTabs[index].isDirty = false
        }
    }
}
