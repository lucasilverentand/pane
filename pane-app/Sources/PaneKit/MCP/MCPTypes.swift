import Foundation

// MARK: - JSON-RPC 2.0

public struct JSONRPCRequest: Codable, Sendable {
    public let jsonrpc: String
    public let id: JSONRPCId?
    public let method: String
    public let params: JSONValue?

    public init(jsonrpc: String = "2.0", id: JSONRPCId? = nil, method: String, params: JSONValue? = nil) {
        self.jsonrpc = jsonrpc
        self.id = id
        self.method = method
        self.params = params
    }
}

public struct JSONRPCResponse: Codable, Sendable {
    public let jsonrpc: String
    public let id: JSONRPCId?
    public var result: JSONValue?
    public var error: JSONRPCError?

    public init(id: JSONRPCId?, result: JSONValue) {
        self.jsonrpc = "2.0"
        self.id = id
        self.result = result
    }

    public init(id: JSONRPCId?, error: JSONRPCError) {
        self.jsonrpc = "2.0"
        self.id = id
        self.error = error
    }
}

public struct JSONRPCError: Codable, Sendable {
    public let code: Int
    public let message: String
    public let data: JSONValue?

    public init(code: Int, message: String, data: JSONValue? = nil) {
        self.code = code
        self.message = message
        self.data = data
    }

    public static let parseError = JSONRPCError(code: -32700, message: "Parse error")
    public static let methodNotFound = JSONRPCError(code: -32601, message: "Method not found")
    public static let invalidParams = JSONRPCError(code: -32602, message: "Invalid params")
    public static func internalError(_ msg: String) -> JSONRPCError {
        JSONRPCError(code: -32603, message: msg)
    }
}

// MARK: - JSON-RPC ID (string or int)

public enum JSONRPCId: Codable, Sendable, Hashable {
    case string(String)
    case int(Int)

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let value = try? container.decode(Int.self) {
            self = .int(value)
        } else {
            self = .string(try container.decode(String.self))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .string(let s): try container.encode(s)
        case .int(let i): try container.encode(i)
        }
    }
}

// MARK: - Dynamic JSON value

public enum JSONValue: Codable, Sendable, Hashable {
    case null
    case bool(Bool)
    case int(Int)
    case double(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let b = try? container.decode(Bool.self) {
            self = .bool(b)
        } else if let i = try? container.decode(Int.self) {
            self = .int(i)
        } else if let d = try? container.decode(Double.self) {
            self = .double(d)
        } else if let s = try? container.decode(String.self) {
            self = .string(s)
        } else if let arr = try? container.decode([JSONValue].self) {
            self = .array(arr)
        } else {
            self = .object(try container.decode([String: JSONValue].self))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case .bool(let b): try container.encode(b)
        case .int(let i): try container.encode(i)
        case .double(let d): try container.encode(d)
        case .string(let s): try container.encode(s)
        case .array(let a): try container.encode(a)
        case .object(let o): try container.encode(o)
        }
    }

    /// Access a string value from an object by key.
    public subscript(key: String) -> JSONValue? {
        if case .object(let dict) = self { return dict[key] }
        return nil
    }

    /// Get string value.
    public var stringValue: String? {
        if case .string(let s) = self { return s }
        return nil
    }
}

// MARK: - MCP-specific types

public struct MCPInitializeResult: Codable, Sendable {
    public let protocolVersion: String
    public let capabilities: MCPCapabilities
    public let serverInfo: MCPServerInfo

    public init() {
        self.protocolVersion = "2024-11-05"
        self.capabilities = MCPCapabilities(tools: .init(listChanged: false))
        self.serverInfo = MCPServerInfo(name: "pane-browser", version: "0.1.0")
    }
}

public struct MCPCapabilities: Codable, Sendable {
    public let tools: MCPToolCapability?
}

public struct MCPToolCapability: Codable, Sendable {
    public let listChanged: Bool
}

public struct MCPServerInfo: Codable, Sendable {
    public let name: String
    public let version: String
}

public struct MCPToolDefinition: Codable, Sendable {
    public let name: String
    public let description: String
    public let inputSchema: JSONValue

    public init(name: String, description: String, inputSchema: JSONValue) {
        self.name = name
        self.description = description
        self.inputSchema = inputSchema
    }
}

public struct MCPToolCallResult: Codable, Sendable {
    public let content: [MCPContent]
    public let isError: Bool?

    public init(content: [MCPContent], isError: Bool? = nil) {
        self.content = content
        self.isError = isError
    }

    public static func text(_ text: String) -> MCPToolCallResult {
        MCPToolCallResult(content: [.text(text)])
    }

    public static func image(_ base64: String, mimeType: String = "image/png") -> MCPToolCallResult {
        MCPToolCallResult(content: [.image(base64, mimeType: mimeType)])
    }

    public static func error(_ message: String) -> MCPToolCallResult {
        MCPToolCallResult(content: [.text(message)], isError: true)
    }
}

public enum MCPContent: Codable, Sendable, Hashable {
    case text(String)
    case image(String, mimeType: String)

    private enum CodingKeys: String, CodingKey {
        case type, text, data, mimeType
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        switch type {
        case "text":
            self = .text(try container.decode(String.self, forKey: .text))
        case "image":
            let data = try container.decode(String.self, forKey: .data)
            let mime = try container.decode(String.self, forKey: .mimeType)
            self = .image(data, mimeType: mime)
        default:
            throw DecodingError.dataCorruptedError(forKey: .type, in: container, debugDescription: "Unknown content type: \(type)")
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .text(let text):
            try container.encode("text", forKey: .type)
            try container.encode(text, forKey: .text)
        case .image(let data, let mimeType):
            try container.encode("image", forKey: .type)
            try container.encode(data, forKey: .data)
            try container.encode(mimeType, forKey: .mimeType)
        }
    }
}
