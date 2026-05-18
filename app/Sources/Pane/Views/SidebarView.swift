import SwiftUI

struct SidebarView: View {
    @Environment(AppModel.self) private var appModel

    var body: some View {
        Group {
            if let workspace = appModel.workspace {
                List {
                    ForEach(workspace.roots, id: \.self) { root in
                        FileTreeSection(root: root)
                    }
                }
                .listStyle(.sidebar)
            } else {
                VStack(spacing: 16) {
                    Image(systemName: "folder.badge.plus")
                        .font(.system(size: 40))
                        .foregroundStyle(.secondary)
                    Text("Open a folder to get started")
                        .foregroundStyle(.secondary)
                    Button("Open Folder...") {
                        openFolder()
                    }
                    .buttonStyle(.borderedProminent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(appModel.workspace?.name ?? "Pane")
    }

    private func openFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.message = "Choose a project folder"

        if panel.runModal() == .OK, let url = panel.url {
            appModel.openWorkspace(at: url)
        }
    }
}

struct FileTreeSection: View {
    let root: URL
    @Environment(AppModel.self) private var appModel

    var body: some View {
        Section(root.lastPathComponent) {
            FileTreeNode(url: root)
        }
    }
}

struct FileTreeNode: View {
    let url: URL
    @Environment(AppModel.self) private var appModel
    @State private var isExpanded = true
    @State private var children: [URL] = []

    var isDirectory: Bool {
        (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true
    }

    var body: some View {
        if isDirectory {
            DisclosureGroup(isExpanded: $isExpanded) {
                ForEach(children, id: \.self) { child in
                    FileTreeNode(url: child)
                }
            } label: {
                Label(url.lastPathComponent, systemImage: "folder")
                    .foregroundStyle(.primary)
            }
            .onAppear { loadChildren() }
        } else {
            Button {
                appModel.openFile(url)
            } label: {
                Label(url.lastPathComponent, systemImage: iconForFile(url))
                    .foregroundStyle(.primary)
            }
            .buttonStyle(.plain)
        }
    }

    private func loadChildren() {
        guard children.isEmpty else { return }
        let fm = FileManager.default
        guard let contents = try? fm.contentsOfDirectory(
            at: url,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        ) else { return }

        let skip: Set<String> = ["node_modules", "target", ".build", "DerivedData", "build"]
        children = contents
            .filter { !skip.contains($0.lastPathComponent) }
            .sorted { lhs, rhs in
                let lDir = (try? lhs.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true
                let rDir = (try? rhs.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true
                if lDir != rDir { return lDir }
                return lhs.lastPathComponent.localizedStandardCompare(rhs.lastPathComponent) == .orderedAscending
            }
    }

    private func iconForFile(_ url: URL) -> String {
        switch url.pathExtension.lowercased() {
        case "swift": "swift"
        case "rs": "gearshape"
        case "ts", "tsx", "js", "jsx": "j.square"
        case "json", "toml", "yaml", "yml": "doc.text"
        case "md": "doc.richtext"
        case "py": "p.square"
        default: "doc"
        }
    }
}
