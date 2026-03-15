# CMUX-Inspired Features for Pane

Research based on [cmux](https://github.com/manaflow-ai/cmux) — a native macOS terminal
built for AI agent workflows by Manaflow (YC S24). CMUX is a GUI app powered by
libghostty; these suggestions adapt its best ideas to Pane's TUI context.

---

## 1. Notification System (Activity Rings)

**What CMUX does:** When an AI agent finishes or needs attention, its pane border
lights up with a blue ring and the sidebar tab badge updates. Desktop notifications
fire when you're not looking at the relevant workspace. `Cmd+Shift+U` jumps to the
most recent unread notification.

**How it maps to Pane:**

- **Border flash/glow on activity** — when a background tab or unfocused window
  produces output after a configurable idle period (e.g. 3s of silence → next output
  triggers a "bell"), tint its border with an `activity` accent color.
- **Workspace bar badge** — show a dot `●` or count next to workspace names that
  have unread activity.
- **Notification log** — a new overlay (like the command palette) listing recent
  notifications with timestamps. Enter jumps to that tab.
- **`JumpToNextActivity` action** — cycle through tabs/windows with unread activity,
  bound to something like `!` in normal mode.
- **OSC 777 / OSC 99 support** — honor standard terminal notification escape
  sequences so tools like Claude Code can trigger notifications natively.
- **Bell handling** — `\x07` (BEL) in PTY output triggers the activity indicator.

**Complexity:** Medium — needs per-tab activity tracking state, a timestamp for
"last user focus", and rendering changes to borders + workspace bar.

**Impact:** High — this is CMUX's killer feature. For anyone running multiple agents,
knowing which one needs you without switching tabs is transformative.

---

## 2. Workspace Metadata Bar (Rich Sidebar Info)

**What CMUX does:** The vertical sidebar shows per-workspace: git branch, linked PR
number + status, working directory, listening ports (auto-detected), latest
notification text, and a progress bar.

**How it maps to Pane:**

Since Pane uses a horizontal workspace bar (not a vertical sidebar), this needs a
different UI treatment:

- **Expanded workspace bar** — when focused (or on hover), show a tooltip/popup with
  metadata: git branch, cwd, listening ports, active process.
- **Git branch in workspace name** — auto-append branch name: `myproject [main]` or
  use the branch as the workspace name when auto-naming from a git repo.
- **Port detection** — periodically scan child PIDs for listening TCP ports via
  `lsof -iTCP -sTCP:LISTEN -P` or `/proc/net/tcp`. Show in status bar or workspace
  tooltip.
- **Workspace info panel** — `i` in normal mode (when workspace bar focused) opens a
  small info popup showing cwd, git status, branch, ports, process tree.

**Complexity:** Medium — git branch is easy (already have cwd detection), port
scanning needs a background task.

**Impact:** Medium — useful for multi-project workflows where you forget which
workspace has which server running.

---

## 3. Programmable Status API

**What CMUX does:** CLI commands let agents set per-workspace metadata:
- `cmux set-status key value` — custom key-value pills in sidebar
- `cmux set-progress 0.75` — progress bar per workspace
- `cmux log info "Building..."` — structured log entries

**How it maps to Pane:**

- **`pane set-status <key> <value>`** — stores arbitrary key-value pairs on the
  workspace, displayed in the status bar or workspace info panel.
- **`pane set-progress <0.0-1.0>`** — renders a progress bar in the workspace bar
  entry (e.g. `▓▓▓▓░░░░ 50%`).
- **`pane log <level> <message>`** — appends to a per-workspace log ring buffer,
  viewable via a `LogPanel` overlay.
- **OSC escape sequences** — define custom OSC codes so processes can set status
  without needing the `pane` CLI: `\e]9999;set-progress=0.5\a`

**Complexity:** Low-Medium — the daemon already handles CLI commands via socket. Add
new `ClientRequest` variants and per-workspace metadata storage.

**Impact:** High for AI agent workflows — agents can report progress without you
switching to their tab.

---

## 4. Session Restoration

**What CMUX does:** On relaunch, restores window/workspace/pane layout, working
directories, and terminal scrollback. Does not resume live processes (re-spawns
shells in the correct directories).

**How it maps to Pane:**

- **`pane save-layout`** / auto-save on clean exit — serialize workspace tree,
  window positions, split ratios, tab cwds, and names to
  `~/.local/state/pane/layout.json`.
- **`pane restore-layout`** / auto-restore on start — if layout file exists,
  recreate the workspace/window/tab structure with shells spawned in the saved cwds.
- **Scrollback persistence** (stretch) — dump vt100 screen history to disk on exit,
  reload into new PTY on restore.

What to save:
```
- Workspace names + order
- Layout tree (split directions + ratios)
- Per-tab: cwd, shell command, tab name, window name
- Folded/zoomed/floating state
- Active workspace/window/tab selections
```

**Complexity:** Medium — serialization is straightforward, but matching restored
state to new PTY sessions needs care (what if cwd no longer exists?).

**Impact:** High — this is the #1 reason people use tmux over alternatives. Pane
currently has no persistence.

---

## 5. Extended CLI / Socket API

**What CMUX does:** Rich CLI for automation: workspace CRUD, split management,
surface targeting, input injection, notification management.

**How it maps to Pane:**

Pane already has `pane send-keys`. Extend the CLI to cover:

```
# Workspace management
pane list-workspaces          # JSON output
pane new-workspace [--name N] [--cwd /path]
pane switch-workspace <name|index>
pane close-workspace <name|index>

# Window/tab management
pane list-windows             # JSON: id, name, tabs, active
pane list-tabs                # JSON: id, name, cwd, process
pane focus <window-id>
pane new-split <right|down>
pane new-tab [--window <id>] [--cmd "..."]

# Targeted input
pane send-keys --tab <id> "text"
pane send-keys --window <id> "text"

# Notifications
pane notify --title "Done" --body "Build complete"
pane list-notifications

# Status
pane set-status <key> <value>
pane set-progress <0.0-1.0>
pane log <info|warn|error> "message"

# Introspection
pane info                     # JSON dump of full state
pane capabilities             # list supported commands
```

**Complexity:** Low-Medium per command — the socket infrastructure exists. Each
command is a new `ClientRequest` variant + handler.

**Impact:** High — makes Pane scriptable for agent orchestrators, CI pipelines, and
custom tooling.

---

## 6. Agent-Aware Tab Detection

**What CMUX does:** Recognizes AI coding agents and provides hooks for their
lifecycle events (task start, completion, error).

**How it maps to Pane:**

Pane already detects `claude` as a foreground process and applies orange border
decoration. Extend this:

- **Agent lifecycle hooks** — detect when Claude Code emits its completion markers
  and trigger a notification + activity indicator.
- **Hook system** — `~/.config/pane/hooks/` directory with scripts that run on
  events: `on_process_exit`, `on_bell`, `on_activity`, `on_idle`.
  Input: JSON with `{workspace, window, tab, process, exit_code}`.
  Output: JSON with optional commands (`notify`, `set-status`, etc.)
- **Agent status in tab bar** — show a spinner `⠋` while agent is running, `✓` on
  success, `✗` on failure (detected via exit code or output parsing).

**Complexity:** Medium — process exit detection exists, but output parsing for agent
status is heuristic-based.

**Impact:** Medium-High — natural extension of Pane's existing process decoration
system.

---

## 7. Suggested Implementation Priority

Based on impact, complexity, and how well each feature fits Pane's TUI architecture:

| Priority | Feature | Why |
|----------|---------|-----|
| **P0** | Notification system (activity rings) | Killer feature for multi-agent use. Medium effort, high reward. |
| **P0** | Extended CLI/socket API | Foundation for all programmable features. Low effort per command. |
| **P1** | Session restoration | Most-requested multiplexer feature. Removes biggest gap vs tmux. |
| **P1** | Programmable status API | Enables agent progress reporting. Builds on CLI work. |
| **P2** | Workspace metadata | Nice-to-have enrichment. Git branch is quick win. |
| **P2** | Agent-aware detection + hooks | Natural extension of existing process decoration. |

---

## 8. What NOT to Port

Some CMUX features don't make sense for Pane:

- **Embedded browser** — CMUX is a GUI app with WebKit. A TUI can't embed a browser.
- **Desktop notifications** — TUI apps can't easily trigger OS notifications (though
  `notify-send` / `osascript` could work as a plugin).
- **Vertical sidebar** — Pane's horizontal workspace bar works well for TUI. A
  sidebar would eat too much horizontal space in a terminal.
- **GPU rendering** — Pane renders through the host terminal. Not applicable.
- **libghostty integration** — CMUX uses it as a rendering engine. Pane uses
  ratatui + crossterm, which is the right choice for a TUI multiplexer.

---

## References

- [cmux GitHub](https://github.com/manaflow-ai/cmux) (5.7k stars, AGPL-3.0)
- [cmux website](https://www.cmux.dev/)
- [Manaflow (YC S24)](https://www.ycombinator.com/companies/manaflow)
- [Show HN discussion](https://news.ycombinator.com/item?id=47079718)
