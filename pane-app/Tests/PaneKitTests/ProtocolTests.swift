import Foundation
import Testing
@testable import PaneKit

// MARK: - ClientRequest JSON compatibility

@Suite("ClientRequest serialization")
struct ClientRequestTests {

    @Test("Unit variant: Attach")
    func attachSerializesToString() throws {
        let json = try encode(ClientRequest.attach)
        #expect(json == "\"Attach\"")
        let decoded = try decode(ClientRequest.self, from: json)
        let roundtrip = try encode(decoded)
        #expect(json == roundtrip)
    }

    @Test("Unit variant: Detach")
    func detachSerializesToString() throws {
        let json = try encode(ClientRequest.detach)
        #expect(json == "\"Detach\"")
    }

    @Test("Unit variant: MouseUp")
    func mouseUpSerializesToString() throws {
        let json = try encode(ClientRequest.mouseUp)
        #expect(json == "\"MouseUp\"")
    }

    @Test("Struct variant: Resize")
    func resizeSerializesCorrectly() throws {
        let request = ClientRequest.resize(width: 120, height: 40)
        let json = try encode(request)
        // Should be {"Resize":{"width":120,"height":40}}
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        let resize = obj["Resize"] as! [String: Any]
        #expect(resize["width"] as! Int == 120)
        #expect(resize["height"] as! Int == 40)

        // Roundtrip
        let decoded = try decode(ClientRequest.self, from: json)
        if case .resize(let w, let h) = decoded {
            #expect(w == 120)
            #expect(h == 40)
        } else {
            Issue.record("Expected .resize")
        }
    }

    @Test("Newtype variant: Key with Char")
    func keyCharSerializesCorrectly() throws {
        let event = SerializableKeyEvent(code: .char("a"), modifiers: KeyModifiers.none)
        let request = ClientRequest.key(event)
        let json = try encode(request)
        // Should be {"Key":{"code":{"Char":"a"},"modifiers":0}}
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        let key = obj["Key"] as! [String: Any]
        let code = key["code"] as! [String: Any]
        #expect(code["Char"] as! String == "a")
        #expect(key["modifiers"] as! Int == 0)
    }

    @Test("Newtype variant: Key with modifier")
    func keyWithControlSerializesCorrectly() throws {
        let event = SerializableKeyEvent(code: .char("c"), modifiers: KeyModifiers.control)
        let request = ClientRequest.key(event)
        let json = try encode(request)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        let key = obj["Key"] as! [String: Any]
        #expect(key["modifiers"] as! Int == 2) // CONTROL = 0b10
    }

    @Test("Newtype variant: Command")
    func commandSerializesCorrectly() throws {
        let request = ClientRequest.command("list-panes")
        let json = try encode(request)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        #expect(obj["Command"] as! String == "list-panes")
    }

    @Test("Newtype variant: KickClient")
    func kickClientSerializesCorrectly() throws {
        let request = ClientRequest.kickClient(42)
        let json = try encode(request)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        #expect(obj["KickClient"] as! Int == 42)
    }

    @Test("Newtype variant: SetActiveWorkspace")
    func setActiveWorkspaceSerializesCorrectly() throws {
        let request = ClientRequest.setActiveWorkspace(2)
        let json = try encode(request)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        #expect(obj["SetActiveWorkspace"] as! Int == 2)
    }

    @Test("Struct variant: MouseDown")
    func mouseDownSerializesCorrectly() throws {
        let request = ClientRequest.mouseDown(x: 10, y: 5)
        let json = try encode(request)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        let payload = obj["MouseDown"] as! [String: Any]
        #expect(payload["x"] as! Int == 10)
        #expect(payload["y"] as! Int == 5)
    }

    @Test("Struct variant: MouseScroll")
    func mouseScrollSerializesCorrectly() throws {
        let request = ClientRequest.mouseScroll(up: true)
        let json = try encode(request)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        let payload = obj["MouseScroll"] as! [String: Any]
        #expect(payload["up"] as! Bool == true)
    }

    @Test("All variants roundtrip")
    func allVariantsRoundtrip() throws {
        let requests: [ClientRequest] = [
            .attach,
            .detach,
            .resize(width: 80, height: 24),
            .key(SerializableKeyEvent(code: .enter, modifiers: 0)),
            .mouseDown(x: 1, y: 2),
            .mouseDrag(x: 3, y: 4),
            .mouseMove(x: 5, y: 6),
            .mouseUp,
            .mouseScroll(up: false),
            .command("help"),
            .commandSync("list-panes"),
            .kickClient(99),
            .setActiveWorkspace(0),
        ]

        for request in requests {
            let json = try encode(request)
            let decoded = try decode(ClientRequest.self, from: json)
            let roundtrip = try encode(decoded)
            #expect(json == roundtrip, "Roundtrip failed for: \(request)")
        }
    }
}

// MARK: - ServerResponse JSON compatibility

@Suite("ServerResponse serialization")
struct ServerResponseTests {

    @Test("Attached")
    func attachedDeserializesCorrectly() throws {
        let json = #"{"Attached":{"client_id":42}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .attached(let id) = response {
            #expect(id == 42)
        } else {
            Issue.record("Expected .attached")
        }
    }

    @Test("SessionEnded")
    func sessionEndedDeserializesCorrectly() throws {
        let json = #""SessionEnded""#
        let response = try decode(ServerResponse.self, from: json)
        if case .sessionEnded = response {
            // pass
        } else {
            Issue.record("Expected .sessionEnded")
        }
    }

    @Test("AllWorkspacesClosed")
    func allWorkspacesClosedDeserializesCorrectly() throws {
        let json = #""AllWorkspacesClosed""#
        let response = try decode(ServerResponse.self, from: json)
        if case .allWorkspacesClosed = response {
            // pass
        } else {
            Issue.record("Expected .allWorkspacesClosed")
        }
    }

    @Test("Error")
    func errorDeserializesCorrectly() throws {
        let json = #"{"Error":"something failed"}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .error(let msg) = response {
            #expect(msg == "something failed")
        } else {
            Issue.record("Expected .error")
        }
    }

    @Test("Kicked")
    func kickedDeserializesCorrectly() throws {
        let json = #"{"Kicked":7}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .kicked(let id) = response {
            #expect(id == 7)
        } else {
            Issue.record("Expected .kicked")
        }
    }

    @Test("PaneOutput with byte array")
    func paneOutputDeserializesCorrectly() throws {
        let uuid = UUID()
        let json = #"{"PaneOutput":{"pane_id":"\#(uuid.uuidString.lowercased())","data":[27,91,72]}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .paneOutput(let paneId, let data) = response {
            #expect(paneId.raw == uuid)
            #expect(data == [27, 91, 72])
        } else {
            Issue.record("Expected .paneOutput")
        }
    }

    @Test("StatsUpdate")
    func statsUpdateDeserializesCorrectly() throws {
        let json = #"{"StatsUpdate":{"cpu_percent":42.5,"memory_percent":67.8,"load_avg_1":1.23,"disk_usage_percent":55.0}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .statsUpdate(let stats) = response {
            #expect(abs(stats.cpuPercent - 42.5) < 0.01)
            #expect(abs(stats.memoryPercent - 67.8) < 0.01)
            #expect(abs(stats.loadAvg1 - 1.23) < 0.001)
            #expect(abs(stats.diskUsagePercent - 55.0) < 0.01)
        } else {
            Issue.record("Expected .statsUpdate")
        }
    }

    @Test("CommandOutput")
    func commandOutputDeserializesCorrectly() throws {
        let json = #"{"CommandOutput":{"output":"done","pane_id":null,"window_id":null,"success":true}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .commandOutput(let output, let paneId, let windowId, let success) = response {
            #expect(output == "done")
            #expect(paneId == nil)
            #expect(windowId == nil)
            #expect(success == true)
        } else {
            Issue.record("Expected .commandOutput")
        }
    }

    @Test("ClientListChanged")
    func clientListChangedDeserializesCorrectly() throws {
        let json = #"{"ClientListChanged":[{"id":1,"width":120,"height":40,"active_workspace":0}]}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .clientListChanged(let entries) = response {
            #expect(entries.count == 1)
            #expect(entries[0].id == 1)
            #expect(entries[0].width == 120)
            #expect(entries[0].height == 40)
            #expect(entries[0].activeWorkspace == 0)
        } else {
            Issue.record("Expected .clientListChanged")
        }
    }

    @Test("LayoutChanged with full RenderState")
    func layoutChangedDeserializesCorrectly() throws {
        let windowId = UUID()
        let tabId = UUID()
        let json = """
        {"LayoutChanged":{"render_state":{"workspaces":[{"name":"ws1","layout":{"Leaf":"\(windowId.uuidString.lowercased())"},"groups":[{"id":"\(windowId.uuidString.lowercased())","tabs":[{"id":"\(tabId.uuidString.lowercased())","kind":"Shell","title":"shell","exited":false,"foreground_process":null,"cwd":"/tmp"}],"active_tab":0}],"active_group":"\(windowId.uuidString.lowercased())","sync_panes":false,"folded_windows":[],"zoomed_window":null,"floating_windows":[]}],"active_workspace":0}}}
        """
        let response = try decode(ServerResponse.self, from: json)
        if case .layoutChanged(let state) = response {
            #expect(state.workspaces.count == 1)
            #expect(state.workspaces[0].name == "ws1")
            #expect(state.workspaces[0].groups.count == 1)
            #expect(state.workspaces[0].groups[0].tabs[0].kind == .shell)
            #expect(state.workspaces[0].groups[0].tabs[0].title == "shell")
        } else {
            Issue.record("Expected .layoutChanged")
        }
    }
}

// MARK: - SerializableKeyCode tests

@Suite("SerializableKeyCode serialization")
struct KeyCodeTests {

    @Test("Unit variants serialize as strings")
    func unitVariantsSerializeAsStrings() throws {
        let cases: [(SerializableKeyCode, String)] = [
            (.backspace, "\"Backspace\""),
            (.enter, "\"Enter\""),
            (.left, "\"Left\""),
            (.right, "\"Right\""),
            (.up, "\"Up\""),
            (.down, "\"Down\""),
            (.home, "\"Home\""),
            (.end, "\"End\""),
            (.pageUp, "\"PageUp\""),
            (.pageDown, "\"PageDown\""),
            (.tab, "\"Tab\""),
            (.backTab, "\"BackTab\""),
            (.delete, "\"Delete\""),
            (.insert, "\"Insert\""),
            (.esc, "\"Esc\""),
            (.null, "\"Null\""),
        ]

        for (code, expected) in cases {
            let json = try encode(code)
            #expect(json == expected, "Expected \(expected), got \(json)")
        }
    }

    @Test("Char variant serializes as object")
    func charSerializesAsObject() throws {
        let json = try encode(SerializableKeyCode.char("x"))
        #expect(json == #"{"Char":"x"}"#)
    }

    @Test("F variant serializes as object")
    func fSerializesAsObject() throws {
        let json = try encode(SerializableKeyCode.f(5))
        #expect(json == #"{"F":5}"#)
    }

    @Test("All key codes roundtrip")
    func allKeyCodesRoundtrip() throws {
        let codes: [SerializableKeyCode] = [
            .char("a"), .char("Z"), .f(1), .f(12),
            .backspace, .enter, .left, .right, .up, .down,
            .home, .end, .pageUp, .pageDown, .tab, .backTab,
            .delete, .insert, .esc, .null,
        ]

        for code in codes {
            let json = try encode(code)
            let decoded = try decode(SerializableKeyCode.self, from: json)
            let roundtrip = try encode(decoded)
            #expect(json == roundtrip, "Roundtrip failed for \(code)")
        }
    }
}

// MARK: - LayoutNode tests

@Suite("LayoutNode serialization")
struct LayoutNodeTests {

    @Test("Leaf serializes correctly")
    func leafSerializesCorrectly() throws {
        let id = WindowId(UUID())
        let node = LayoutNode.leaf(id)
        let json = try encode(node)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        #expect(obj["Leaf"] as! String == id.raw.uuidString.lowercased())
    }

    @Test("Split serializes correctly")
    func splitSerializesCorrectly() throws {
        let id1 = WindowId(UUID())
        let id2 = WindowId(UUID())
        let node = LayoutNode.split(
            direction: .horizontal,
            ratio: 0.5,
            first: .leaf(id1),
            second: .leaf(id2)
        )
        let json = try encode(node)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
        let split = obj["Split"] as! [String: Any]
        #expect(split["direction"] as! String == "Horizontal")
        #expect(split["ratio"] as! Double == 0.5)
    }

    @Test("Nested split roundtrips")
    func nestedSplitRoundtrips() throws {
        let id1 = WindowId(UUID())
        let id2 = WindowId(UUID())
        let id3 = WindowId(UUID())
        let node = LayoutNode.split(
            direction: .horizontal,
            ratio: 0.5,
            first: .leaf(id1),
            second: .split(
                direction: .vertical,
                ratio: 0.3,
                first: .leaf(id2),
                second: .leaf(id3)
            )
        )

        let json = try encode(node)
        let decoded = try decode(LayoutNode.self, from: json)
        let roundtrip = try encode(decoded)
        #expect(json == roundtrip)
    }

    @Test("windowIds collects all IDs")
    func windowIdsCollectsAll() {
        let id1 = WindowId(UUID())
        let id2 = WindowId(UUID())
        let id3 = WindowId(UUID())
        let node = LayoutNode.split(
            direction: .horizontal,
            ratio: 0.5,
            first: .leaf(id1),
            second: .split(
                direction: .vertical,
                ratio: 0.5,
                first: .leaf(id2),
                second: .leaf(id3)
            )
        )

        let ids = node.windowIds
        #expect(ids.count == 3)
        #expect(ids.contains(id1))
        #expect(ids.contains(id2))
        #expect(ids.contains(id3))
    }
}

// MARK: - TabId / WindowId tests

@Suite("ID types")
struct IDTests {

    @Test("TabId encodes as lowercase UUID string")
    func tabIdEncodesCorrectly() throws {
        let uuid = UUID()
        let tabId = TabId(uuid)
        let json = try encode(tabId)
        #expect(json == "\"\(uuid.uuidString.lowercased())\"")
    }

    @Test("WindowId encodes as lowercase UUID string")
    func windowIdEncodesCorrectly() throws {
        let uuid = UUID()
        let windowId = WindowId(uuid)
        let json = try encode(windowId)
        #expect(json == "\"\(uuid.uuidString.lowercased())\"")
    }

    @Test("TabId roundtrips")
    func tabIdRoundtrips() throws {
        let original = TabId(UUID())
        let json = try encode(original)
        let decoded = try decode(TabId.self, from: json)
        #expect(original == decoded)
    }

    @Test("WindowId roundtrips")
    func windowIdRoundtrips() throws {
        let original = WindowId(UUID())
        let json = try encode(original)
        let decoded = try decode(WindowId.self, from: json)
        #expect(original == decoded)
    }
}

// MARK: - RenderState tests

@Suite("RenderState serialization")
struct RenderStateTests {

    @Test("Full RenderState from Rust-compatible JSON")
    func fullRenderStateFromRust() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "workspaces": [{
                "name": "dev",
                "layout": {"Leaf": "\(wid)"},
                "groups": [{
                    "id": "\(wid)",
                    "tabs": [{
                        "id": "\(tid)",
                        "kind": "Agent",
                        "title": "claude",
                        "exited": false,
                        "foreground_process": "python",
                        "cwd": "/home/user/project"
                    }],
                    "active_tab": 0
                }],
                "active_group": "\(wid)",
                "sync_panes": true,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": []
            }],
            "active_workspace": 0
        }
        """

        let state = try decode(RenderState.self, from: json)
        #expect(state.workspaces.count == 1)
        #expect(state.activeWorkspace == 0)

        let ws = state.workspaces[0]
        #expect(ws.name == "dev")
        #expect(ws.syncPanes == true)
        #expect(ws.foldedWindows.isEmpty)
        #expect(ws.zoomedWindow == nil)
        #expect(ws.floatingWindows.isEmpty)

        let window = ws.groups[0]
        #expect(window.tabs.count == 1)
        #expect(window.activeTab == 0)

        let tab = window.tabs[0]
        #expect(tab.kind == .agent)
        #expect(tab.title == "claude")
        #expect(tab.exited == false)
        #expect(tab.foregroundProcess == "python")
        #expect(tab.cwd == "/home/user/project")
    }

    @Test("WorkspaceSnapshot with folded_windows absent defaults to empty")
    func foldedWindowsDefaultsToEmpty() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        // Omit folded_windows entirely — should default to empty set
        let json = """
        {
            "name": "ws",
            "layout": {"Leaf": "\(wid)"},
            "groups": [{
                "id": "\(wid)",
                "tabs": [{"id": "\(tid)", "kind": "Shell", "title": "sh", "exited": false, "foreground_process": null, "cwd": "/"}],
                "active_tab": 0
            }],
            "active_group": "\(wid)",
            "sync_panes": false,
            "zoomed_window": null,
            "floating_windows": []
        }
        """
        let ws = try decode(WorkspaceSnapshot.self, from: json)
        #expect(ws.foldedWindows.isEmpty)
    }

    @Test("TabKind all variants")
    func tabKindAllVariants() throws {
        let cases: [(String, TabKind)] = [
            ("\"Shell\"", .shell),
            ("\"Agent\"", .agent),
            ("\"Nvim\"", .nvim),
            ("\"DevServer\"", .devServer),
        ]
        for (json, expected) in cases {
            let decoded = try decode(TabKind.self, from: json)
            #expect(decoded == expected)
            let encoded = try encode(decoded)
            #expect(encoded == json)
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
    let data = Data(json.utf8)
    return try JSONDecoder().decode(type, from: data)
}
