import SwiftUI
import PaneKit

/// Which tab type is currently active in a pane.
enum ActivePaneTab: Equatable {
    case terminal(Int)     // daemon tab index
    case browser(UUID)     // client-side browser tab id
}

/// Renders the split layout tree from a workspace's `LayoutNode`.
/// Each leaf maps to a `TerminalView` for the corresponding window.
///
/// When `workspace.zoomedWindow` is set the zoomed window fills the entire
/// container, mirroring the TUI zoom behaviour.
///
/// Folded windows (those in `workspace.foldedWindows`) are rendered as
/// narrow fold-bar strips rather than full panes, matching TUI fold behaviour.
struct TerminalContainer: View {
    let workspace: WorkspaceSnapshot

    /// Thickness (in pts) of a folded-window strip.
    private static let foldBarThickness: CGFloat = 30

    var body: some View {
        GeometryReader { geometry in
            if let zoomedId = workspace.zoomedWindow {
                let window = workspace.groups.first { $0.id == zoomedId }
                TerminalPane(windowId: zoomedId, window: window, isActive: true, isFolded: false)
            } else {
                layoutView(node: workspace.layout, in: geometry.size)
            }
        }
    }

    /// Recursive layout builder. Uses `AnyView` because the return type
    /// depends on the tree depth which is not statically known.
    private func layoutView(node: LayoutNode, in size: CGSize) -> AnyView {
        switch node {
        case .leaf(let windowId):
            let window = workspace.groups.first { $0.id == windowId }
            let isActive = windowId == workspace.activeGroup
            let isFolded = workspace.foldedWindows.contains(windowId)
            return AnyView(
                TerminalPane(
                    windowId: windowId,
                    window: window,
                    isActive: isActive,
                    isFolded: isFolded
                )
            )

        case .split(let direction, let ratio, let first, let second):
            let firstFolded = isDirectLeafFolded(first)
            let secondFolded = isDirectLeafFolded(second)

            switch direction {
            case .horizontal:
                let firstWidth = foldAwareWidth(
                    total: size.width,
                    ratio: ratio,
                    firstFolded: firstFolded,
                    secondFolded: secondFolded
                )
                return AnyView(
                    HStack(spacing: 1) {
                        layoutView(node: first, in: CGSize(width: firstWidth, height: size.height))
                            .frame(width: firstWidth)
                        Divider()
                        layoutView(node: second, in: size)
                    }
                )
            case .vertical:
                let firstHeight = foldAwareHeight(
                    total: size.height,
                    ratio: ratio,
                    firstFolded: firstFolded,
                    secondFolded: secondFolded
                )
                return AnyView(
                    VStack(spacing: 1) {
                        layoutView(node: first, in: CGSize(width: size.width, height: firstHeight))
                            .frame(height: firstHeight)
                        Divider()
                        layoutView(node: second, in: size)
                    }
                )
            }
        }
    }

    /// Returns `true` only if `node` is a `.leaf` whose window is currently folded.
    private func isDirectLeafFolded(_ node: LayoutNode) -> Bool {
        guard case .leaf(let windowId) = node else { return false }
        return workspace.foldedWindows.contains(windowId)
    }

    /// Compute the first child's width for a horizontal split, respecting fold state.
    private func foldAwareWidth(total: CGFloat, ratio: Double, firstFolded: Bool, secondFolded: Bool) -> CGFloat {
        if firstFolded && secondFolded {
            return Self.foldBarThickness
        } else if firstFolded {
            return min(Self.foldBarThickness, total)
        } else if secondFolded {
            return max(total - Self.foldBarThickness, 0)
        } else {
            return total * ratio
        }
    }

    /// Compute the first child's height for a vertical split, respecting fold state.
    private func foldAwareHeight(total: CGFloat, ratio: Double, firstFolded: Bool, secondFolded: Bool) -> CGFloat {
        if firstFolded && secondFolded {
            return Self.foldBarThickness
        } else if firstFolded {
            return min(Self.foldBarThickness, total)
        } else if secondFolded {
            return max(total - Self.foldBarThickness, 0)
        } else {
            return total * ratio
        }
    }
}

/// Pane view for a single window — shows terminal tabs, browser tabs, and content.
/// When `isFolded` is true, renders as a compact fold-bar strip instead.
struct TerminalPane: View {
    let windowId: WindowId
    let window: WindowSnapshot?
    let isActive: Bool
    let isFolded: Bool

    @Environment(BrowserManager.self) private var browser
    @Environment(PaneClient.self) private var client
    @State private var activeTab: ActivePaneTab

    init(windowId: WindowId, window: WindowSnapshot?, isActive: Bool, isFolded: Bool) {
        self.windowId = windowId
        self.window = window
        self.isActive = isActive
        self.isFolded = isFolded
        _activeTab = State(initialValue: .terminal(window?.activeTab ?? 0))
    }

    private var browserTabs: [BrowserTab] {
        browser.browserTabs(for: windowId)
    }

    var body: some View {
        if isFolded {
            foldBarView
        } else {
            normalView
        }
    }

    // MARK: - Fold bar

    /// A compact strip shown when the window is folded.
    /// Tapping it sends a FocusWindow command, which the daemon uses to unfold
    /// (focus_group removes the window from folded_windows automatically).
    private var foldBarView: some View {
        GeometryReader { geo in
            ZStack {
                (isActive ? Color.accentColor.opacity(0.15) : Color.secondary.opacity(0.1))
                Text(window?.name ?? "Window")
                    .font(.caption2)
                    .lineLimit(1)
                    .rotationEffect(geo.size.width < geo.size.height ? .degrees(-90) : .degrees(0))
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .onTapGesture {
            Task { try? await client.focusWindow(id: windowId) }
        }
    }

    // MARK: - Normal view

    @ViewBuilder
    private var normalView: some View {
        VStack(spacing: 0) {
            // Always show tab bar (matches TUI behavior — even with a single tab)
            if let window {
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
