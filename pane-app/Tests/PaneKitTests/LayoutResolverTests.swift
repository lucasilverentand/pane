import Foundation
import CoreGraphics
import Testing
@testable import PaneKit

@Suite("LayoutResolver")
struct LayoutResolverTests {

    private let fullRect = CGRect(x: 0, y: 0, width: 1000, height: 800)

    // MARK: - Leaf node

    @Test("Leaf fills the entire rect")
    func leafFillsEntireRect() {
        let id = WindowId()
        let node = LayoutNode.leaf(id)
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        #expect(results.count == 1)
        #expect(results[0].windowId == id)
        #expect(results[0].frame == fullRect)
    }

    @Test("Leaf at non-zero origin preserves origin")
    func leafPreservesOrigin() {
        let id = WindowId()
        let rect = CGRect(x: 100, y: 50, width: 400, height: 300)
        let results = LayoutResolver.resolve(node: .leaf(id), in: rect)

        #expect(results.count == 1)
        #expect(results[0].frame.origin == rect.origin)
        #expect(results[0].frame.size == rect.size)
    }

    // MARK: - Horizontal split (left / right)

    @Test("Horizontal split 50/50 produces equal widths")
    func horizontalSplitEqual() {
        let left = WindowId()
        let right = WindowId()
        let node = LayoutNode.split(
            direction: .horizontal, ratio: 0.5,
            first: .leaf(left), second: .leaf(right)
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        #expect(results.count == 2)
        let leftResult = results.first { $0.windowId == left }!
        let rightResult = results.first { $0.windowId == right }!

        // Widths should be equal (500 each)
        #expect(abs(leftResult.frame.width - 500) < 0.001)
        #expect(abs(rightResult.frame.width - 500) < 0.001)

        // Heights fill the full rect
        #expect(abs(leftResult.frame.height - fullRect.height) < 0.001)
        #expect(abs(rightResult.frame.height - fullRect.height) < 0.001)

        // Left window starts at x=0
        #expect(abs(leftResult.frame.minX - 0) < 0.001)
        // Right window starts at x=500
        #expect(abs(rightResult.frame.minX - 500) < 0.001)

        // Both at y=0
        #expect(abs(leftResult.frame.minY - 0) < 0.001)
        #expect(abs(rightResult.frame.minY - 0) < 0.001)
    }

    @Test("Horizontal split 30/70 produces proportional widths")
    func horizontalSplitAsymmetric() {
        let left = WindowId()
        let right = WindowId()
        let node = LayoutNode.split(
            direction: .horizontal, ratio: 0.3,
            first: .leaf(left), second: .leaf(right)
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        let leftResult = results.first { $0.windowId == left }!
        let rightResult = results.first { $0.windowId == right }!

        #expect(abs(leftResult.frame.width - 300) < 0.001)
        #expect(abs(rightResult.frame.width - 700) < 0.001)
        #expect(abs(rightResult.frame.minX - 300) < 0.001)
    }

    // MARK: - Vertical split (top / bottom)

    @Test("Vertical split 50/50 produces equal heights")
    func verticalSplitEqual() {
        let top = WindowId()
        let bottom = WindowId()
        let node = LayoutNode.split(
            direction: .vertical, ratio: 0.5,
            first: .leaf(top), second: .leaf(bottom)
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        #expect(results.count == 2)
        let topResult = results.first { $0.windowId == top }!
        let bottomResult = results.first { $0.windowId == bottom }!

        #expect(abs(topResult.frame.height - 400) < 0.001)
        #expect(abs(bottomResult.frame.height - 400) < 0.001)
        #expect(abs(topResult.frame.width - fullRect.width) < 0.001)
        #expect(abs(bottomResult.frame.width - fullRect.width) < 0.001)

        #expect(abs(topResult.frame.minY - 0) < 0.001)
        #expect(abs(bottomResult.frame.minY - 400) < 0.001)
        #expect(abs(topResult.frame.minX - 0) < 0.001)
        #expect(abs(bottomResult.frame.minX - 0) < 0.001)
    }

    @Test("Vertical split 25/75 produces proportional heights")
    func verticalSplitAsymmetric() {
        let top = WindowId()
        let bottom = WindowId()
        let node = LayoutNode.split(
            direction: .vertical, ratio: 0.25,
            first: .leaf(top), second: .leaf(bottom)
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        let topResult = results.first { $0.windowId == top }!
        let bottomResult = results.first { $0.windowId == bottom }!

        #expect(abs(topResult.frame.height - 200) < 0.001)
        #expect(abs(bottomResult.frame.height - 600) < 0.001)
        #expect(abs(bottomResult.frame.minY - 200) < 0.001)
    }

    // MARK: - Nested splits

    @Test("Three-pane layout: left | (top / bottom)")
    func threePaneLayout() {
        let left = WindowId()
        let topRight = WindowId()
        let bottomRight = WindowId()

        let rightPane = LayoutNode.split(
            direction: .vertical, ratio: 0.5,
            first: .leaf(topRight), second: .leaf(bottomRight)
        )
        let node = LayoutNode.split(
            direction: .horizontal, ratio: 0.5,
            first: .leaf(left), second: rightPane
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        #expect(results.count == 3)

        let leftResult = results.first { $0.windowId == left }!
        let topRightResult = results.first { $0.windowId == topRight }!
        let bottomRightResult = results.first { $0.windowId == bottomRight }!

        // Left pane: x=0, y=0, w=500, h=800
        #expect(abs(leftResult.frame.minX - 0) < 0.001)
        #expect(abs(leftResult.frame.minY - 0) < 0.001)
        #expect(abs(leftResult.frame.width - 500) < 0.001)
        #expect(abs(leftResult.frame.height - 800) < 0.001)

        // Top-right pane: x=500, y=0, w=500, h=400
        #expect(abs(topRightResult.frame.minX - 500) < 0.001)
        #expect(abs(topRightResult.frame.minY - 0) < 0.001)
        #expect(abs(topRightResult.frame.width - 500) < 0.001)
        #expect(abs(topRightResult.frame.height - 400) < 0.001)

        // Bottom-right pane: x=500, y=400, w=500, h=400
        #expect(abs(bottomRightResult.frame.minX - 500) < 0.001)
        #expect(abs(bottomRightResult.frame.minY - 400) < 0.001)
        #expect(abs(bottomRightResult.frame.width - 500) < 0.001)
        #expect(abs(bottomRightResult.frame.height - 400) < 0.001)
    }

    @Test("Four-pane grid: (top-left | top-right) / (bottom-left | bottom-right)")
    func fourPaneGrid() {
        let tl = WindowId()
        let tr = WindowId()
        let bl = WindowId()
        let br = WindowId()

        let topRow = LayoutNode.split(direction: .horizontal, ratio: 0.5, first: .leaf(tl), second: .leaf(tr))
        let bottomRow = LayoutNode.split(direction: .horizontal, ratio: 0.5, first: .leaf(bl), second: .leaf(br))
        let node = LayoutNode.split(direction: .vertical, ratio: 0.5, first: topRow, second: bottomRow)

        let results = LayoutResolver.resolve(node: node, in: fullRect)

        #expect(results.count == 4)

        let tlResult = results.first { $0.windowId == tl }!
        let trResult = results.first { $0.windowId == tr }!
        let blResult = results.first { $0.windowId == bl }!
        let brResult = results.first { $0.windowId == br }!

        // Top-left: (0, 0, 500, 400)
        #expect(abs(tlResult.frame.minX - 0) < 0.001)
        #expect(abs(tlResult.frame.minY - 0) < 0.001)
        #expect(abs(tlResult.frame.width - 500) < 0.001)
        #expect(abs(tlResult.frame.height - 400) < 0.001)

        // Top-right: (500, 0, 500, 400)
        #expect(abs(trResult.frame.minX - 500) < 0.001)
        #expect(abs(trResult.frame.minY - 0) < 0.001)
        #expect(abs(trResult.frame.width - 500) < 0.001)
        #expect(abs(trResult.frame.height - 400) < 0.001)

        // Bottom-left: (0, 400, 500, 400)
        #expect(abs(blResult.frame.minX - 0) < 0.001)
        #expect(abs(blResult.frame.minY - 400) < 0.001)
        #expect(abs(blResult.frame.width - 500) < 0.001)
        #expect(abs(blResult.frame.height - 400) < 0.001)

        // Bottom-right: (500, 400, 500, 400)
        #expect(abs(brResult.frame.minX - 500) < 0.001)
        #expect(abs(brResult.frame.minY - 400) < 0.001)
        #expect(abs(brResult.frame.width - 500) < 0.001)
        #expect(abs(brResult.frame.height - 400) < 0.001)
    }

    @Test("Deeply nested tree produces correct count of windows")
    func deeplyNestedTree() {
        // Build a 10-level deep tree (11 windows total)
        var node = LayoutNode.leaf(WindowId())
        for _ in 0..<10 {
            node = .split(
                direction: .horizontal, ratio: 0.5,
                first: node, second: .leaf(WindowId())
            )
        }
        let results = LayoutResolver.resolve(node: node, in: fullRect)
        #expect(results.count == 11)
    }

    // MARK: - Edge cases: extreme ratios

    @Test("Horizontal split ratio 0.0 gives first window zero width")
    func horizontalSplitRatioZero() {
        let left = WindowId()
        let right = WindowId()
        let node = LayoutNode.split(
            direction: .horizontal, ratio: 0.0,
            first: .leaf(left), second: .leaf(right)
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        let leftResult = results.first { $0.windowId == left }!
        let rightResult = results.first { $0.windowId == right }!

        #expect(abs(leftResult.frame.width - 0) < 0.001)
        #expect(abs(rightResult.frame.width - fullRect.width) < 0.001)
        #expect(abs(rightResult.frame.minX - 0) < 0.001)
    }

    @Test("Horizontal split ratio 1.0 gives second window zero width")
    func horizontalSplitRatioOne() {
        let left = WindowId()
        let right = WindowId()
        let node = LayoutNode.split(
            direction: .horizontal, ratio: 1.0,
            first: .leaf(left), second: .leaf(right)
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        let leftResult = results.first { $0.windowId == left }!
        let rightResult = results.first { $0.windowId == right }!

        #expect(abs(leftResult.frame.width - fullRect.width) < 0.001)
        #expect(abs(rightResult.frame.width - 0) < 0.001)
    }

    @Test("Vertical split ratio 0.0 gives first window zero height")
    func verticalSplitRatioZero() {
        let top = WindowId()
        let bottom = WindowId()
        let node = LayoutNode.split(
            direction: .vertical, ratio: 0.0,
            first: .leaf(top), second: .leaf(bottom)
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        let topResult = results.first { $0.windowId == top }!
        let bottomResult = results.first { $0.windowId == bottom }!

        #expect(abs(topResult.frame.height - 0) < 0.001)
        #expect(abs(bottomResult.frame.height - fullRect.height) < 0.001)
    }

    // MARK: - Resolution order

    @Test("Results are ordered depth-first left-to-right")
    func depthFirstLeftToRightOrdering() {
        let a = WindowId()
        let b = WindowId()
        let c = WindowId()
        // Layout: a | (b | c)
        let node = LayoutNode.split(
            direction: .horizontal, ratio: 0.5,
            first: .leaf(a),
            second: .split(
                direction: .horizontal, ratio: 0.5,
                first: .leaf(b), second: .leaf(c)
            )
        )
        let results = LayoutResolver.resolve(node: node, in: fullRect)

        #expect(results.count == 3)
        #expect(results[0].windowId == a)
        #expect(results[1].windowId == b)
        #expect(results[2].windowId == c)
    }

    // MARK: - Non-origin rect

    @Test("Resolution in offset rect preserves offsets through splits")
    func offsetRect() {
        let left = WindowId()
        let right = WindowId()
        let offsetRect = CGRect(x: 100, y: 200, width: 800, height: 600)
        let node = LayoutNode.split(
            direction: .horizontal, ratio: 0.5,
            first: .leaf(left), second: .leaf(right)
        )
        let results = LayoutResolver.resolve(node: node, in: offsetRect)

        let leftResult = results.first { $0.windowId == left }!
        let rightResult = results.first { $0.windowId == right }!

        // Left pane starts at the offset origin
        #expect(abs(leftResult.frame.minX - 100) < 0.001)
        #expect(abs(leftResult.frame.minY - 200) < 0.001)
        #expect(abs(leftResult.frame.width - 400) < 0.001)
        #expect(abs(leftResult.frame.height - 600) < 0.001)

        // Right pane starts at x=500
        #expect(abs(rightResult.frame.minX - 500) < 0.001)
        #expect(abs(rightResult.frame.minY - 200) < 0.001)
        #expect(abs(rightResult.frame.width - 400) < 0.001)
    }

    // MARK: - Zero-size rect

    @Test("Zero-size rect resolves to zero-size frames")
    func zeroSizeRect() {
        let id = WindowId()
        let results = LayoutResolver.resolve(node: .leaf(id), in: .zero)

        #expect(results.count == 1)
        #expect(results[0].frame == .zero)
    }

    @Test("Split of zero-size rect produces zero-size frames")
    func splitZeroSizeRect() {
        let a = WindowId()
        let b = WindowId()
        let node = LayoutNode.split(direction: .horizontal, ratio: 0.5, first: .leaf(a), second: .leaf(b))
        let results = LayoutResolver.resolve(node: node, in: .zero)

        #expect(results.count == 2)
        #expect(results[0].frame.width == 0)
        #expect(results[1].frame.width == 0)
    }

    // MARK: - ResolvedWindow public initializer

    @Test("ResolvedWindow public init is accessible")
    func resolvedWindowInit() {
        let id = WindowId()
        let frame = CGRect(x: 10, y: 20, width: 300, height: 400)
        let rw = LayoutResolver.ResolvedWindow(windowId: id, frame: frame)

        #expect(rw.windowId == id)
        #expect(rw.frame == frame)
    }
}
