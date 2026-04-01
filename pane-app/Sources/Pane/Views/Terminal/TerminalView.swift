#if canImport(AppKit)
import AppKit
import SwiftUI
import PaneKit

/// Wraps a ghostty terminal surface for use in SwiftUI.
///
/// Each TerminalView creates a GhosttyBridge for piping daemon bytes into the
/// ghostty renderer. The ghostty surface runs `nc -U <bridge-socket>` as its
/// command, creating a bidirectional relay.
///
/// - Output: daemon → bridge socket → nc stdout → ghostty PTY → Metal render
/// - Input:  user types → ghostty processes key → nc stdin → bridge socket → daemon
struct TerminalView: NSViewRepresentable {
    let windowId: WindowId

    @Environment(PaneClient.self) private var client
    @Environment(AppState.self) private var appState

    func makeNSView(context: Context) -> GhosttyTerminalNSView {
        let view = GhosttyTerminalNSView(windowId: windowId, client: client)
        return view
    }

    func updateNSView(_ nsView: GhosttyTerminalNSView, context: Context) {
        let prevWindowId = nsView.windowId
        nsView.windowId = windowId
        nsView.client = client
        // Re-register the subscription if the window this view represents has changed.
        if prevWindowId != windowId {
            nsView.updatePaneOutputSubscription()
        }
    }
}

// MARK: - GhosttyTerminalNSView

/// The actual NSView that hosts a ghostty surface.
///
/// Ghostty creates a CAMetalLayer on this view and renders directly to it.
/// Keyboard input is processed natively by GhosttyKit (via `ghostty_surface_key`),
/// which handles IME, dead keys, option-as-alt, and full Unicode. The resulting
/// bytes flow through the nc bridge relay to the Pane daemon.
final class GhosttyTerminalNSView: NSView, NSTextInputClient {
    var windowId: WindowId
    var client: PaneClient

    private var surface: ghostty_surface_t?
    private let bridge = GhosttyBridge()
    private var subscriptionId: UUID?

    /// Marked text for IME input (e.g. CJK composition).
    private var markedText = NSMutableAttributedString()

    /// Accumulator for text produced during a keyDown → interpretKeyEvents cycle.
    private var keyTextAccumulator: [String]?

    init(windowId: WindowId, client: PaneClient) {
        self.windowId = windowId
        self.client = client
        super.init(frame: NSRect(x: 0, y: 0, width: 800, height: 600))

        wantsLayer = true
        // Make the view layer transparent so the ghostty Metal content
        // composites cleanly with the native window background.
        layer?.isOpaque = false
        layer?.backgroundColor = .clear

        setupBridge()
        setupSurface()
        registerForPaneOutput()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    deinit {
        if let id = subscriptionId {
            let c = client
            MainActor.assumeIsolated { c.unsubscribePaneOutput(id) }
        }
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

        // When ghostty writes key output to nc → bytes arrive here → send to daemon as raw input.
        // We send focusWindow before rawInput so the daemon knows which pane this input targets.
        bridge.onInput = { [weak self] bytes in
            guard let self else { return }
            let data = Data(bytes)
            let c = self.client
            let wid = self.windowId
            Task { @MainActor in
                try? await c.focusWindow(id: wid)
                try? await c.rawInput(data)
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

        guard let surface else {
            print("[GhosttyTerminal] Failed to create ghostty surface")
            return
        }

        // Tell the surface which color scheme to use so themes with light/dark
        // variants pick the correct one.
        let scheme: ghostty_color_scheme_e = GhosttyAppManager.shared.isLightTheme
            ? GHOSTTY_COLOR_SCHEME_LIGHT
            : GHOSTTY_COLOR_SCHEME_DARK
        ghostty_surface_set_color_scheme(surface, scheme)
    }

    private func registerForPaneOutput() {
        // Use subscription-based routing so multiple TerminalViews coexist without overwriting each other.
        // Each view filters to only accept output for tabs that belong to its window.
        let capturedWindowId = self.windowId
        let capturedClient = self.client
        let capturedBridge = self.bridge
        subscriptionId = MainActor.assumeIsolated {
            capturedClient.subscribePaneOutput { tabId, bytes in
                let window = capturedClient.renderState?.workspaces
                    .flatMap { $0.groups }
                    .first { $0.id == capturedWindowId }
                guard window?.tabs.contains(where: { $0.id == tabId }) == true else { return }
                capturedBridge.write(bytes: bytes)
            }
        }
    }

    /// Re-register the pane output subscription, e.g. when the windowId changes.
    func updatePaneOutputSubscription() {
        if let id = subscriptionId {
            MainActor.assumeIsolated { client.unsubscribePaneOutput(id) }
            subscriptionId = nil
        }
        registerForPaneOutput()
    }

    // MARK: - View lifecycle

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let surface {
            ghostty_surface_set_focus(surface, window?.isKeyWindow ?? false)
        }
        // Apply the terminal theme to the host window: background color,
        // dark/light appearance, and background blur for translucency.
        if let window {
            GhosttyAppManager.shared.configureWindowAppearance(for: window)
            GhosttyAppManager.shared.applyBackgroundBlur(to: window)
        }
    }

    override func layout() {
        super.layout()
        guard let surface else { return }
        let scaledSize = convertToBacking(bounds.size)
        ghostty_surface_set_size(surface, UInt32(scaledSize.width), UInt32(scaledSize.height))

        // Resize this specific pane's PTY — avoids overwriting other panes' sizes.
        let cols = max(1, UInt16(bounds.width / 8))
        let rows = max(1, UInt16(bounds.height / 16))
        let pw = UInt16(scaledSize.width)
        let ph = UInt16(scaledSize.height)
        let c = self.client
        let wid = self.windowId
        Task { @MainActor in
            guard let window = c.renderState?.workspaces
                .flatMap(\.groups)
                .first(where: { $0.id == wid }),
                  window.activeTab < window.tabs.count else { return }
            let tabId = window.tabs[window.activeTab].id
            try? await c.setPaneSize(tabId: tabId, cols: cols, rows: rows, pixelWidth: pw, pixelHeight: ph)
        }
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
        // Tell the daemon this pane is now focused so input routes here.
        let c = client
        let wid = windowId
        Task { @MainActor in try? await c.focusWindow(id: wid) }
        return true
    }

    override func resignFirstResponder() -> Bool {
        if let surface { ghostty_surface_set_focus(surface, false) }
        return true
    }

    // MARK: - Keyboard events → GhosttyKit native processing

    override func keyDown(with event: NSEvent) {
        guard let surface else {
            interpretKeyEvents([event])
            return
        }

        // Translate modifiers for option-as-alt support
        let translatedMods = ghostty_surface_key_translation_mods(surface, ghosttyMods(event.modifierFlags))
        let translatedNSMods = eventModifierFlags(from: translatedMods)

        // Build the translation event if modifiers changed (for option-as-alt etc.)
        let translationEvent: NSEvent
        if translatedNSMods == event.modifierFlags {
            translationEvent = event
        } else {
            translationEvent = NSEvent.keyEvent(
                with: event.type,
                location: event.locationInWindow,
                modifierFlags: translatedNSMods,
                timestamp: event.timestamp,
                windowNumber: event.windowNumber,
                context: nil,
                characters: event.characters(byApplyingModifiers: translatedNSMods) ?? "",
                charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
                isARepeat: event.isARepeat,
                keyCode: event.keyCode
            ) ?? event
        }

        let action = event.isARepeat ? GHOSTTY_ACTION_REPEAT : GHOSTTY_ACTION_PRESS

        // Enter the interpretKeyEvents cycle — this calls insertText/setMarkedText
        // for IME and composed input.
        let markedTextBefore = markedText.length > 0
        keyTextAccumulator = []
        defer { keyTextAccumulator = nil }

        interpretKeyEvents([translationEvent])

        // Sync preedit state for IME
        syncPreedit(clearIfNeeded: markedTextBefore)

        // If we got composed text, send it through ghostty with the text attached
        if let texts = keyTextAccumulator, !texts.isEmpty {
            for text in texts {
                _ = sendKeyToSurface(event: event, translationEvent: translationEvent, action: action, text: text)
            }
        } else {
            // No composed text — send the raw key event for ghostty to encode
            _ = sendKeyToSurface(event: event, translationEvent: translationEvent, action: action)
        }
    }

    override func keyUp(with event: NSEvent) {
        guard surface != nil else { return }
        _ = sendKeyToSurface(event: event, action: GHOSTTY_ACTION_RELEASE)
    }

    override func flagsChanged(with event: NSEvent) {
        guard surface != nil else { return }

        // Determine which modifier key changed
        let mod: UInt32
        switch event.keyCode {
        case 0x39: mod = GHOSTTY_MODS_CAPS.rawValue
        case 0x38, 0x3C: mod = GHOSTTY_MODS_SHIFT.rawValue
        case 0x3B, 0x3E: mod = GHOSTTY_MODS_CTRL.rawValue
        case 0x3A, 0x3D: mod = GHOSTTY_MODS_ALT.rawValue
        case 0x37, 0x36: mod = GHOSTTY_MODS_SUPER.rawValue
        default: return
        }

        // Don't process modifier changes during IME composition
        if hasMarkedText() { return }

        let mods = ghosttyMods(event.modifierFlags)

        // Determine press vs release, accounting for left/right modifier sides
        var action = GHOSTTY_ACTION_RELEASE
        if mods.rawValue & mod != 0 {
            let sidePressed: Bool
            switch event.keyCode {
            case 0x3C: sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERSHIFTKEYMASK) != 0
            case 0x3E: sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERCTLKEYMASK) != 0
            case 0x3D: sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERALTKEYMASK) != 0
            case 0x36: sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERCMDKEYMASK) != 0
            default: sidePressed = true
            }
            if sidePressed {
                action = GHOSTTY_ACTION_PRESS
            }
        }

        _ = sendKeyToSurface(event: event, action: action)
    }

    /// Build a `ghostty_input_key_s` and send it to the surface.
    @discardableResult
    private func sendKeyToSurface(
        event: NSEvent,
        translationEvent: NSEvent? = nil,
        action: ghostty_input_action_e,
        text: String? = nil,
        composing: Bool = false
    ) -> Bool {
        guard let surface else { return false }

        var key_ev = ghostty_input_key_s()
        key_ev.action = action
        key_ev.keycode = UInt32(event.keyCode)
        key_ev.mods = ghosttyMods(event.modifierFlags)
        key_ev.consumed_mods = ghosttyMods(
            (translationEvent?.modifierFlags ?? event.modifierFlags)
                .subtracting([.control, .command])
        )
        key_ev.composing = composing

        // Unshifted codepoint: the character with no modifiers applied
        key_ev.unshifted_codepoint = 0
        if event.type == .keyDown || event.type == .keyUp {
            if let chars = event.characters(byApplyingModifiers: []),
               let codepoint = chars.unicodeScalars.first {
                key_ev.unshifted_codepoint = codepoint.value
            }
        }

        // For text, only encode UTF-8 if it's not a single control character
        // (ghostty handles control character encoding internally).
        if let text, !text.isEmpty,
           let codepoint = text.utf8.first, codepoint >= 0x20 {
            return text.withCString { ptr in
                key_ev.text = ptr
                return ghostty_surface_key(surface, key_ev)
            }
        } else {
            key_ev.text = nil
            return ghostty_surface_key(surface, key_ev)
        }
    }

    /// Sync ghostty's preedit state with our marked text.
    private func syncPreedit(clearIfNeeded: Bool) {
        guard let surface else { return }
        if markedText.length > 0 {
            let str = markedText.string
            str.withCString { ptr in
                ghostty_surface_preedit(surface, ptr, UInt(str.utf8.count))
            }
        } else if clearIfNeeded {
            ghostty_surface_preedit(surface, nil, 0)
        }
    }

    // MARK: - NSTextInputClient (IME support)

    func insertText(_ string: Any, replacementRange: NSRange) {
        // We're done composing — clear marked text.
        unmarkText()

        let chars: String
        switch string {
        case let s as NSAttributedString: chars = s.string
        case let s as String: chars = s
        default: return
        }

        // If we're inside a keyDown cycle, accumulate text for later processing.
        if keyTextAccumulator != nil {
            keyTextAccumulator?.append(chars)
            return
        }

        // Direct insertText outside keyDown (e.g. from emoji picker) — send as raw input.
        let c = self.client
        let wid = self.windowId
        let data = Data(chars.utf8)
        Task { @MainActor in
            try? await c.focusWindow(id: wid)
            try? await c.rawInput(data)
        }
    }

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        switch string {
        case let s as NSAttributedString:
            markedText = NSMutableAttributedString(attributedString: s)
        case let s as String:
            markedText = NSMutableAttributedString(string: s)
        default:
            markedText = NSMutableAttributedString()
        }
    }

    func unmarkText() {
        markedText.mutableString.setString("")
    }

    func selectedRange() -> NSRange {
        NSRange(location: NSNotFound, length: 0)
    }

    func markedRange() -> NSRange {
        if markedText.length > 0 {
            return NSRange(location: 0, length: markedText.length)
        }
        return NSRange(location: NSNotFound, length: 0)
    }

    func hasMarkedText() -> Bool {
        markedText.length > 0
    }

    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
        nil
    }

    func validAttributedString(for proposedString: NSAttributedString, selectedRange: NSRange, proposedSelectedRange: NSRangePointer?) -> NSAttributedString {
        proposedString
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        // Return a rect near the cursor for IME candidate window positioning.
        guard let window else { return .zero }
        let viewRect = NSRect(x: 0, y: 0, width: bounds.width, height: 20)
        return window.convertToScreen(convert(viewRect, to: nil))
    }

    func characterIndex(for point: NSPoint) -> Int {
        0
    }

    nonisolated func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    // MARK: - Tracking area (required for mouseMoved events)

    override func updateTrackingAreas() {
        trackingAreas.forEach { removeTrackingArea($0) }
        addTrackingArea(NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .mouseMoved, .inVisibleRect, .activeAlways],
            owner: self,
            userInfo: nil
        ))
        super.updateTrackingAreas()
    }

    // MARK: - Mouse events → GhosttyKit native

    /// Convert a window-relative mouse location to ghostty surface coordinates.
    /// Ghostty expects (0,0) at the top-left corner in point units.
    private func surfacePoint(from event: NSEvent) -> CGPoint {
        let pos = convert(event.locationInWindow, from: nil)
        return CGPoint(x: pos.x, y: frame.height - pos.y)
    }

    override func mouseMoved(with event: NSEvent) {
        guard let surface else { return }
        let pos = surfacePoint(from: event)
        let mods = ghosttyMods(event.modifierFlags)
        ghostty_surface_mouse_pos(surface, pos.x, pos.y, mods)
    }

    override func mouseDown(with event: NSEvent) {
        // Claim focus so the daemon knows which pane is active.
        window?.makeFirstResponder(self)
        guard let surface else { return }
        let mods = ghosttyMods(event.modifierFlags)
        ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_LEFT, mods)
    }

    override func mouseUp(with event: NSEvent) {
        guard let surface else { return }
        let mods = ghosttyMods(event.modifierFlags)
        ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_LEFT, mods)
    }

    override func rightMouseDown(with event: NSEvent) {
        guard let surface else { return super.rightMouseDown(with: event) }
        let mods = ghosttyMods(event.modifierFlags)
        if !ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_RIGHT, mods) {
            super.rightMouseDown(with: event)
        }
    }

    override func rightMouseUp(with event: NSEvent) {
        guard let surface else { return super.rightMouseUp(with: event) }
        let mods = ghosttyMods(event.modifierFlags)
        if !ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_RIGHT, mods) {
            super.rightMouseUp(with: event)
        }
    }

    override func mouseDragged(with event: NSEvent) {
        mouseMoved(with: event)
    }

    override func rightMouseDragged(with event: NSEvent) {
        mouseMoved(with: event)
    }

    override func mouseExited(with event: NSEvent) {
        guard let surface else { return }
        ghostty_surface_mouse_pos(surface, -1, -1, GHOSTTY_MODS_NONE)
    }

    override func scrollWheel(with event: NSEvent) {
        // Forward scroll to the daemon instead of Ghostty. The daemon manages
        // scrollback and mouse-mode forwarding; Ghostty's own scrollback is
        // disabled since it only sees the nc relay, not the real PTY.
        let deltaY = event.scrollingDeltaY
        guard abs(deltaY) > 0.5 else { return }
        let up = deltaY > 0
        let c = self.client
        let wid = self.windowId
        Task { @MainActor in
            try? await c.focusWindow(id: wid)
            try? await c.send(.mouseScroll(up: up))
        }
    }

    // MARK: - Modifier conversion helpers

    private func ghosttyMods(_ flags: NSEvent.ModifierFlags) -> ghostty_input_mods_e {
        var mods: UInt32 = GHOSTTY_MODS_NONE.rawValue
        if flags.contains(.shift)    { mods |= GHOSTTY_MODS_SHIFT.rawValue }
        if flags.contains(.control)  { mods |= GHOSTTY_MODS_CTRL.rawValue }
        if flags.contains(.option)   { mods |= GHOSTTY_MODS_ALT.rawValue }
        if flags.contains(.command)  { mods |= GHOSTTY_MODS_SUPER.rawValue }
        if flags.contains(.capsLock) { mods |= GHOSTTY_MODS_CAPS.rawValue }
        return ghostty_input_mods_e(mods)
    }

    private func eventModifierFlags(from mods: ghostty_input_mods_e) -> NSEvent.ModifierFlags {
        var flags = NSEvent.ModifierFlags(rawValue: 0)
        if mods.rawValue & GHOSTTY_MODS_SHIFT.rawValue != 0 { flags.insert(.shift) }
        if mods.rawValue & GHOSTTY_MODS_CTRL.rawValue != 0  { flags.insert(.control) }
        if mods.rawValue & GHOSTTY_MODS_ALT.rawValue != 0   { flags.insert(.option) }
        if mods.rawValue & GHOSTTY_MODS_SUPER.rawValue != 0 { flags.insert(.command) }
        return flags
    }
}
#endif
