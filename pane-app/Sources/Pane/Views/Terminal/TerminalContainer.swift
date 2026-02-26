import SwiftUI
import PaneKit

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

/// Placeholder pane view for a single terminal window.
/// In the full implementation, this wraps SwiftTerm's TerminalView.
struct TerminalPane: View {
    let windowId: WindowId
    let window: WindowSnapshot?
    let isActive: Bool

    var body: some View {
        VStack(spacing: 0) {
            // Tab bar (only shown when >1 tab)
            if let window, window.tabs.count > 1 {
                tabBar(window)
            }

            // Terminal content area
            TerminalView(windowId: windowId)
                .border(isActive ? Color.accentColor : Color.clear, width: 1)
        }
    }

    @ViewBuilder
    private func tabBar(_ window: WindowSnapshot) -> some View {
        HStack(spacing: 0) {
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
                .background(index == window.activeTab ? Color.accentColor.opacity(0.3) : Color.clear)
            }
            Spacer()
        }
        .background(.bar)
    }
}
