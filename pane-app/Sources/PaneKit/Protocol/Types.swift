import Foundation

// MARK: - TabId / WindowId

/// Mirrors Rust `TabId` which is `uuid::Uuid`.
/// Serializes as a UUID string (e.g. "550e8400-e29b-41d4-a716-446655440000").
public struct TabId: Codable, Hashable, Sendable {
    public let raw: UUID

    public init(_ raw: UUID = UUID()) {
        self.raw = raw
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        let string = try container.decode(String.self)
        guard let uuid = UUID(uuidString: string) else {
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Invalid UUID string: \(string)"
            )
        }
        self.raw = uuid
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(raw.uuidString.lowercased())
    }
}

/// Mirrors Rust `WindowId` which is `uuid::Uuid`.
/// Serializes as a UUID string.
public struct WindowId: Codable, Hashable, Sendable {
    public let raw: UUID

    public init(_ raw: UUID = UUID()) {
        self.raw = raw
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        let string = try container.decode(String.self)
        guard let uuid = UUID(uuidString: string) else {
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Invalid UUID string: \(string)"
            )
        }
        self.raw = uuid
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(raw.uuidString.lowercased())
    }
}

// MARK: - TabKind

/// Mirrors Rust `TabKind` enum.
/// Serde default (externally tagged): `"Shell"`, `"Agent"`, etc.
public enum TabKind: String, Codable, Hashable, Sendable {
    case shell = "Shell"
    case agent = "Agent"
    case nvim = "Nvim"
    case devServer = "DevServer"

    public var label: String {
        switch self {
        case .shell: "shell"
        case .agent: "claude"
        case .nvim: "nvim"
        case .devServer: "server"
        }
    }
}

// MARK: - SplitDirection

/// Mirrors Rust `SplitDirection` enum.
public enum SplitDirection: String, Codable, Hashable, Sendable {
    case horizontal = "Horizontal"
    case vertical = "Vertical"
}

// MARK: - SerializableKeyCode

/// Mirrors Rust `SerializableKeyCode` enum.
/// Serde externally tagged: `"Backspace"`, `{"Char": "a"}`, `{"F": 5}`, etc.
public enum SerializableKeyCode: Codable, Hashable, Sendable {
    case char(Character)
    case f(UInt8)
    case backspace
    case enter
    case left
    case right
    case up
    case down
    case home
    case end
    case pageUp
    case pageDown
    case tab
    case backTab
    case delete
    case insert
    case esc
    case null

    private enum CodingKeys: String, CodingKey {
        case char = "Char"
        case f = "F"
        case backspace = "Backspace"
        case enter = "Enter"
        case left = "Left"
        case right = "Right"
        case up = "Up"
        case down = "Down"
        case home = "Home"
        case end = "End"
        case pageUp = "PageUp"
        case pageDown = "PageDown"
        case tab = "Tab"
        case backTab = "BackTab"
        case delete = "Delete"
        case insert = "Insert"
        case esc = "Esc"
        case null = "Null"
    }

    public init(from decoder: any Decoder) throws {
        // Try unit variant (plain string)
        if let container = try? decoder.singleValueContainer(),
           let str = try? container.decode(String.self)
        {
            switch str {
            case "Backspace": self = .backspace; return
            case "Enter": self = .enter; return
            case "Left": self = .left; return
            case "Right": self = .right; return
            case "Up": self = .up; return
            case "Down": self = .down; return
            case "Home": self = .home; return
            case "End": self = .end; return
            case "PageUp": self = .pageUp; return
            case "PageDown": self = .pageDown; return
            case "Tab": self = .tab; return
            case "BackTab": self = .backTab; return
            case "Delete": self = .delete; return
            case "Insert": self = .insert; return
            case "Esc": self = .esc; return
            case "Null": self = .null; return
            default: break
            }
        }

        // Try newtype variants (keyed container)
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if let str = try container.decodeIfPresent(String.self, forKey: .char) {
            guard let ch = str.first, str.count == 1 else {
                throw DecodingError.dataCorruptedError(
                    forKey: .char, in: container,
                    debugDescription: "Expected single character"
                )
            }
            self = .char(ch)
        } else if let n = try container.decodeIfPresent(UInt8.self, forKey: .f) {
            self = .f(n)
        } else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "Unknown SerializableKeyCode")
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .char(let c):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(String(c), forKey: .char)
        case .f(let n):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(n, forKey: .f)
        case .backspace:
            var container = encoder.singleValueContainer()
            try container.encode("Backspace")
        case .enter:
            var container = encoder.singleValueContainer()
            try container.encode("Enter")
        case .left:
            var container = encoder.singleValueContainer()
            try container.encode("Left")
        case .right:
            var container = encoder.singleValueContainer()
            try container.encode("Right")
        case .up:
            var container = encoder.singleValueContainer()
            try container.encode("Up")
        case .down:
            var container = encoder.singleValueContainer()
            try container.encode("Down")
        case .home:
            var container = encoder.singleValueContainer()
            try container.encode("Home")
        case .end:
            var container = encoder.singleValueContainer()
            try container.encode("End")
        case .pageUp:
            var container = encoder.singleValueContainer()
            try container.encode("PageUp")
        case .pageDown:
            var container = encoder.singleValueContainer()
            try container.encode("PageDown")
        case .tab:
            var container = encoder.singleValueContainer()
            try container.encode("Tab")
        case .backTab:
            var container = encoder.singleValueContainer()
            try container.encode("BackTab")
        case .delete:
            var container = encoder.singleValueContainer()
            try container.encode("Delete")
        case .insert:
            var container = encoder.singleValueContainer()
            try container.encode("Insert")
        case .esc:
            var container = encoder.singleValueContainer()
            try container.encode("Esc")
        case .null:
            var container = encoder.singleValueContainer()
            try container.encode("Null")
        }
    }
}

// MARK: - SerializableKeyEvent

/// Mirrors Rust `SerializableKeyEvent`.
/// `modifiers` is a bitfield (`u8`) matching crossterm `KeyModifiers`.
public struct SerializableKeyEvent: Codable, Hashable, Sendable {
    public let code: SerializableKeyCode
    public let modifiers: UInt8

    public init(code: SerializableKeyCode, modifiers: UInt8 = 0) {
        self.code = code
        self.modifiers = modifiers
    }
}

// MARK: - KeyModifiers bitfield constants (matches crossterm)

public enum KeyModifiers: Sendable {
    public static let none: UInt8 = 0b0000_0000
    public static let shift: UInt8 = 0b0000_0001
    public static let control: UInt8 = 0b0000_0010
    public static let alt: UInt8 = 0b0000_0100
}

// MARK: - PluginSegment

/// Mirrors Rust `PluginSegment`.
public struct PluginSegment: Codable, Hashable, Sendable {
    public let text: String
    public let style: String

    public init(text: String, style: String = "dim") {
        self.text = text
        self.style = style
    }
}

// MARK: - ClientListEntry

/// Mirrors Rust `ClientListEntry`.
public struct ClientListEntry: Codable, Hashable, Sendable {
    public let id: UInt64
    public let width: UInt16
    public let height: UInt16
    public let activeWorkspace: Int

    private enum CodingKeys: String, CodingKey {
        case id
        case width
        case height
        case activeWorkspace = "active_workspace"
    }
}

// MARK: - SerializableSystemStats

/// Mirrors Rust `SerializableSystemStats`.
public struct SerializableSystemStats: Codable, Hashable, Sendable {
    public let cpuPercent: Float
    public let memoryPercent: Float
    public let loadAvg1: Double
    public let diskUsagePercent: Float

    private enum CodingKeys: String, CodingKey {
        case cpuPercent = "cpu_percent"
        case memoryPercent = "memory_percent"
        case loadAvg1 = "load_avg_1"
        case diskUsagePercent = "disk_usage_percent"
    }
}
