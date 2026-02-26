import Foundation

/// Length-prefixed framing protocol matching the Rust daemon.
///
/// Each frame is: 4-byte big-endian length prefix + JSON payload.
/// Max frame size: 16 MiB.
public enum Framing: Sendable {
    /// Maximum frame size: 16 MiB.
    public static let maxFrameSize: UInt32 = 16 * 1024 * 1024

    /// Encode a Codable value into a length-prefixed frame.
    public static func encode<T: Encodable & Sendable>(_ value: T) throws -> Data {
        let json = try JSONEncoder().encode(value)
        let length = UInt32(json.count)
        guard length <= maxFrameSize else {
            throw FramingError.frameTooLarge(Int(length))
        }
        var data = Data(capacity: 4 + json.count)
        // 4-byte big-endian length prefix
        var be = length.bigEndian
        data.append(Data(bytes: &be, count: 4))
        data.append(json)
        return data
    }

    /// Decode a length prefix from 4 bytes of data (big-endian).
    public static func decodeLength(_ bytes: Data) throws -> UInt32 {
        guard bytes.count >= 4 else {
            throw FramingError.incompleteLengthPrefix
        }
        let length = bytes.withUnsafeBytes { ptr in
            ptr.load(as: UInt32.self).bigEndian
        }
        guard length <= maxFrameSize else {
            throw FramingError.frameTooLarge(Int(length))
        }
        return length
    }

    /// Decode a JSON payload into a Codable value.
    public static func decodePayload<T: Decodable>(_ data: Data) throws -> T {
        try JSONDecoder().decode(T.self, from: data)
    }
}

// MARK: - FramingError

public enum FramingError: Error, Sendable {
    case frameTooLarge(Int)
    case incompleteLengthPrefix
    case connectionClosed
}
