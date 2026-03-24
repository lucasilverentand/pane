<p align="center">
  <h1 align="center">pane</h1>
  <p align="center">A modern terminal multiplexer with a TUI interface, built in Rust.</p>
</p>

<p align="center">
  <a href="#features">Features</a> &bull;
  <a href="#installation">Installation</a> &bull;
  <a href="#usage">Usage</a> &bull;
  <a href="#keybindings">Keybindings</a> &bull;
  <a href="#configuration">Configuration</a> &bull;
  <a href="#architecture">Architecture</a>
</p>

---

Pane is a terminal multiplexer in the spirit of tmux, built from scratch with [ratatui](https://github.com/ratatui/ratatui). It uses a client-daemon architecture — the daemon owns PTY sessions and persists in the background while lightweight TUI clients connect, disconnect, and reconnect freely.

## Features

- **Splits and layouts** — horizontal/vertical splits with vim-style navigation (`h`/`j`/`k`/`l`) and resize (`H`/`J`/`K`/`L`)
- **Tabbed windows** — each pane is a window that can hold multiple tabs (shell, neovim, dev servers, AI agents)
- **Workspaces** — named groups of windows, each with its own layout and working directory
- **Floating windows** — pop out a pane as a floating overlay
- **Fold and zoom** — collapse inactive splits into a thin fold bar, or zoom a single pane fullscreen
- **Copy mode** — vi-style text selection and system clipboard integration
- **Command palette** — fuzzy-searchable command palette (`:`)
- **Mouse support** — click to focus, drag to resize, scroll to browse history
- **Themes** — built-in presets (default, dracula, catppuccin, tokyo-night) with full color customization
- **Process-aware borders** — auto-colored borders for known programs (claude, nvim, cargo, docker, etc.)
- **Nerd Font support** — optional glyphs for tab kinds and status bar
- **tmux compatibility shim** — `pane tmux ...` translates common tmux commands
- **Light/dark auto-detection** — adapts border colors to your terminal background
- **Multi-client** — multiple TUI clients can attach to the same daemon simultaneously

## Installation

### From source

```sh
cargo install --path crates/pane-tui
```

Or build manually:

```sh
git clone https://github.com/lucasilverentand/pane.git
cd pane
cargo build --release
# Binary is at target/release/pane
```

## Usage

```sh
pane              # Start daemon (if needed) and attach
pane -d           # Start daemon in background without attaching
pane kill         # Kill the running daemon and all sessions
pane send-keys -t <target> <keys>  # Send keys to a pane
```

Once attached, pane starts in **Normal mode** where all keys are commands. Press `i` or `Enter` to switch to **Interact mode** where keystrokes go to the shell. Press `Ctrl+Space` to return to Normal mode.

## Keybindings

### Global (all modes)

| Key | Action |
|---|---|
| `Ctrl+Space` | Return to Normal mode |
| `Shift+PageUp` | Enter scroll mode |

### Normal mode

| Key | Action |
|---|---|
| `i` / `Enter` | Enter Interact mode |
| `h` `j` `k` `l` / arrows | Navigate panes |
| `Tab` / `Shift+Tab` | Next / previous tab |
| `n` | New tab (opens tab picker) |
| `d` | Close tab |
| `s` | Split horizontal |
| `v` / `Shift+S` | Split vertical |
| `Shift+H/J/K/L` | Resize pane |
| `Alt+H/J/K/L` | Move tab between windows |
| `m` | Maximize focused pane |
| `z` | Toggle zoom |
| `f` | Toggle fold |
| `Shift+F` | New floating window |
| `=` | Equalize split sizes |
| `c` | Copy mode |
| `p` | Paste from clipboard |
| `:` | Command palette |
| `q` | Quit |

All bindings are customizable in `~/.config/pane/config.toml`.

## Configuration

Pane reads its config from `~/.config/pane/config.toml`.

```toml
[theme]
preset = "catppuccin"  # "default", "dracula", "catppuccin", "tokyo-night"
# Or set colors directly:
# accent = "#cba6f7"
# border_inactive = "#373747"

[behavior]
mouse = true
nerd_fonts = false
vim_navigator = false
# default_shell = "/bin/zsh"
# terminal_title_format = "{session} - {workspace}"

[keys]
# Override any keybinding:
# "ctrl+t" = "NewTab"
# "ctrl+d" = "CloseTab"

[normal_keys]
# Override Normal mode bindings:
# "x" = "CloseTab"

[[pane_decorations]]
process = "claude"
border_color = "#f97316"
```

### Theme presets

| Preset | Accent |
|---|---|
| `default` | Cyan (adapts to light/dark) |
| `dracula` | Purple |
| `catppuccin` | Mauve |
| `tokyo-night` | Blue |

## Architecture

Pane is a Rust workspace with four crates:

```
pane-tui (binary)  -->  pane-daemon  -->  pane-protocol
                        \-> vt100-patched
```

- **pane-protocol** — shared types, wire protocol (length-prefixed JSON over Unix sockets), layout tree, config, and keybind definitions
- **pane-daemon** — background server that owns PTY sessions, manages workspaces/windows/tabs, and broadcasts state to clients
- **pane-tui** — the TUI client binary, renders with ratatui, forwards input to the daemon
- **vt100-patched** — local fork of the `vt100` crate with extended SGR attribute support

Communication happens over Unix domain sockets at `$TMPDIR/pane-{uid}/pane.sock` (debug builds use `pane-dev.sock`). Override with `PANE_SOCKET=name`.

## Development

```sh
cargo build          # Debug build
cargo test           # Run all tests (341 tests)
cargo test -p pane-tui -- snapshot_tests  # Snapshot tests only
cargo insta review   # Review snapshot diffs after UI changes
```

CI runs on both Ubuntu and macOS.

## License

MIT
