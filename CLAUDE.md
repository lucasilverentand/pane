# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Pane

Pane is a TUI terminal multiplexer written in Rust — similar to tmux but with a modern ratatui-based interface. It uses a client-daemon architecture communicating over Unix domain sockets with length-prefixed JSON frames.

## Build & Test Commands

```bash
cargo build                          # debug build
cargo build --release                # release build
cargo test                           # run all tests
cargo test -p pane-tui               # test a single crate
cargo test snapshot_tests            # run snapshot tests only
cargo test -p pane-tui -- tests_palette  # run a specific test module
cargo insta review                   # review snapshot changes interactively
```

CI runs `cargo build --release` and `cargo test` on both ubuntu-latest and macos-latest.

## Architecture

### Crate dependency graph

```
pane-tui (binary: `pane`) → pane-daemon → pane-protocol
                           ↘ vt100-patched (local patch of vt100 crate)
```

### pane-protocol

Shared types with zero runtime — defines the wire protocol and everything both sides need:

- **`protocol.rs`** — `ClientRequest` / `ServerResponse` enums, `RenderState` and snapshot types (`WorkspaceSnapshot`, `WindowSnapshot`, `TabSnapshot`)
- **`framing.rs`** — length-prefixed async frame read/write over `UnixStream`
- **`layout.rs`** — `LayoutNode` tree (Leaf | Split) with `resolve()` to compute rects, `TabId = Uuid`
- **`config.rs`** — `Config`, `Theme`, `Action` enum (all bindable keybinds), TOML config loading from `~/.config/pane/`
- **`keys.rs`** — `key_to_bytes()` converts crossterm `KeyEvent` to PTY-compatible byte sequences
- **`window_types.rs`** — `WindowId`, `TabKind` (Shell, Agent, Nvim, DevServer)
- **`default_keys.rs`** — default keybinding tables
- **`app.rs`** — shared UI state types (`LeaderState`, `ResizeState`)

### pane-daemon

Server process that owns PTYs and session state. Spawns as a background process, clients connect via socket.

- **`server/daemon.rs`** — socket lifecycle (`start_daemon`, `run_server`, `kill_daemon`), client connection handling. Debug builds use `pane-dev.sock` to avoid colliding with release installs. Override with `PANE_SOCKET` env var.
- **`server/state.rs`** — `ServerState` holds workspaces, config, stats. Contains `render_state_from_server` / `render_state_for_client` to produce `RenderState` snapshots for clients.
- **`server/command.rs`** — `Command` enum (typed, parsed commands like `SplitWindow`, `NewWorkspace`, etc.) and execution logic
- **`server/command_parser.rs`** — parses string commands (tmux-style syntax) into `Command` values
- **`server/tmux_shim.rs`** — `pane tmux ...` compatibility layer
- **`server/id_map.rs`** — bidirectional UUID ↔ numeric ID mapping for tmux compat
- **`window/`** — `Window` (contains tabs), `Tab` (owns a PTY via `portable-pty`), terminal emulation via patched `vt100`
- **`workspace.rs`** — `Workspace` manages a layout tree of windows

### pane-tui

The TUI client binary (`pane`). Connects to the daemon, renders with ratatui, forwards input.

- **`main.rs`** — CLI (clap): default attaches, subcommands `kill`, `send-keys`, `daemon`, `tmux`
- **`client.rs`** — `Client` struct holds all client-side state. `Focus` enum is the unified focus/mode system (Normal, Interact, WorkspaceBar, Palette, Leader, Confirm, Rename, TabPicker, WidgetPicker, ContextMenu, Resize, NewWorkspace, Scroll)
- **`ui/mod.rs`** — `render_client()` entry point, composes workspace bar + body + status bar
- **`ui/` submodules** — each UI component: `palette`, `context_menu`, `dialog`, `status_bar`, `tab_picker`, `widget_picker`, `window_view`, `workspace_bar`, `layout_render`
- **`copy_mode.rs`** / **`clipboard.rs`** — vi-style copy mode and system clipboard integration

### vt100-patched

Local fork of the `vt100` crate with extended SGR attribute support. Registered as a `[patch.crates-io]` in the workspace root.

## Key Concepts

- **Workspace** — a named group of windows with its own layout tree and working directory
- **Window** (referred to as "group" in protocol snapshots) — contains one or more tabs, displayed in a bordered pane
- **Tab** — a single PTY session within a window, with a kind (Shell, Agent, Nvim, DevServer)
- **LayoutNode** — binary split tree; each leaf holds a `WindowId`; the tree resolves to `(WindowId, Rect)` pairs
- **Focus** — unified enum replacing the old mode/location system; modals push onto a focus stack

## Testing

- **Snapshot tests** (`pane-tui/src/ui/snapshot_tests.rs`) use `insta` with ratatui's `TestBackend` to capture rendered output as text. Test modules are separate files: `tests_palette.rs`, `tests_context_menu.rs`, `tests_dialog.rs`, `tests_resize.rs`, `tests_status_bar.rs`, `tests_tab_picker.rs`, `tests_window_view.rs`, `tests_workspace_bar.rs`.
- Standard terminal size for snapshots: 120×36.
- After changing UI rendering, run `cargo insta review` to accept/reject snapshot diffs.
- Snapshots live in `crates/pane-tui/src/ui/snapshots/`.

## Socket Paths

Debug builds: `$TMPDIR/pane-{uid}/pane-dev.sock`
Release builds: `$TMPDIR/pane-{uid}/pane.sock`
Override: `PANE_SOCKET=name` → `pane-name.sock`
