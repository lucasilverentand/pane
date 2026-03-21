#if canImport(AppKit)
import AppKit
import SwiftUI
import GhosttyKit
import PaneKit

/// Wraps a ghostty terminal surface for use in SwiftUI.
///
/// Each TerminalView creates a GhosttyBridge for piping daemon bytes into the
/// ghostty renderer. The ghostty surface runs `nc -U <bridge-socket>` as its
/// command, creating a bidirectional relay.
///
/// - Output: daemon → bridge socket → nc stdout → ghostty PTY → Metal render
/// - Input:  user types → ghostty PTY → nc stdin → bridge socket → daemon
struct TerminalView: NSViewRepresentable {
    let windowId: WindowId

    @Environment(PaneClient.self) private var client
    @Environment(AppState.self) private var appState

    func makeNSView(context: Context) -> GhosttyTerminalNSView {
        let view = GhosttyTerminalNSView(windowId: windowId, client: client)
        return view
    }

    func updateNSView(_ nsView: GhosttyTerminalNSView, context: Context) {
        nsView.windowId = windowId
        nsView.client = client
    }
}

// MARK: - GhosttyTerminalNSView

/// The actual NSView that hosts a ghostty surface.
///
/// Ghostty creates a CAMetalLayer on this view and renders directly to it.
/// We forward keyboard/mouse events to the Pane daemon (not ghostty) since
/// the daemon manages the real PTY and terminal state.
final class GhosttyTerminalNSView: NSView {
    var windowId: WindowId
    var client: PaneClient

    private var surface: ghostty_surface_t?
    private let bridge = GhosttyBridge()

    init(windowId: WindowId, client: PaneClient) {
        self.windowId = windowId
        self.client = client
        super.init(frame: NSRect(x: 0, y: 0, width: 800, height: 600))

        wantsLayer = true

        setupBridge()
        setupSurface()
        registerForPaneOutput()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    deinit {
        bridge.stop()
        if let surface { ghostty_surface_free(surface) }
    }

    // MARK: - Setup

    private func setupBridge() {
        do {
            try bridge.start()
        } catch {
            print("[GhosttyTerminal] Bridge start failed: \(error)")
            return
        }

        // When user types in ghostty → bytes come through the bridge → send to daemon
        bridge.onInput = { [weak self] bytes in
            guard let self else { return }
            let client = self.client
            Task { @MainActor in
                // Send raw bytes as individual key events
                for byte in bytes {
                    let code: SerializableKeyCode = .char(Character(UnicodeScalar(byte)))
                    try? await client.sendKey(code: code)
                }
            }
        }
    }

    private func setupSurface() {
        guard let app = GhosttyAppManager.shared.app else {
            print("[GhosttyTerminal] GhosttyAppManager has no app")
            return
        }

        var config = ghostty_surface_config_new()
        config.userdata = Unmanaged.passUnretained(self).toOpaque()
        config.platform_tag = GHOSTTY_PLATFORM_MACOS
        config.platform = ghostty_platform_u(
            macos: ghostty_platform_macos_s(nsview: Unmanaged.passUnretained(self).toOpaque())
        )
        config.scale_factor = Double(NSScreen.main?.backingScaleFactor ?? 2.0)
        config.font_size = 0 // Use ghostty default
        config.context = GHOSTTY_SURFACE_CONTEXT_WINDOW

        // Run nc as the relay command — connects to our bridge socket
        let command = "/usr/bin/nc -U \(bridge.socketPath)"
        command.withCString { cmdPtr in
            config.command = cmdPtr
            surface = ghostty_surface_new(app, &config)
        }

        guard surface != nil else {
            print("[GhosttyTerminal] Failed to create ghostty surface")
            return
        }
    }

    private func registerForPaneOutput() {
        let windowId = self.windowId
        client.onPaneOutput = { [weak self] tabId, bytes in
            guard let self else { return }
            // Route output to this terminal's bridge
            self.bridge.write(bytes: bytes)
        }
    }

    // MARK: - View lifecycle

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let surface {
            ghostty_surface_set_focus(surface, window?.isKeyWindow ?? false)
        }
    }

    override func layout() {
        super.layout()
        guard let surface else { return }
        let scaledSize = convertToBacking(bounds.size)
        ghostty_surface_set_size(surface, UInt32(scaledSize.width), UInt32(scaledSize.height))
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard let surface else { return }
        if let scale = window?.backingScaleFactor {
            ghostty_surface_set_content_scale(surface, scale, scale)
        }
        let scaledSize = convertToBacking(bounds.size)
        ghostty_surface_set_size(surface, UInt32(scaledSize.width), UInt32(scaledSize.height))
    }

    override var acceptsFirstResponder: Bool { true }

    override func becomeFirstResponder() -> Bool {
        if let surface { ghostty_surface_set_focus(surface, true) }
        return true
    }

    override func resignFirstResponder() -> Bool {
        if let surface { ghostty_surface_set_focus(surface, false) }
        return true
    }

    // MARK: - Keyboard events → forward to Pane daemon

    override func keyDown(with event: NSEvent) {
        let client = self.client
        let mods = modifiers(from: event)
        if let key = ghosttyKeyCode(from: event) {
            Task { @MainActor in
                try? await client.sendKey(code: key, modifiers: mods)
            }
        }
    }

    override func keyUp(with event: NSEvent) {
        // Daemon doesn't need key-up events
    }

    // MARK: - Mouse events → forward to ghostty (for selection, scrolling)

    override func mouseDown(with event: NSEvent) {
        guard let surface else { return }
        let pos = convertToLayer(convert(event.locationInWindow, from: nil))
        ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_LEFT, GHOSTTY_MODS_NONE)
        ghostty_surface_mouse_pos(surface, pos.x, pos.y, GHOSTTY_MODS_NONE)
    }

    override func mouseUp(with event: NSEvent) {
        guard let surface else { return }
        ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_LEFT, GHOSTTY_MODS_NONE)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let surface else { return }
        let pos = convertToLayer(convert(event.locationInWindow, from: nil))
        ghostty_surface_mouse_pos(surface, pos.x, pos.y, GHOSTTY_MODS_NONE)
    }

    override func scrollWheel(with event: NSEvent) {
        guard let surface else { return }
        ghostty_surface_mouse_scroll(
            surface,
            event.scrollingDeltaX,
            event.scrollingDeltaY,
            0
        )
    }

    // MARK: - Key conversion

    private func modifiers(from event: NSEvent) -> UInt8 {
        var mods: UInt8 = 0
        if event.modifierFlags.contains(.shift) { mods |= 1 }
        if event.modifierFlags.contains(.control) { mods |= 2 }
        if event.modifierFlags.contains(.option) { mods |= 4 }
        if event.modifierFlags.contains(.command) { mods |= 8 }
        return mods
    }

    private func ghosttyKeyCode(from event: NSEvent) -> SerializableKeyCode? {
        // Map characters to SerializableKeyCode
        if let chars = event.charactersIgnoringModifiers, let scalar = chars.unicodeScalars.first {
            switch scalar {
            case "\u{1b}": return .esc
            case "\r", "\n": return .enter
            case "\t": return .tab
            case "\u{7f}": return .backspace
            default:
                if scalar.value < 128 {
                    return .char(Character(scalar))
                }
            }
        }

        // Map special keys
        switch event.keyCode {
        case 123: return .left
        case 124: return .right
        case 125: return .down
        case 126: return .up
        case 115: return .home
        case 119: return .end
        case 116: return .pageUp
        case 121: return .pageDown
        case 117: return .delete
        case 122: return .f(1)
        case 120: return .f(2)
        case 99: return .f(3)
        case 118: return .f(4)
        case 96: return .f(5)
        case 97: return .f(6)
        case 98: return .f(7)
        case 100: return .f(8)
        case 101: return .f(9)
        case 109: return .f(10)
        case 103: return .f(11)
        case 111: return .f(12)
        default: return nil
        }
    }
}
#endif
