import Foundation

/// Low-level Unix socket connection to the Pane daemon.
///
/// Uses POSIX sockets (`socket`, `connect`) with `DispatchIO` for async read/write.
/// Implements the 4-byte BE length-prefixed JSON framing protocol.
public final class PaneConnection: Sendable {
    private let fd: Int32
    private let readQueue = DispatchQueue(label: "pane.connection.read")
    private let writeQueue = DispatchQueue(label: "pane.connection.write")

    /// The default socket path for the current user.
    public static var defaultSocketPath: String {
        let uid = getuid()
        return "/tmp/pane-\(uid)/pane.sock"
    }

    private init(fd: Int32) {
        self.fd = fd
    }

    deinit {
        Darwin.close(fd)
    }

    // MARK: - Connect

    /// Connect to the Pane daemon at the given socket path.
    public static func connect(path: String? = nil) throws -> PaneConnection {
        let socketPath = path ?? defaultSocketPath

        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else {
            throw ConnectionError.socketCreationFailed(errno: errno)
        }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)

        let pathBytes = socketPath.utf8CString
        guard pathBytes.count <= MemoryLayout.size(ofValue: addr.sun_path) else {
            Darwin.close(fd)
            throw ConnectionError.pathTooLong
        }

        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                for (i, byte) in pathBytes.enumerated() {
                    dest[i] = byte
                }
            }
        }

        let addrLen = socklen_t(MemoryLayout<sockaddr_un>.size)
        let result = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                Darwin.connect(fd, sockPtr, addrLen)
            }
        }

        guard result == 0 else {
            let err = errno
            Darwin.close(fd)
            throw ConnectionError.connectFailed(errno: err)
        }

        return PaneConnection(fd: fd)
    }

    // MARK: - Send

    /// Send a framed message to the daemon.
    public func send<T: Encodable & Sendable>(_ message: T) async throws {
        let frameData = try Framing.encode(message)
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            writeQueue.async { [fd] in
                let result = frameData.withUnsafeBytes { ptr in
                    Darwin.send(fd, ptr.baseAddress!, ptr.count, 0)
                }
                if result < 0 {
                    continuation.resume(throwing: ConnectionError.writeFailed(errno: errno))
                } else {
                    continuation.resume()
                }
            }
        }
    }

    /// Send raw bytes (already framed) to the daemon.
    public func sendRaw(_ data: Data) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            writeQueue.async { [fd] in
                let result = data.withUnsafeBytes { ptr in
                    Darwin.send(fd, ptr.baseAddress!, ptr.count, 0)
                }
                if result < 0 {
                    continuation.resume(throwing: ConnectionError.writeFailed(errno: errno))
                } else {
                    continuation.resume()
                }
            }
        }
    }

    // MARK: - Receive

    /// Read the next framed message from the daemon.
    /// Returns `nil` on clean disconnect (EOF).
    public func receive<T: Decodable>(_ type: T.Type) async throws -> T? {
        // Read 4-byte length prefix
        guard let lengthData = try await readExact(count: 4) else {
            return nil // EOF
        }
        let length = try Framing.decodeLength(lengthData)

        // Read payload
        guard let payloadData = try await readExact(count: Int(length)) else {
            throw FramingError.connectionClosed
        }

        return try Framing.decodePayload(payloadData)
    }

    /// Read exactly `count` bytes from the socket.
    /// Returns `nil` on clean EOF before any bytes are read.
    private func readExact(count: Int) async throws -> Data? {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Data?, any Error>) in
            readQueue.async { [fd] in
                var buffer = Data(count: count)
                var totalRead = 0

                while totalRead < count {
                    let result = buffer.withUnsafeMutableBytes { ptr in
                        Darwin.recv(fd, ptr.baseAddress! + totalRead, count - totalRead, 0)
                    }
                    if result > 0 {
                        totalRead += result
                    } else if result == 0 {
                        // EOF
                        if totalRead == 0 {
                            continuation.resume(returning: nil)
                        } else {
                            continuation.resume(throwing: FramingError.connectionClosed)
                        }
                        return
                    } else {
                        continuation.resume(throwing: ConnectionError.readFailed(errno: errno))
                        return
                    }
                }

                continuation.resume(returning: buffer)
            }
        }
    }

    // MARK: - Disconnect

    /// Shut down the connection.
    public func disconnect() {
        shutdown(fd, SHUT_RDWR)
    }
}

// MARK: - ConnectionError

public enum ConnectionError: Error, Sendable, CustomStringConvertible {
    case socketCreationFailed(errno: Int32)
    case pathTooLong
    case connectFailed(errno: Int32)
    case writeFailed(errno: Int32)
    case readFailed(errno: Int32)

    public var description: String {
        switch self {
        case .socketCreationFailed(let err):
            "Failed to create socket: \(String(cString: strerror(err)))"
        case .pathTooLong:
            "Socket path exceeds maximum length"
        case .connectFailed(let err):
            "Failed to connect: \(String(cString: strerror(err)))"
        case .writeFailed(let err):
            "Write failed: \(String(cString: strerror(err)))"
        case .readFailed(let err):
            "Read failed: \(String(cString: strerror(err)))"
        }
    }
}
