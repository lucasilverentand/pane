import SwiftUI
import CodeEditSourceEditor
import CodeEditLanguages

struct EditorArea: View {
    @Environment(AppModel.self) private var appModel

    var body: some View {
        VStack(spacing: 0) {
            if !appModel.openTabs.isEmpty {
                TabBar()
            }

            if let tab = appModel.activeTab,
               let index = appModel.openTabs.firstIndex(where: { $0.id == tab.id }) {
                SourceEditorWrapper(tab: Bindable(appModel).openTabs[index])
            } else {
                WelcomeView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct TabBar: View {
    @Environment(AppModel.self) private var appModel

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 0) {
                ForEach(appModel.openTabs) { tab in
                    TabItem(tab: tab)
                }
            }
        }
        .frame(height: 32)
        .background(.bar)
        .overlay(alignment: .bottom) {
            Divider()
        }
    }
}

struct TabItem: View {
    let tab: FileTab
    @Environment(AppModel.self) private var appModel

    var isActive: Bool {
        appModel.activeTabId == tab.id
    }

    var body: some View {
        Button {
            appModel.activeTabId = tab.id
        } label: {
            HStack(spacing: 6) {
                Text(tab.filename)
                    .font(.system(size: 12))
                    .lineLimit(1)
                if tab.isDirty {
                    Circle()
                        .fill(.primary.opacity(0.5))
                        .frame(width: 6, height: 6)
                }
                Button {
                    appModel.closeTab(tab.id)
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(isActive ? Color.accentColor.opacity(0.15) : Color.clear)
        }
        .buttonStyle(.plain)
    }
}

struct SourceEditorWrapper: View {
    @Binding var tab: FileTab
    @Environment(\.colorScheme) private var colorScheme
    @State private var editorState = SourceEditorState(
        cursorPositions: [CursorPosition(line: 1, column: 1)]
    )

    private var theme: EditorTheme {
        colorScheme == .dark ? .paneDark : .paneLight
    }

    private var configuration: SourceEditorConfiguration {
        SourceEditorConfiguration(
            appearance: .init(
                theme: theme,
                font: .monospacedSystemFont(ofSize: 13, weight: .regular),
                wrapLines: true
            )
        )
    }

    var body: some View {
        SourceEditor(
            $tab.content,
            language: codeLanguage(for: tab.language),
            configuration: configuration,
            state: $editorState
        )
        .onChange(of: tab.content) {
            tab.isDirty = true
        }
    }

    private func codeLanguage(for lang: String) -> CodeLanguage {
        switch lang {
        case "swift": .swift
        case "rust": .rust
        case "typescript": .tsx
        case "javascript": .javascript
        case "python": .python
        case "go": .go
        case "json": .json
        case "yaml": .yaml
        case "html": .html
        case "css": .css
        case "markdown": .markdown
        default: .default
        }
    }
}

struct WelcomeView: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "text.cursor")
                .font(.system(size: 48))
                .foregroundStyle(.tertiary)
            Text("Select a file to start editing")
                .font(.title3)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
