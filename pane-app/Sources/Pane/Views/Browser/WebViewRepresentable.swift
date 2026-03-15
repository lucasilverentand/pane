#if canImport(AppKit)
import SwiftUI
import WebKit

/// Wraps a WKWebView instance for use in SwiftUI.
/// The WKWebView is owned by the BrowserTab and kept alive across view updates.
struct WebViewRepresentable: NSViewRepresentable {
    let browserTab: BrowserTab

    func makeNSView(context: Context) -> WKWebView {
        browserTab.webView.navigationDelegate = context.coordinator
        return browserTab.webView
    }

    func updateNSView(_ nsView: WKWebView, context: Context) {
        context.coordinator.browserTab = browserTab
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(browserTab: browserTab)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var browserTab: BrowserTab

        init(browserTab: BrowserTab) {
            self.browserTab = browserTab
        }

        func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
            Task { @MainActor in
                browserTab.isLoading = true
            }
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            Task { @MainActor in
                browserTab.isLoading = false
                browserTab.title = webView.title ?? "Untitled"
                browserTab.url = webView.url
                browserTab.canGoBack = webView.canGoBack
                browserTab.canGoForward = webView.canGoForward
            }
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: any Error) {
            Task { @MainActor in
                browserTab.isLoading = false
            }
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: any Error) {
            Task { @MainActor in
                browserTab.isLoading = false
                browserTab.title = "Error"
            }
        }
    }
}
#endif
