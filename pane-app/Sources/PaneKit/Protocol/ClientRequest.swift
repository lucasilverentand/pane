import Foundation

/// Mirrors Rust `ClientRequest` enum.
///
/// Serde externally tagged format:
/// - `"Attach"` / `"Detach"` / `"MouseUp"` (unit variants)
/// - `{"Resize": {"width": 120, "height": 40}}` (struct variants)
/// - `{"Key": {"code": ..., "modifiers": 0}}` (newtype variant)
/// - `{"Command": "list-panes"}` (newtype variant)
/// - `{"KickClient": 42}` (newtype variant)
/// - `{"SetActiveWorkspace": 0}` (newtype variant)
public enum ClientRequest: Codable, Sendable {
    case attach
    case detach
    case resize(width: UInt16, height: UInt16)
    case key(SerializableKeyEvent)
    case mouseDown(x: UInt16, y: UInt16)
    case mouseDrag(x: UInt16, y: UInt16)
    case mouseMove(x: UInt16, y: UInt16)
    case mouseUp
    case mouseScroll(up: Bool)
    case command(String)
    case commandSync(String)
    case kickClient(UInt64)
    case setActiveWorkspace(Int)

    private enum CodingKeys: String, CodingKey {
        case attach = "Attach"
        case detach = "Detach"
        case resize = "Resize"
        case key = "Key"
        case mouseDown = "MouseDown"
        case mouseDrag = "MouseDrag"
        case mouseMove = "MouseMove"
        case mouseUp = "MouseUp"
        case mouseScroll = "MouseScroll"
        case command = "Command"
        case commandSync = "CommandSync"
        case kickClient = "KickClient"
        case setActiveWorkspace = "SetActiveWorkspace"
    }

    // Struct payloads for struct variants
    private struct ResizePayload: Codable {
        let width: UInt16
        let height: UInt16
    }

    private struct MousePositionPayload: Codable {
        let x: UInt16
        let y: UInt16
    }

    private struct MouseScrollPayload: Codable {
        let up: Bool
    }

    public init(from decoder: any Decoder) throws {
        // Try unit variants first (plain string)
        if let container = try? decoder.singleValueContainer(),
           let str = try? container.decode(String.self)
        {
            switch str {
            case "Attach": self = .attach; return
            case "Detach": self = .detach; return
            case "MouseUp": self = .mouseUp; return
            default: break
            }
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)

        if let _ = try? container.decode(EmptyPayload.self, forKey: .attach) {
            self = .attach
        } else if let _ = try? container.decode(EmptyPayload.self, forKey: .detach) {
            self = .detach
        } else if let _ = try? container.decode(EmptyPayload.self, forKey: .mouseUp) {
            self = .mouseUp
        } else if let payload = try container.decodeIfPresent(ResizePayload.self, forKey: .resize) {
            self = .resize(width: payload.width, height: payload.height)
        } else if let event = try container.decodeIfPresent(SerializableKeyEvent.self, forKey: .key) {
            self = .key(event)
        } else if let payload = try container.decodeIfPresent(MousePositionPayload.self, forKey: .mouseDown) {
            self = .mouseDown(x: payload.x, y: payload.y)
        } else if let payload = try container.decodeIfPresent(MousePositionPayload.self, forKey: .mouseDrag) {
            self = .mouseDrag(x: payload.x, y: payload.y)
        } else if let payload = try container.decodeIfPresent(MousePositionPayload.self, forKey: .mouseMove) {
            self = .mouseMove(x: payload.x, y: payload.y)
        } else if let payload = try container.decodeIfPresent(MouseScrollPayload.self, forKey: .mouseScroll) {
            self = .mouseScroll(up: payload.up)
        } else if let cmd = try container.decodeIfPresent(String.self, forKey: .command) {
            self = .command(cmd)
        } else if let cmd = try container.decodeIfPresent(String.self, forKey: .commandSync) {
            self = .commandSync(cmd)
        } else if let id = try container.decodeIfPresent(UInt64.self, forKey: .kickClient) {
            self = .kickClient(id)
        } else if let idx = try container.decodeIfPresent(Int.self, forKey: .setActiveWorkspace) {
            self = .setActiveWorkspace(idx)
        } else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "Unknown ClientRequest variant")
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .attach:
            var container = encoder.singleValueContainer()
            try container.encode("Attach")
        case .detach:
            var container = encoder.singleValueContainer()
            try container.encode("Detach")
        case .resize(let width, let height):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(ResizePayload(width: width, height: height), forKey: .resize)
        case .key(let event):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(event, forKey: .key)
        case .mouseDown(let x, let y):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(MousePositionPayload(x: x, y: y), forKey: .mouseDown)
        case .mouseDrag(let x, let y):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(MousePositionPayload(x: x, y: y), forKey: .mouseDrag)
        case .mouseMove(let x, let y):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(MousePositionPayload(x: x, y: y), forKey: .mouseMove)
        case .mouseUp:
            var container = encoder.singleValueContainer()
            try container.encode("MouseUp")
        case .mouseScroll(let up):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(MouseScrollPayload(up: up), forKey: .mouseScroll)
        case .command(let cmd):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(cmd, forKey: .command)
        case .commandSync(let cmd):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(cmd, forKey: .commandSync)
        case .kickClient(let id):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(id, forKey: .kickClient)
        case .setActiveWorkspace(let idx):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(idx, forKey: .setActiveWorkspace)
        }
    }
}

// Helper for decoding unit variants that might appear as keyed with null/empty content
private struct EmptyPayload: Codable {}
