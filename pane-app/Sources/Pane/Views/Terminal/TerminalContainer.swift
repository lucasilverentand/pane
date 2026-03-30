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

    @Environment(PaneClient.self) private var client

    /// Thickness (in pts) of a folded-window strip.
    private static let foldBarThickness: CGFloat = 30

    /// How many points of drag translate to one resize-pane step (0.05 ratio).
    private static let dragPointsPerStep: CGFloat = 30

    var body: some View {
        GeometryReader { geometry in
            if let zoomedId = workspace.zoomedWindow {
                let window = workspace.groups.first { $0.id == zoomedId }
                TerminalPane(windowId: zoomedId, window: window, isActive: true, isFolded: false)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 8)
            } else {
                layoutView(node: workspace.layout, in: geometry.size)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 8)
            }
        }
    }

    /// Recursive layout builder.
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
            // Pick the first leaf's window ID in the "first" child as the resize target.
            let firstLeafId = first.windowIds.first

            switch direction {
            case .horizontal:
                let availableWidth = size.width - 8 // account for divider + gaps
                let firstWidth = foldAwareWidth(
                    total: availableWidth,
                    ratio: ratio,
                    firstFolded: firstFolded,
                    secondFolded: secondFolded
                )
                return AnyView(
                    HStack(spacing: 0) {
                        layoutView(node: first, in: CGSize(width: firstWidth, height: size.height))
                            .frame(width: firstWidth)
                        SplitDividerView(
                            direction: .horizontal,
                            firstLeafId: firstLeafId,
                            client: client
                        )
                        layoutView(node: second, in: size)
                    }
                )
            case .vertical:
                let availableHeight = size.height - 8 // account for divider + gaps
                let firstHeight = foldAwareHeight(
                    total: availableHeight,
                    ratio: ratio,
                    firstFolded: firstFolded,
                    secondFolded: secondFolded
                )
                return AnyView(
                    VStack(spacing: 0) {
                        layoutView(node: first, in: CGSize(width: size.width, height: firstHeight))
                            .frame(height: firstHeight)
                        SplitDividerView(
                            direction: .vertical,
                            firstLeafId: firstLeafId,
                            client: client
                        )
                        layoutView(node: second, in: size)
                    }
                )
            }
        }
    }

    private func isDirectLeafFolded(_ node: LayoutNode) -> Bool {
        guard case .leaf(let windowId) = node else { return false }
        return workspace.foldedWindows.contains(windowId)
    }

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

// MARK: - Draggable split divider

/// An interactive divider between split panes. Dragging sends incremental
/// `resize-pane` commands to the daemon, which adjusts the split ratio.
private struct SplitDividerView: View {
    let direction: SplitDirection
    let firstLeafId: WindowId?
    let client: PaneClient

    /// How many points of drag translate to one resize step.
    private static let pointsPerStep: CGFloat = 30

    /// Accumulated drag distance not yet converted to a step.
    @State private var dragAccumulator: CGFloat = 0
    @State private var isHovering = false

    var body: some View {
        let isHorizontal = direction == .horizontal

        Rectangle()
            .fill(isHovering ? Color.accentColor.opacity(0.6) : Color.primary.opacity(0.06))
            .frame(
                width: isHorizontal ? 2 : nil,
                height: isHorizontal ? nil : 2
            )
            .padding(isHorizontal ? .horizontal : .vertical, 3)
            .contentShape(Rectangle())
            .onContinuousHover { phase in
                switch phase {
                case .active: isHovering = true
                case .ended: isHovering = false
                }
            }
            #if canImport(AppKit)
            .onHover { hovering in
                if hovering {
                    NSCursor.resizeLeftRight.set()
                    if !isHorizontal { NSCursor.resizeUpDown.set() }
                } else {
                    NSCursor.arrow.set()
                }
            }
            #endif
            .gesture(
                DragGesture(minimumDistance: 2)
                    .onChanged { value in
                        let delta = isHorizontal ? value.translation.width : value.translation.height
                        let totalDrag = dragAccumulator + delta

                        let steps = Int(totalDrag / Self.pointsPerStep)
                        if steps != 0 {
                            dragAccumulator = totalDrag - CGFloat(steps) * Self.pointsPerStep
                            let amount = abs(steps)
                            let cmd: String
                            if isHorizontal {
                                cmd = steps > 0 ? "resize-pane -R \(amount)" : "resize-pane -L \(amount)"
                            } else {
                                cmd = steps > 0 ? "resize-pane -D \(amount)" : "resize-pane -U \(amount)"
                            }
                            // Focus the first-child window before resizing so the daemon
                            // knows which split edge to move.
                            Task {
                                if let id = firstLeafId {
                                    try? await client.focusWindow(id: id)
                                }
                                try? await client.sendCommand(cmd)
                            }
                        } else {
                            dragAccumulator = totalDrag
                        }
                    }
                    .onEnded { _ in
                        dragAccumulator = 0
                    }
            )
            .animation(.easeInOut(duration: 0.15), value: isHovering)
    }
}

// MARK: - TerminalPane

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

    private var foldBarView: some View {
        GeometryReader { geo in
            let isVerticalStrip = geo.size.width < geo.size.height
            Text(window?.name ?? "Window")
                .font(.caption2)
                .lineLimit(1)
                .rotationEffect(isVerticalStrip ? .degrees(-90) : .degrees(0))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .glassEffect(
                    isActive ? .regular.tint(.white.opacity(0.12)) : .regular,
                    in: .rect(cornerRadius: 8)
                )
        }
        .onTapGesture {
            Task { try? await client.focusWindow(id: windowId) }
        }
        .contextMenu { paneContextMenu }
    }

    // MARK: - Normal view

    private var themeBg: Color {
        GhosttyAppManager.shared.terminalBackgroundColor.opacity(0.65)
    }

    @ViewBuilder
    private var normalView: some View {
        VStack(spacing: 0) {
            if let window {
                tabBar(window)
            }

            switch activeTab {
            case .terminal:
                TerminalView(windowId: windowId)
                    .padding(4)
            case .browser(let tabId):
                if let tab = browserTabs.first(where: { $0.id == tabId }) {
                    BrowserPaneView(tab: tab)
                        .padding(4)
                } else {
                    TerminalView(windowId: windowId)
                        .padding(4)
                        .onAppear { activeTab = .terminal(window?.activeTab ?? 0) }
                }
            }
        }
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(themeBg)
        )
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(isActive ? Color.accentColor.opacity(0.35) : Color.primary.opacity(0.06), lineWidth: 1)
        )
        .contextMenu { paneContextMenu }
        .onChange(of: window?.activeTab) { _, newIndex in
            if case .terminal = activeTab, let newIndex {
                activeTab = .terminal(newIndex)
            }
        }
    }

    // MARK: - Context menu

    @ViewBuilder
    private var paneContextMenu: some View {
        Button("Split Right") {
            Task {
                try? await client.focusWindow(id: windowId)
                try? await client.splitHorizontal()
            }
        }
        Button("Split Down") {
            Task {
                try? await client.focusWindow(id: windowId)
                try? await client.splitVertical()
            }
        }

        Divider()

        Button("Toggle Zoom") {
            Task {
                try? await client.focusWindow(id: windowId)
                try? await client.toggleZoom()
            }
        }
        Button("Toggle Fold") {
            Task {
                try? await client.focusWindow(id: windowId)
                try? await client.toggleFold()
            }
        }
        Button("Equalize Layout") {
            Task { try? await client.equalizeLayout() }
        }

        Divider()

        Button("Close Pane") {
            Task {
                try? await client.focusWindow(id: windowId)
                try? await client.closePane()
            }
        }
        Button("Close Window") {
            Task {
                try? await client.focusWindow(id: windowId)
                try? await client.closeWindow()
            }
        }
    }

    // MARK: - Glass tab bar

    @Namespace private var tabNamespace

    @ViewBuilder
    private func tabBar(_ window: WindowSnapshot) -> some View {
        GlassEffectContainer(spacing: 10) {
            HStack(spacing: 6) {
                segmentedTabs(window)

                Button(action: {
                    let tab = browser.openTab(in: windowId)
                    activeTab = .browser(tab.id)
                }) {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .medium))
                        .frame(width: 30, height: 30)
                        .contentShape(Circle())
                        .glassEffect(.regular.interactive(), in: .circle)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
        }
    }

    @ViewBuilder
    private func segmentedTabs(_ window: WindowSnapshot) -> some View {
        let allTerminalTabs = Array(window.tabs.enumerated())
        let allBrowserTabs = browserTabs

        ForEach(allTerminalTabs, id: \.element.id) { index, tab in
            let isSelected = activeTab == .terminal(index)
            tabSegment(
                icon: iconForTabKind(tab.kind),
                title: tab.title.isEmpty ? tab.kind.label : tab.title,
                isSelected: isSelected,
                showClose: window.tabs.count > 1,
                onSelect: {
                    activeTab = .terminal(index)
                    Task { try? await client.selectTab(windowId: windowId, tabIndex: index) }
                },
                onClose: {
                    Task {
                        try? await client.focusWindow(id: windowId)
                        try? await client.selectTab(windowId: windowId, tabIndex: index)
                        try? await client.closePane()
                    }
                }
            )
        }

        ForEach(allBrowserTabs) { tab in
            let isSelected = activeTab == .browser(tab.id)
            tabSegment(
                icon: "globe",
                title: tab.title,
                isSelected: isSelected,
                showClose: true,
                onSelect: {
                    activeTab = .browser(tab.id)
                    browser.setActiveBrowserTab(tab.id, in: windowId)
                },
                onClose: {
                    browser.closeTab(id: tab.id, in: windowId)
                    if case .browser(let id) = activeTab, id == tab.id {
                        activeTab = .terminal(window.activeTab)
                    }
                }
            )
        }
    }

    @ViewBuilder
    private func tabSegment(
        icon: String,
        title: String,
        isSelected: Bool,
        showClose: Bool = false,
        onSelect: @escaping () -> Void,
        onClose: (() -> Void)? = nil
    ) -> some View {
        Button(action: onSelect) {
            HStack(spacing: 5) {
                Image(systemName: icon)
                    .font(.system(size: 10.5))
                Text(title)
                    .font(.system(size: 11.5))
                    .lineLimit(1)
                    .truncationMode(.tail)

                if showClose, let onClose {
                    Spacer(minLength: 0)
                    Button(action: onClose) {
                        Image(systemName: "xmark")
                            .font(.system(size: 8, weight: .semibold))
                            .foregroundStyle(.tertiary)
                            .frame(width: 16, height: 16)
                            .contentShape(Circle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .foregroundStyle(isSelected ? .primary : .secondary)
            .frame(maxWidth: .infinity)
            .frame(height: 32)
            .padding(.horizontal, 10)
            .glassEffect(
                isSelected ? .regular.tint(.white.opacity(0.12)).interactive() : .regular.interactive(),
                in: .capsule
            )
            .contentShape(.capsule)
        }
        .buttonStyle(.plain)
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
