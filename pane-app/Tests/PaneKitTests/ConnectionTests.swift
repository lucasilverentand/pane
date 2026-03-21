import Foundation
import Testing
@testable import PaneKit

/// Tests for PaneConnection: low-level socket connect, send, receive, and error handling.
@Suite("PaneConnection")
struct ConnectionTests {

    // MARK: - Connect / Disconnect

    @Test("Connects to a running mock server")
    func connectsToMockServer() throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        try server.acceptClient(timeout: 2)
        connection.disconnect()
    }

    @Test("Connection fails when no server is listening")
    func connectionFailsWhenNoServer() {
        let bogusPath = NSTemporaryDirectory() + "pane-nonexistent-\(UUID().uuidString).sock"
        #expect(throws: ConnectionError.self) {
            _ = try PaneConnection.connect(path: bogusPath)
        }
    }

    @Test("Connection fails with excessively long socket path")
    func connectionFailsWithLongPath() {
        let longPath = String(repeating: "a", count: 500) + ".sock"
        #expect(throws: ConnectionError.self) {
            _ = try PaneConnection.connect(path: longPath)
        }
    }

    // MARK: - Send / Receive framed messages

    @Test("Send and receive a framed ClientRequest")
    func sendAndReceiveRequest() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        // Client sends a request
        try await connection.send(ClientRequest.attach)

        // Server receives it
        let received = try server.receive()
        if case .attach = received {
            // pass
        } else {
            Issue.record("Expected .attach, got \(received)")
        }
    }

    @Test("Receive a ServerResponse from mock server")
    func receiveResponse() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        // Server sends a response
        try server.send(.attached)

        // Client receives it
        let response: ServerResponse? = try await connection.receive(ServerResponse.self)
        guard let response else {
            Issue.record("Expected response, got nil")
            return
        }
        if case .attached = response {
            // pass
        } else {
            Issue.record("Expected .attached, got \(response)")
        }
    }

    @Test("Send and receive multiple messages in sequence")
    func multipleMessagesInSequence() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        // Client sends multiple requests
        try await connection.send(ClientRequest.attach)
        try await connection.send(ClientRequest.resize(width: 120, height: 40))
        try await connection.send(ClientRequest.paste("hello"))

        // Server reads them all
        let r1 = try server.receive()
        let r2 = try server.receive()
        let r3 = try server.receive()

        if case .attach = r1 {} else { Issue.record("Expected .attach") }
        if case .resize(let w, let h) = r2 {
            #expect(w == 120)
            #expect(h == 40)
        } else { Issue.record("Expected .resize") }
        if case .paste(let text) = r3 {
            #expect(text == "hello")
        } else { Issue.record("Expected .paste") }
    }

    @Test("Bidirectional communication")
    func bidirectionalCommunication() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        // Client sends attach
        try await connection.send(ClientRequest.attach)
        let _ = try server.receive()

        // Server responds with attached
        try server.send(.attached)
        let resp: ServerResponse? = try await connection.receive(ServerResponse.self)
        if case .attached = resp {
            // pass
        } else {
            Issue.record("Expected .attached")
        }

        // Client sends resize
        try await connection.send(ClientRequest.resize(width: 80, height: 24))
        let r2 = try server.receive()
        if case .resize(let w, let h) = r2 {
            #expect(w == 80)
            #expect(h == 24)
        } else {
            Issue.record("Expected .resize")
        }
    }

    // MARK: - Disconnect handling

    @Test("Receive returns nil on server disconnect")
    func receiveReturnsNilOnDisconnect() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        // Server disconnects
        server.disconnectClient()

        // Client should get nil (EOF)
        let response: ServerResponse? = try await connection.receive(ServerResponse.self)
        #expect(response == nil)
    }

    @Test("Send fails after server disconnects")
    func sendFailsAfterDisconnect() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        try server.acceptClient(timeout: 2)

        // Server disconnects
        server.disconnectClient()

        // Give OS time to propagate the close
        try await Task.sleep(for: .milliseconds(100))

        // With SO_NOSIGPIPE set, send should return an error instead of crashing.
        // It may take a couple of attempts before the OS reports the broken pipe.
        var didFail = false
        for _ in 0..<10 {
            do {
                try await connection.send(ClientRequest.resize(width: 80, height: 24))
                try await Task.sleep(for: .milliseconds(20))
            } catch {
                didFail = true
                break
            }
        }
        #expect(didFail, "Expected send to fail after server disconnect")
        connection.disconnect()
    }

    // MARK: - All ClientRequest variants over the wire

    @Test("All ClientRequest variants survive the wire")
    func allClientRequestVariantsOverWire() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        let windowId = WindowId(UUID())
        let requests: [ClientRequest] = [
            .attach,
            .detach,
            .resize(width: 200, height: 50),
            .key(SerializableKeyEvent(code: .char("q"), modifiers: KeyModifiers.control)),
            .mouseDown(x: 5, y: 10),
            .mouseDrag(x: 15, y: 20),
            .mouseMove(x: 25, y: 30),
            .mouseUp(x: 35, y: 40),
            .mouseScroll(up: true),
            .mouseScroll(up: false),
            .command("split-h"),
            .paste("pasted text\nwith newlines"),
            .commandSync("list-panes"),
            .focusWindow(id: windowId),
            .selectTab(windowId: windowId, tabIndex: 3),
        ]

        for request in requests {
            try await connection.send(request)
        }

        // Verify all arrive and roundtrip correctly
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]

        for original in requests {
            let received = try server.receive()
            let originalJson = String(data: try encoder.encode(original), encoding: .utf8)!
            let receivedJson = String(data: try encoder.encode(received), encoding: .utf8)!
            #expect(originalJson == receivedJson, "Mismatch for request")
        }
    }

    // MARK: - All ServerResponse variants over the wire

    @Test("All ServerResponse variants survive the wire")
    func allServerResponseVariantsOverWire() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        let tabId = TabId(UUID())

        let responses: [ServerResponse] = [
            .attached,
            .paneOutput(paneId: tabId, data: [27, 91, 72]),
            .paneExited(paneId: tabId),
            .statsUpdate(SerializableSystemStats(
                cpuPercent: 50.0, memoryPercent: 75.0,
                loadAvg1: 1.5, diskUsagePercent: 60.0
            )),
            .pluginSegments([[PluginSegment(text: "hello", style: "bold")]]),
            .sessionEnded,
            .fullScreenDump(paneId: tabId, data: [0x1b, 0x5b, 0x32, 0x4a]),
            .clientCountChanged(3),
            .error("test error"),
            .commandOutput(output: "ok", paneId: nil, windowId: nil, success: true),
        ]

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]

        for response in responses {
            try server.send(response)
            guard let received: ServerResponse = try await connection.receive(ServerResponse.self) else {
                Issue.record("Unexpected nil for response")
                continue
            }
            let originalJson = String(data: try encoder.encode(response), encoding: .utf8)!
            let receivedJson = String(data: try encoder.encode(received), encoding: .utf8)!
            #expect(originalJson == receivedJson, "Mismatch for response")
        }
    }

    // MARK: - Large payload

    @Test("Large pane output survives the wire")
    func largePaneOutput() async throws {
        let server = try MockPaneServer()
        let connection = try PaneConnection.connect(path: server.socketPath)
        defer { connection.disconnect() }
        try server.acceptClient(timeout: 2)

        // 1 KB of output data
        let bigData = [UInt8](repeating: 0x41, count: 1_000)
        let tabId = TabId(UUID())

        // Send from background since large frames may block
        let sendTask = Task.detached {
            try server.send(.paneOutput(paneId: tabId, data: bigData))
        }

        guard let received: ServerResponse = try await connection.receive(ServerResponse.self) else {
            Issue.record("Expected response")
            return
        }
        try await sendTask.value

        if case .paneOutput(let id, let data) = received {
            #expect(id == tabId)
            #expect(data.count == 1_000)
            #expect(data.allSatisfy { $0 == 0x41 })
        } else {
            Issue.record("Expected .paneOutput")
        }
    }
}


// Make SerializableSystemStats constructible in tests
extension SerializableSystemStats {
    init(cpuPercent: Float, memoryPercent: Float, loadAvg1: Double, diskUsagePercent: Float) {
        self = try! JSONDecoder().decode(
            SerializableSystemStats.self,
            from: JSONSerialization.data(withJSONObject: [
                "cpu_percent": cpuPercent,
                "memory_percent": memoryPercent,
                "load_avg_1": loadAvg1,
                "disk_usage_percent": diskUsagePercent,
            ])
        )
    }
}
