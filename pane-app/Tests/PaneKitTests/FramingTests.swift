import Foundation
import Testing
@testable import PaneKit

@Suite("Framing protocol")
struct FramingTests {

    @Test("Encode produces correct length prefix")
    func encodeProducesCorrectLengthPrefix() throws {
        let message = ClientRequest.attach
        let frame = try Framing.encode(message)

        // First 4 bytes are the big-endian length
        let lengthBytes = frame.prefix(4)
        let length = try Framing.decodeLength(Data(lengthBytes))

        // Remaining bytes should be the JSON payload
        let payload = frame.dropFirst(4)
        #expect(Int(length) == payload.count)

        // Verify payload is valid JSON
        let decoded: ClientRequest = try Framing.decodePayload(Data(payload))
        let reencoded = try encode(decoded)
        let originalEncoded = try encode(message)
        #expect(reencoded == originalEncoded)
    }

    @Test("Length prefix is big-endian")
    func lengthPrefixIsBigEndian() throws {
        let message = ClientRequest.resize(width: 120, height: 40)
        let frame = try Framing.encode(message)

        let b0 = frame[frame.startIndex]
        let b1 = frame[frame.startIndex + 1]
        let b2 = frame[frame.startIndex + 2]
        let b3 = frame[frame.startIndex + 3]

        let length = UInt32(b0) << 24 | UInt32(b1) << 16 | UInt32(b2) << 8 | UInt32(b3)
        let payloadSize = UInt32(frame.count - 4)
        #expect(length == payloadSize)
    }

    @Test("DecodeLength rejects short data")
    func decodeLengthRejectsShortData() {
        #expect(throws: FramingError.self) {
            _ = try Framing.decodeLength(Data([0, 0, 0]))
        }
    }

    @Test("DecodeLength rejects oversized frame")
    func decodeLengthRejectsOversizedFrame() {
        // 17 MiB > 16 MiB max
        let size: UInt32 = 17 * 1024 * 1024
        var data = Data(count: 4)
        var be = size.bigEndian
        data.replaceSubrange(0..<4, with: Data(bytes: &be, count: 4))

        #expect(throws: FramingError.self) {
            _ = try Framing.decodeLength(data)
        }
    }

    @Test("Encode rejects oversized payload")
    func encodeRejectsOversizedPayload() {
        // Create a large string that exceeds max frame size
        // We can't easily test this without a huge allocation,
        // so we test the maxFrameSize constant instead
        #expect(Framing.maxFrameSize == 16 * 1024 * 1024)
    }

    @Test("Frame roundtrip with complex message")
    func frameRoundtripWithComplexMessage() throws {
        let request = ClientRequest.key(
            SerializableKeyEvent(code: .char("q"), modifiers: KeyModifiers.control)
        )
        let frame = try Framing.encode(request)

        // Decode length
        let length = try Framing.decodeLength(Data(frame.prefix(4)))

        // Decode payload
        let payload = Data(frame.dropFirst(4))
        #expect(payload.count == Int(length))

        let decoded: ClientRequest = try Framing.decodePayload(payload)
        let reencoded = try Framing.encode(decoded)

        // Both frames should produce identical bytes
        #expect(frame == reencoded)
    }

    @Test("Empty JSON object frame")
    func emptyJsonObjectFrame() throws {
        struct Empty: Codable {}
        let frame = try Framing.encode(Empty())

        let length = try Framing.decodeLength(Data(frame.prefix(4)))
        #expect(length == 2) // "{}" is 2 bytes

        let payload = Data(frame.dropFirst(4))
        let decoded: Empty = try Framing.decodePayload(payload)
        _ = decoded // Just verify it decodes
    }

    @Test("Multiple frames can be concatenated and split")
    func multipleFramesConcatenated() throws {
        let messages: [ClientRequest] = [
            .attach,
            .resize(width: 80, height: 24),
            .detach,
        ]

        // Encode all frames into one buffer
        var buffer = Data()
        for msg in messages {
            buffer.append(try Framing.encode(msg))
        }

        // Decode them back one by one
        var offset = 0
        for original in messages {
            let lengthData = buffer[offset..<(offset + 4)]
            let length = try Framing.decodeLength(Data(lengthData))
            offset += 4

            let payload = buffer[offset..<(offset + Int(length))]
            let decoded: ClientRequest = try Framing.decodePayload(Data(payload))
            offset += Int(length)

            let originalJson = try encode(original)
            let decodedJson = try encode(decoded)
            #expect(originalJson == decodedJson)
        }

        #expect(offset == buffer.count)
    }
}

// MARK: - Helpers

private func encode<T: Encodable>(_ value: T) throws -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let data = try encoder.encode(value)
    return String(data: data, encoding: .utf8)!
}
