import Foundation
import PaneKit

/// Resolves a LayoutNode tree into concrete CGRect frames for each window.
///
/// This mirrors the Rust `LayoutNode::resolve()` method, producing the same
/// layout positions as the TUI client.
enum LayoutResolver {
    /// A resolved leaf: the window ID and its computed frame.
    struct ResolvedWindow {
        let windowId: WindowId
        let frame: CGRect
    }

    /// Resolve the layout tree into flat (WindowId, CGRect) pairs.
    static func resolve(node: LayoutNode, in rect: CGRect) -> [ResolvedWindow] {
        var results: [ResolvedWindow] = []
        resolveInner(node: node, in: rect, results: &results)
        return results
    }

    private static func resolveInner(
        node: LayoutNode,
        in rect: CGRect,
        results: inout [ResolvedWindow]
    ) {
        switch node {
        case .leaf(let windowId):
            results.append(ResolvedWindow(windowId: windowId, frame: rect))

        case .split(let direction, let ratio, let first, let second):
            let (firstRect, secondRect) = splitRect(rect, direction: direction, ratio: ratio)
            resolveInner(node: first, in: firstRect, results: &results)
            resolveInner(node: second, in: secondRect, results: &results)
        }
    }

    private static func splitRect(
        _ rect: CGRect,
        direction: SplitDirection,
        ratio: Double
    ) -> (CGRect, CGRect) {
        switch direction {
        case .horizontal:
            let firstWidth = rect.width * ratio
            let first = CGRect(x: rect.minX, y: rect.minY, width: firstWidth, height: rect.height)
            let second = CGRect(
                x: rect.minX + firstWidth, y: rect.minY,
                width: rect.width - firstWidth, height: rect.height
            )
            return (first, second)

        case .vertical:
            let firstHeight = rect.height * ratio
            let first = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: firstHeight)
            let second = CGRect(
                x: rect.minX, y: rect.minY + firstHeight,
                width: rect.width, height: rect.height - firstHeight
            )
            return (first, second)
        }
    }
}
