#if canImport(AppKit)
import Foundation
import WebKit
import PaneKit

/// Manages browser tabs across all windows.
/// Each window can have zero or more browser tabs, independent of daemon-managed terminal tabs.
@MainActor @Observable
final class BrowserManager {
    /// Browser tabs per window.
    var tabs: [WindowId: [BrowserTab]] = [:]

    /// Active browser tab per window.
    var activeTabId: [WindowId: UUID] = [:]

    // MARK: - Tab management

    @discardableResult
    func openTab(in windowId: WindowId, url: URL? = nil) -> BrowserTab {
        let tab = BrowserTab(url: url)
        tabs[windowId, default: []].append(tab)
        activeTabId[windowId] = tab.id
        return tab
    }

    func closeTab(id: UUID, in windowId: WindowId) {
        guard var windowTabs = tabs[windowId] else { return }
        windowTabs.removeAll { $0.id == id }
        tabs[windowId] = windowTabs.isEmpty ? nil : windowTabs

        // Update active tab if we closed the active one
        if activeTabId[windowId] == id {
            activeTabId[windowId] = windowTabs.last?.id
        }
    }

    func browserTabs(for windowId: WindowId) -> [BrowserTab] {
        tabs[windowId] ?? []
    }

    func activeBrowserTab(for windowId: WindowId) -> BrowserTab? {
        guard let activeId = activeTabId[windowId] else { return nil }
        return tabs[windowId]?.first { $0.id == activeId }
    }

    func setActiveBrowserTab(_ tabId: UUID, in windowId: WindowId) {
        activeTabId[windowId] = tabId
    }

    // MARK: - MCP-callable methods

    /// Navigate a tab to a URL. Returns the tab that was navigated.
    func navigate(url: URL, tabId: UUID? = nil) -> BrowserTab? {
        let tab = resolveTab(id: tabId)
        tab?.navigate(to: url)
        return tab
    }

    /// Get the text content of the page (or a CSS-selected element).
    func getPageContent(tabId: UUID? = nil, selector: String? = nil) async -> String {
        guard let tab = resolveTab(id: tabId) else { return "" }
        let js: String
        if let selector {
            js = "document.querySelector('\(selector.replacingOccurrences(of: "'", with: "\\'"))')?.innerText ?? ''"
        } else {
            js = "document.body.innerText"
        }
        return (try? await tab.webView.evaluateJavaScript(js) as? String) ?? ""
    }

    /// Take a screenshot of the page as PNG data.
    func takeScreenshot(tabId: UUID? = nil) async -> Data? {
        guard let tab = resolveTab(id: tabId) else { return nil }
        let config = WKSnapshotConfiguration()
        return try? await tab.webView.takeSnapshot(configuration: config).tiffRepresentation
            .flatMap { NSBitmapImageRep(data: $0) }?
            .representation(using: .png, properties: [:])
    }

    /// Click an element matching a CSS selector.
    func click(selector: String, tabId: UUID? = nil) async -> Bool {
        guard let tab = resolveTab(id: tabId) else { return false }
        let js = """
        (() => {
            const el = document.querySelector('\(selector.replacingOccurrences(of: "'", with: "\\'"))');
            if (el) { el.click(); return true; }
            return false;
        })()
        """
        return (try? await tab.webView.evaluateJavaScript(js) as? Bool) ?? false
    }

    /// Type text into an element matching a CSS selector.
    func typeText(selector: String, text: String, tabId: UUID? = nil) async -> Bool {
        guard let tab = resolveTab(id: tabId) else { return false }
        let escapedText = text.replacingOccurrences(of: "'", with: "\\'")
            .replacingOccurrences(of: "\n", with: "\\n")
        let js = """
        (() => {
            const el = document.querySelector('\(selector.replacingOccurrences(of: "'", with: "\\'"))');
            if (el) {
                el.focus();
                el.value = '\(escapedText)';
                el.dispatchEvent(new Event('input', { bubbles: true }));
                el.dispatchEvent(new Event('change', { bubbles: true }));
                return true;
            }
            return false;
        })()
        """
        return (try? await tab.webView.evaluateJavaScript(js) as? Bool) ?? false
    }

    /// Evaluate arbitrary JavaScript and return the result as a string.
    func evaluateJavaScript(script: String, tabId: UUID? = nil) async -> String {
        guard let tab = resolveTab(id: tabId) else { return "" }
        let result = try? await tab.webView.evaluateJavaScript(script)
        if let result {
            return String(describing: result)
        }
        return ""
    }

    // MARK: - Tab resolution

    /// Returns any active browser tab, or nil if none exist.
    func resolveAnyTab() -> BrowserTab? {
        resolveTab(id: nil)
    }

    /// Resolve a tab ID to a BrowserTab. If nil, returns the frontmost active browser tab.
    func resolveTab(id tabId: UUID?) -> BrowserTab? {
        if let tabId {
            for windowTabs in tabs.values {
                if let tab = windowTabs.first(where: { $0.id == tabId }) {
                    return tab
                }
            }
            return nil
        }
        // Return the first active browser tab we can find
        for (windowId, id) in activeTabId {
            if let tab = tabs[windowId]?.first(where: { $0.id == id }) {
                return tab
            }
        }
        // Fallback to any tab
        return tabs.values.first?.first
    }
}
#endif
