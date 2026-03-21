import Foundation

/// Mirrors Rust `ClientRequest` enum.
///
/// Serde externally tagged format:
/// - `"Attach"` / `"Detach"` (unit variants)
/// - `{"Resize": {"width": 120, "height": 40}}` (struct variants)
/// - `{"Key": {"code": ..., "modifiers": 0}}` (newtype variant)
/// - `{"Command": "list-panes"}` (newtype variant)
/// - `{"Paste": "text"}` (newtype variant)
/// - `{"FocusWindow": {"id": "uuid"}}` (struct variant)
/// - `{"SelectTab": {"window_id": "uuid", "tab_index": 0}}` (struct variant)
/// - `{"SetActiveWorkspace": 0}` (newtype variant)
public enum ClientRequest: Codable, Sendable {
    case attach
    case detach
    case resize(width: UInt16, height: UInt16)
    case key(SerializableKeyEvent)
    case mouseDown(x: UInt16, y: UInt16)
    case mouseDrag(x: UInt16, y: UInt16)
    case mouseMove(x: UInt16, y: UInt16)
    case mouseUp(x: UInt16, y: UInt16)
    case mouseScroll(up: Bool)
    case command(String)
    case paste(String)
    case commandSync(String)
    case focusWindow(id: WindowId)
    case selectTab(windowId: WindowId, tabIndex: Int)

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
        case paste = "Paste"
        case commandSync = "CommandSync"
        case focusWindow = "FocusWindow"
        case selectTab = "SelectTab"
    }

    // Struct payloads
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

    private struct FocusWindowPayload: Codable {
        let id: WindowId
    }

    private struct SelectTabPayload: Codable {
        let window_id: WindowId
        let tab_index: Int
    }

    public init(from decoder: any Decoder) throws {
        // Try unit variants first (plain string)
        if let container = try? decoder.singleValueContainer(),
           let str = try? container.decode(String.self)
        {
            switch str {
            case "Attach": self = .attach; return
            case "Detach": self = .detach; return
            default: break
            }
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)

        if let _ = try? container.decode(EmptyPayload.self, forKey: .attach) {
            self = .attach
        } else if let _ = try? container.decode(EmptyPayload.self, forKey: .detach) {
            self = .detach
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
        } else if let payload = try container.decodeIfPresent(MousePositionPayload.self, forKey: .mouseUp) {
            self = .mouseUp(x: payload.x, y: payload.y)
        } else if let payload = try container.decodeIfPresent(MouseScrollPayload.self, forKey: .mouseScroll) {
            self = .mouseScroll(up: payload.up)
        } else if let cmd = try container.decodeIfPresent(String.self, forKey: .command) {
            self = .command(cmd)
        } else if let text = try container.decodeIfPresent(String.self, forKey: .paste) {
            self = .paste(text)
        } else if let cmd = try container.decodeIfPresent(String.self, forKey: .commandSync) {
            self = .commandSync(cmd)
        } else if let payload = try container.decodeIfPresent(FocusWindowPayload.self, forKey: .focusWindow) {
            self = .focusWindow(id: payload.id)
        } else if let payload = try container.decodeIfPresent(SelectTabPayload.self, forKey: .selectTab) {
            self = .selectTab(windowId: payload.window_id, tabIndex: payload.tab_index)
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
        case .mouseUp(let x, let y):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(MousePositionPayload(x: x, y: y), forKey: .mouseUp)
        case .mouseScroll(let up):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(MouseScrollPayload(up: up), forKey: .mouseScroll)
        case .command(let cmd):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(cmd, forKey: .command)
        case .paste(let text):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(text, forKey: .paste)
        case .commandSync(let cmd):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(cmd, forKey: .commandSync)
        case .focusWindow(let id):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(FocusWindowPayload(id: id), forKey: .focusWindow)
        case .selectTab(let windowId, let tabIndex):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(SelectTabPayload(window_id: windowId, tab_index: tabIndex), forKey: .selectTab)
        }
    }
}

// Helper for decoding unit variants that might appear as keyed with null/empty content
private struct EmptyPayload: Codable {}
