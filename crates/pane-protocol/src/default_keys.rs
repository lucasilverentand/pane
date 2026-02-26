//! Default key bindings — single source of truth for all modes.
//!
//! Edit bindings here, or override per-user in `~/.config/pane/config.toml`
//! under `[keys]`, `[normal_keys]`, or `[leader_keys]`.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{parse_key, Action, LeaderNode};

// ---------------------------------------------------------------------------
// Global keys — active in ALL modes (Normal, Interact)
//
// These bindings work even in Interact mode where keys are forwarded to the
// PTY. Use modifier combos (ctrl, alt) to avoid conflicts with programs
// running inside panes.
// ---------------------------------------------------------------------------

pub fn global_defaults() -> Vec<(&'static str, Action)> {
    vec![
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
        ("i", Action::EnterInteract), // Switch to Interact mode (keys go to PTY)
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
        ("shift+l", Action::NextTab),   // Next tab (shift+l)
        ("shift+h", Action::PrevTab),   // Previous tab (shift+h)
        ("n", Action::NewTab),          // Open the tab picker to create a new tab
        ("d", Action::CloseTab),        // Close the active tab (context-dependent)
        // ── Splits ──────────────────────────────────────────────────────
        ("s", Action::SplitHorizontal),     // Split the focused pane to the right
        ("shift+s", Action::SplitVertical), // Split the focused pane to the bottom
        // ── Layout ──────────────────────────────────────────────────────
        ("m", Action::MaximizeFocused), // Maximize the focused pane (hide others)
        ("z", Action::ToggleZoom),      // Toggle zoom on the focused pane
        ("f", Action::ToggleFold),      // Fold/unfold the focused split
        ("shift+f", Action::NewFloat),  // Create a new floating pane
        ("=", Action::Equalize),        // Reset all panes to equal sizes
        // ── Resize ──────────────────────────────────────────────────────
        ("r", Action::ResizeMode),          // Enter resize mode (hjkl to resize)
        ("shift+j", Action::ResizeGrowV),   // Grow focused pane vertically
        ("shift+k", Action::ResizeShrinkV), // Shrink focused pane vertically
        // ── Move tabs ───────────────────────────────────────────────────
        ("alt+h", Action::MoveTabLeft),  // Move active tab to the window on the left
        ("alt+j", Action::MoveTabDown),  // Move active tab to the window below
        ("alt+k", Action::MoveTabUp),    // Move active tab to the window above
        ("alt+l", Action::MoveTabRight), // Move active tab to the window on the right
        // ── Tools ───────────────────────────────────────────────────────
        ("c", Action::CopyMode),       // Enter copy mode to select and copy text
        ("p", Action::NewPane),        // Open directional pane split picker
        ("/", Action::CommandPalette), // Open the command palette
        ("?", Action::CommandPalette), // Open the command palette (alias)
        ("q", Action::CloseTab),       // Close the active tab (quit pane, server keeps running)
    ]
}

// ---------------------------------------------------------------------------
// Leader key tree — activated by pressing the leader key (default: space)
//
// After pressing the leader key, a popup shows available sub-keys. Each
// group is a second keypress that opens a category, and a third keypress
// executes the action. For example: space → w → d = split horizontal.
//
// Root-level shortcuts (space d, space y, etc.) skip the group step for
// frequently used actions.
// ---------------------------------------------------------------------------

pub fn default_leader_tree() -> LeaderNode {
    let mut root = HashMap::new();

    // ── Window group (space w) ──────────────────────────────────────────
    // Manage pane focus, splits, and window layout.
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "h", Action::FocusLeft, "Focus Left");
        insert_leaf(&mut children, "j", Action::FocusDown, "Focus Down");
        insert_leaf(&mut children, "k", Action::FocusUp, "Focus Up");
        insert_leaf(&mut children, "l", Action::FocusRight, "Focus Right");
        for n in 1..=9u8 {
            let ch = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            children.insert(
                key,
                LeaderNode::Leaf {
                    action: Action::FocusGroupN(n),
                    label: format!("Focus {}", n),
                },
            );
        }
        insert_leaf(&mut children, "d", Action::SplitHorizontal, "Split H");
        insert_leaf(&mut children, "shift+d", Action::SplitVertical, "Split V");
        insert_leaf(&mut children, "c", Action::CloseTab, "Close");
        insert_leaf(&mut children, "=", Action::Equalize, "Equalize");
        insert_leaf(&mut children, "r", Action::RestartPane, "Restart");
        insert_leaf(&mut children, "m", Action::MaximizeFocused, "Maximize");
        insert_leaf(&mut children, "z", Action::ToggleZoom, "Zoom");
        insert_leaf(&mut children, "f", Action::ToggleFloat, "Float");
        insert_leaf(&mut children, "F", Action::NewFloat, "New Float");
        let key = parse_key("w").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Window".into(),
                children,
            },
        );
    }

    // ── Tab group (space t) ─────────────────────────────────────────────
    // Create, close, cycle, and move tabs between windows.
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "n", Action::NewTab, "New Tab");
        insert_leaf(&mut children, "c", Action::CloseTab, "Close Tab");
        insert_leaf(&mut children, "]", Action::NextTab, "Next Tab");
        insert_leaf(&mut children, "[", Action::PrevTab, "Prev Tab");
        insert_leaf(&mut children, "h", Action::MoveTabLeft, "Move Left");
        insert_leaf(&mut children, "j", Action::MoveTabDown, "Move Down");
        insert_leaf(&mut children, "k", Action::MoveTabUp, "Move Up");
        insert_leaf(&mut children, "l", Action::MoveTabRight, "Move Right");
        let key = parse_key("t").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Tab".into(),
                children,
            },
        );
    }

    // ── Session group (space s) ─────────────────────────────────────────
    // Open the command palette or detach.
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "p", Action::CommandPalette, "Palette");
        insert_leaf(&mut children, "d", Action::Detach, "Detach");
        insert_leaf(&mut children, "c", Action::ClientPicker, "Clients");
        let key = parse_key("s").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Session".into(),
                children,
            },
        );
    }

    // ── Workspace group (space W) ───────────────────────────────────────
    // Create, close, and switch between workspaces.
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "n", Action::NewWorkspace, "New");
        insert_leaf(&mut children, "c", Action::CloseWorkspace, "Close");
        for n in 1..=9u8 {
            let ch = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            children.insert(
                key,
                LeaderNode::Leaf {
                    action: Action::SwitchWorkspace(n),
                    label: format!("Switch {}", n),
                },
            );
        }
        let key = parse_key("shift+w").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Workspace".into(),
                children,
            },
        );
    }

    // ── Resize mode (space r) ────────────────────────────────────────────
    // Enter sticky resize mode (hjkl to resize, esc to exit).
    insert_leaf(&mut root, "r", Action::ResizeMode, "Resize");

    // ── Root-level shortcuts ────────────────────────────────────────────
    // Two-key shortcuts that skip the group step for common actions.
    insert_leaf(&mut root, "d", Action::SplitHorizontal, "Split H"); // Quick horizontal split
    insert_leaf(&mut root, "shift+d", Action::SplitVertical, "Split V"); // Quick vertical split
    insert_leaf(&mut root, "y", Action::PasteClipboard, "Paste"); // Paste from clipboard
    insert_leaf(&mut root, "/", Action::CommandPalette, "Command Palette"); // Open command palette

    // space space → send a literal space to the PTY (passthrough)
    let space_key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
    root.insert(space_key, LeaderNode::PassThrough);

    LeaderNode::Group {
        label: "Leader".into(),
        children: root,
    }
}

fn insert_leaf(
    map: &mut HashMap<KeyEvent, LeaderNode>,
    key_str: &str,
    action: Action,
    label: &str,
) {
    if let Some(key) = parse_key(key_str) {
        map.insert(
            key,
            LeaderNode::Leaf {
                action,
                label: label.to_string(),
            },
        );
    }
}
