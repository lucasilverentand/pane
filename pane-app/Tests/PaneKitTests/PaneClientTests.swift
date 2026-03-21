import Foundation
import Testing
@testable import PaneKit

/// Integration tests for PaneClient using a mock server.
///
/// These test the full client lifecycle: connect → receive loop → state updates → disconnect.
/// Each test spins up a MockPaneServer, connects a real PaneClient to it, and verifies
/// that the client's observable state is updated correctly.
@Suite("PaneClient integration")
struct PaneClientTests {

    // MARK: - Connection lifecycle

    @Test("Connect sets state to connected after Attached response")
    @MainActor
    func connectSetsStateToConnected() async throws {
        let server = try MockPaneServer()
        let client = PaneClient()

        // Accept + respond on background thread
        let serverTask = Task.detached {
            try server.acceptClient(timeout: 5)
            let request = try server.receive()
            guard case .attach = request else {
                Issue.record("Expected .attach, got \(request)")
                return
            }
            try server.send(.attached)
        }

        try await client.connect(path: server.socketPath)

        // Wait for Attached response to be processed
        try await serverTask.value
        try await Task.sleep(for: .milliseconds(100))

        if case .connected = client.connectionState {
            // pass
        } else {
            Issue.record("Expected .connected, got \(client.connectionState)")
        }

        client.disconnect()
    }

    @Test("Disconnect resets client state")
    @MainActor
    func disconnectResetsState() async throws {
        let server = try MockPaneServer()
        let client = PaneClient()

        let serverTask = Task.detached {
            try server.acceptClient(timeout: 5)
            _ = try server.receive()
            try server.send(.attached)
        }

        try await client.connect(path: server.socketPath)
        try await serverTask.value
        try await Task.sleep(for: .milliseconds(100))

        client.disconnect()

        if case .disconnected = client.connectionState {} else {
            Issue.record("Expected .disconnected")
        }
        #expect(client.renderState == nil)
        #expect(client.systemStats == nil)
    }

    @Test("Connection error sets error state")
    @MainActor
    func connectionErrorSetsErrorState() async {
        let client = PaneClient()
        let bogusPath = NSTemporaryDirectory() + "pane-nonexistent-\(UUID().uuidString).sock"

        do {
            try await client.connect(path: bogusPath)
            Issue.record("Expected connection to throw")
        } catch {
            if case .error = client.connectionState {} else {
                Issue.record("Expected .error state")
            }
        }
    }

    // MARK: - LayoutChanged updates renderState

    @Test("LayoutChanged updates renderState")
    @MainActor
    func layoutChangedUpdatesRenderState() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        let windowId = WindowId(UUID())
        let tabId = TabId(UUID())
        let renderState = makeRenderState(
            workspaceName: "test-ws",
            windowId: windowId,
            tabId: tabId,
            tabTitle: "zsh"
        )

        try server.send(.layoutChanged(renderState: renderState))
        try await Task.sleep(for: .milliseconds(100))

        guard let state = client.renderState else {
            Issue.record("Expected renderState to be set")
            return
        }
        #expect(state.workspaces.count == 1)
        #expect(state.workspaces[0].name == "test-ws")
        #expect(state.workspaces[0].groups[0].tabs[0].title == "zsh")
        #expect(state.workspaces[0].groups[0].tabs[0].id == tabId)
    }

    // MARK: - StatsUpdate

    @Test("StatsUpdate updates systemStats")
    @MainActor
    func statsUpdateUpdatesSystemStats() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try server.send(.statsUpdate(SerializableSystemStats(
            cpuPercent: 42.5, memoryPercent: 67.8,
            loadAvg1: 1.23, diskUsagePercent: 55.0
        )))
        try await Task.sleep(for: .milliseconds(100))

        guard let stats = client.systemStats else {
            Issue.record("Expected systemStats to be set")
            return
        }
        #expect(abs(stats.cpuPercent - 42.5) < 0.01)
        #expect(abs(stats.memoryPercent - 67.8) < 0.01)
    }

    // MARK: - PaneOutput callback

    @Test("PaneOutput invokes onPaneOutput callback")
    @MainActor
    func paneOutputCallback() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        let tabId = TabId(UUID())
        let expectedData: [UInt8] = [27, 91, 72, 101, 108, 108, 111]
        var receivedPaneId: TabId?
        var receivedData: [UInt8]?

        client.onPaneOutput = { paneId, data in
            receivedPaneId = paneId
            receivedData = data
        }

        try server.send(.paneOutput(paneId: tabId, data: expectedData))
        try await Task.sleep(for: .milliseconds(100))

        #expect(receivedPaneId == tabId)
        #expect(receivedData == expectedData)
    }

    @Test("FullScreenDump invokes onPaneOutput callback")
    @MainActor
    func fullScreenDumpCallback() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        let tabId = TabId(UUID())
        let dumpData: [UInt8] = [0x1b, 0x5b, 0x32, 0x4a]
        var receivedData: [UInt8]?

        client.onPaneOutput = { _, data in
            receivedData = data
        }

        try server.send(.fullScreenDump(paneId: tabId, data: dumpData))
        try await Task.sleep(for: .milliseconds(100))

        #expect(receivedData == dumpData)
    }

    // MARK: - Session events

    @Test("SessionEnded invokes onSessionEvent")
    @MainActor
    func sessionEndedEvent() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        var receivedEvent: PaneClient.SessionEvent?
        client.onSessionEvent = { event in
            receivedEvent = event
        }

        try server.send(.sessionEnded)
        try await Task.sleep(for: .milliseconds(100))

        if case .sessionEnded = receivedEvent {} else {
            Issue.record("Expected .sessionEnded, got \(String(describing: receivedEvent))")
        }
    }

    // MARK: - ClientCountChanged

    @Test("ClientCountChanged updates clientCount")
    @MainActor
    func clientCountChanged() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try server.send(.clientCountChanged(3))
        try await Task.sleep(for: .milliseconds(100))

        #expect(client.clientCount == 3)
    }

    // MARK: - PluginSegments

    @Test("PluginSegments updates pluginSegments")
    @MainActor
    func pluginSegmentsUpdated() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        let segments: [[PluginSegment]] = [
            [PluginSegment(text: "cpu: 42%", style: "bold"), PluginSegment(text: "mem: 67%", style: "dim")],
            [PluginSegment(text: "git: main", style: "green")],
        ]
        try server.send(.pluginSegments(segments))
        try await Task.sleep(for: .milliseconds(100))

        #expect(client.pluginSegments.count == 2)
        #expect(client.pluginSegments[0].count == 2)
        #expect(client.pluginSegments[0][0].text == "cpu: 42%")
        #expect(client.pluginSegments[1][0].text == "git: main")
    }

    // MARK: - Error response

    @Test("Error response sets error state")
    @MainActor
    func errorResponseSetsErrorState() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try server.send(.error("something failed"))
        try await Task.sleep(for: .milliseconds(100))

        if case .error(let msg) = client.connectionState {
            #expect(msg == "something failed")
        } else {
            Issue.record("Expected .error state")
        }
    }

    // MARK: - Sending convenience methods

    @Test("Client sends resize request correctly")
    @MainActor
    func clientSendsResize() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try await client.resize(width: 200, height: 50)
        let received = try server.receive()

        if case .resize(let w, let h) = received {
            #expect(w == 200)
            #expect(h == 50)
        } else {
            Issue.record("Expected .resize, got \(received)")
        }
    }

    @Test("Client sends key event correctly")
    @MainActor
    func clientSendsKey() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try await client.sendKey(code: .char("q"), modifiers: KeyModifiers.control)
        let received = try server.receive()

        if case .key(let event) = received {
            if case .char(let c) = event.code {
                #expect(c == "q")
            } else {
                Issue.record("Expected .char")
            }
            #expect(event.modifiers == KeyModifiers.control)
        } else {
            Issue.record("Expected .key, got \(received)")
        }
    }

    @Test("Client sends paste correctly")
    @MainActor
    func clientSendsPaste() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try await client.paste("hello\nworld")
        let received = try server.receive()

        if case .paste(let text) = received {
            #expect(text == "hello\nworld")
        } else {
            Issue.record("Expected .paste")
        }
    }

    @Test("Client sends focusWindow correctly")
    @MainActor
    func clientSendsFocusWindow() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        let id = WindowId(UUID())
        try await client.focusWindow(id: id)
        let received = try server.receive()

        if case .focusWindow(let receivedId) = received {
            #expect(receivedId == id)
        } else {
            Issue.record("Expected .focusWindow")
        }
    }

    @Test("Client sends selectTab correctly")
    @MainActor
    func clientSendsSelectTab() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        let id = WindowId(UUID())
        try await client.selectTab(windowId: id, tabIndex: 2)
        let received = try server.receive()

        if case .selectTab(let receivedId, let idx) = received {
            #expect(receivedId == id)
            #expect(idx == 2)
        } else {
            Issue.record("Expected .selectTab")
        }
    }

    @Test("Client sends command correctly")
    @MainActor
    func clientSendsCommand() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try await client.sendCommand("split-h")
        let received = try server.receive()

        if case .command(let cmd) = received {
            #expect(cmd == "split-h")
        } else {
            Issue.record("Expected .command")
        }
    }

    @Test("setActiveWorkspace sends select-workspace command")
    @MainActor
    func setActiveWorkspaceSendsCommand() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        try await client.setActiveWorkspace(2)
        let received = try server.receive()

        if case .command(let cmd) = received {
            // Workspace 2 (0-indexed) → select-workspace -t 3 (1-indexed)
            #expect(cmd == "select-workspace -t 3")
        } else {
            Issue.record("Expected .command, got \(received)")
        }
    }

    @Test("Send fails when not connected")
    @MainActor
    func sendFailsWhenNotConnected() async {
        let client = PaneClient()
        do {
            try await client.send(.attach)
            Issue.record("Expected ClientError.notConnected")
        } catch is ClientError {
            // expected
        } catch {
            Issue.record("Unexpected error: \(error)")
        }
    }

    // MARK: - Server disconnect handling

    @Test("Client transitions to disconnected on server close")
    @MainActor
    func clientHandlesServerClose() async throws {
        let (server, client) = try await connectClientToServer()

        server.disconnectClient()
        try await Task.sleep(for: .milliseconds(200))

        if case .disconnected = client.connectionState {} else {
            Issue.record("Expected .disconnected, got \(client.connectionState)")
        }
    }

    // MARK: - Multiple rapid state updates

    @Test("Multiple rapid LayoutChanged updates apply correctly")
    @MainActor
    func rapidLayoutUpdates() async throws {
        let (server, client) = try await connectClientToServer()
        defer { client.disconnect() }

        // Send 10 layout updates in quick succession
        for i in 0..<10 {
            let state = makeRenderState(
                workspaceName: "ws-\(i)",
                windowId: WindowId(UUID()),
                tabId: TabId(UUID()),
                tabTitle: "tab-\(i)"
            )
            try server.send(.layoutChanged(renderState: state))
        }

        // Wait for all to be processed
        try await Task.sleep(for: .milliseconds(300))

        // Last update wins
        guard let state = client.renderState else {
            Issue.record("Expected renderState")
            return
        }
        #expect(state.workspaces[0].name == "ws-9")
        #expect(state.workspaces[0].groups[0].tabs[0].title == "tab-9")
    }

    // MARK: - Helpers

    /// Set up a connected client/server pair, past the Attach/Attached handshake.
    @MainActor
    private func connectClientToServer() async throws -> (MockPaneServer, PaneClient) {
        let server = try MockPaneServer()
        let client = PaneClient()

        let serverTask = Task.detached {
            try server.acceptClient(timeout: 5)
            _ = try server.receive() // consume Attach
            try server.send(.attached)
        }

        try await client.connect(path: server.socketPath)
        try await serverTask.value
        try await Task.sleep(for: .milliseconds(100))

        return (server, client)
    }

    private func makeRenderState(
        workspaceName: String,
        windowId: WindowId,
        tabId: TabId,
        tabTitle: String
    ) -> RenderState {
        let json = """
        {
            "workspaces": [{
                "name": "\(workspaceName)",
                "cwd": "/tmp",
                "layout": {"Leaf": "\(windowId.raw.uuidString.lowercased())"},
                "groups": [{
                    "id": "\(windowId.raw.uuidString.lowercased())",
                    "tabs": [{
                        "id": "\(tabId.raw.uuidString.lowercased())",
                        "kind": "Shell",
                        "title": "\(tabTitle)",
                        "exited": false,
                        "foreground_process": null,
                        "cwd": "/tmp"
                    }],
                    "active_tab": 0
                }],
                "active_group": "\(windowId.raw.uuidString.lowercased())",
                "sync_panes": false,
                "folded_windows": [],
                "zoomed_window": null,
                "floating_windows": []
            }],
            "active_workspace": 0
        }
        """
        return try! JSONDecoder().decode(RenderState.self, from: Data(json.utf8))
    }

}
