import Foundation

/// Mirrors Rust `LayoutNode` enum — a recursive binary split tree.
///
/// Serde externally tagged format:
/// - `{"Leaf": "uuid-string"}`
/// - `{"Split": {"direction": "Horizontal", "ratio": 0.5, "first": {...}, "second": {...}}}`
public indirect enum LayoutNode: Codable, Hashable, Sendable {
    case leaf(WindowId)
    case split(direction: SplitDirection, ratio: Double, first: LayoutNode, second: LayoutNode)

    private enum CodingKeys: String, CodingKey {
        case leaf = "Leaf"
        case split = "Split"
    }

    private struct SplitData: Codable, Hashable {
        let direction: SplitDirection
        let ratio: Double
        let first: LayoutNode
        let second: LayoutNode
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        if let windowId = try container.decodeIfPresent(WindowId.self, forKey: .leaf) {
            self = .leaf(windowId)
        } else if let data = try container.decodeIfPresent(SplitData.self, forKey: .split) {
            self = .split(
                direction: data.direction,
                ratio: data.ratio,
                first: data.first,
                second: data.second
            )
        } else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "Unknown LayoutNode variant")
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)

        switch self {
        case .leaf(let windowId):
            try container.encode(windowId, forKey: .leaf)
        case .split(let direction, let ratio, let first, let second):
            try container.encode(
                SplitData(direction: direction, ratio: ratio, first: first, second: second),
                forKey: .split
            )
        }
    }

    // MARK: - Helpers

    /// Collect all window IDs from the layout tree.
    public var windowIds: [WindowId] {
        switch self {
        case .leaf(let id):
            [id]
        case .split(_, _, let first, let second):
            first.windowIds + second.windowIds
        }
    }
}
