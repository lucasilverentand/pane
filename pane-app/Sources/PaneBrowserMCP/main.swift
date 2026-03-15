import Foundation

/// Stdio-to-Unix-socket bridge for the Pane browser MCP server.
/// Reads JSON-RPC from stdin, forwards to the MCP socket, and writes responses to stdout.

let uid = getuid()
let socketPath = "/tmp/pane-\(uid)/browser-mcp.sock"

// Connect to the MCP socket
let fd = socket(AF_UNIX, SOCK_STREAM, 0)
guard fd >= 0 else {
    FileHandle.standardError.write("Failed to create socket\n".data(using: .utf8)!)
    exit(1)
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
let connectResult = withUnsafePointer(to: &addr) { ptr in
    ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
        Darwin.connect(fd, sockPtr, addrLen)
    }
}
guard connectResult == 0 else {
    FileHandle.standardError.write("Failed to connect to Pane browser MCP at \(socketPath)\n".data(using: .utf8)!)
    exit(1)
}

// Read from socket in background, write to stdout
let readThread = Thread {
    let bufSize = 65536
    let buf = UnsafeMutablePointer<UInt8>.allocate(capacity: bufSize)
    defer { buf.deallocate() }

    while true {
        let bytesRead = recv(fd, buf, bufSize, 0)
        guard bytesRead > 0 else { break }
        let data = Data(bytes: buf, count: bytesRead)
        FileHandle.standardOutput.write(data)
    }
    exit(0)
}
readThread.start()

// Read from stdin, write to socket
while let line = readLine(strippingNewline: false) {
    guard let data = line.data(using: .utf8) else { continue }
    _ = data.withUnsafeBytes { ptr in
        send(fd, ptr.baseAddress!, ptr.count, 0)
    }
}

Darwin.close(fd)
