import Foundation
@testable import PaneKit

/// A lightweight mock Pane daemon for integration tests.
///
/// Listens on a temporary Unix socket, accepts one client connection,
/// and provides helpers to send/receive framed JSON messages.
/// Mirrors the real daemon's 4-byte BE length-prefixed JSON protocol.
final class MockPaneServer: @unchecked Sendable {
    let socketPath: String
    private let serverFd: Int32
    private var clientFd: Int32 = -1
    private let queue = DispatchQueue(label: "mock-pane-server")

    /// Create a mock server bound to a temporary Unix socket.
    init() throws {
        let dir = NSTemporaryDirectory()
        socketPath = "\(dir)pane-test-\(UUID().uuidString).sock"

        serverFd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard serverFd >= 0 else {
            throw MockServerError.socketFailed(errno: errno)
        }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = socketPath.utf8CString
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
                Darwin.bind(serverFd, sockPtr, addrLen)
            }
        }
        guard bindResult == 0 else {
            Darwin.close(serverFd)
            throw MockServerError.bindFailed(errno: errno)
        }

        guard listen(serverFd, 1) == 0 else {
            Darwin.close(serverFd)
            throw MockServerError.listenFailed(errno: errno)
        }
    }

    deinit {
        if clientFd >= 0 { Darwin.close(clientFd) }
        Darwin.close(serverFd)
        unlink(socketPath)
    }

    // MARK: - Accept

    /// Accept a single client connection (blocking).
    func acceptClient() throws {
        let fd = accept(serverFd, nil, nil)
        guard fd >= 0 else {
            throw MockServerError.acceptFailed(errno: errno)
        }
        clientFd = fd
    }

    /// Accept a single client connection with a timeout.
    func acceptClient(timeout: TimeInterval) throws {
        var readSet = fd_set()
        __darwin_fd_zero(&readSet)
        __darwin_fd_set(serverFd, &readSet)

        var tv = timeval(tv_sec: Int(timeout), tv_usec: Int32((timeout.truncatingRemainder(dividingBy: 1)) * 1_000_000))
        let result = select(serverFd + 1, &readSet, nil, nil, &tv)
        guard result > 0 else {
            throw MockServerError.acceptTimeout
        }
        try acceptClient()
    }

    // MARK: - Send

    /// Send a ServerResponse to the connected client.
    func send(_ response: ServerResponse) throws {
        guard clientFd >= 0 else { throw MockServerError.noClient }
        let frame = try Framing.encode(response)
        let written = frame.withUnsafeBytes { ptr in
            Darwin.send(clientFd, ptr.baseAddress!, ptr.count, 0)
        }
        guard written == frame.count else {
            throw MockServerError.writeFailed(errno: errno)
        }
    }

    /// Send raw bytes to the connected client (for testing malformed data).
    func sendRaw(_ data: Data) throws {
        guard clientFd >= 0 else { throw MockServerError.noClient }
        let written = data.withUnsafeBytes { ptr in
            Darwin.send(clientFd, ptr.baseAddress!, ptr.count, 0)
        }
        guard written == data.count else {
            throw MockServerError.writeFailed(errno: errno)
        }
    }

    // MARK: - Receive

    /// Receive and decode a ClientRequest from the connected client.
    func receive() throws -> ClientRequest {
        guard clientFd >= 0 else { throw MockServerError.noClient }

        // Read 4-byte length prefix
        let lengthData = try readExact(count: 4)
        let length = try Framing.decodeLength(lengthData)

        // Read payload
        let payload = try readExact(count: Int(length))
        return try Framing.decodePayload(payload)
    }

    /// Receive raw bytes from the connected client.
    func receiveRaw(count: Int) throws -> Data {
        guard clientFd >= 0 else { throw MockServerError.noClient }
        return try readExact(count: count)
    }

    private func readExact(count: Int) throws -> Data {
        var buffer = Data(count: count)
        var totalRead = 0
        while totalRead < count {
            let result = buffer.withUnsafeMutableBytes { ptr in
                Darwin.recv(clientFd, ptr.baseAddress! + totalRead, count - totalRead, 0)
            }
            if result > 0 {
                totalRead += result
            } else if result == 0 {
                throw MockServerError.clientDisconnected
            } else {
                throw MockServerError.readFailed(errno: errno)
            }
        }
        return buffer
    }

    // MARK: - Disconnect

    /// Close the client connection.
    func disconnectClient() {
        if clientFd >= 0 {
            shutdown(clientFd, SHUT_RDWR)
            Darwin.close(clientFd)
            clientFd = -1
        }
    }
}

// MARK: - fd_set helpers

// These are needed because Swift doesn't expose the FD macros directly.
private func __darwin_fd_zero(_ set: inout fd_set) {
    withUnsafeMutablePointer(to: &set) { ptr in
        let raw = UnsafeMutableRawPointer(ptr)
        memset(raw, 0, MemoryLayout<fd_set>.size)
    }
}

private func __darwin_fd_set(_ fd: Int32, _ set: inout fd_set) {
    let intOffset = Int(fd) / (MemoryLayout<Int32>.size * 8)
    let bitOffset = Int(fd) % (MemoryLayout<Int32>.size * 8)
    withUnsafeMutablePointer(to: &set) { ptr in
        let raw = UnsafeMutableRawPointer(ptr).assumingMemoryBound(to: Int32.self)
        raw[intOffset] |= Int32(1 << bitOffset)
    }
}

// MARK: - MockServerError

enum MockServerError: Error, Sendable {
    case socketFailed(errno: Int32)
    case bindFailed(errno: Int32)
    case listenFailed(errno: Int32)
    case acceptFailed(errno: Int32)
    case acceptTimeout
    case noClient
    case writeFailed(errno: Int32)
    case readFailed(errno: Int32)
    case clientDisconnected
}
