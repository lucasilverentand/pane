//! Default key bindings — single source of truth for all modes.
//!
//! Edit bindings here, or override per-user in `~/.config/pane/config.toml`
//! under `[keys]`, `[normal_keys]`, or `[leader_keys]`.

use crate::config::Action;

// ---------------------------------------------------------------------------
// Global keys — active in ALL modes (Normal, Interact)
//
// These bindings work even in Interact mode where keys are forwarded to the
// PTY. Ctrl+Space exits to Normal mode. Use modifier combos (ctrl, alt) to
// avoid conflicts with programs running inside panes.
// ---------------------------------------------------------------------------

pub fn global_defaults() -> Vec<(&'static str, Action)> {
    vec![
        ("ctrl+space", Action::EnterNormal),  // Exit interact mode → normal mode
        ("shift+pageup", Action::ScrollMode), // Enter scroll mode to browse output history
    ]
}

// ---------------------------------------------------------------------------
// Normal mode — vim-style command mode, no PTY passthrough
//
// All keypresses are intercepted; nothing reaches the shell. Press `i` to
// enter Interact mode and type in a pane.
//
// Navigation keys (h/j/k/l, arrows, 1-9) are context-dependent: when the
// workspace bar has focus they navigate workspaces, otherwise they navigate
// panes. This is handled in execute_action via FocusLeft/Right/Up/Down.
// ---------------------------------------------------------------------------

pub fn normal_defaults() -> Vec<(&'static str, Action)> {
    vec![
        // ── Mode ────────────────────────────────────────────────────────
        ("i", Action::EnterInteract),     // Switch to Interact mode (keys go to PTY)
        ("enter", Action::EnterInteract), // Switch to Interact mode (keys go to PTY)
        // ── Navigation ─────────────────────────────────────────────────
        ("h", Action::FocusLeft),      // Focus left (context-dependent)
        ("j", Action::FocusDown),      // Focus down (context-dependent)
        ("k", Action::FocusUp),        // Focus up (context-dependent)
        ("l", Action::FocusRight),     // Focus right (context-dependent)
        ("left", Action::FocusLeft),   // Focus left (arrow key)
        ("down", Action::FocusDown),   // Focus down (arrow key)
        ("up", Action::FocusUp),       // Focus up (arrow key)
        ("right", Action::FocusRight), // Focus right (arrow key)
        // ── Tabs ────────────────────────────────────────────────────────
        ("tab", Action::NextTab),       // Cycle to next tab in the active window
        ("shift+tab", Action::PrevTab), // Cycle to previous tab in the active window
        ("]", Action::NextTab),         // Next tab (bracket)
        ("[", Action::PrevTab),         // Previous tab (bracket)
        ("n", Action::NewTab),          // Open the tab picker to create a new tab
        ("d", Action::CloseTab),        // Close the active tab (context-dependent)
        // ── Splits ──────────────────────────────────────────────────────
        ("s", Action::SplitHorizontal),     // Split the focused pane to the right
        ("v", Action::SplitVertical),       // Split the focused pane to the bottom
        ("shift+s", Action::SplitVertical), // Split the focused pane to the bottom (alias)
        // ── Layout ──────────────────────────────────────────────────────
        ("m", Action::MaximizeFocused), // Maximize the focused pane (hide others)
        ("z", Action::ToggleZoom),      // Toggle zoom on the focused pane
        ("f", Action::ToggleFold),      // Fold/unfold the focused split
        ("shift+f", Action::NewFloat),  // Create a new floating pane
        ("=", Action::Equalize),        // Reset all panes to equal sizes
        // ── Resize ──────────────────────────────────────────────────────
        ("shift+h", Action::ResizeShrinkH), // Shrink focused pane horizontally
        ("shift+j", Action::ResizeGrowV),   // Grow focused pane vertically
        ("shift+k", Action::ResizeShrinkV), // Shrink focused pane vertically
        ("shift+l", Action::ResizeGrowH),   // Grow focused pane horizontally
        // ── Move tabs ───────────────────────────────────────────────────
        ("alt+h", Action::MoveTabLeft),  // Move active tab to the window on the left
        ("alt+j", Action::MoveTabDown),  // Move active tab to the window below
        ("alt+k", Action::MoveTabUp),    // Move active tab to the window above
        ("alt+l", Action::MoveTabRight), // Move active tab to the window on the right
        // ── Tools ───────────────────────────────────────────────────────
        ("c", Action::CopyMode),         // Enter copy mode to select and copy text
        ("p", Action::PasteClipboard),   // Paste from system clipboard
        (":", Action::CommandPalette),   // Open the command palette
        // ── Session ─────────────────────────────────────────────────────
        ("q", Action::Quit),             // Quit (detach from the session)
    ]
}

