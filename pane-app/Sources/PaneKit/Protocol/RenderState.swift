import Foundation

/// Mirrors Rust `RenderState` — the full snapshot sent to clients for rendering.
public struct RenderState: Codable, Hashable, Sendable {
    public let workspaces: [WorkspaceSnapshot]
    public let activeWorkspace: Int

    private enum CodingKeys: String, CodingKey {
        case workspaces
        case activeWorkspace = "active_workspace"
    }
}

/// Mirrors Rust `WorkspaceSnapshot`.
public struct WorkspaceSnapshot: Codable, Hashable, Sendable {
    public let name: String
    public let layout: LayoutNode
    public let groups: [WindowSnapshot]
    public let activeGroup: WindowId
    public let syncPanes: Bool
    public let foldedWindows: Set<WindowId>
    public let zoomedWindow: WindowId?
    public let floatingWindows: [FloatingWindowSnapshot]

    private enum CodingKeys: String, CodingKey {
        case name
        case layout
        case groups
        case activeGroup = "active_group"
        case syncPanes = "sync_panes"
        case foldedWindows = "folded_windows"
        case zoomedWindow = "zoomed_window"
        case floatingWindows = "floating_windows"
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        name = try container.decode(String.self, forKey: .name)
        layout = try container.decode(LayoutNode.self, forKey: .layout)
        groups = try container.decode([WindowSnapshot].self, forKey: .groups)
        activeGroup = try container.decode(WindowId.self, forKey: .activeGroup)
        syncPanes = try container.decode(Bool.self, forKey: .syncPanes)
        foldedWindows = try container.decodeIfPresent(Set<WindowId>.self, forKey: .foldedWindows) ?? []
        zoomedWindow = try container.decodeIfPresent(WindowId.self, forKey: .zoomedWindow)
        floatingWindows = try container.decode([FloatingWindowSnapshot].self, forKey: .floatingWindows)
    }
}

/// Mirrors Rust `FloatingWindowSnapshot`.
public struct FloatingWindowSnapshot: Codable, Hashable, Sendable {
    public let id: WindowId
    public let x: UInt16
    public let y: UInt16
    public let width: UInt16
    public let height: UInt16
}

/// Mirrors Rust `WindowSnapshot`.
public struct WindowSnapshot: Codable, Hashable, Sendable {
    public let id: WindowId
    public let tabs: [TabSnapshot]
    public let activeTab: Int

    private enum CodingKeys: String, CodingKey {
        case id
        case tabs
        case activeTab = "active_tab"
    }
}

/// Mirrors Rust `TabSnapshot`.
public struct TabSnapshot: Codable, Hashable, Sendable {
    public let id: TabId
    public let kind: TabKind
    public let title: String
    public let exited: Bool
    public let foregroundProcess: String?
    public let cwd: String

    private enum CodingKeys: String, CodingKey {
        case id
        case kind
        case title
        case exited
        case foregroundProcess = "foreground_process"
        case cwd
    }
}
