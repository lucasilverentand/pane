#if canImport(AppKit)
import Foundation
import WebKit

/// A single browser tab, holding a persistent WKWebView instance.
@MainActor @Observable
final class BrowserTab: Identifiable {
    let id = UUID()
    var url: URL?
    var title: String = "New Tab"
    var canGoBack = false
    var canGoForward = false
    var isLoading = false
    let webView: WKWebView

    init(url: URL? = nil) {
        let config = WKWebViewConfiguration()
        config.preferences.isElementFullscreenEnabled = true
        self.webView = WKWebView(frame: .zero, configuration: config)
        self.url = url
        if let url {
            webView.load(URLRequest(url: url))
        }
    }

    func navigate(to url: URL) {
        self.url = url
        webView.load(URLRequest(url: url))
    }

    func goBack() {
        webView.goBack()
    }

    func goForward() {
        webView.goForward()
    }

    func reload() {
        webView.reload()
    }
}
#endif
