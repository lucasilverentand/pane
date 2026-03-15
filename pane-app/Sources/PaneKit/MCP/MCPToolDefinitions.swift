import Foundation

/// All browser MCP tool definitions.
public enum MCPToolDefinitions {
    public static let all: [MCPToolDefinition] = [
        navigate,
        getUrl,
        readContent,
        screenshot,
        click,
        type,
        evaluate,
    ]

    public static let navigate = MCPToolDefinition(
        name: "browser_navigate",
        description: "Navigate the browser to a URL. Opens a new browser tab if none exists.",
        inputSchema: .object([
            "type": .string("object"),
            "properties": .object([
                "url": .object([
                    "type": .string("string"),
                    "description": .string("The URL to navigate to"),
                ]),
                "tabId": .object([
                    "type": .string("string"),
                    "description": .string("Browser tab ID. Defaults to the active tab."),
                ]),
            ]),
            "required": .array([.string("url")]),
        ])
    )

    public static let getUrl = MCPToolDefinition(
        name: "browser_get_url",
        description: "Get the current URL and title of a browser tab.",
        inputSchema: .object([
            "type": .string("object"),
            "properties": .object([
                "tabId": .object([
                    "type": .string("string"),
                    "description": .string("Browser tab ID. Defaults to the active tab."),
                ]),
            ]),
        ])
    )

    public static let readContent = MCPToolDefinition(
        name: "browser_read_content",
        description: "Read the text content of the page or a specific element.",
        inputSchema: .object([
            "type": .string("object"),
            "properties": .object([
                "tabId": .object([
                    "type": .string("string"),
                    "description": .string("Browser tab ID. Defaults to the active tab."),
                ]),
                "selector": .object([
                    "type": .string("string"),
                    "description": .string("CSS selector to read content from. Defaults to the entire page body."),
                ]),
            ]),
        ])
    )

    public static let screenshot = MCPToolDefinition(
        name: "browser_screenshot",
        description: "Take a screenshot of the browser tab. Returns a base64-encoded PNG image.",
        inputSchema: .object([
            "type": .string("object"),
            "properties": .object([
                "tabId": .object([
                    "type": .string("string"),
                    "description": .string("Browser tab ID. Defaults to the active tab."),
                ]),
            ]),
        ])
    )

    public static let click = MCPToolDefinition(
        name: "browser_click",
        description: "Click an element on the page matching a CSS selector.",
        inputSchema: .object([
            "type": .string("object"),
            "properties": .object([
                "selector": .object([
                    "type": .string("string"),
                    "description": .string("CSS selector of the element to click"),
                ]),
                "tabId": .object([
                    "type": .string("string"),
                    "description": .string("Browser tab ID. Defaults to the active tab."),
                ]),
            ]),
            "required": .array([.string("selector")]),
        ])
    )

    public static let type = MCPToolDefinition(
        name: "browser_type",
        description: "Type text into an input element matching a CSS selector.",
        inputSchema: .object([
            "type": .string("object"),
            "properties": .object([
                "selector": .object([
                    "type": .string("string"),
                    "description": .string("CSS selector of the input element"),
                ]),
                "text": .object([
                    "type": .string("string"),
                    "description": .string("Text to type into the element"),
                ]),
                "tabId": .object([
                    "type": .string("string"),
                    "description": .string("Browser tab ID. Defaults to the active tab."),
                ]),
            ]),
            "required": .array([.string("selector"), .string("text")]),
        ])
    )

    public static let evaluate = MCPToolDefinition(
        name: "browser_evaluate",
        description: "Evaluate JavaScript in the browser tab and return the result.",
        inputSchema: .object([
            "type": .string("object"),
            "properties": .object([
                "script": .object([
                    "type": .string("string"),
                    "description": .string("JavaScript code to evaluate"),
                ]),
                "tabId": .object([
                    "type": .string("string"),
                    "description": .string("Browser tab ID. Defaults to the active tab."),
                ]),
            ]),
            "required": .array([.string("script")]),
        ])
    )
}
