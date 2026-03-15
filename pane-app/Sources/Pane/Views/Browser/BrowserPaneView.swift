#if canImport(AppKit)
import SwiftUI

/// Browser tab view with URL bar and WebView.
struct BrowserPaneView: View {
    let tab: BrowserTab

    @State private var urlText: String = ""

    var body: some View {
        VStack(spacing: 0) {
            // URL bar
            HStack(spacing: 4) {
                Button(action: { tab.goBack() }) {
                    Image(systemName: "chevron.left")
                }
                .disabled(!tab.canGoBack)
                .buttonStyle(.borderless)

                Button(action: { tab.goForward() }) {
                    Image(systemName: "chevron.right")
                }
                .disabled(!tab.canGoForward)
                .buttonStyle(.borderless)

                Button(action: { tab.reload() }) {
                    Image(systemName: tab.isLoading ? "xmark" : "arrow.clockwise")
                }
                .buttonStyle(.borderless)

                TextField("URL", text: $urlText)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        navigateToUrlText()
                    }
                    .onChange(of: tab.url) { _, newUrl in
                        urlText = newUrl?.absoluteString ?? ""
                    }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(.bar)

            // WebView
            WebViewRepresentable(browserTab: tab)
        }
        .onAppear {
            urlText = tab.url?.absoluteString ?? ""
        }
    }

    private func navigateToUrlText() {
        var text = urlText.trimmingCharacters(in: .whitespacesAndNewlines)
        if !text.contains("://") {
            text = "https://\(text)"
        }
        if let url = URL(string: text) {
            tab.navigate(to: url)
        }
    }
}
#endif
