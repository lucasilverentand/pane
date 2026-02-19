# Pane — Conceptual Architecture

## Hierarchy

```
Server (one per machine, runs as daemon)
├── Workspace (full-screen split layout, shared)
│   ├── Window (region in split tree, holds tabs)
│   │   └── Tab (running process)
│   └── Floating Window (overlay, not in split tree)
└── Client (independent view into workspaces)
```

## Server

- A single long-running daemon per machine.
- Running `pane` auto-starts the server if not already running, then connects a client.
- The server keeps running when all clients disconnect (like tmux detach).
- Processes inside tabs stay alive across client disconnects and reconnects.
- Persists full state across reboots: workspaces, window layouts, and tabs are restored on server start (tabs re-run their commands).
- Clients connect via **Unix domain socket** (local only for now).
- When the last workspace is closed, the **server shuts down**.

## Client

- A client is a terminal that connects to the server and renders the TUI.
- Each client has an **independent view**: it can be on a different workspace than other clients.
- Multiple clients viewing the same workspace have **independent focus** (each can focus a different window/tab).

## Workspace

- A workspace is a **full-screen layout of windows** (a binary split tree).
- Workspaces live on the server and are shared — any client can switch to any workspace.
- Switching workspace switches the entire visible layout.
- **Auto-named** from context (git repo → cwd → number), but user can rename anytime.
- Creating a new workspace starts with **one window containing a default shell tab**.
- Workspaces are shown in a workspace bar when more than one exists.

## Window

- A window is a **rectangular region** in the workspace's split tree.
- Created by **splitting the focused window** (binary split — horizontal or vertical).
- A window contains one or more **tabs**, displaying one tab at a time (like browser tabs).
- A tab bar appears inside the window when it has more than one tab.
- Closing the last tab in a window **closes the window** (the split layout adjusts).
- Closing the last window in a workspace closes the workspace.

### Resizing

- **Mouse drag** on split borders.
- **Keyboard shortcuts** for incremental resize.
- **Presets**: equalize all, maximize focused, etc.

### Zoom Mode

- Toggling zoom on the focused window shows it as a **centered overlay with ~1-2 cell margin**.
- The other windows remain visible but **dimmed** behind the overlay.
- Not a separate workspace — it's a temporary visual mode on the current layout.

### Floating Windows

- Windows can exist **outside the split tree** as floating overlays.
- Used for scratch terminals, popup tools, etc.
- Pane's own UI (pickers, menus) also uses floating panels.

## Tab

- A tab is a **running process with output** (typically a shell, but can be any command).
- Tabs live inside a window but can be **moved between windows** within the same workspace.
- One tab is active/visible per window at a time; the others are backgrounded.
- Creating a new tab shows a **fuzzy-search picker** over available shells and recently-run commands.
- Each tab has its own **scrollback history**; selections copy to the **system clipboard**.

## Modal System

Pane uses a **vim-inspired modal model** with two primary modes:

### Normal Mode (default)

- All keys are intercepted by pane for navigation and structural commands.
- **Vim-style navigation**: `h/j/k/l` for window focus, `1-9` for workspaces, `tab/shift+tab` for tabs.
- **Single-key structural ops**: `d` to split, `x` to close, `m` to move tab, etc.
- **Escape in normal mode** is forwarded to the focused tab (so vim/etc. still receive it).
- Access to command palette, search, and all pane commands.

### Interact Mode

- Entered by pressing `i` (vim-style insert).
- All keys are forwarded to the focused tab's process.
- **Escape** exits back to normal mode.
- The only key pane intercepts is Escape.

### Mode Indicator

- **Status bar** shows the current mode label (NORMAL / INTERACT / SCROLL).
- **Active window border color** changes based on mode (visual cue).

## Command Palette

- Fuzzy-search palette (like VS Code's ctrl+p) listing all available actions and keybinds.
- Accessible from normal mode.
- Shows action names, descriptions, and their keybinds for discoverability.

## Visual Style

- **Informative borders**: window borders show tab/window info (title, tab count, etc.).
- Active window border color reflects current mode.
- Configurable outer terminal title format (e.g., `{workspace} — {process}`).

## Plugin System

- Plugins are external processes that receive a **JSON payload** from pane with context (workspace info, window state, system stats, etc.).
- Plugins return structured output (e.g., status bar segments, formatted text).
- Used for extending the status bar, adding custom commands, or reacting to events.
- Similar to how Claude Code's MCP system provides context as JSON for tools to consume.

## Naming Summary

| Concept   | What it is                                    | tmux equivalent          |
|-----------|-----------------------------------------------|--------------------------|
| Server    | Background daemon, one per machine            | tmux server              |
| Client    | Terminal connection to the server              | tmux client              |
| Workspace | Full-screen split layout                       | tmux session             |
| Window    | Region in split tree, holds tabs               | tmux window+pane hybrid  |
| Tab       | Running process (shell, vim, etc.)             | —                        |
