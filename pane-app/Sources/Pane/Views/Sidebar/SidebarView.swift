import SwiftUI
import PaneKit

/// Sidebar showing workspace cards with status details.
struct SidebarView: View {
    @Environment(AppState.self) private var appState
    @Environment(PaneClient.self) private var client
    @State private var showNewWorkspace = false

    var body: some View {
        ScrollView {
            if let state = client.renderState, !state.workspaces.isEmpty {
                GlassEffectContainer(spacing: 10) {
                    LazyVStack(spacing: 8) {
                        ForEach(Array(state.workspaces.enumerated()), id: \.offset) { index, workspace in
                            WorkspaceRow(
                                workspace: workspace,
                                isSelected: index == appState.selectedWorkspaceIndex
                            ) {
                                appState.selectWorkspace(index)
                            }
                        }
                    }
                    .padding(.horizontal, 12)
                }
            } else {
                ContentUnavailableView {
                    Label("No Workspaces", systemImage: "rectangle.stack")
                } description: {
                    Text("Connect to the daemon to see workspaces.")
                }
                .padding(.top, 40)
            }
        }
        .navigationTitle("Pane")
        .toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button(action: { showNewWorkspace = true }) {
                    Image(systemName: "plus")
                }
                .disabled(!appState.isConnected)
            }
        }
        .sheet(isPresented: $showNewWorkspace) {
            NewWorkspaceSheet()
        }
    }
}

// MARK: - New Workspace Sheet

private struct NewWorkspaceSheet: View {
    @Environment(PaneClient.self) private var client
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var directory = ""
    @State private var recentDirectories: [String] = []
    @FocusState private var nameFieldFocused: Bool

    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("New Workspace")
                    .font(.headline)
                Spacer()
                Button(action: { dismiss() }) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.title3)
                        .foregroundStyle(.tertiary)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 20)
            .padding(.top, 20)
            .padding(.bottom, 16)

            // Form
            VStack(spacing: 16) {
                // Name field
                VStack(alignment: .leading, spacing: 6) {
                    Text("Name")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    TextField("my-project", text: $name)
                        .textFieldStyle(.roundedBorder)
                        .focused($nameFieldFocused)
                        .onSubmit { if canCreate { create() } }
                }

                // Directory field
                VStack(alignment: .leading, spacing: 6) {
                    Text("Working Directory")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    HStack(spacing: 8) {
                        TextField("~/Developer/project", text: $directory)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit { if canCreate { create() } }
                        Button("Browse") {
                            pickDirectory()
                        }
                        .buttonStyle(.glass)
                    }
                }

                // Recent directories
                if !recentDirectories.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Recent")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        GlassEffectContainer(spacing: 6) {
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 6) {
                                    ForEach(recentDirectories, id: \.self) { dir in
                                        Button(action: {
                                            directory = dir
                                            if name.isEmpty {
                                                name = URL(fileURLWithPath: dir).lastPathComponent
                                            }
                                        }) {
                                            HStack(spacing: 4) {
                                                Image(systemName: "folder")
                                                    .font(.system(size: 10))
                                                Text(abbreviatePath(dir))
                                                    .font(.system(size: 11))
                                                    .lineLimit(1)
                                            }
                                            .padding(.horizontal, 10)
                                            .padding(.vertical, 6)
                                            .glassEffect(
                                                directory == dir
                                                    ? .regular.tint(.accentColor).interactive()
                                                    : .regular.interactive(),
                                                in: .capsule
                                            )
                                        }
                                        .buttonStyle(.plain)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            .padding(.horizontal, 20)

            Spacer(minLength: 16)

            // Actions
            HStack {
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)

                Spacer()

                Button(action: create) {
                    Text("Create Workspace")
                }
                .buttonStyle(.glass)
                .keyboardShortcut(.defaultAction)
                .disabled(!canCreate)
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 20)
        }
        .frame(width: 420, height: recentDirectories.isEmpty ? 280 : 360)
        .onAppear {
            nameFieldFocused = true
            loadRecentDirectories()
        }
    }

    private func create() {
        let trimmedName = name.trimmingCharacters(in: .whitespaces)
        let trimmedDir = directory.trimmingCharacters(in: .whitespaces)
        let resolvedDir = trimmedDir.isEmpty ? nil : expandPath(trimmedDir)

        Task {
            try? await client.newWorkspace(
                name: trimmedName.isEmpty ? nil : trimmedName,
                cwd: resolvedDir
            )
        }
        dismiss()
    }

    private func pickDirectory() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.prompt = "Select"
        panel.message = "Choose a working directory for the workspace"

        if let url = expandedDirectoryURL() {
            panel.directoryURL = url
        }

        if panel.runModal() == .OK, let url = panel.url {
            directory = url.path
            if name.isEmpty {
                name = url.lastPathComponent
            }
        }
    }

    private func expandedDirectoryURL() -> URL? {
        let trimmed = directory.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return nil }
        return URL(fileURLWithPath: expandPath(trimmed))
    }

    private func expandPath(_ path: String) -> String {
        if path.hasPrefix("~") {
            let home = FileManager.default.homeDirectoryForCurrentUser.path
            return home + path.dropFirst()
        }
        return path
    }

    /// Gather working directories from existing workspaces as recent suggestions.
    private func loadRecentDirectories() {
        guard let state = client.renderState else { return }
        var seen = Set<String>()
        var dirs: [String] = []
        for ws in state.workspaces where !ws.cwd.isEmpty {
            if seen.insert(ws.cwd).inserted {
                dirs.append(ws.cwd)
            }
        }
        recentDirectories = dirs
    }

    private func abbreviatePath(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        if path.hasPrefix(home) {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }
}

// MARK: - Git Status

private struct GitStatus: Equatable {
    let branch: String
    let dirty: Int   // number of changed files
    let staged: Int  // number of staged files

    var isDirty: Bool { dirty > 0 }
    var isClean: Bool { dirty == 0 && staged == 0 }
}

private func fetchGitStatus(for path: String) async -> GitStatus? {
    guard !path.isEmpty, FileManager.default.fileExists(atPath: path) else { return nil }

    async let branchResult = runGit(["rev-parse", "--abbrev-ref", "HEAD"], in: path)
    async let statusResult = runGit(["status", "--porcelain"], in: path)

    guard let branch = await branchResult else { return nil }
    let statusOutput = await statusResult ?? ""

    var dirty = 0
    var staged = 0
    for line in statusOutput.split(separator: "\n") where line.count >= 2 {
        let index = line[line.startIndex]
        let worktree = line[line.index(after: line.startIndex)]
        if worktree != " " && worktree != "?" { dirty += 1 }
        if index != " " && index != "?" { staged += 1 }
    }

    return GitStatus(
        branch: branch.trimmingCharacters(in: .whitespacesAndNewlines),
        dirty: dirty,
        staged: staged
    )
}

private func runGit(_ args: [String], in directory: String) async -> String? {
    await withCheckedContinuation { continuation in
        DispatchQueue.global(qos: .utility).async {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
            process.arguments = args
            process.currentDirectoryURL = URL(fileURLWithPath: directory)
            process.environment = ["GIT_TERMINAL_PROMPT": "0"]

            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = FileHandle.nullDevice

            do {
                try process.run()
                process.waitUntilExit()
                guard process.terminationStatus == 0 else {
                    continuation.resume(returning: nil)
                    return
                }
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                continuation.resume(returning: String(data: data, encoding: .utf8))
            } catch {
                continuation.resume(returning: nil)
            }
        }
    }
}

// MARK: - Workspace Row

private struct WorkspaceRow: View {
    let workspace: WorkspaceSnapshot
    let isSelected: Bool
    let action: () -> Void

    @Environment(PaneClient.self) private var client
    @State private var gitStatus: GitStatus?

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                // Icon
                Image(systemName: "rectangle.stack")
                    .font(.system(size: 14))
                    .foregroundStyle(isSelected ? .primary : .secondary)
                    .frame(width: 24)

                // Name + details
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(workspace.name)
                            .font(.system(size: 13, weight: .medium))
                            .foregroundStyle(.primary)
                            .lineLimit(1)

                        if workspace.syncPanes {
                            Circle().fill(.orange).frame(width: 5, height: 5)
                        }
                        if workspace.zoomedWindow != nil {
                            Circle().fill(.blue).frame(width: 5, height: 5)
                        }
                    }

                    // Folder line
                    if !workspace.cwd.isEmpty {
                        HStack(spacing: 4) {
                            Image(systemName: "folder")
                                .font(.system(size: 8.5))
                            Text(abbreviatePath(workspace.cwd))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                    }

                    // Git + meta line
                    HStack(spacing: 0) {
                        if let git = gitStatus {
                            Image(systemName: "arrow.triangle.branch")
                                .font(.system(size: 8))
                                .foregroundStyle(.tertiary)
                            Text(" \(git.branch)")
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)

                            if git.isDirty || git.staged > 0 {
                                Text("  ")
                                if git.staged > 0 {
                                    Text("+\(git.staged)")
                                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                                        .foregroundStyle(.green)
                                }
                                if git.dirty > 0 {
                                    Text(git.staged > 0 ? " " : "")
                                    Text("~\(git.dirty)")
                                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                                        .foregroundStyle(.orange)
                                }
                            }

                            Text("  ·  ")
                                .font(.system(size: 10))
                                .foregroundStyle(.quaternary)
                        }

                        let windowCount = workspace.groups.count
                        Text("\(windowCount) pane\(windowCount == 1 ? "" : "s")")
                            .font(.system(size: 10))
                            .foregroundStyle(.tertiary)
                    }
                    .lineLimit(1)
                }

                Spacer(minLength: 0)

                // Tab count badge
                let totalTabs = workspace.groups.reduce(0) { $0 + $1.tabs.count }
                Text("\(totalTabs)")
                    .font(.system(size: 10, weight: .semibold, design: .rounded))
                    .foregroundStyle(isSelected ? .primary : .tertiary)
                    .frame(minWidth: 20, minHeight: 20)
                    .glassEffect(.regular, in: .circle)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassEffect(
                isSelected ? .regular.tint(.primary.opacity(0.12)).interactive() : .regular.interactive(),
                in: .rect(cornerRadius: 10)
            )
            .contentShape(.rect(cornerRadius: 10))
        }
        .buttonStyle(.plain)
        .task(id: workspace.cwd) {
            gitStatus = await fetchGitStatus(for: workspace.cwd)
        }
        .task(id: workspace.cwd) {
            // Refresh git status periodically
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(10))
                gitStatus = await fetchGitStatus(for: workspace.cwd)
            }
        }
        .contextMenu {
            Button("Rename...") { }
            Button("Close Workspace") {
                Task {
                    try? await client.closeWorkspace()
                }
            }
        }
    }

    private func abbreviatePath(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        if path.hasPrefix(home) {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }
}
