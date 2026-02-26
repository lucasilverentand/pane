import Foundation

/// Mirrors Rust `ServerResponse` enum.
///
/// Serde externally tagged format:
/// - `{"Attached": {"client_id": 0}}`
/// - `{"PaneOutput": {"pane_id": "uuid", "data": [27, 91, ...]}}`
/// - `{"PaneExited": {"pane_id": "uuid"}}`
/// - `{"LayoutChanged": {"render_state": {...}}}`
/// - `{"StatsUpdate": {"cpu_percent": 42.5, ...}}`
/// - `{"PluginSegments": [[{...}]]}`
/// - `"SessionEnded"` / `"AllWorkspacesClosed"`
/// - `{"FullScreenDump": {"pane_id": "uuid", "data": [...]}`
/// - `{"ClientListChanged": [...]}`
/// - `{"Kicked": 42}`
/// - `{"Error": "message"}`
/// - `{"CommandOutput": {"output": "...", "pane_id": null, "window_id": null, "success": true}}`
public enum ServerResponse: Codable, Sendable {
    case attached(clientId: UInt64)
    case paneOutput(paneId: TabId, data: [UInt8])
    case paneExited(paneId: TabId)
    case layoutChanged(renderState: RenderState)
    case statsUpdate(SerializableSystemStats)
    case pluginSegments([[PluginSegment]])
    case sessionEnded
    case allWorkspacesClosed
    case fullScreenDump(paneId: TabId, data: [UInt8])
    case clientListChanged([ClientListEntry])
    case kicked(UInt64)
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
        case allWorkspacesClosed = "AllWorkspacesClosed"
        case fullScreenDump = "FullScreenDump"
        case clientListChanged = "ClientListChanged"
        case kicked = "Kicked"
        case error = "Error"
        case commandOutput = "CommandOutput"
    }

    private struct AttachedPayload: Codable {
        let client_id: UInt64
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
            case "SessionEnded": self = .sessionEnded; return
            case "AllWorkspacesClosed": self = .allWorkspacesClosed; return
            default: break
            }
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)

        if let payload = try container.decodeIfPresent(AttachedPayload.self, forKey: .attached) {
            self = .attached(clientId: payload.client_id)
        } else if let payload = try container.decodeIfPresent(PaneOutputPayload.self, forKey: .paneOutput) {
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
        } else if let entries = try container.decodeIfPresent([ClientListEntry].self, forKey: .clientListChanged) {
            self = .clientListChanged(entries)
        } else if let id = try container.decodeIfPresent(UInt64.self, forKey: .kicked) {
            self = .kicked(id)
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
        case .attached(let clientId):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(AttachedPayload(client_id: clientId), forKey: .attached)
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
        case .allWorkspacesClosed:
            var container = encoder.singleValueContainer()
            try container.encode("AllWorkspacesClosed")
        case .fullScreenDump(let paneId, let data):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(FullScreenDumpPayload(pane_id: paneId, data: data), forKey: .fullScreenDump)
        case .clientListChanged(let entries):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(entries, forKey: .clientListChanged)
        case .kicked(let id):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(id, forKey: .kicked)
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
