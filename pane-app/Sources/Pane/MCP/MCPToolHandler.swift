#if canImport(AppKit)
import Foundation
import PaneKit

/// Maps MCP tool names to BrowserManager method calls.
@MainActor
enum MCPToolHandler {
    static func handle(
        tool: String,
        arguments: JSONValue,
        browser: BrowserManager
    ) async -> MCPToolCallResult {
        let tabId = arguments["tabId"]?.stringValue.flatMap { UUID(uuidString: $0) }

        switch tool {
        case "browser_navigate":
            guard let urlString = arguments["url"]?.stringValue,
                  let url = URL(string: urlString)
            else {
                return .error("Missing or invalid 'url' parameter")
            }
            // If no tabs exist, create one
            if browser.resolveAnyTab() == nil {
                // Find first window ID from tabs, or create with a placeholder
                let windowId = browser.tabs.keys.first ?? PaneKit.WindowId()
                browser.openTab(in: windowId, url: url)
            } else if let tab = browser.navigate(url: url, tabId: tabId) {
                _ = tab
            } else {
                return .error("No browser tab found")
            }
            return .text("Navigated to \(urlString)")

        case "browser_get_url":
            guard let tab = browser.resolveTab(id: tabId) else {
                return .error("No browser tab found")
            }
            let url = tab.url?.absoluteString ?? ""
            let title = tab.title
            return .text("URL: \(url)\nTitle: \(title)")

        case "browser_read_content":
            let selector = arguments["selector"]?.stringValue
            let content = await browser.getPageContent(tabId: tabId, selector: selector)
            return .text(content)

        case "browser_screenshot":
            guard let pngData = await browser.takeScreenshot(tabId: tabId) else {
                return .error("Failed to take screenshot")
            }
            return .image(pngData.base64EncodedString())

        case "browser_click":
            guard let selector = arguments["selector"]?.stringValue else {
                return .error("Missing 'selector' parameter")
            }
            let success = await browser.click(selector: selector, tabId: tabId)
            return success ? .text("Clicked element") : .error("Element not found: \(selector)")

        case "browser_type":
            guard let selector = arguments["selector"]?.stringValue else {
                return .error("Missing 'selector' parameter")
            }
            guard let text = arguments["text"]?.stringValue else {
                return .error("Missing 'text' parameter")
            }
            let success = await browser.typeText(selector: selector, text: text, tabId: tabId)
            return success ? .text("Typed text into element") : .error("Element not found: \(selector)")

        case "browser_evaluate":
            guard let script = arguments["script"]?.stringValue else {
                return .error("Missing 'script' parameter")
            }
            let result = await browser.evaluateJavaScript(script: script, tabId: tabId)
            return .text(result)

        default:
            return .error("Unknown tool: \(tool)")
        }
    }
}
#endif
