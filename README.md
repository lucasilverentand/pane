<h1 align="center">pane</h1>
<p align="center">
  A terminal multiplexer for modern development workflows.
  <br />
  Persistent daemon, reconnectable TUI clients, workspaces, tabs, floating windows, and vim-style control.
</p>

<p align="center">
  <a href="#about">About</a>
  ·
  <a href="docs/INSTALL.md">Install</a>
  ·
  <a href="docs/CONFIGURATION.md">Configuration</a>
  ·
  <a href="docs/ARCHITECTURE.md">Architecture</a>
  ·
  <a href="docs/DEVELOPMENT.md">Development</a>
</p>

## About

`pane` is a Rust terminal multiplexer in the spirit of tmux, but built around a
local daemon plus reconnectable TUI clients.

The daemon owns PTYs and workspace state. Clients are disposable: you can
attach, detach, and reattach without losing running processes. The interface is
optimized for keyboard-first development work with shells, editors, agents, and
dev servers running side by side.

## Why Pane

- Persistent sessions without living inside a single terminal process
- Workspaces, split layouts, tabs, floating windows, fold, zoom, and overview
- Vim-style modal navigation with a command palette and leader-key support
- Multi-client attach for shared or parallel views into the same daemon
- Process-aware pane decorations for tools like `claude`, `codex`, `nvim`, `k9s`, and more
- Configurable themes, keymaps, status bar segments, plugins, and tab picker entries
- A `tmux` compatibility shim for common tmux-oriented tooling

## Quick Start

Install from source:

```sh
cargo install --path crates/pane-tui
```

Run `pane` to start the daemon if needed and attach a client:

```sh
pane
```

Useful CLI commands:

```sh
pane -d
pane kill
pane send-keys -t <target> <keys>
pane daemon
```

Once attached:

- `i` or `Enter` enters Interact mode and forwards keys to the focused process
- `Ctrl+Space` returns to Normal mode
- `:` opens the command palette
- `n` opens the tab picker
- `o` toggles the workspace overview

## Documentation

- [Install and Usage](docs/INSTALL.md)
- [Configuration](docs/CONFIGURATION.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Development](docs/DEVELOPMENT.md)
- [Design Notes](DESIGN.md)

## Current Scope

`pane` is already usable as a daily-driver multiplexer for local development.
The project is still evolving quickly, especially around workspace UX, agent
workflows, and automation APIs.

If you want implementation details instead of end-user docs, start with the
crate map in [Architecture](docs/ARCHITECTURE.md) and the build/test workflow in
[Development](docs/DEVELOPMENT.md).

## License

MIT
