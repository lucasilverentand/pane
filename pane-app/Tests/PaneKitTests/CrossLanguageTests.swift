import Foundation
import Testing
@testable import PaneKit

/// Cross-language compatibility tests.
///
/// These use hardcoded JSON strings matching the exact output of Rust's
/// `serde_json::to_string()` for the pane-protocol crate. If the Rust
/// protocol changes, these tests should be updated to match.
///
/// Purpose: catch protocol drift between the Rust daemon and Swift client.
@Suite("Cross-language JSON compatibility")
struct CrossLanguageTests {

    // MARK: - ClientRequest: Swift → Rust

    /// Verify that Swift-encoded ClientRequests match what Rust expects.

    @Test("Attach encodes as Rust unit variant")
    func attachMatchesRust() throws {
        // Rust: serde_json::to_string(&ClientRequest::Attach) → "\"Attach\""
        let json = try encode(ClientRequest.attach)
        #expect(json == "\"Attach\"")
    }

    @Test("Detach encodes as Rust unit variant")
    func detachMatchesRust() throws {
        let json = try encode(ClientRequest.detach)
        #expect(json == "\"Detach\"")
    }

    @Test("Resize matches Rust struct variant")
    func resizeMatchesRust() throws {
        // Rust: {"Resize":{"width":120,"height":40}}
        let json = try encode(ClientRequest.resize(width: 120, height: 40))
        let expected = #"{"Resize":{"height":40,"width":120}}"#
        #expect(json == expected)
    }

    @Test("Key with Char matches Rust")
    func keyCharMatchesRust() throws {
        // Rust: {"Key":{"code":{"Char":"a"},"modifiers":0}}
        let event = SerializableKeyEvent(code: .char("a"), modifiers: 0)
        let json = try encode(ClientRequest.key(event))
        let expected = #"{"Key":{"code":{"Char":"a"},"modifiers":0}}"#
        #expect(json == expected)
    }

    @Test("Key with modifier matches Rust bitfield")
    func keyModifierMatchesRust() throws {
        // Rust CONTROL = 0b10 = 2
        let event = SerializableKeyEvent(code: .char("c"), modifiers: KeyModifiers.control)
        let json = try encode(ClientRequest.key(event))
        let obj = try jsonObject(json)
        let key = obj["Key"] as! [String: Any]
        #expect(key["modifiers"] as! Int == 2)
    }

    @Test("Key with combined modifiers matches Rust bitfield")
    func keyCombinedModifiersMatchRust() throws {
        // Rust CONTROL | SHIFT = 0b11 = 3
        let mods = KeyModifiers.control | KeyModifiers.shift
        let event = SerializableKeyEvent(code: .char("a"), modifiers: mods)
        let json = try encode(ClientRequest.key(event))
        let obj = try jsonObject(json)
        let key = obj["Key"] as! [String: Any]
        #expect(key["modifiers"] as! Int == 3)
    }

    @Test("MouseDown matches Rust struct variant")
    func mouseDownMatchesRust() throws {
        // Rust: {"MouseDown":{"x":10,"y":5}}
        let json = try encode(ClientRequest.mouseDown(x: 10, y: 5))
        let expected = #"{"MouseDown":{"x":10,"y":5}}"#
        #expect(json == expected)
    }

    @Test("MouseScroll matches Rust struct variant")
    func mouseScrollMatchesRust() throws {
        // Rust: {"MouseScroll":{"up":true}}
        let json = try encode(ClientRequest.mouseScroll(up: true))
        let expected = #"{"MouseScroll":{"up":true}}"#
        #expect(json == expected)
    }

    @Test("Command matches Rust newtype variant")
    func commandMatchesRust() throws {
        // Rust: {"Command":"list-panes"}
        let json = try encode(ClientRequest.command("list-panes"))
        let expected = #"{"Command":"list-panes"}"#
        #expect(json == expected)
    }

    @Test("Paste matches Rust newtype variant")
    func pasteMatchesRust() throws {
        // Rust: {"Paste":"hello"}
        let json = try encode(ClientRequest.paste("hello"))
        let expected = #"{"Paste":"hello"}"#
        #expect(json == expected)
    }

    @Test("CommandSync matches Rust newtype variant")
    func commandSyncMatchesRust() throws {
        let json = try encode(ClientRequest.commandSync("split-h"))
        let expected = #"{"CommandSync":"split-h"}"#
        #expect(json == expected)
    }

    @Test("FocusWindow matches Rust struct variant with snake_case")
    func focusWindowMatchesRust() throws {
        let id = WindowId(UUID(uuidString: "550e8400-e29b-41d4-a716-446655440000")!)
        let json = try encode(ClientRequest.focusWindow(id: id))
        let expected = #"{"FocusWindow":{"id":"550e8400-e29b-41d4-a716-446655440000"}}"#
        #expect(json == expected)
    }

    @Test("SelectTab matches Rust struct variant with snake_case fields")
    func selectTabMatchesRust() throws {
        let id = WindowId(UUID(uuidString: "550e8400-e29b-41d4-a716-446655440000")!)
        let json = try encode(ClientRequest.selectTab(windowId: id, tabIndex: 2))
        let expected = #"{"SelectTab":{"tab_index":2,"window_id":"550e8400-e29b-41d4-a716-446655440000"}}"#
        #expect(json == expected)
    }

    // MARK: - ServerResponse: Rust → Swift

    /// Verify that Rust-generated JSON is correctly decoded by Swift.

    @Test("Rust Attached unit variant decodes correctly")
    func rustAttachedDecodes() throws {
        // Rust: serde_json::to_string(&ServerResponse::Attached) → "\"Attached\""
        let json = #""Attached""#
        let response = try decode(ServerResponse.self, from: json)
        if case .attached = response {
            // pass
        } else {
            Issue.record("Expected .attached")
        }
    }

    @Test("Rust PaneOutput with byte array decodes correctly")
    func rustPaneOutputDecodes() throws {
        // Rust serializes Vec<u8> as a JSON array of numbers
        let json = #"{"PaneOutput":{"pane_id":"550e8400-e29b-41d4-a716-446655440000","data":[27,91,72,101,108,108,111]}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .paneOutput(let paneId, let data) = response {
            #expect(paneId.raw == UUID(uuidString: "550e8400-e29b-41d4-a716-446655440000")!)
            #expect(data == [27, 91, 72, 101, 108, 108, 111])
        } else {
            Issue.record("Expected .paneOutput")
        }
    }

    @Test("Rust PaneExited decodes correctly")
    func rustPaneExitedDecodes() throws {
        let json = #"{"PaneExited":{"pane_id":"550e8400-e29b-41d4-a716-446655440000"}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .paneExited(let paneId) = response {
            #expect(paneId.raw == UUID(uuidString: "550e8400-e29b-41d4-a716-446655440000")!)
        } else {
            Issue.record("Expected .paneExited")
        }
    }

    @Test("Rust SessionEnded unit variant decodes correctly")
    func rustSessionEndedDecodes() throws {
        // Rust: "\"SessionEnded\""
        let json = #""SessionEnded""#
        let response = try decode(ServerResponse.self, from: json)
        if case .sessionEnded = response {} else {
            Issue.record("Expected .sessionEnded")
        }
    }

    @Test("Rust ClientCountChanged decodes correctly")
    func rustClientCountChangedDecodes() throws {
        // Rust: {"ClientCountChanged":3}
        let json = #"{"ClientCountChanged":3}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .clientCountChanged(let count) = response {
            #expect(count == 3)
        } else {
            Issue.record("Expected .clientCountChanged")
        }
    }

    @Test("Rust Error newtype variant decodes correctly")
    func rustErrorDecodes() throws {
        let json = #"{"Error":"connection refused"}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .error(let msg) = response {
            #expect(msg == "connection refused")
        } else {
            Issue.record("Expected .error")
        }
    }

    @Test("Rust FullScreenDump decodes correctly")
    func rustFullScreenDumpDecodes() throws {
        let json = #"{"FullScreenDump":{"pane_id":"550e8400-e29b-41d4-a716-446655440000","data":[27,91,50,74]}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .fullScreenDump(let paneId, let data) = response {
            #expect(paneId.raw == UUID(uuidString: "550e8400-e29b-41d4-a716-446655440000")!)
            #expect(data == [27, 91, 50, 74])
        } else {
            Issue.record("Expected .fullScreenDump")
        }
    }

    @Test("Rust CommandOutput decodes correctly")
    func rustCommandOutputDecodes() throws {
        let json = #"{"CommandOutput":{"output":"pane-1 split-h\npane-2 split-v","pane_id":null,"window_id":null,"success":true}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .commandOutput(let output, let paneId, let windowId, let success) = response {
            #expect(output == "pane-1 split-h\npane-2 split-v")
            #expect(paneId == nil)
            #expect(windowId == nil)
            #expect(success == true)
        } else {
            Issue.record("Expected .commandOutput")
        }
    }

    @Test("Rust CommandOutput with IDs decodes correctly")
    func rustCommandOutputWithIdsDecodes() throws {
        let json = #"{"CommandOutput":{"output":"ok","pane_id":42,"window_id":7,"success":false}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .commandOutput(let output, let paneId, let windowId, let success) = response {
            #expect(output == "ok")
            #expect(paneId == 42)
            #expect(windowId == 7)
            #expect(success == false)
        } else {
            Issue.record("Expected .commandOutput")
        }
    }

    @Test("Rust PluginSegments decodes correctly")
    func rustPluginSegmentsDecodes() throws {
        let json = #"{"PluginSegments":[[{"text":"cpu: 42%","style":"bold"},{"text":"mem: 67%","style":"dim"}],[{"text":"git: main","style":"green"}]]}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .pluginSegments(let segments) = response {
            #expect(segments.count == 2)
            #expect(segments[0][0].text == "cpu: 42%")
            #expect(segments[0][0].style == "bold")
            #expect(segments[0][1].text == "mem: 67%")
            #expect(segments[1][0].text == "git: main")
        } else {
            Issue.record("Expected .pluginSegments")
        }
    }

    // MARK: - Full RenderState from Rust

    @Test("Complex Rust RenderState with splits, folds, and floating windows decodes correctly")
    func complexRustRenderState() throws {
        // This JSON mirrors what the Rust daemon actually sends
        let w1 = "11111111-1111-1111-1111-111111111111"
        let w2 = "22222222-2222-2222-2222-222222222222"
        let t1 = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
        let t2 = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"
        let t3 = "cccccccc-cccc-cccc-cccc-cccccccccccc"
        let fw = "dddddddd-dddd-dddd-dddd-dddddddddddd"

        let json = """
        {"LayoutChanged":{"render_state":{"workspaces":[{
            "name":"dev",
            "cwd":"/home/user/project",
            "layout":{"Split":{"direction":"Horizontal","ratio":0.5,"first":{"Leaf":"\(w1)"},"second":{"Leaf":"\(w2)"}}},
            "groups":[
                {"id":"\(w1)","tabs":[
                    {"id":"\(t1)","kind":"Shell","title":"zsh","exited":false,"foreground_process":"cargo","cwd":"/home/user/project"},
                    {"id":"\(t2)","kind":"Agent","title":"claude","exited":false,"foreground_process":null,"cwd":"/home/user/project"}
                ],"active_tab":1},
                {"id":"\(w2)","tabs":[
                    {"id":"\(t3)","kind":"Nvim","title":"nvim","exited":false,"foreground_process":"nvim","cwd":"/home/user/project"}
                ],"active_tab":0}
            ],
            "active_group":"\(w1)",
            "sync_panes":false,
            "folded_windows":["\(w2)"],
            "zoomed_window":null,
            "floating_windows":[{"id":"\(fw)","x":10,"y":5,"width":60,"height":20}]
        },{
            "name":"ops",
            "cwd":"/var/log",
            "layout":{"Leaf":"\(w1)"},
            "groups":[{"id":"\(w1)","tabs":[{"id":"\(t1)","kind":"Shell","title":"logs","exited":false,"foreground_process":"tail","cwd":"/var/log"}],"active_tab":0}],
            "active_group":"\(w1)",
            "sync_panes":true,
            "folded_windows":[],
            "zoomed_window":"\(w1)",
            "floating_windows":[]
        }],"active_workspace":0}}}
        """

        let response = try decode(ServerResponse.self, from: json)
        guard case .layoutChanged(let state) = response else {
            Issue.record("Expected .layoutChanged")
            return
        }

        // Two workspaces
        #expect(state.workspaces.count == 2)
        #expect(state.activeWorkspace == 0)

        // First workspace: dev
        let dev = state.workspaces[0]
        #expect(dev.name == "dev")
        #expect(dev.cwd == "/home/user/project")
        #expect(dev.syncPanes == false)
        #expect(dev.zoomedWindow == nil)

        // Layout is a horizontal split
        if case .split(let dir, let ratio, _, _) = dev.layout {
            #expect(dir == .horizontal)
            #expect(ratio == 0.5)
        } else {
            Issue.record("Expected .split layout")
        }

        // Two windows
        #expect(dev.groups.count == 2)
        #expect(dev.groups[0].tabs.count == 2)
        #expect(dev.groups[0].activeTab == 1)
        #expect(dev.groups[0].tabs[0].kind == .shell)
        #expect(dev.groups[0].tabs[0].foregroundProcess == "cargo")
        #expect(dev.groups[0].tabs[1].kind == .agent)
        #expect(dev.groups[0].tabs[1].title == "claude")
        #expect(dev.groups[1].tabs[0].kind == .nvim)

        // Folded windows
        #expect(dev.foldedWindows.count == 1)
        #expect(dev.foldedWindows.contains(WindowId(UUID(uuidString: w2)!)))

        // Floating windows
        #expect(dev.floatingWindows.count == 1)
        #expect(dev.floatingWindows[0].x == 10)
        #expect(dev.floatingWindows[0].width == 60)

        // Second workspace: ops
        let ops = state.workspaces[1]
        #expect(ops.name == "ops")
        #expect(ops.syncPanes == true)
        #expect(ops.zoomedWindow == WindowId(UUID(uuidString: w1)!))
        #expect(ops.groups[0].tabs[0].foregroundProcess == "tail")
    }

    // MARK: - Protocol fields: cols/rows/name/is_home

    @Test("TabSnapshot cols and rows decode from Rust JSON")
    func tabSnapshotColsRowsDecode() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "workspaces": [{
                "name": "test",
                "cwd": "/tmp",
                "layout": {"Leaf": "\(wid)"},
                "groups": [{
                    "id": "\(wid)",
                    "tabs": [{
                        "id": "\(tid)",
                        "kind": "Shell",
                        "title": "sh",
                        "exited": false,
                        "foreground_process": null,
                        "cwd": "/tmp",
                        "cols": 120,
                        "rows": 40
                    }],
                    "active_tab": 0
                }],
                "active_group": "\(wid)",
                "sync_panes": false,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": []
            }],
            "active_workspace": 0
        }
        """
        let state = try decode(RenderState.self, from: json)
        let tab = state.workspaces[0].groups[0].tabs[0]
        #expect(tab.cols == 120)
        #expect(tab.rows == 40)
    }

    @Test("TabSnapshot cols and rows default to 80/24 when absent")
    func tabSnapshotColsRowsDefaults() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "workspaces": [{
                "name": "test",
                "cwd": "/tmp",
                "layout": {"Leaf": "\(wid)"},
                "groups": [{
                    "id": "\(wid)",
                    "tabs": [{
                        "id": "\(tid)",
                        "kind": "Shell",
                        "title": "sh",
                        "exited": false,
                        "foreground_process": null,
                        "cwd": "/tmp"
                    }],
                    "active_tab": 0
                }],
                "active_group": "\(wid)",
                "sync_panes": false,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": []
            }],
            "active_workspace": 0
        }
        """
        let state = try decode(RenderState.self, from: json)
        let tab = state.workspaces[0].groups[0].tabs[0]
        #expect(tab.cols == 80)
        #expect(tab.rows == 24)
    }

    @Test("WindowSnapshot name decodes from Rust JSON")
    func windowSnapshotNameDecodes() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "workspaces": [{
                "name": "test",
                "cwd": "/tmp",
                "layout": {"Leaf": "\(wid)"},
                "groups": [{
                    "id": "\(wid)",
                    "tabs": [{
                        "id": "\(tid)",
                        "kind": "Shell",
                        "title": "sh",
                        "exited": false,
                        "foreground_process": null,
                        "cwd": "/tmp"
                    }],
                    "active_tab": 0,
                    "name": "editor"
                }],
                "active_group": "\(wid)",
                "sync_panes": false,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": []
            }],
            "active_workspace": 0
        }
        """
        let state = try decode(RenderState.self, from: json)
        #expect(state.workspaces[0].groups[0].name == "editor")
    }

    @Test("WindowSnapshot name defaults to nil when absent")
    func windowSnapshotNameDefaultsToNil() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "workspaces": [{
                "name": "test",
                "cwd": "/tmp",
                "layout": {"Leaf": "\(wid)"},
                "groups": [{
                    "id": "\(wid)",
                    "tabs": [{
                        "id": "\(tid)",
                        "kind": "Shell",
                        "title": "sh",
                        "exited": false,
                        "foreground_process": null,
                        "cwd": "/tmp"
                    }],
                    "active_tab": 0
                }],
                "active_group": "\(wid)",
                "sync_panes": false,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": []
            }],
            "active_workspace": 0
        }
        """
        let state = try decode(RenderState.self, from: json)
        #expect(state.workspaces[0].groups[0].name == nil)
    }

    @Test("WorkspaceSnapshot is_home decodes from Rust JSON")
    func workspaceSnapshotIsHomeDecodes() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "workspaces": [{
                "name": "home",
                "cwd": "/tmp",
                "layout": {"Leaf": "\(wid)"},
                "groups": [{
                    "id": "\(wid)",
                    "tabs": [{"id": "\(tid)", "kind": "Shell", "title": "sh", "exited": false, "foreground_process": null, "cwd": "/tmp"}],
                    "active_tab": 0
                }],
                "active_group": "\(wid)",
                "sync_panes": false,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": [],
                "is_home": true
            }],
            "active_workspace": 0
        }
        """
        let state = try decode(RenderState.self, from: json)
        #expect(state.workspaces[0].isHome == true)
    }

    @Test("WorkspaceSnapshot is_home defaults to false when absent")
    func workspaceSnapshotIsHomeDefaultsFalse() throws {
        let wid = UUID().uuidString.lowercased()
        let tid = UUID().uuidString.lowercased()
        let json = """
        {
            "workspaces": [{
                "name": "ws",
                "cwd": "/tmp",
                "layout": {"Leaf": "\(wid)"},
                "groups": [{
                    "id": "\(wid)",
                    "tabs": [{"id": "\(tid)", "kind": "Shell", "title": "sh", "exited": false, "foreground_process": null, "cwd": "/tmp"}],
                    "active_tab": 0
                }],
                "active_group": "\(wid)",
                "sync_panes": false,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": []
            }],
            "active_workspace": 0
        }
        """
        let state = try decode(RenderState.self, from: json)
        #expect(state.workspaces[0].isHome == false)
    }

    // MARK: - SerializableKeyCode: all F-keys

    @Test("F-keys F1 through F12 match Rust format")
    func fKeysMatchRust() throws {
        for n: UInt8 in 1...12 {
            let json = try encode(SerializableKeyCode.f(n))
            #expect(json == "{\"F\":\(n)}")

            let decoded = try decode(SerializableKeyCode.self, from: json)
            if case .f(let decoded_n) = decoded {
                #expect(decoded_n == n)
            } else {
                Issue.record("Expected .f(\(n))")
            }
        }
    }

    // MARK: - StatsUpdate matches Rust field names

    @Test("StatsUpdate with Rust snake_case fields decodes correctly")
    func statsUpdateSnakeCaseDecodes() throws {
        // Rust: {"StatsUpdate":{"cpu_percent":...}}
        let json = #"{"StatsUpdate":{"cpu_percent":99.9,"memory_percent":0.1,"load_avg_1":0.01,"disk_usage_percent":100.0}}"#
        let response = try decode(ServerResponse.self, from: json)
        if case .statsUpdate(let stats) = response {
            #expect(abs(stats.cpuPercent - 99.9) < 0.1)
            #expect(abs(stats.memoryPercent - 0.1) < 0.1)
            #expect(abs(stats.loadAvg1 - 0.01) < 0.001)
            #expect(abs(stats.diskUsagePercent - 100.0) < 0.1)
        } else {
            Issue.record("Expected .statsUpdate")
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

private func jsonObject(_ json: String) throws -> [String: Any] {
    try JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
}
