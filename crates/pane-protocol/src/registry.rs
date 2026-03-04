//! Central action registry — single source of truth for action metadata.
//!
//! Every bindable action is registered here with its config name, display name,
//! description, category, and palette visibility. The help screen, command
//! palette, and config parser all derive from this registry.

use crate::config::Action;

/// Metadata for a single action.
pub struct ActionMeta {
    /// TOML config name, e.g. `"split_horizontal"`.
    pub name: &'static str,
    /// Human-readable name for UI display, e.g. `"Split Right"`.
    pub display_name: &'static str,
    /// One-line description for the command palette.
    pub description: &'static str,
    /// Category for help screen grouping.
    pub category: ActionCategory,
    /// Whether this action appears in the command palette.
    pub palette_visible: bool,
    /// Canonical action value.
    pub action: Action,
}

/// Category for grouping actions in the help screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionCategory {
    Mode,
    Navigation,
    Tabs,
    Splits,
    Resize,
    Layout,
    Workspaces,
    Session,
    Tools,
}

impl ActionCategory {
    /// Display label used as section heading.
    pub fn label(self) -> &'static str {
        match self {
            Self::Mode => "Mode",
            Self::Navigation => "Navigation",
            Self::Tabs => "Tabs",
            Self::Splits => "Splits",
            Self::Resize => "Resizing",
            Self::Layout => "Layout",
            Self::Workspaces => "Workspaces",
            Self::Session => "Session",
            Self::Tools => "Tools",
        }
    }

    /// Ordering for display — matches the order we want in the help screen.
    fn sort_key(self) -> u8 {
        match self {
            Self::Mode => 0,
            Self::Navigation => 1,
            Self::Tabs => 2,
            Self::Splits => 3,
            Self::Resize => 4,
            Self::Layout => 5,
            Self::Workspaces => 6,
            Self::Session => 7,
            Self::Tools => 8,
        }
    }
}

/// All registered actions. This is the single source of truth.
pub fn action_registry() -> &'static [ActionMeta] {
    use Action::*;
    use ActionCategory::*;

    static REGISTRY: &[ActionMeta] = &[
        // ── Mode ────────────────────────────────────────────────────────
        ActionMeta {
            name: "enter_interact",
            display_name: "Enter Interact Mode",
            description: "Switch to interact mode (forward keys to PTY)",
            category: Mode,
            palette_visible: true,
            action: EnterInteract,
        },
        ActionMeta {
            name: "enter_normal",
            display_name: "Enter Normal Mode",
            description: "Switch to normal mode (vim-style navigation)",
            category: Mode,
            palette_visible: true,
            action: EnterNormal,
        },
        // ── Navigation ──────────────────────────────────────────────────
        ActionMeta {
            name: "focus_left",
            display_name: "Focus Left",
            description: "Move focus to the left window",
            category: Navigation,
            palette_visible: true,
            action: FocusLeft,
        },
        ActionMeta {
            name: "focus_down",
            display_name: "Focus Down",
            description: "Move focus to the window below",
            category: Navigation,
            palette_visible: true,
            action: FocusDown,
        },
        ActionMeta {
            name: "focus_up",
            display_name: "Focus Up",
            description: "Move focus to the window above",
            category: Navigation,
            palette_visible: true,
            action: FocusUp,
        },
        ActionMeta {
            name: "focus_right",
            display_name: "Focus Right",
            description: "Move focus to the right window",
            category: Navigation,
            palette_visible: true,
            action: FocusRight,
        },
        // ── Tabs ────────────────────────────────────────────────────────
        ActionMeta {
            name: "new_tab",
            display_name: "New Tab",
            description: "Open a new tab in the current window",
            category: Tabs,
            palette_visible: true,
            action: NewTab,
        },
        ActionMeta {
            name: "dev_server_input",
            display_name: "New Dev Server Tab",
            description: "Open a dev server tab",
            category: Tabs,
            palette_visible: true,
            action: DevServerInput,
        },
        ActionMeta {
            name: "next_tab",
            display_name: "Next Tab",
            description: "Switch to the next tab",
            category: Tabs,
            palette_visible: true,
            action: NextTab,
        },
        ActionMeta {
            name: "prev_tab",
            display_name: "Previous Tab",
            description: "Switch to the previous tab",
            category: Tabs,
            palette_visible: true,
            action: PrevTab,
        },
        ActionMeta {
            name: "close_tab",
            display_name: "Close Tab",
            description: "Close the current tab",
            category: Tabs,
            palette_visible: true,
            action: CloseTab,
        },
        ActionMeta {
            name: "move_tab_left",
            display_name: "Move Tab Left",
            description: "Move the current tab to the left window",
            category: Tabs,
            palette_visible: true,
            action: MoveTabLeft,
        },
        ActionMeta {
            name: "move_tab_down",
            display_name: "Move Tab Down",
            description: "Move the current tab to the window below",
            category: Tabs,
            palette_visible: true,
            action: MoveTabDown,
        },
        ActionMeta {
            name: "move_tab_up",
            display_name: "Move Tab Up",
            description: "Move the current tab to the window above",
            category: Tabs,
            palette_visible: true,
            action: MoveTabUp,
        },
        ActionMeta {
            name: "move_tab_right",
            display_name: "Move Tab Right",
            description: "Move the current tab to the right window",
            category: Tabs,
            palette_visible: true,
            action: MoveTabRight,
        },
        // ── Splits ──────────────────────────────────────────────────────
        ActionMeta {
            name: "split_horizontal",
            display_name: "Split Right",
            description: "Split the focused window horizontally",
            category: Splits,
            palette_visible: true,
            action: SplitHorizontal,
        },
        ActionMeta {
            name: "split_vertical",
            display_name: "Split Down",
            description: "Split the focused window vertically",
            category: Splits,
            palette_visible: true,
            action: SplitVertical,
        },
        ActionMeta {
            name: "new_pane",
            display_name: "New Pane",
            description: "Open directional pane split picker",
            category: Splits,
            palette_visible: true,
            action: NewPane,
        },
        ActionMeta {
            name: "restart_pane",
            display_name: "Restart Pane",
            description: "Restart the exited pane process",
            category: Splits,
            palette_visible: true,
            action: RestartPane,
        },
        // ── Resize ──────────────────────────────────────────────────────
        ActionMeta {
            name: "resize_shrink_h",
            display_name: "Shrink Horizontally",
            description: "Decrease the focused window width",
            category: Resize,
            palette_visible: true,
            action: ResizeShrinkH,
        },
        ActionMeta {
            name: "resize_grow_h",
            display_name: "Grow Horizontally",
            description: "Increase the focused window width",
            category: Resize,
            palette_visible: true,
            action: ResizeGrowH,
        },
        ActionMeta {
            name: "resize_grow_v",
            display_name: "Grow Vertically",
            description: "Increase the focused window height",
            category: Resize,
            palette_visible: true,
            action: ResizeGrowV,
        },
        ActionMeta {
            name: "resize_shrink_v",
            display_name: "Shrink Vertically",
            description: "Decrease the focused window height",
            category: Resize,
            palette_visible: true,
            action: ResizeShrinkV,
        },
        ActionMeta {
            name: "equalize",
            display_name: "Equalize Panes",
            description: "Reset all split ratios to equal",
            category: Resize,
            palette_visible: true,
            action: Equalize,
        },
        // ── Layout ──────────────────────────────────────────────────────
        ActionMeta {
            name: "maximize_focused",
            display_name: "Maximize Focused",
            description: "Toggle maximize the focused window",
            category: Layout,
            palette_visible: true,
            action: MaximizeFocused,
        },
        ActionMeta {
            name: "toggle_zoom",
            display_name: "Toggle Zoom",
            description: "Toggle full-screen zoom on the focused window",
            category: Layout,
            palette_visible: true,
            action: ToggleZoom,
        },
        ActionMeta {
            name: "toggle_float",
            display_name: "Toggle Float",
            description: "Toggle floating mode for the focused window",
            category: Layout,
            palette_visible: true,
            action: ToggleFloat,
        },
        ActionMeta {
            name: "new_float",
            display_name: "New Float",
            description: "Create a new floating window",
            category: Layout,
            palette_visible: true,
            action: NewFloat,
        },
        ActionMeta {
            name: "toggle_fold",
            display_name: "Toggle Fold",
            description: "Fold or unfold the focused split",
            category: Layout,
            palette_visible: true,
            action: ToggleFold,
        },
        // ── Workspaces ──────────────────────────────────────────────────
        ActionMeta {
            name: "new_workspace",
            display_name: "New Workspace",
            description: "Create a new workspace",
            category: Workspaces,
            palette_visible: true,
            action: NewWorkspace,
        },
        ActionMeta {
            name: "close_workspace",
            display_name: "Close Workspace",
            description: "Close the current workspace",
            category: Workspaces,
            palette_visible: true,
            action: CloseWorkspace,
        },
        // ── Session ─────────────────────────────────────────────────────
        ActionMeta {
            name: "session_picker",
            display_name: "Session Picker",
            description: "Open the session picker",
            category: Session,
            palette_visible: true,
            action: SessionPicker,
        },
        ActionMeta {
            name: "client_picker",
            display_name: "Client Picker",
            description: "Show connected clients",
            category: Session,
            palette_visible: true,
            action: ClientPicker,
        },
        ActionMeta {
            name: "detach",
            display_name: "Detach",
            description: "Detach from the session",
            category: Session,
            palette_visible: true,
            action: Detach,
        },
        ActionMeta {
            name: "quit",
            display_name: "Quit",
            description: "Exit pane and close the session",
            category: Session,
            palette_visible: true,
            action: Quit,
        },
        // ── Tools ───────────────────────────────────────────────────────
        ActionMeta {
            name: "command_palette",
            display_name: "Command Palette",
            description: "Open the command palette",
            category: Tools,
            palette_visible: true,
            action: CommandPalette,
        },
        ActionMeta {
            name: "help",
            display_name: "Help",
            description: "Show keybinding help",
            category: Tools,
            palette_visible: true,
            action: Help,
        },
        ActionMeta {
            name: "scroll_mode",
            display_name: "Scroll Mode",
            description: "Enter scroll mode for the focused pane",
            category: Tools,
            palette_visible: true,
            action: ScrollMode,
        },
        ActionMeta {
            name: "copy_mode",
            display_name: "Copy Mode",
            description: "Enter copy mode with vim-style selection",
            category: Tools,
            palette_visible: true,
            action: CopyMode,
        },
        ActionMeta {
            name: "paste_clipboard",
            display_name: "Paste from Clipboard",
            description: "Paste system clipboard into the focused pane",
            category: Tools,
            palette_visible: true,
            action: PasteClipboard,
        },
        ActionMeta {
            name: "toggle_sync_panes",
            display_name: "Toggle Sync Panes",
            description: "Broadcast input to all panes in workspace",
            category: Tools,
            palette_visible: true,
            action: ToggleSyncPanes,
        },
        ActionMeta {
            name: "rename_window",
            display_name: "Rename Window",
            description: "Rename the current window",
            category: Tools,
            palette_visible: true,
            action: RenameWindow,
        },
        ActionMeta {
            name: "rename_workspace",
            display_name: "Rename Workspace",
            description: "Rename the current workspace",
            category: Workspaces,
            palette_visible: true,
            action: RenameWorkspace,
        },
        ActionMeta {
            name: "rename_pane",
            display_name: "Rename Pane",
            description: "Rename the current pane",
            category: Tools,
            palette_visible: true,
            action: RenamePane,
        },
    ];

    REGISTRY
}

/// Look up an action by its TOML config name.
pub fn action_by_name(name: &str) -> Option<&'static ActionMeta> {
    action_registry().iter().find(|m| m.name == name)
}

/// Iterate over actions visible in the command palette.
pub fn palette_actions() -> impl Iterator<Item = &'static ActionMeta> {
    action_registry().iter().filter(|m| m.palette_visible)
}

/// Group actions by category for the help screen.
pub fn actions_by_category() -> Vec<(ActionCategory, Vec<&'static ActionMeta>)> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<u8, (ActionCategory, Vec<&'static ActionMeta>)> = BTreeMap::new();
    for meta in action_registry() {
        let key = meta.category.sort_key();
        groups
            .entry(key)
            .or_insert_with(|| (meta.category, Vec::new()))
            .1
            .push(meta);
    }
    groups.into_values().collect()
}

/// Get the display name for an action (handles parameterized variants).
pub fn display_name_for(action: &Action) -> &'static str {
    match action {
        Action::FocusGroupN(_) => "Focus Group N",
        Action::SwitchWorkspace(_) => "Switch Workspace",
        Action::SelectLayout(_) => "Select Layout",
        _ => {
            // Look up in registry by matching action variant
            action_registry()
                .iter()
                .find(|m| m.action == *action)
                .map(|m| m.display_name)
                .unwrap_or("Unknown")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_all_non_parameterized_actions() {
        // Spot-check key actions are present
        assert!(action_by_name("quit").is_some());
        assert!(action_by_name("split_horizontal").is_some());
        assert!(action_by_name("enter_interact").is_some());
        assert!(action_by_name("toggle_fold").is_some());
    }

    #[test]
    fn action_by_name_returns_correct_action() {
        let meta = action_by_name("split_horizontal").unwrap();
        assert_eq!(meta.action, Action::SplitHorizontal);
        assert_eq!(meta.display_name, "Split Right");
    }

    #[test]
    fn palette_actions_excludes_nothing_by_default() {
        // All current actions are palette-visible
        let count = palette_actions().count();
        assert_eq!(count, action_registry().len());
    }

    #[test]
    fn actions_by_category_covers_all() {
        let groups = actions_by_category();
        let total: usize = groups.iter().map(|(_, v)| v.len()).sum();
        assert_eq!(total, action_registry().len());
    }

    #[test]
    fn display_name_for_parameterized() {
        assert_eq!(display_name_for(&Action::FocusGroupN(3)), "Focus Group N");
        assert_eq!(
            display_name_for(&Action::SwitchWorkspace(1)),
            "Switch Workspace"
        );
    }

    #[test]
    fn display_name_for_normal_action() {
        assert_eq!(display_name_for(&Action::Quit), "Quit");
        assert_eq!(display_name_for(&Action::SplitHorizontal), "Split Right");
    }

    #[test]
    fn no_duplicate_names() {
        let registry = action_registry();
        let mut seen = std::collections::HashSet::new();
        for meta in registry {
            assert!(
                seen.insert(meta.name),
                "duplicate registry name: {}",
                meta.name
            );
        }
    }
}
