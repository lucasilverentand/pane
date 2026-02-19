# Tasks — ARCHITECTURE.md Gap Analysis

## Legend

- **Done** = implemented and tested

---

## 1. Naming Rename: PaneGroup → Window, Pane → Tab

**Status: Done**

Renamed all types, modules, and methods:
- `PaneGroup→Window`, `PaneGroupId→WindowId`, `Pane→Tab`, `PaneId→TabId`, `PaneKind→TabKind`
- `PaneGroupConfig→WindowConfig`, `PaneConfig→TabConfig`
- `GroupSnapshot→WindowSnapshot`, `PaneSnapshot→TabSnapshot`
- `src/pane/`→`src/window/`, `ui/pane_view.rs`→`ui/window_view.rs`
- Methods: `active_pane→active_tab`, `find_pane_mut→find_tab_mut`, etc.

---

## 2. Per-Client Independent Workspace Views

**Status: Done**

ClientRegistry stores `ClientInfo { width, height, active_workspace }` per client. Before executing commands/keys, the server sets `state.active_workspace` to the calling client's value and syncs it back after execution. `RenderState::for_client()` generates per-client render state.

---

## 3. Server Shutdown on Last Workspace Close

**Status: Done**

`close_workspace()` returns `true` when the last workspace is closed. The command handler broadcasts `SessionEnded` to all clients, triggering graceful shutdown.

---

## 4. Modal System: Normal + Interact Modes

**Status: Done**

Strict vim-style modal system:
- `Mode::Interact`: forwards ALL keys to PTY except Escape (→ Normal), leader key, and global keymap
- `Mode::Normal`: intercepts ALL keys — hjkl focus, d/D split, x close, n new tab, m maximize, z zoom, f float, s scroll, c copy, p paste, / palette, ? help, = equalize, H/J/K/L resize, Tab/BackTab cycle, 1-9 focus group, i → Interact
- New sessions start in Interact mode
- Status bar shows `[NORMAL]` / `[INTERACT]` / `[SCROLL]` etc.

---

## 5. Mode-Dependent Border Colors

**Status: Done**

Theme fields: `border_normal` (Cyan), `border_interact` (Green), `border_scroll` (Yellow). Active window border color changes based on current mode. Decoration overrides still take priority.

---

## 6. Zoom Mode

**Status: Done**

`zoomed_window: Option<WindowId>` on Workspace and WorkspaceSnapshot. `Command::ToggleZoom` toggles zoom on active window. When zoomed, renders only the zoomed window filling the body area. Auto-unzoom on toggle. Bound to `z` in Normal mode and `\w z` in leader.

---

## 7. Floating Windows

**Status: Done**

`FloatingWindow { id, x, y, width, height }` on Workspace. Commands: `ToggleFloat` (moves window between tiled/floating), `NewFloat` (new floating shell). Floating windows render as overlays above tiled layout with `Clear` + bordered block. Bound to `f`/`F` in Normal mode and `\w f`/`\w F` in leader.

---

## 8. Fuzzy-Search Picker for New Tab

**Status: Done**

`Mode::TabPicker` with `TabPickerState` centered popup overlay. Detects available shells (bash, zsh, fish) and tools (htop, btop, python, node). Fuzzy filtering by name/description/command. `Action::NewTab` opens picker instead of directly spawning. Enter spawns selected entry via `new-window -c <cmd>`, Esc cancels.

---

## 9. Workspace Auto-Naming from Git / CWD

**Status: Done**

`auto_workspace_name()` tries git repo basename → cwd basename → incremental number. Used by both `new_session()` and `new_workspace()`.

---

## 10. Workspace Bar Conditional Display

**Status: Done**

`workspace_bar_height()` returns 0 when `workspaces.len() <= 1`. UI layout conditionally includes/excludes the header row. PTY resize calculations account for dynamic header height.

---

## 11. Tab Bar Conditional Display

**Status: Done**

Tab bar is only rendered when a window has >1 tab. `resize_all_tabs` subtracts 2 or 3 rows based on tab count. Cursor positioning also adjusts for the conditional tab bar.

---

## 12. Mouse Drag Resize (Server-Side)

**Status: Done**

`DragState { split_path, direction, body }` on ServerState. MouseDown hit-tests split borders via `layout.hit_test_split_border()`. MouseDrag computes new ratio from position, clamped [0.05, 0.95], updates layout via `set_ratio_at_path()`. MouseUp clears drag state, updates leaf mins, resizes tabs, broadcasts LayoutChanged.

---

## 13. Configurable Outer Terminal Title

**Status: Done**

`terminal_title_format` in Behavior config (default `"{session} - {workspace}"`). Client emits OSC 0 escape sequence (`\x1b]0;{title}\x07`) via `update_terminal_title()` after every `LayoutChanged`.

---

## 14. Informative Borders (Extended)

**Status: Done**

Top border shows tab count `[2/3]` when window has >1 tab. Bottom border shows mode indicator: NORMAL, INTERACT, SCROLL, COPY, SELECT, ACTIVE.

---

## 15. Command Palette Descriptions

**Status: Done**

Replaced tuple with `CommandEntry { action, name, keybind, description }` struct. All actions have descriptions. `filter_commands()` searches both name and description. Selected entry shows description as dimmed text below the entry name.

---

## 16. Plugin System

**Status: Done**

`[[plugins]]` config with `command`, `events`, `refresh_interval_secs`. PluginManager spawns child processes with JSON stdin/stdout protocol:
- Input: `{"event":"...","workspace":"...","system_stats":{...}}`
- Output: `{"segments":[{"text":"...","style":"accent"}],"commands":["..."]}`
- 2s write timeout, crash recovery with restart
- Plugin segments rendered in status bar between left and right sections
- Plugin-returned commands executed via existing command dispatch

---

## 17. Maximize Focused Window Preset

**Status: Done**

`Action::MaximizeFocused`, `Command::MaximizeFocused`. `saved_ratios: Option<LayoutNode>` on Workspace for toggle-restore. `layout.maximize_leaf(target)` pushes ratios to 0.95/0.05 toward focused leaf. Toggle: if saved_ratios exist, restore them. Bound to `m` in Normal mode and `\w m` in leader.
