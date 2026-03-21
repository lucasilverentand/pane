#if canImport(AppKit)
import AppKit
import GhosttyKit

/// Manages the singleton ghostty_app_t instance and its lifecycle.
///
/// Each Pane app process has exactly one GhosttyAppManager. It creates the
/// ghostty app with runtime callbacks, ticks it via a display link, and
/// provides surface creation for terminal views.
@MainActor
final class GhosttyAppManager {
    static let shared = GhosttyAppManager()

    private(set) var app: ghostty_app_t?
    private var displayLink: CVDisplayLink?
    private var tickTimer: Timer?

    private init() {
        // Create config
        guard let config = ghostty_config_new() else { return }

        // Minimal config — we don't load user's ghostty config files
        // since this is an embedded terminal, not standalone Ghostty.
        ghostty_config_finalize(config)

        // Set up runtime callbacks
        var runtime = ghostty_runtime_config_s()
        runtime.userdata = Unmanaged.passUnretained(self).toOpaque()
        runtime.supports_selection_clipboard = false
        runtime.wakeup_cb = { userdata in
            guard let userdata else { return }
            let mgr = Unmanaged<GhosttyAppManager>.fromOpaque(userdata).takeUnretainedValue()
            DispatchQueue.main.async {
                mgr.tick()
            }
        }
        runtime.action_cb = { app, target, action in
            GhosttyAppManager.handleAction(app, target: target, action: action)
        }
        runtime.read_clipboard_cb = { userdata, location, state in
            GhosttyAppManager.readClipboard(userdata, location: location, state: state)
        }
        runtime.confirm_read_clipboard_cb = nil
        runtime.write_clipboard_cb = { userdata, location, content, len, confirm in
            GhosttyAppManager.writeClipboard(userdata, location: location, content: content, len: len)
        }
        runtime.close_surface_cb = { userdata, processAlive in
            // Surface closed — no-op for now
        }

        guard let ghosttyApp = ghostty_app_new(&runtime, config) else {
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
        guard let surface = surfaceFromUserdata(state) else { return false }
        let pasteboard = NSPasteboard.general
        guard let str = pasteboard.string(forType: .string) else { return false }
        str.withCString { ptr in
            ghostty_surface_complete_clipboard_request(surface, ptr, state, true)
        }
        return true
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

    private static func surfaceFromUserdata(_ ptr: UnsafeMutableRawPointer?) -> ghostty_surface_t? {
        // The surface userdata is the GhosttyTerminalView
        // We don't need to recover the surface from here for clipboard
        return nil
    }
}
#endif
