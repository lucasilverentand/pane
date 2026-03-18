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
        ActionMeta {
            name: "resize_mode",
            display_name: "Resize Mode",
            description: "Enter interactive resize mode to select and move borders",
            category: Resize,
            palette_visible: true,
            action: ResizeMode,
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
        ActionMeta {
            name: "project_hub",
            display_name: "Project Hub",
            description: "Browse and open project repositories",
            category: Workspaces,
            palette_visible: true,
            action: ProjectHub,
        },
        // ── Session ─────────────────────────────────────────────────────
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
            name: "reload_config",
            display_name: "Reload Config",
            description: "Reload configuration from disk",
            category: Tools,
            palette_visible: true,
            action: ReloadConfig,
        },
        ActionMeta {
            name: "change_widget",
            display_name: "Change Widget",
            description: "Replace the focused widget with a different one",
            category: Tools,
            palette_visible: true,
            action: ChangeWidget,
        },
        ActionMeta {
            name: "add_widget_right",
            display_name: "Add Widget to the Right",
            description: "Split the focused widget right and pick a new widget",
            category: Tools,
            palette_visible: true,
            action: AddWidgetRight,
        },
        ActionMeta {
            name: "add_widget_below",
            display_name: "Add Widget Below",
            description: "Split the focused widget down and pick a new widget",
            category: Tools,
            palette_visible: true,
            action: AddWidgetBelow,
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

    // --- Every action has a non-empty display_name ---

    #[test]
    fn every_action_has_display_name() {
        for meta in action_registry() {
            assert!(
                !meta.display_name.is_empty(),
                "Action '{}' has empty display_name",
                meta.name
            );
        }
    }

    #[test]
    fn every_action_has_description() {
        for meta in action_registry() {
            assert!(
                !meta.description.is_empty(),
                "Action '{}' has empty description",
                meta.name
            );
        }
    }

    #[test]
    fn every_action_has_non_empty_name() {
        for meta in action_registry() {
            assert!(!meta.name.is_empty(), "Found action with empty name");
        }
    }

    // --- Category assignment for specific actions ---

    #[test]
    fn split_actions_in_splits_category() {
        let split_h = action_by_name("split_horizontal").unwrap();
        assert_eq!(split_h.category, ActionCategory::Splits);

        let split_v = action_by_name("split_vertical").unwrap();
        assert_eq!(split_v.category, ActionCategory::Splits);
    }

    #[test]
    fn navigation_actions_in_navigation_category() {
        for name in ["focus_left", "focus_down", "focus_up", "focus_right"] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Navigation,
                "{} should be in Navigation category",
                name
            );
        }
    }

    #[test]
    fn tab_actions_in_tabs_category() {
        for name in [
            "new_tab",
            "next_tab",
            "prev_tab",
            "close_tab",
            "move_tab_left",
            "move_tab_down",
            "move_tab_up",
            "move_tab_right",
        ] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Tabs,
                "{} should be in Tabs category",
                name
            );
        }
    }

    #[test]
    fn resize_actions_in_resize_category() {
        for name in [
            "resize_shrink_h",
            "resize_grow_h",
            "resize_grow_v",
            "resize_shrink_v",
            "equalize",
            "resize_mode",
        ] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Resize,
                "{} should be in Resize category",
                name
            );
        }
    }

    #[test]
    fn layout_actions_in_layout_category() {
        for name in [
            "maximize_focused",
            "toggle_zoom",
            "toggle_float",
            "new_float",
            "toggle_fold",
        ] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Layout,
                "{} should be in Layout category",
                name
            );
        }
    }

    #[test]
    fn workspace_actions_in_workspaces_category() {
        for name in ["new_workspace", "close_workspace", "rename_workspace"] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Workspaces,
                "{} should be in Workspaces category",
                name
            );
        }
    }

    #[test]
    fn session_actions_in_session_category() {
        for name in ["detach", "quit"] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Session,
                "{} should be in Session category",
                name
            );
        }
    }

    #[test]
    fn tools_actions_in_tools_category() {
        for name in [
            "command_palette",
            "help",
            "scroll_mode",
            "copy_mode",
            "paste_clipboard",
            "toggle_sync_panes",
            "rename_window",
            "reload_config",
        ] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Tools,
                "{} should be in Tools category",
                name
            );
        }
    }

    // --- Parameterized actions excluded from registry ---

    #[test]
    fn parameterized_actions_not_in_registry() {
        // FocusGroupN, SwitchWorkspace, SelectLayout are parameterized
        // and should NOT appear in the registry
        let registry = action_registry();
        for meta in registry {
            assert!(
                !matches!(meta.action, Action::FocusGroupN(_)),
                "FocusGroupN should not be in registry"
            );
            assert!(
                !matches!(meta.action, Action::SwitchWorkspace(_)),
                "SwitchWorkspace should not be in registry"
            );
            assert!(
                !matches!(meta.action, Action::SelectLayout(_)),
                "SelectLayout should not be in registry"
            );
        }
    }

    // --- display_name_for coverage ---

    #[test]
    fn display_name_for_select_layout() {
        assert_eq!(
            display_name_for(&Action::SelectLayout("grid".to_string())),
            "Select Layout"
        );
    }

    #[test]
    fn display_name_for_all_registry_actions() {
        for meta in action_registry() {
            let name = display_name_for(&meta.action);
            assert_eq!(
                name, meta.display_name,
                "display_name_for mismatch for {}",
                meta.name
            );
        }
    }

    // --- action_by_name returns None for unknown ---

    #[test]
    fn action_by_name_unknown_returns_none() {
        assert!(action_by_name("nonexistent_action").is_none());
        assert!(action_by_name("").is_none());
    }

    // --- ActionCategory ---

    #[test]
    fn action_category_labels_are_non_empty() {
        let categories = [
            ActionCategory::Mode,
            ActionCategory::Navigation,
            ActionCategory::Tabs,
            ActionCategory::Splits,
            ActionCategory::Resize,
            ActionCategory::Layout,
            ActionCategory::Workspaces,
            ActionCategory::Session,
            ActionCategory::Tools,
        ];
        for cat in categories {
            assert!(!cat.label().is_empty(), "{:?} has empty label", cat);
        }
    }

    #[test]
    fn action_category_sort_keys_are_unique() {
        let categories = [
            ActionCategory::Mode,
            ActionCategory::Navigation,
            ActionCategory::Tabs,
            ActionCategory::Splits,
            ActionCategory::Resize,
            ActionCategory::Layout,
            ActionCategory::Workspaces,
            ActionCategory::Session,
            ActionCategory::Tools,
        ];
        let mut keys: Vec<u8> = categories.iter().map(|c| c.sort_key()).collect();
        let len_before = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(
            keys.len(),
            len_before,
            "Sort keys should be unique across categories"
        );
    }

    #[test]
    fn actions_by_category_groups_are_ordered() {
        let groups = actions_by_category();
        // Check that Mode comes before Navigation, etc.
        let category_order: Vec<ActionCategory> = groups.iter().map(|(cat, _)| *cat).collect();
        for i in 0..category_order.len() - 1 {
            assert!(
                category_order[i].sort_key() < category_order[i + 1].sort_key(),
                "{:?} should come before {:?}",
                category_order[i],
                category_order[i + 1]
            );
        }
    }

    #[test]
    fn mode_actions_in_mode_category() {
        for name in ["enter_interact", "enter_normal"] {
            let meta = action_by_name(name).unwrap();
            assert_eq!(
                meta.category,
                ActionCategory::Mode,
                "{} should be in Mode category",
                name
            );
        }
    }

    // --- Palette visibility ---

    #[test]
    fn all_current_actions_are_palette_visible() {
        // The existing test checks count, let's also verify the flag directly
        for meta in action_registry() {
            assert!(
                meta.palette_visible,
                "Action '{}' should be palette-visible",
                meta.name
            );
        }
    }

    #[test]
    fn palette_actions_returns_same_as_registry_when_all_visible() {
        let palette: Vec<&ActionMeta> = palette_actions().collect();
        let registry = action_registry();
        assert_eq!(palette.len(), registry.len());
        for (p, r) in palette.iter().zip(registry.iter()) {
            assert_eq!(p.name, r.name);
        }
    }
}
