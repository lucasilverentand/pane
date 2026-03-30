#if canImport(AppKit)
import AppKit
import SwiftUI

/// Manages the singleton ghostty_app_t instance and its lifecycle.
///
/// Each Pane app process has exactly one GhosttyAppManager. It creates the
/// ghostty app with runtime callbacks, ticks it via a display link, and
/// provides surface creation for terminal views.
@MainActor
final class GhosttyAppManager {
    static let shared = GhosttyAppManager()

    private(set) var app: ghostty_app_t?
    private var tickTimer: Timer?

    /// The background color configured for the ghostty terminal.
    /// Used by the host views to match their backgrounds to the terminal.
    private(set) var terminalBackground: NSColor = .black

    /// SwiftUI-friendly terminal background color from the ghostty theme.
    var terminalBackgroundColor: Color { Color(nsColor: terminalBackground) }

    /// Whether the terminal theme uses a light background color.
    var isLightTheme: Bool {
        guard let srgb = terminalBackground.usingColorSpace(.sRGB) else { return false }
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        srgb.getRed(&r, green: &g, blue: &b, alpha: &a)
        let luminance = 0.299 * r + 0.587 * g + 0.114 * b
        return luminance > 0.5
    }

    /// Configure a window to be transparent with the correct light/dark appearance.
    func configureWindowAppearance(for nsWindow: NSWindow) {
        nsWindow.isOpaque = false
        nsWindow.backgroundColor = .clear
        nsWindow.appearance = isLightTheme
            ? NSAppearance(named: .aqua)
            : NSAppearance(named: .darkAqua)
    }

    private init() {
        // Initialize the ghostty library first
        let initResult = ghostty_init(UInt(CommandLine.argc), CommandLine.unsafeArgv)
        guard initResult == GHOSTTY_SUCCESS else {
            print("[GhosttyAppManager] ghostty_init failed: \(initResult)")
            return
        }

        // Create config
        guard let config = ghostty_config_new() else {
            print("[GhosttyAppManager] ghostty_config_new failed")
            return
        }

        // Load user's ghostty config for fonts/theme, then overlay Pane-specific settings
        ghostty_config_load_default_files(config)
        loadPaneConfig(config)
        ghostty_config_finalize(config)

        // Extract the resolved background color for theme blending
        terminalBackground = Self.resolvedBackgroundColor(from: config)

        // Set up runtime callbacks
        var runtime = ghostty_runtime_config_s()
        runtime.userdata = Unmanaged.passUnretained(self).toOpaque()
        runtime.supports_selection_clipboard = false
        runtime.wakeup_cb = { userdata in
            guard let userdata else { return }
            let _ = Unmanaged<GhosttyAppManager>.fromOpaque(userdata).takeUnretainedValue()
            DispatchQueue.main.async {
                GhosttyAppManager.shared.tick()
            }
        }
        runtime.action_cb = { app, target, action in
            return GhosttyAppManager.handleAction(app, target: target, action: action)
        }
        runtime.read_clipboard_cb = { userdata, location, state in
            return GhosttyAppManager.readClipboard(userdata, location: location, state: state)
        }
        runtime.confirm_read_clipboard_cb = nil
        runtime.write_clipboard_cb = { userdata, location, content, len, confirm in
            GhosttyAppManager.writeClipboard(userdata, location: location, content: content, len: len)
        }
        runtime.close_surface_cb = { userdata, processAlive in
            // Surface closed — no-op for now
        }

        guard let ghosttyApp = ghostty_app_new(&runtime, config) else {
            print("[GhosttyAppManager] ghostty_app_new failed")
            ghostty_config_free(config)
            return
        }
        self.app = ghosttyApp

        // Tick at 60fps via a timer (simpler than CVDisplayLink for now)
        tickTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.tick()
            }
        }
    }

    deinit {
        tickTimer?.invalidate()
        if let app { ghostty_app_free(app) }
    }

    func tick() {
        guard let app else { return }
        ghostty_app_tick(app)
    }

    // MARK: - Callbacks

    private static func handleAction(
        _ app: ghostty_app_t?,
        target: ghostty_target_s,
        action: ghostty_action_s
    ) -> Bool {
        switch action.tag {
        case GHOSTTY_ACTION_SET_TITLE:
            // Titles managed by the Pane daemon, ignore
            return true

        case GHOSTTY_ACTION_RING_BELL:
            NSSound.beep()
            return true

        case GHOSTTY_ACTION_MOUSE_SHAPE:
            let shape = action.action.mouse_shape
            let cursor: NSCursor = switch shape {
            case GHOSTTY_MOUSE_SHAPE_TEXT: .iBeam
            case GHOSTTY_MOUSE_SHAPE_POINTER: .pointingHand
            case GHOSTTY_MOUSE_SHAPE_CROSSHAIR: .crosshair
            default: .arrow
            }
            cursor.set()
            return true

        case GHOSTTY_ACTION_CLOSE_WINDOW:
            // Process exited in the bridge — ignore
            return true

        default:
            return false
        }
    }

    private static func readClipboard(
        _ userdata: UnsafeMutableRawPointer?,
        location: ghostty_clipboard_e,
        state: UnsafeMutableRawPointer?
    ) -> Bool {
        // Clipboard reads require completing via ghostty_surface_complete_clipboard_request,
        // which needs the surface pointer. In our bridge model the surface userdata is the
        // GhosttyTerminalNSView — we don't have a clean way to recover it here yet.
        // Return false to indicate we couldn't handle this.
        return false
    }

    private static func writeClipboard(
        _ userdata: UnsafeMutableRawPointer?,
        location: ghostty_clipboard_e,
        content: UnsafePointer<ghostty_clipboard_content_s>?,
        len: Int
    ) {
        guard let content, len > 0 else { return }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        if let data = content.pointee.data {
            pasteboard.setString(String(cString: data), forType: .string)
        }
    }

    // MARK: - Pane theme config

    /// Load Pane-specific ghostty configuration that makes the terminal
    /// blend seamlessly into the app chrome.
    private func loadPaneConfig(_ config: ghostty_config_t) {
        // Pane overrides: transparent background so the terminal blends with the
        // native window, no extra padding (the host view handles spacing), and
        // a font that looks good in the macOS context.
        let lines = [
            // Transparent background — the SwiftUI host view provides the background color
            "background-opacity = 0",
            // No window decorations — Pane provides its own chrome
            "window-decoration = false",
            // Snug padding — the SwiftUI layout provides outer spacing
            "window-padding-x = 4",
            "window-padding-y = 2",
            // Let the app control the title
            "window-title-font-family = ",
        ].joined(separator: "\n")

        let tmpURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("pane-ghostty-\(ProcessInfo.processInfo.processIdentifier).conf")
        do {
            try lines.write(to: tmpURL, atomically: true, encoding: .utf8)
            tmpURL.path.withCString { path in
                ghostty_config_load_file(config, path)
            }
            try? FileManager.default.removeItem(at: tmpURL)
        } catch {
            print("[GhosttyAppManager] Failed to write Pane config overlay: \(error)")
        }
    }

    /// Extract the resolved background color from a finalized ghostty config.
    private static func resolvedBackgroundColor(from config: ghostty_config_t) -> NSColor {
        var color = ghostty_config_color_s()
        let found = "background".withCString { key in
            ghostty_config_get(config, &color, key, UInt(MemoryLayout<ghostty_config_color_s>.size))
        }
        if found {
            return NSColor(
                red: CGFloat(color.r) / 255.0,
                green: CGFloat(color.g) / 255.0,
                blue: CGFloat(color.b) / 255.0,
                alpha: 1.0
            )
        }
        return .black
    }

    /// Apply background blur to a window hosting a ghostty surface.
    func applyBackgroundBlur(to nsWindow: NSWindow) {
        guard let app else { return }
        let rawWindow = Unmanaged.passUnretained(nsWindow).toOpaque()
        ghostty_set_window_background_blur(app, rawWindow)
    }
}
#endif
