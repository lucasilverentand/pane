# Pane - Rust TUI for AI Coding Agents & Dev Servers

## UI Design

### Layout Model: Binary Split Tree

Panes are arranged in a **binary tree** — any pane can be split horizontally or
vertically, creating nested layouts of any complexity.

```
         root
        ┌──┴──┐
     split(H)
    ┌───┴───┐
  pane:1  split(V)
          ┌──┴──┐
       pane:2  pane:3
```

### Main View — horizontal + vertical splits, rounded borders

Active pane border is **highlighted** (colored). Inactive panes are **dimmed**.

```
 pane ── session: my-project ──────────────────────────────────────────────────────────────
  [1 claude]   *[2 nvim]    [3 server]    [4 shell]                            [+] new pane
 ──────────────────────────────────────────────────────────────────────────────────────────

 ╭─ claude ──────────────────╮ ╭─ nvim · ACTIVE ─────────────────────────────────────────╮
 │                           │ │                                                          │
 │  > Analyzing codebase...  │ │  fn main() {                                             │
 │                           │ │      let app = App::new();                                │
 │  Found 12 files to review │ │      app.run()?;                                          │
 │                           │ │  }                                                        │
 │  Fixing auth.rs...        │ │                                                           │
 │  ████████░░ 80%           │ │  ~                                                        │
 │                           │ │  ~                                                        │
 │                           │ │  -- INSERT --                                              │
 │                           │ ╰───────────────────────────────────────────────────────────╯
 │                           │ ╭─ server ────────────────────╮ ╭─ shell ──────────────────╮
 │                           │ │                              │ │                          │
 │                           │ │  $ npm run dev               │ │  ~/project (main) $      │
 │                           │ │                              │ │                          │
 │                           │ │  ready on localhost:3000     │ │                          │
 │                           │ │  GET / 200 12ms              │ │                          │
 │                           │ │  GET /api 200 3ms            │ │                          │
 ╰───────────────────────────╯ ╰──────────────────────────────╯ ╰──────────────────────────╯
                                                                                ctrl+? help
```

**Split tree for the layout above:**

```
              root
           ┌────┴────┐
        split(H)
      ┌────┴────┐
   pane:1     split(V)
   claude    ┌────┴────┐
          pane:2     split(H)
           nvim     ┌───┴───┐
                 pane:3   pane:4
                 server    shell
```

### Alternative: Full-width top pane, two below

```
 pane ── session: api-work ────────────────────────────────────────────────────────────────
  *[1 nvim]    [2 claude]    [3 server]                                        [+] new pane
 ──────────────────────────────────────────────────────────────────────────────────────────

 ╭─ nvim · ACTIVE ────────────────────────────────────────────────────────────────────────╮
 │                                                                                        │
 │  use ratatui::prelude::*;                                                               │
 │                                                                                         │
 │  pub struct App {                                                                        │
 │      panes: Vec<Pane>,                                                                   │
 │      active: usize,                                                                      │
 │  }                                                                                       │
 │                                                                                          │
 │  -- NORMAL --                                                                            │
 ╰────────────────────────────────────────────────────────────────────────────────────────╯
 ╭─ claude ──────────────────────────────────╮ ╭─ server ────────────────────────────────╮
 │                                            │ │                                         │
 │  > Working on pane/mod.rs...               │ │  $ cargo run                            │
 │  Added App struct with pane management     │ │  Compiling pane v0.1.0                  │
 │                                            │ │  Finished dev [optimized] in 2.3s       │
 │  What should I work on next?               │ │  Running `target/debug/pane`             │
 │                                            │ │                                         │
 ╰────────────────────────────────────────────╯ ╰─────────────────────────────────────────╯
                                                                                ctrl+? help
```

**Split tree:**

```
           root
        ┌────┴────┐
      split(V)
     ┌───┴───┐
  pane:1   split(H)
   nvim   ┌───┴───┐
       pane:2   pane:3
       claude   server
```

### Design Elements

- **All panes**: Rounded borders using `╭ ╮ ╰ ╯ │ ─`
- **Active pane**: Border rendered in **accent color** (cyan/blue) + label says `· ACTIVE`
- **Inactive panes**: Border rendered in **dim/gray**
- **Active tab**: Prefixed with `*` in tab bar
- **Tab bar**: Numbered tabs `[1 name]` for quick switching with `Alt+N`
- **Session name**: Shown in top bar, persists across restarts
- **Status bar**: Bottom-right, contextual help hint

### Splitting & Resizing

Splitting the active pane:

```
  Before split-right:          After split-right (ctrl+d):

  ╭─ nvim ──────────────╮      ╭─ nvim ─────────╮ ╭─ shell ────────╮
  │                      │      │                 │ │                │
  │                      │  →   │                 │ │  $ _           │
  │                      │      │                 │ │                │
  ╰──────────────────────╯      ╰─────────────────╯ ╰────────────────╯

  Before split-down:           After split-down (ctrl+shift+d):

  ╭─ nvim ──────────────╮      ╭─ nvim ──────────────╮
  │                      │      │                      │
  │                      │  →   ╰──────────────────────╯
  │                      │      ╭─ shell ──────────────╮
  ╰──────────────────────╯      │  $ _                  │
                                ╰──────────────────────╯
```

Resizing moves the split boundary:

```
  ctrl+alt+→  (grow active pane right)

  ╭─ nvim ─────────╮ ╭─ shell ────╮       ╭─ nvim ────────────────╮ ╭─ shell ─╮
  │                 │ │            │   →   │                        │ │         │
  │                 │ │            │       │                        │ │         │
  ╰─────────────────╯ ╰────────────╯       ╰────────────────────────╯ ╰─────────╯
```

### Session Picker (on startup / ctrl+s)

```
 pane ─────────────────────────────────────────────────────────────────────────────────────

                     ╭─ sessions ──────────────────────────────────╮
                     │                                             │
                     │   ▸ my-project          3 panes   2m ago   │
                     │     api-refactor        2 panes   1h ago   │
                     │     bugfix-auth         4 panes   3h ago   │
                     │     feature-payments    1 pane    1d ago   │
                     │                                             │
                     │   ──────────────────────────────────────    │
                     │   [n] new session    [d] delete    [enter]  │
                     ╰─────────────────────────────────────────────╯

```

### New Pane Menu (ctrl+n or [+])

```
                     ╭─ new pane ──────────────────────────────────╮
                     │                                             │
                     │   [a]  AI Agent     (claude, cursor, etc.)  │
                     │   [n]  Neovim       (open editor)           │
                     │   [s]  Shell        (terminal)              │
                     │   [d]  Dev Server   (run & monitor)         │
                     │                                             │
                     ╰─────────────────────────────────────────────╯
```

## Pane Types

| Type | What it runs | PTY |
|------|-------------|-----|
| **AI Agent** | `claude` CLI or similar AI coding tool | Yes |
| **Neovim** | `nvim` with full TUI support | Yes |
| **Shell** | User's default shell (`$SHELL`) | Yes |
| **Dev Server** | Custom command (e.g. `npm run dev`) | Yes |

All pane types run in a real PTY with `vt100` terminal emulation, so ncurses/TUI apps like nvim render correctly inside panes.

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `Alt+1..9` | Switch to pane N |
| `Alt+h/j/k/l` | Focus pane left/down/up/right |

### Splitting

| Key | Action |
|-----|--------|
| `Ctrl+d` | Split active pane right (horizontal) |
| `Ctrl+Shift+d` | Split active pane down (vertical) |

### Resizing

| Key | Action |
|-----|--------|
| `Ctrl+Alt+h` / `Ctrl+Alt+Left` | Shrink active pane from right |
| `Ctrl+Alt+l` / `Ctrl+Alt+Right` | Grow active pane to right |
| `Ctrl+Alt+k` / `Ctrl+Alt+Up` | Shrink active pane from bottom |
| `Ctrl+Alt+j` / `Ctrl+Alt+Down` | Grow active pane down |
| `Ctrl+Alt+=` | Equalize all pane sizes |

### Session & Pane Management

| Key | Action |
|-----|--------|
| `Ctrl+n` | New pane menu |
| `Ctrl+s` | Session picker / save |
| `Ctrl+w` | Close active pane |
| `Ctrl+q` | Quit (auto-save) |
| `Ctrl+?` | Help overlay |

All other input is forwarded to the active pane's PTY.

## Split Tree Data Structure

```rust
enum LayoutNode {
    Pane(PaneId),
    Split {
        direction: Direction,  // Horizontal | Vertical
        ratio: f64,            // 0.0..1.0, position of the divider
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}
```

Splitting a pane replaces its `Pane(id)` node with a `Split` containing the
original pane and a new pane. Resizing adjusts the `ratio`. Closing a pane
collapses the `Split` back to the remaining sibling.

## Sessions

- Sessions persist to `~/.pane/sessions/{id}.json`
- Auto-save on quit
- Stores: layout tree + pane configs (command, cwd, env, kind, title) + scrollback
- On resume: rebuilds layout tree, re-spawns PTYs, replays scrollback

## Tech Stack

| Crate | Purpose |
|-------|---------|
| `ratatui` | TUI framework (widgets, layout, rendering) |
| `crossterm` | Terminal backend (input, raw mode, alternate screen) |
| `tokio` | Async runtime (event loop, background tasks) |
| `portable-pty` | PTY management (spawn/read/write/resize) |
| `vt100` | Terminal emulation (parse ANSI for nvim etc.) |
| `serde` + `serde_json` | Session serialization |
| `uuid` | Unique pane/session IDs |
| `chrono` | Timestamps |
| `dirs` | Config/data paths |

## Architecture (Elm / TEA pattern)

```
                    ╭──────────────╮
                    │   Crossterm  │  keyboard/mouse/resize events
                    ╰──────┬───────╯
                           │
                    ╭──────▼───────╮
                    │  Event Loop  │  tokio::select! on all sources
                    │   (app.rs)   │◄──── PTY output (via channels)
                    ╰──────┬───────╯◄──── Tick timer (render rate)
                           │
                    ╭──────▼───────╮
                    │    Update    │  match message → mutate state
                    ╰──────┬───────╯
                           │
                    ╭──────▼───────╮
                    │     View     │  state → ratatui widgets
                    │   (ui/*.rs)  │
                    ╰──────────────╯
```

## Project Structure

```
pane/
├── Cargo.toml
├── .gitignore
└── src/
    ├── main.rs              # Entry point, arg parsing, tokio runtime
    ├── app.rs               # App state, event loop, message dispatch
    ├── event.rs             # Event types (key, pty output, tick)
    ├── tui.rs               # Terminal setup/teardown, rendering bridge
    ├── layout.rs            # LayoutNode split tree, resize, split/close
    ├── session/
    │   ├── mod.rs           # Session struct, save/load
    │   └── store.rs         # Session file management (~/.pane/sessions/)
    ├── pane/
    │   ├── mod.rs           # Pane enum (Shell, Agent, Nvim, DevServer)
    │   ├── pty.rs           # PTY spawning, read/write, resize
    │   └── terminal.rs      # vt100 screen buffer per pane
    └── ui/
        ├── mod.rs           # Root render function
        ├── tab_bar.rs       # Top tab bar with session name
        ├── pane_view.rs     # Individual pane rendering with borders
        ├── layout_render.rs # Recursive split tree → ratatui layout
        ├── session_picker.rs # Session list overlay
        └── help.rs          # Help overlay
```
