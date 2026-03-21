#if canImport(AppKit)
import Foundation

/// Bidirectional bridge between the Pane daemon and a ghostty surface.
///
/// Creates a Unix domain socket listener. The ghostty surface runs `nc -U <path>`
/// as its command, establishing a relay:
/// - Output: daemon bytes → socket → nc stdout → ghostty PTY → rendered
/// - Input:  user keystrokes → ghostty PTY → nc stdin → socket → sent to daemon
final class GhosttyBridge: @unchecked Sendable {
    let socketPath: String
    private let id = UUID()
    private var serverFd: Int32 = -1
    private var clientFd: Int32 = -1
    private var readSource: DispatchSourceRead?

    /// Called when the user types — bytes arrive from the ghostty surface
    /// through the relay. These should be forwarded to the Pane daemon.
    var onInput: (([UInt8]) -> Void)?

    init() {
        let tmpDir = NSTemporaryDirectory()
        socketPath = "\(tmpDir)pane-bridge-\(id.uuidString).sock"
    }

    deinit {
        stop()
    }

    /// Start listening for the relay connection.
    func start() throws {
        // Create the Unix domain socket
        serverFd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard serverFd >= 0 else {
            throw BridgeError.socketCreationFailed
        }

        // Remove any stale socket file
        unlink(socketPath)

        // Bind
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        socketPath.withCString { ptr in
            withUnsafeMutablePointer(to: &addr.sun_path) { pathPtr in
                let pathBuf = UnsafeMutableRawPointer(pathPtr)
                    .assumingMemoryBound(to: CChar.self)
                _ = strcpy(pathBuf, ptr)
            }
        }

        let bindResult = withUnsafePointer(to: &addr) { addrPtr in
            addrPtr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                bind(serverFd, sockaddrPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard bindResult == 0 else {
            close(serverFd)
            serverFd = -1
            throw BridgeError.bindFailed(errno)
        }

        // Listen
        guard listen(serverFd, 1) == 0 else {
            close(serverFd)
            serverFd = -1
            throw BridgeError.listenFailed(errno)
        }

        // Accept connection in background
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            self?.acceptConnection()
        }
    }

    /// Write daemon output bytes to the bridge (goes to ghostty for rendering).
    func write(bytes: [UInt8]) {
        guard clientFd >= 0 else { return }
        bytes.withUnsafeBufferPointer { buf in
            guard let base = buf.baseAddress else { return }
            var remaining = buf.count
            var offset = 0
            while remaining > 0 {
                let written = Darwin.write(clientFd, base + offset, remaining)
                if written <= 0 { break }
                offset += written
                remaining -= written
            }
        }
    }

    /// Stop the bridge and clean up.
    func stop() {
        readSource?.cancel()
        readSource = nil
        if clientFd >= 0 { close(clientFd); clientFd = -1 }
        if serverFd >= 0 { close(serverFd); serverFd = -1 }
        unlink(socketPath)
    }

    // MARK: - Private

    private func acceptConnection() {
        var clientAddr = sockaddr_un()
        var clientLen = socklen_t(MemoryLayout<sockaddr_un>.size)
        let fd = withUnsafeMutablePointer(to: &clientAddr) { addrPtr in
            addrPtr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                accept(serverFd, sockaddrPtr, &clientLen)
            }
        }
        guard fd >= 0 else { return }
        clientFd = fd

        // Set non-blocking for reads
        let flags = fcntl(fd, F_GETFL)
        _ = fcntl(fd, F_SETFL, flags | O_NONBLOCK)

        // Set up dispatch source to read user input from the relay
        let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: .global(qos: .userInitiated))
        source.setEventHandler { [weak self] in
            guard let self, self.clientFd >= 0 else { return }
            var buffer = [UInt8](repeating: 0, count: 65536)
            let n = read(self.clientFd, &buffer, buffer.count)
            if n > 0 {
                let data = Array(buffer.prefix(n))
                self.onInput?(data)
            }
        }
        source.setCancelHandler { [weak self] in
            if let fd = self?.clientFd, fd >= 0 {
                close(fd)
                self?.clientFd = -1
            }
        }
        source.resume()
        readSource = source
    }

    enum BridgeError: Error {
        case socketCreationFailed
        case bindFailed(Int32)
        case listenFailed(Int32)
    }
}
#endif
