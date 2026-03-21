import SwiftUI
import PaneKit

/// Which tab type is currently active in a pane.
enum ActivePaneTab: Equatable {
    case terminal(Int)     // daemon tab index
    case browser(UUID)     // client-side browser tab id
}

/// Renders the split layout tree from a workspace's `LayoutNode`.
/// Each leaf maps to a `TerminalView` for the corresponding window.
struct TerminalContainer: View {
    let workspace: WorkspaceSnapshot

    var body: some View {
        GeometryReader { geometry in
            layoutView(node: workspace.layout, in: geometry.size)
        }
    }

    /// Recursive layout builder. Uses `AnyView` because the return type
    /// depends on the tree depth which is not statically known.
    private func layoutView(node: LayoutNode, in size: CGSize) -> AnyView {
        switch node {
        case .leaf(let windowId):
            let window = workspace.groups.first { $0.id == windowId }
            let isActive = windowId == workspace.activeGroup
            return AnyView(
                TerminalPane(
                    windowId: windowId,
                    window: window,
                    isActive: isActive
                )
            )

        case .split(let direction, let ratio, let first, let second):
            switch direction {
            case .horizontal:
                return AnyView(
                    HStack(spacing: 1) {
                        layoutView(node: first, in: size)
                            .frame(width: size.width * ratio)
                        Divider()
                        layoutView(node: second, in: size)
                    }
                )
            case .vertical:
                return AnyView(
                    VStack(spacing: 1) {
                        layoutView(node: first, in: size)
                            .frame(height: size.height * ratio)
                        Divider()
                        layoutView(node: second, in: size)
                    }
                )
            }
        }
    }
}

/// Pane view for a single window — shows terminal tabs, browser tabs, and content.
struct TerminalPane: View {
    let windowId: WindowId
    let window: WindowSnapshot?
    let isActive: Bool

    @Environment(BrowserManager.self) private var browser
    @Environment(PaneClient.self) private var client
    @State private var activeTab: ActivePaneTab = .terminal(0)

    private var browserTabs: [BrowserTab] {
        browser.browserTabs(for: windowId)
    }

    var body: some View {
        VStack(spacing: 0) {
            // Always show tab bar when there are browser tabs, or when >1 terminal tab
            if let window, window.tabs.count > 1 || !browserTabs.isEmpty {
                tabBar(window)
            }

            // Content area
            switch activeTab {
            case .terminal:
                TerminalView(windowId: windowId)
                    .border(isActive ? Color.accentColor : Color.clear, width: 1)
            case .browser(let tabId):
                if let tab = browserTabs.first(where: { $0.id == tabId }) {
                    BrowserPaneView(tab: tab)
                        .border(isActive ? Color.accentColor : Color.clear, width: 1)
                } else {
                    // Tab was closed, fall back to terminal
                    TerminalView(windowId: windowId)
                        .border(isActive ? Color.accentColor : Color.clear, width: 1)
                        .onAppear { activeTab = .terminal(window?.activeTab ?? 0) }
                }
            }
        }
        .onChange(of: window?.activeTab) { _, newIndex in
            if case .terminal = activeTab, let newIndex {
                activeTab = .terminal(newIndex)
            }
        }
    }

    @ViewBuilder
    private func tabBar(_ window: WindowSnapshot) -> some View {
        HStack(spacing: 0) {
            // Terminal tabs
            ForEach(Array(window.tabs.enumerated()), id: \.element.id) { index, tab in
                HStack(spacing: 4) {
                    Text(tab.kind.label)
                        .font(.caption2)
                    Text(tab.title)
                        .font(.caption)
                        .lineLimit(1)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(activeTab == .terminal(index) ? Color.accentColor.opacity(0.3) : Color.clear)
                .onTapGesture {
                    activeTab = .terminal(index)
                    Task { try? await client.selectTab(windowId: windowId, tabIndex: index) }
                }
            }

            // Browser tabs
            ForEach(browserTabs) { tab in
                HStack(spacing: 4) {
                    Image(systemName: "globe")
                        .font(.caption2)
                    Text(tab.title)
                        .font(.caption)
                        .lineLimit(1)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(activeTab == .browser(tab.id) ? Color.accentColor.opacity(0.3) : Color.clear)
                .onTapGesture {
                    activeTab = .browser(tab.id)
                    browser.setActiveBrowserTab(tab.id, in: windowId)
                }
                .contextMenu {
                    Button("Close Tab") {
                        browser.closeTab(id: tab.id, in: windowId)
                        if case .browser(let id) = activeTab, id == tab.id {
                            activeTab = .terminal(window.activeTab)
                        }
                    }
                }
            }

            // New browser tab button
            Button(action: {
                let tab = browser.openTab(in: windowId)
                activeTab = .browser(tab.id)
            }) {
                Image(systemName: "plus")
                    .font(.caption2)
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)

            Spacer()
        }
        .background(.bar)
    }
}
