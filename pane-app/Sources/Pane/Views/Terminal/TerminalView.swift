#if canImport(AppKit)
import SwiftUI
import SwiftTerm
import PaneKit

/// Wraps SwiftTerm's AppKit `TerminalView` for use in SwiftUI via NSViewRepresentable.
///
/// This uses the base `TerminalView` (not `LocalProcessTerminalView`) because
/// the PTY processes are managed by the Pane daemon, not locally. Data flows
/// through the PaneClient connection.
struct TerminalView: NSViewRepresentable {
    let windowId: WindowId

    @Environment(PaneClient.self) private var client

    func makeNSView(context: Context) -> SwiftTerm.TerminalView {
        let terminalView = SwiftTerm.TerminalView(frame: .zero)
        terminalView.terminalDelegate = context.coordinator
        context.coordinator.terminalView = terminalView
        return terminalView
    }

    func updateNSView(_ nsView: SwiftTerm.TerminalView, context: Context) {
        context.coordinator.windowId = windowId
        context.coordinator.client = client
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(windowId: windowId, client: client)
    }

    final class Coordinator: NSObject, SwiftTerm.TerminalViewDelegate {
        var windowId: WindowId
        var client: PaneClient
        weak var terminalView: SwiftTerm.TerminalView?

        init(windowId: WindowId, client: PaneClient) {
            self.windowId = windowId
            self.client = client
        }

        // MARK: - TerminalViewDelegate required methods

        func sizeChanged(source: SwiftTerm.TerminalView, newCols: Int, newRows: Int) {
            let client = self.client
            Task {
                try? await client.resize(width: UInt16(newCols), height: UInt16(newRows))
            }
        }

        func setTerminalTitle(source: SwiftTerm.TerminalView, title: String) {
            // Title is managed by the daemon
        }

        func send(source: SwiftTerm.TerminalView, data: ArraySlice<UInt8>) {
            // User typed something -- forward to daemon as raw key data
            let bytes = Array(data)
            let client = self.client
            Task {
                for byte in bytes {
                    let code: SerializableKeyCode = .char(Character(UnicodeScalar(byte)))
                    try? await client.sendKey(code: code)
                }
            }
        }

        func scrolled(source: SwiftTerm.TerminalView, position: Double) {
            // Scroll position managed by daemon
        }

        func hostCurrentDirectoryUpdate(source: SwiftTerm.TerminalView, directory: String?) {
            // CWD tracked by daemon
        }

        func requestOpenLink(source: SwiftTerm.TerminalView, link: String, params: [String: String]) {
            if let url = URL(string: link) {
                NSWorkspace.shared.open(url)
            }
        }

        func clipboardCopy(source: SwiftTerm.TerminalView, content: Data) {
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            pasteboard.setData(content, forType: .string)
        }

        func rangeChanged(source: SwiftTerm.TerminalView, startY: Int, endY: Int) {
            // Display range changed -- no action needed for remote terminal
        }

        // MARK: - Feed data from daemon

        /// Feed PTY output bytes into the terminal emulator.
        @MainActor
        func feedData(_ data: [UInt8]) {
            let slice = data[data.startIndex...]
            terminalView?.feed(byteArray: slice)
        }
    }
}
#endif
