import Foundation

/// Mirrors Rust `ServerResponse` enum.
///
/// Serde externally tagged format:
/// - `"Attached"` / `"SessionEnded"` (unit variants)
/// - `{"PaneOutput": {"pane_id": "uuid", "data": [27, 91, ...]}}`
/// - `{"PaneExited": {"pane_id": "uuid"}}`
/// - `{"LayoutChanged": {"render_state": {...}}}`
/// - `{"StatsUpdate": {"cpu_percent": 42.5, ...}}`
/// - `{"PluginSegments": [[{...}]]}`
/// - `{"FullScreenDump": {"pane_id": "uuid", "data": [...]}}`
/// - `{"ClientCountChanged": 3}`
/// - `{"Error": "message"}`
/// - `{"CommandOutput": {"output": "...", "pane_id": null, "window_id": null, "success": true}}`
public enum ServerResponse: Codable, Sendable {
    case attached
    case paneOutput(paneId: TabId, data: [UInt8])
    case paneExited(paneId: TabId)
    case layoutChanged(renderState: RenderState)
    case statsUpdate(SerializableSystemStats)
    case pluginSegments([[PluginSegment]])
    case sessionEnded
    case fullScreenDump(paneId: TabId, data: [UInt8])
    case clientCountChanged(UInt32)
    case error(String)
    case commandOutput(output: String, paneId: UInt32?, windowId: UInt32?, success: Bool)

    private enum CodingKeys: String, CodingKey {
        case attached = "Attached"
        case paneOutput = "PaneOutput"
        case paneExited = "PaneExited"
        case layoutChanged = "LayoutChanged"
        case statsUpdate = "StatsUpdate"
        case pluginSegments = "PluginSegments"
        case sessionEnded = "SessionEnded"
        case fullScreenDump = "FullScreenDump"
        case clientCountChanged = "ClientCountChanged"
        case error = "Error"
        case commandOutput = "CommandOutput"
    }

    private struct PaneOutputPayload: Codable {
        let pane_id: TabId
        let data: [UInt8]
    }

    private struct PaneExitedPayload: Codable {
        let pane_id: TabId
    }

    private struct LayoutChangedPayload: Codable {
        let render_state: RenderState
    }

    private struct FullScreenDumpPayload: Codable {
        let pane_id: TabId
        let data: [UInt8]
    }

    private struct CommandOutputPayload: Codable {
        let output: String
        let pane_id: UInt32?
        let window_id: UInt32?
        let success: Bool
    }

    public init(from decoder: any Decoder) throws {
        // Try unit variants first
        if let container = try? decoder.singleValueContainer(),
           let str = try? container.decode(String.self)
        {
            switch str {
            case "Attached": self = .attached; return
            case "SessionEnded": self = .sessionEnded; return
            default: break
            }
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)

        if let payload = try container.decodeIfPresent(PaneOutputPayload.self, forKey: .paneOutput) {
            self = .paneOutput(paneId: payload.pane_id, data: payload.data)
        } else if let payload = try container.decodeIfPresent(PaneExitedPayload.self, forKey: .paneExited) {
            self = .paneExited(paneId: payload.pane_id)
        } else if let payload = try container.decodeIfPresent(LayoutChangedPayload.self, forKey: .layoutChanged) {
            self = .layoutChanged(renderState: payload.render_state)
        } else if let stats = try container.decodeIfPresent(SerializableSystemStats.self, forKey: .statsUpdate) {
            self = .statsUpdate(stats)
        } else if let segments = try container.decodeIfPresent([[PluginSegment]].self, forKey: .pluginSegments) {
            self = .pluginSegments(segments)
        } else if let payload = try container.decodeIfPresent(FullScreenDumpPayload.self, forKey: .fullScreenDump) {
            self = .fullScreenDump(paneId: payload.pane_id, data: payload.data)
        } else if let count = try container.decodeIfPresent(UInt32.self, forKey: .clientCountChanged) {
            self = .clientCountChanged(count)
        } else if let msg = try container.decodeIfPresent(String.self, forKey: .error) {
            self = .error(msg)
        } else if let payload = try container.decodeIfPresent(CommandOutputPayload.self, forKey: .commandOutput) {
            self = .commandOutput(
                output: payload.output,
                paneId: payload.pane_id,
                windowId: payload.window_id,
                success: payload.success
            )
        } else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "Unknown ServerResponse variant")
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .attached:
            var container = encoder.singleValueContainer()
            try container.encode("Attached")
        case .paneOutput(let paneId, let data):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(PaneOutputPayload(pane_id: paneId, data: data), forKey: .paneOutput)
        case .paneExited(let paneId):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(PaneExitedPayload(pane_id: paneId), forKey: .paneExited)
        case .layoutChanged(let renderState):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(LayoutChangedPayload(render_state: renderState), forKey: .layoutChanged)
        case .statsUpdate(let stats):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(stats, forKey: .statsUpdate)
        case .pluginSegments(let segments):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(segments, forKey: .pluginSegments)
        case .sessionEnded:
            var container = encoder.singleValueContainer()
            try container.encode("SessionEnded")
        case .fullScreenDump(let paneId, let data):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(FullScreenDumpPayload(pane_id: paneId, data: data), forKey: .fullScreenDump)
        case .clientCountChanged(let count):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(count, forKey: .clientCountChanged)
        case .error(let msg):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(msg, forKey: .error)
        case .commandOutput(let output, let paneId, let windowId, let success):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(
                CommandOutputPayload(output: output, pane_id: paneId, window_id: windowId, success: success),
                forKey: .commandOutput
            )
        }
    }
}
