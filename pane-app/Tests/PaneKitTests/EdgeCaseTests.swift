import Foundation
import Testing
@testable import PaneKit

/// Edge case tests for the protocol, framing, and serialization layers.
///
/// Covers: Unicode, empty data, boundary values, malformed JSON,
/// partial frames, backwards compatibility with missing fields.
@Suite("Edge cases")
struct EdgeCaseTests {

    // MARK: - Unicode handling

    @Test("Paste with emoji roundtrips correctly")
    func pasteWithEmoji() throws {
        let text = "Hello 🌍🚀 World! 日本語 العربية"
        let request = ClientRequest.paste(text)
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .paste(let decodedText) = decoded {
            #expect(decodedText == text)
        } else {
            Issue.record("Expected .paste")
        }
    }

    @Test("Paste with escape sequences roundtrips correctly")
    func pasteWithEscapeSequences() throws {
        let text = "line1\nline2\ttab\r\nwindows\0null"
        let request = ClientRequest.paste(text)
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .paste(let decodedText) = decoded {
            #expect(decodedText == text)
        } else {
            Issue.record("Expected .paste")
        }
    }

    @Test("Paste with empty string")
    func pasteWithEmptyString() throws {
        let request = ClientRequest.paste("")
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .paste(let text) = decoded {
            #expect(text == "")
        } else {
            Issue.record("Expected .paste")
        }
    }

    @Test("Command with special characters")
    func commandWithSpecialChars() throws {
        let cmd = "rename-workspace \"my workspace / test\""
        let request = ClientRequest.command(cmd)
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .command(let decodedCmd) = decoded {
            #expect(decodedCmd == cmd)
        } else {
            Issue.record("Expected .command")
        }
    }

    @Test("Char key with Unicode character")
    func charKeyUnicode() throws {
        let code = SerializableKeyCode.char("é")
        let json = try encode(code)
        let decoded = try decode(SerializableKeyCode.self, from: json)
        if case .char(let c) = decoded {
            #expect(c == "é")
        } else {
            Issue.record("Expected .char")
        }
    }

    @Test("Workspace name with Unicode")
    func workspaceNameUnicode() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let name = "开发环境 🖥️"
        let json = """
        {
            "name": "\(name)",
            "cwd": "/tmp",
            "layout": {"Leaf": "\(wid)"},
            "groups": [{"id": "\(wid)", "tabs": [{"id": "\(tid)", "kind": "Shell", "title": "sh", "exited": false, "foreground_process": null, "cwd": "/tmp"}], "active_tab": 0}],
            "active_group": "\(wid)",
            "sync_panes": false,
            "folded_windows": [],
            "zoomed_window": null,
            "floating_windows": []
        }
        """
        let ws = try decode(WorkspaceSnapshot.self, from: json)
        #expect(ws.name == name)
    }

    // MARK: - Boundary values

    @Test("Resize with max UInt16 values")
    func resizeMaxValues() throws {
        let request = ClientRequest.resize(width: UInt16.max, height: UInt16.max)
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .resize(let w, let h) = decoded {
            #expect(w == UInt16.max)
            #expect(h == UInt16.max)
        } else {
            Issue.record("Expected .resize")
        }
    }

    @Test("Resize with zero values")
    func resizeZeroValues() throws {
        let request = ClientRequest.resize(width: 0, height: 0)
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .resize(let w, let h) = decoded {
            #expect(w == 0)
            #expect(h == 0)
        } else {
            Issue.record("Expected .resize")
        }
    }

    @Test("Mouse position with max UInt16")
    func mouseMaxPosition() throws {
        let request = ClientRequest.mouseDown(x: UInt16.max, y: UInt16.max)
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .mouseDown(let x, let y) = decoded {
            #expect(x == UInt16.max)
            #expect(y == UInt16.max)
        } else {
            Issue.record("Expected .mouseDown")
        }
    }

    @Test("SelectTab with large tab index")
    func selectTabLargeIndex() throws {
        let id = WindowId(UUID())
        let request = ClientRequest.selectTab(windowId: id, tabIndex: 999)
        let json = try encode(request)
        let decoded = try decode(ClientRequest.self, from: json)
        if case .selectTab(let windowId, let idx) = decoded {
            #expect(windowId == id)
            #expect(idx == 999)
        } else {
            Issue.record("Expected .selectTab")
        }
    }

    @Test("ClientCountChanged with max UInt32 count")
    func clientCountChangedMaxValue() throws {
        let json = #"{"ClientCountChanged":4294967295}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .clientCountChanged(let count) = response {
            #expect(count == UInt32.max)
        } else {
            Issue.record("Expected .clientCountChanged")
        }
    }

    @Test("PaneOutput with empty data array")
    func paneOutputEmptyData() throws {
        let uuid = UUID().uuidString.lowercased()
        let json = #"{"PaneOutput":{"pane_id":"\#(uuid)","data":[]}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .paneOutput(_, let data) = response {
            #expect(data.isEmpty)
        } else {
            Issue.record("Expected .paneOutput")
        }
    }

    @Test("PaneOutput with all byte values 0-255")
    func paneOutputAllByteValues() throws {
        let allBytes = Array(0...255).map { UInt8($0) }
        let uuid = UUID().uuidString.lowercased()
        let dataJson = allBytes.map { String($0) }.joined(separator: ",")
        let json = #"{"PaneOutput":{"pane_id":"\#(uuid)","data":[\#(dataJson)]}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .paneOutput(_, let data) = response {
            #expect(data.count == 256)
            for i in 0..<256 {
                #expect(data[i] == UInt8(i))
            }
        } else {
            Issue.record("Expected .paneOutput")
        }
    }

    @Test("ClientCountChanged with zero count")
    func clientCountChangedZero() throws {
        let json = #"{"ClientCountChanged":0}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .clientCountChanged(let count) = response {
            #expect(count == 0)
        } else {
            Issue.record("Expected .clientCountChanged")
        }
    }

    @Test("Empty workspace list")
    func emptyWorkspaceList() throws {
        let json = #"{"workspaces":[],"active_workspace":0}"#
        let state = try decode(RenderState.self, from: json)
        #expect(state.workspaces.isEmpty)
        #expect(state.activeWorkspace == 0)
    }

    @Test("Workspace with no groups")
    func workspaceNoGroups() throws {
        let wid = UUID().uuidString.lowercased()
        let json = """
        {
            "name": "empty",
            "cwd": "",
            "layout": {"Leaf": "\(wid)"},
            "groups": [],
            "active_group": "\(wid)",
            "sync_panes": false,
            "folded_windows": [],
            "zoomed_window": null,
            "floating_windows": []
        }
        """
        let ws = try decode(WorkspaceSnapshot.self, from: json)
        #expect(ws.groups.isEmpty)
    }

    @Test("Window with no tabs")
    func windowNoTabs() throws {
        let wid = UUID().uuidString.lowercased()
        let json = """
        {"id": "\(wid)", "tabs": [], "active_tab": 0}
        """
        let window = try decode(WindowSnapshot.self, from: json)
        #expect(window.tabs.isEmpty)
    }

    // MARK: - Malformed JSON handling

    @Test("Completely invalid JSON throws")
    func invalidJsonThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(ServerResponse.self, from: "not json at all")
        }
    }

    @Test("Unknown variant key throws")
    func unknownVariantThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(ServerResponse.self, from: #"{"UnknownVariant": 42}"#)
        }
    }

    @Test("Unknown ClientRequest variant throws")
    func unknownClientRequestVariantThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(ClientRequest.self, from: #"{"UnknownCommand": "test"}"#)
        }
    }

    @Test("Truncated JSON throws")
    func truncatedJsonThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(ServerResponse.self, from: #"{"Attached":{"client_id":"#)
        }
    }

    @Test("Wrong type for field throws")
    func wrongFieldTypeThrows() {
        #expect(throws: (any Error).self) {
            // client_id should be a number, not a string
            _ = try decode(ServerResponse.self, from: #"{"Attached":{"client_id":"not a number"}}"#)
        }
    }

    @Test("Invalid UUID string throws")
    func invalidUuidThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(TabId.self, from: #""not-a-uuid""#)
        }
    }

    @Test("Empty string for UUID throws")
    func emptyUuidThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(TabId.self, from: #""""#)
        }
    }

    @Test("Char key with empty string throws")
    func charKeyEmptyStringThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(SerializableKeyCode.self, from: #"{"Char":""}"#)
        }
    }

    @Test("Char key with multi-character string throws")
    func charKeyMultiCharThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(SerializableKeyCode.self, from: #"{"Char":"ab"}"#)
        }
    }

    @Test("Unknown key code string throws")
    func unknownKeyCodeThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(SerializableKeyCode.self, from: #""CapsLock""#)
        }
    }

    @Test("Unknown TabKind throws")
    func unknownTabKindThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(TabKind.self, from: #""Browser""#)
        }
    }

    @Test("Unknown SplitDirection throws")
    func unknownSplitDirectionThrows() {
        #expect(throws: (any Error).self) {
            _ = try decode(SplitDirection.self, from: #""Diagonal""#)
        }
    }

    // MARK: - Framing edge cases

    @Test("Zero-length frame")
    func zeroLengthFrame() throws {
        var data = Data(count: 4)
        var zero: UInt32 = 0
        data.replaceSubrange(0..<4, with: Data(bytes: &zero, count: 4))
        let length = try Framing.decodeLength(data)
        #expect(length == 0)
    }

    @Test("Frame at exactly max size boundary")
    func frameAtMaxSizeBoundary() throws {
        let maxSize: UInt32 = Framing.maxFrameSize
        var data = Data(count: 4)
        var be = maxSize.bigEndian
        data.replaceSubrange(0..<4, with: Data(bytes: &be, count: 4))
        let length = try Framing.decodeLength(data)
        #expect(length == maxSize)
    }

    @Test("Frame one byte over max size is rejected")
    func frameOverMaxSizeRejected() {
        let overSize: UInt32 = Framing.maxFrameSize + 1
        var data = Data(count: 4)
        var be = overSize.bigEndian
        data.replaceSubrange(0..<4, with: Data(bytes: &be, count: 4))
        #expect(throws: FramingError.self) {
            _ = try Framing.decodeLength(data)
        }
    }

    @Test("Empty data for length prefix throws")
    func emptyLengthPrefixThrows() {
        #expect(throws: FramingError.self) {
            _ = try Framing.decodeLength(Data())
        }
    }

    @Test("One-byte data for length prefix throws")
    func oneByteForLengthThrows() {
        #expect(throws: FramingError.self) {
            _ = try Framing.decodeLength(Data([0]))
        }
    }

    @Test("Two-byte data for length prefix throws")
    func twoBytesForLengthThrows() {
        #expect(throws: FramingError.self) {
            _ = try Framing.decodeLength(Data([0, 0]))
        }
    }

    @Test("Three-byte data for length prefix throws")
    func threeBytesForLengthThrows() {
        #expect(throws: FramingError.self) {
            _ = try Framing.decodeLength(Data([0, 0, 0]))
        }
    }

    // MARK: - Backwards compatibility: missing optional fields

    @Test("WorkspaceSnapshot without cwd defaults to empty")
    func workspaceWithoutCwd() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "name": "test",
            "layout": {"Leaf": "\(wid)"},
            "groups": [{"id": "\(wid)", "tabs": [{"id": "\(tid)", "kind": "Shell", "title": "sh", "exited": false, "foreground_process": null, "cwd": "/"}], "active_tab": 0}],
            "active_group": "\(wid)",
            "sync_panes": false,
            "folded_windows": [],
            "zoomed_window": null,
            "floating_windows": []
        }
        """
        let ws = try decode(WorkspaceSnapshot.self, from: json)
        #expect(ws.cwd == "")
    }

    @Test("WorkspaceSnapshot without folded_windows defaults to empty set")
    func workspaceWithoutFoldedWindows() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "name": "test",
            "cwd": "/tmp",
            "layout": {"Leaf": "\(wid)"},
            "groups": [{"id": "\(wid)", "tabs": [{"id": "\(tid)", "kind": "Shell", "title": "sh", "exited": false, "foreground_process": null, "cwd": "/"}], "active_tab": 0}],
            "active_group": "\(wid)",
            "sync_panes": false,
            "zoomed_window": null,
            "floating_windows": []
        }
        """
        let ws = try decode(WorkspaceSnapshot.self, from: json)
        #expect(ws.foldedWindows.isEmpty)
    }

    @Test("TabSnapshot with null foreground_process")
    func tabWithNullForegroundProcess() throws {
        let tid = UUID().uuidString.lowercased()
        let json = """
        {"id": "\(tid)", "kind": "Shell", "title": "sh", "exited": false, "foreground_process": null, "cwd": "/tmp"}
        """
        let tab = try decode(TabSnapshot.self, from: json)
        #expect(tab.foregroundProcess == nil)
    }

    @Test("TabSnapshot with foreground_process string")
    func tabWithForegroundProcess() throws {
        let tid = UUID().uuidString.lowercased()
        let json = """
        {"id": "\(tid)", "kind": "Shell", "title": "zsh", "exited": false, "foreground_process": "vim", "cwd": "/home"}
        """
        let tab = try decode(TabSnapshot.self, from: json)
        #expect(tab.foregroundProcess == "vim")
    }

    // MARK: - LayoutNode edge cases

    @Test("Deeply nested split tree roundtrips")
    func deeplyNestedSplitTree() throws {
        // Build a 10-level deep split tree
        var node = LayoutNode.leaf(WindowId(UUID()))
        for _ in 0..<10 {
            node = .split(
                direction: .horizontal,
                ratio: 0.5,
                first: node,
                second: .leaf(WindowId(UUID()))
            )
        }
        let json = try encode(node)
        let decoded = try decode(LayoutNode.self, from: json)
        let roundtrip = try encode(decoded)
        #expect(json == roundtrip)

        // Should contain 11 window IDs (1 initial + 10 added)
        #expect(decoded.windowIds.count == 11)
    }

    @Test("Split with extreme ratio values")
    func splitExtremeRatios() throws {
        let id1 = WindowId(UUID())
        let id2 = WindowId(UUID())

        // Ratio 0.0
        let zero = LayoutNode.split(direction: .horizontal, ratio: 0.0, first: .leaf(id1), second: .leaf(id2))
        let zeroJson = try encode(zero)
        let decodedZero = try decode(LayoutNode.self, from: zeroJson)
        if case .split(_, let ratio, _, _) = decodedZero {
            #expect(ratio == 0.0)
        }

        // Ratio 1.0
        let one = LayoutNode.split(direction: .vertical, ratio: 1.0, first: .leaf(id1), second: .leaf(id2))
        let oneJson = try encode(one)
        let decodedOne = try decode(LayoutNode.self, from: oneJson)
        if case .split(_, let ratio, _, _) = decodedOne {
            #expect(ratio == 1.0)
        }
    }

    // MARK: - Error message edge cases

    @Test("Error response with empty message")
    func errorEmptyMessage() throws {
        let json = #"{"Error":""}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .error(let msg) = response {
            #expect(msg == "")
        } else {
            Issue.record("Expected .error")
        }
    }

    @Test("Error response with very long message")
    func errorLongMessage() throws {
        let longMsg = String(repeating: "x", count: 10_000)
        let json = "{\"Error\":\"\(longMsg)\"}"
        let response = try decode(ServerResponse.self, from: json)
        if case .error(let msg) = response {
            #expect(msg.count == 10_000)
        } else {
            Issue.record("Expected .error")
        }
    }

    // MARK: - Framing over the wire

    @Test("Malformed frame payload is detected during receive")
    func malformedPayloadDetected() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        // Send a frame with valid length but invalid JSON payload
        let badPayload = Data("not valid json!".utf8)
        var length = UInt32(badPayload.count).bigEndian
        var frame = Data(bytes: &length, count: 4)
        frame.append(badPayload)
        try server.sendRaw(frame)

        // Client should fail to decode
        do {
            let _: ServerResponse? = try await connection.receive(ServerResponse.self)
            Issue.record("Expected decoding error")
        } catch {
            // Expected — malformed JSON
        }
    }

    // MARK: - Multiple workspaces with various states

    @Test("Multiple workspaces with different configurations")
    func multipleWorkspaceConfigs() throws {
        let w1 = UUID().uuidString.lowercased()
        let w2 = UUID().uuidString.lowercased()
        let w3 = UUID().uuidString.lowercased()
        let t1 = UUID().uuidString.lowercased()
        let t2 = UUID().uuidString.lowercased()
        let t3 = UUID().uuidString.lowercased()

        let json = """
        {
            "workspaces": [
                {
                    "name": "code",
                    "cwd": "/project",
                    "layout": {"Leaf": "\(w1)"},
                    "groups": [{"id": "\(w1)", "tabs": [{"id": "\(t1)", "kind": "Agent", "title": "claude", "exited": false, "foreground_process": null, "cwd": "/project"}], "active_tab": 0}],
                    "active_group": "\(w1)",
                    "sync_panes": false,
                    "folded_windows": [],
                    "zoomed_window": "\(w1)",
                    "floating_windows": []
                },
                {
                    "name": "servers",
                    "cwd": "/app",
                    "layout": {"Split": {"direction": "Vertical", "ratio": 0.3, "first": {"Leaf": "\(w2)"}, "second": {"Leaf": "\(w3)"}}},
                    "groups": [
                        {"id": "\(w2)", "tabs": [{"id": "\(t2)", "kind": "DevServer", "title": "next dev", "exited": false, "foreground_process": "node", "cwd": "/app"}], "active_tab": 0},
                        {"id": "\(w3)", "tabs": [{"id": "\(t3)", "kind": "Shell", "title": "logs", "exited": true, "foreground_process": null, "cwd": "/var/log"}], "active_tab": 0}
                    ],
                    "active_group": "\(w2)",
                    "sync_panes": true,
                    "folded_windows": ["\(w3)"],
                    "zoomed_window": null,
                    "floating_windows": []
                }
            ],
            "active_workspace": 1
        }
        """

        let state = try decode(RenderState.self, from: json)
        #expect(state.workspaces.count == 2)
        #expect(state.activeWorkspace == 1)

        let code = state.workspaces[0]
        #expect(code.name == "code")
        #expect(code.zoomedWindow != nil)
        #expect(code.groups[0].tabs[0].kind == .agent)

        let servers = state.workspaces[1]
        #expect(servers.name == "servers")
        #expect(servers.syncPanes == true)
        #expect(servers.foldedWindows.count == 1)
        #expect(servers.groups.count == 2)
        #expect(servers.groups[0].tabs[0].kind == .devServer)
        #expect(servers.groups[0].tabs[0].foregroundProcess == "node")
        #expect(servers.groups[1].tabs[0].exited == true)

        if case .split(let dir, let ratio, _, _) = servers.layout {
            #expect(dir == .vertical)
            #expect(abs(ratio - 0.3) < 0.001)
        } else {
            Issue.record("Expected .split layout")
        }
    }
}

// MARK: - Helpers

private func encode<T: Encodable>(_ value: T) throws -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let data = try encoder.encode(value)
    return String(data: data, encoding: .utf8)!
}

private func decode<T: Decodable>(_ type: T.Type, from json: String) throws -> T {
    try JSONDecoder().decode(type, from: Data(json.utf8))
}
