#if canImport(AppKit)
import Foundation
import PaneKit

/// In-app MCP server that listens on a Unix socket for JSON-RPC requests.
/// Dispatches tool calls to BrowserManager on the main actor.
@MainActor
final class BrowserMCPServer {
    private let browser: BrowserManager
    private var serverFd: Int32 = -1
    private var listenTask: Task<Void, Never>?
    private var clientTasks: [Task<Void, Never>] = []

    static var socketPath: String {
        let uid = getuid()
        return "/tmp/pane-\(uid)/browser-mcp.sock"
    }

    init(browser: BrowserManager) {
        self.browser = browser
    }

    func start() {
        let path = Self.socketPath

        // Ensure directory exists
        let dir = (path as NSString).deletingLastPathComponent
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)

        // Remove stale socket
        unlink(path)

        // Create socket
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = path.utf8CString
        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                for (i, byte) in pathBytes.enumerated() {
                    dest[i] = byte
                }
            }
        }

        let addrLen = socklen_t(MemoryLayout<sockaddr_un>.size)
        let bindResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                Darwin.bind(fd, sockPtr, addrLen)
            }
        }
        guard bindResult == 0 else {
            Darwin.close(fd)
            return
        }

        guard Darwin.listen(fd, 5) == 0 else {
            Darwin.close(fd)
            return
        }

        serverFd = fd

        listenTask = Task.detached { [weak self] in
            while !Task.isCancelled {
                var clientAddr = sockaddr_un()
                var clientAddrLen = socklen_t(MemoryLayout<sockaddr_un>.size)
                let clientFd = withUnsafeMutablePointer(to: &clientAddr) { ptr in
                    ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                        Darwin.accept(fd, sockPtr, &clientAddrLen)
                    }
                }
                guard clientFd >= 0 else { break }

                let task = Task.detached { [weak self] in
                    await self?.handleClient(fd: clientFd)
                    return
                }
                await MainActor.run { [weak self] in
                    self?.clientTasks.append(task)
                }
            }
        }
    }

    func stop() {
        listenTask?.cancel()
        listenTask = nil
        for task in clientTasks {
            task.cancel()
        }
        clientTasks.removeAll()
        if serverFd >= 0 {
            Darwin.close(serverFd)
            serverFd = -1
        }
        unlink(Self.socketPath)
    }

    // MARK: - Client handling

    private func handleClient(fd: Int32) async {
        // Read newline-delimited JSON-RPC messages
        var buffer = Data()
        let bufSize = 65536
        let readBuf = UnsafeMutablePointer<UInt8>.allocate(capacity: bufSize)
        defer {
            readBuf.deallocate()
            Darwin.close(fd)
        }

        while !Task.isCancelled {
            let bytesRead = Darwin.recv(fd, readBuf, bufSize, 0)
            guard bytesRead > 0 else { break }

            buffer.append(readBuf, count: bytesRead)

            // Process complete lines
            while let newlineIndex = buffer.firstIndex(of: UInt8(ascii: "\n")) {
                let lineData = buffer[buffer.startIndex..<newlineIndex]
                buffer = Data(buffer[(newlineIndex + 1)...])

                guard !lineData.isEmpty else { continue }

                let response = await processMessage(lineData)
                if let responseData = response {
                    var toSend = responseData
                    toSend.append(UInt8(ascii: "\n"))
                    _ = toSend.withUnsafeBytes { ptr in
                        Darwin.send(fd, ptr.baseAddress!, ptr.count, 0)
                    }
                }
            }
        }
    }

    private func processMessage(_ data: Data) async -> Data? {
        guard let request = try? JSONDecoder().decode(JSONRPCRequest.self, from: data) else {
            let error = JSONRPCResponse(id: nil, error: .parseError)
            return try? JSONEncoder().encode(error)
        }

        let response: JSONRPCResponse

        switch request.method {
        case "initialize":
            let result = MCPInitializeResult()
            let encoded = try! JSONEncoder().encode(result)
            let jsonValue = try! JSONDecoder().decode(JSONValue.self, from: encoded)
            response = JSONRPCResponse(id: request.id, result: jsonValue)

        case "notifications/initialized":
            return nil // No response for notifications

        case "tools/list":
            let tools = MCPToolDefinitions.all
            let encoded = try! JSONEncoder().encode(["tools": tools])
            let jsonValue = try! JSONDecoder().decode(JSONValue.self, from: encoded)
            response = JSONRPCResponse(id: request.id, result: jsonValue)

        case "tools/call":
            let result = await handleToolCall(request.params)
            let encoded = try! JSONEncoder().encode(result)
            let jsonValue = try! JSONDecoder().decode(JSONValue.self, from: encoded)
            response = JSONRPCResponse(id: request.id, result: jsonValue)

        default:
            response = JSONRPCResponse(id: request.id, error: .methodNotFound)
        }

        return try? JSONEncoder().encode(response)
    }

    @MainActor
    private func handleToolCall(_ params: JSONValue?) async -> MCPToolCallResult {
        guard let params,
              let name = params["name"]?.stringValue
        else {
            return .error("Missing tool name")
        }

        let args = params["arguments"] ?? .object([:])

        return await MCPToolHandler.handle(
            tool: name,
            arguments: args,
            browser: browser
        )
    }
}
#endif
