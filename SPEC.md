# Pane — Product Spec

## Vision

Pane is a native macOS IDE where AI agents and code editing are equal citizens on an infinite canvas. Workspaces are persistent environments — not just file containers — with their own agents, history, layout, and settings.

---

## Core Concepts

### Workspace

A **workspace** is a long-lived, persistent environment. It contains:

- **Projects** — one or more git repos / directories in scope
- **Panes** — arranged on an infinite canvas (editors, agents, terminals, previews, etc.)
- **Agents** — running or paused, with full conversation history
- **Layout state** — canvas position, zoom level, pane arrangement
- **Settings** — workspace-level configuration (keybindings, theme overrides, agent templates)

Closing and reopening a workspace restores everything: agent conversations, running state, pane layout, editor tabs, scroll positions. Like reopening a browser session.

### Pane

A **pane** is any interactive surface on the canvas. Pane types are extensible:

| Type | Content |
|------|---------|
| **Editor** | Code editor (CodeEditSourceEditor), one or more tabs |
| **Agent** | AI agent conversation — input, message history, status |
| **Terminal** | Integrated terminal session |
| **Preview** | Browser preview, rendered output, image viewer |
| **Planner** | Visual task planning — input a goal, get a task graph, edit and approve |
| **Search** | File search, text search, symbol search across all workspace projects |
| **Custom** | Future: docs, diagrams, database viewer, etc. |

All pane types share the same layout system. They can be created, moved, resized, and closed independently.

### Agent

An **agent** is an AI-powered collaborator running inside a workspace. Key properties:

- **Provider** — chosen per-agent: Anthropic API, OpenAI API, CLI wrapper (Claude Code, Codex), or any future provider
- **Template** — user-defined templates with custom system prompts, tool access, and default configuration. No built-in roles; users build their own library
- **Context** — all agents in a workspace share access to workspace state: files, git, and summaries of what other agents are doing
- **Persistence** — agents fully restore when a workspace is reopened (conversation history, active tasks, running state)

---

## Infinite Canvas

The workspace layout is an **infinite 2D canvas** on which panes are placed freely. This is the core differentiator.

### Rendering

Hybrid approach: **SwiftUI for pane content, Metal for canvas chrome**.

- Each pane renders its content using SwiftUI (editor, terminal, agent conversation)
- The canvas itself (zoom, pan, pane borders, connection lines between agents) is rendered via Metal
- Panes are rendered to textures at their native resolution; Metal composites them onto the canvas with smooth zoom/pan
- **Live content at all zoom levels** — when zoomed out, you see actual scaled-down code and conversations, not placeholders

### Navigation

Full support for both input methods:

**Trackpad:**
- Two-finger pan to scroll the canvas
- Pinch to zoom in/out
- Click to focus a pane

**Keyboard:**
- Hotkeys to jump between panes (next/prev, by type, by name)
- Hotkeys for zoom levels (fit all, 100%, focus selected)
- Saved positions / bookmarks on the canvas

### Pane Arrangement

- Panes are freely positioned on the canvas (no grid snapping required, but optional)
- Drag to move, drag edges to resize
- No enforced tiling — but smart guides / snapping for alignment
- Panes can overlap (z-ordering)

---

## Agent System

### Spawning

Agents can be created by:

1. **User action** — explicitly create a new agent pane (Cmd+N agent, context menu, etc.)
2. **Another agent** — a running agent can spawn sub-agents to delegate subtasks

When an agent spawns a sub-agent:
- The sub-agent gets its **own pane** on the canvas
- A **visual link** (line/arrow) connects parent to child pane
- The parent-child relationship is visible but both panes are fully interactive

### Context Sharing

All agents in a workspace share a common context layer:

- **File system** — read/write access to all projects in the workspace
- **Git state** — branch, status, recent commits
- **Agent awareness** — each agent can see summaries of other agents (what they're working on, which files they're touching)
- No explicit message-passing needed for basic coordination; the shared workspace state is the communication channel

### Live Sync with Editor

When an agent edits a file:
- Changes appear **in real-time** in any editor pane that has the file open
- Characters appear as they're written, like watching a collaborator type
- The editor buffer is the source of truth — agent writes go through the buffer system
- Standard undo (Cmd+Z) can revert agent changes

### Agent Templates

Users create and save templates:

```
Template {
    name: String,
    system_prompt: String,
    allowed_tools: [Tool],
    default_provider: Provider?,     // optional, can override at spawn
    description: String,
}
```

Templates are stored per-workspace or globally. Users build their own library of agents tailored to their workflow.

### Provider Architecture

Each agent has a provider, chosen at creation time:

| Provider | Implementation |
|----------|---------------|
| **Anthropic API** | Direct API calls, Claude models, tool use |
| **OpenAI API** | Direct API calls, GPT/o-series models |
| **Claude Code CLI** | Subprocess wrapper, streaming stdout |
| **Codex CLI** | Subprocess wrapper |
| **Custom** | Future: local models, other APIs |

Providers implement a common trait (`AgentProvider`) with streaming message support, tool execution, and cancellation.

---

## Task System

Tasks are the unit of delegated work. A task is a well-defined piece of work assigned to agents, executed in isolation on a git worktree, and merged back after review.

### Task Lifecycle

```
Create → Assign → Run → Review → Merge
```

1. **Create** — define the task: title, description, target branch, dependencies
2. **Assign** — assign one or more agents (with provider + template choices)
3. **Run** — Pane creates a git worktree + branch; agents work in isolation. Your main working copy is unaffected
4. **Validate** — agent runs the project's configured checks (tests, lint, typecheck) before signaling completion. Failures loop back to the agent to fix
5. **Review** — in-app diff review when the agent signals completion. Approve, request changes, or reject
6. **Merge** — merge the task branch back into the target branch from within Pane
7. **Ship** — push to remote, create a GitHub PR, and/or trigger deploy pipelines

### Parallel Execution

Multiple tasks can run simultaneously, each on its own worktree and branch. You keep editing on your main branch while agents work on tasks in the background — all visible live on your canvas.

### Task Dependencies

Tasks can declare explicit dependencies on other tasks. A dependent task won't start running until its dependencies are merged. This enables sequencing work that builds on prior changes.

### Live Visibility

Running tasks appear live on the canvas. Each task's agent panes are visible — you can zoom in to watch an agent work in real-time, or zoom out to see all tasks at a glance. Tasks are visually grouped on the canvas so you can distinguish task work from your own work.

### Git Integration

Each running task gets:
- A **git worktree** — isolated copy of the repo at the task's base branch
- A **branch** — named after the task (e.g., `pane/task/<slug>`)
- Agents work entirely within their worktree, so there are no conflicts with your working copy or other tasks

### Storage (git-native)

Tasks are stored in the repo itself, making them visible to all team members via git:

```
.pane/tasks/
    <task-id>.toml    # one file per task
```

Each task file contains:

```toml
[task]
id = "abc123"
title = "Add input validation to signup form"
description = "..."
status = "running"          # created | assigned | running | review | merged | closed
branch = "pane/task/signup-validation"
base = "main"
created_by = "luca"
created_at = 2026-04-05T10:30:00Z

[assign]
agents = ["agent-1"]        # agent IDs assigned to this task
template = "code-writer"     # agent template to use
provider = "anthropic"       # provider for the agents

[deps]
depends_on = ["def456"]      # task IDs that must merge before this one starts
```

Team members see tasks by pulling the repo. No external service needed. Task status updates are committed and pushed as they happen.

### Task Creation

Tasks can be created through multiple entry points:

1. **Manual** — write a title + description directly. For well-understood, scoped work
2. **From GitHub issues** — import an issue as a task. The issue body becomes the task description, labels inform template selection
3. **AI-assisted planning** — describe a high-level goal in the **Planner pane**. An agent breaks it into tasks with dependencies. You edit the plan visually before approving

### Planner Pane

A dedicated pane type for turning goals into executable task plans:

1. Input a goal (e.g., "add OAuth login with Google and GitHub")
2. An agent analyzes the codebase and produces a task graph — tasks with descriptions, dependency edges, and suggested templates/providers
3. The plan renders as a **visual node graph** — you can:
   - Drag to reorder tasks
   - Draw/remove dependency lines
   - Edit task descriptions inline
   - Add or remove tasks
   - Change assigned templates and providers
4. Approve the plan → tasks are created in `.pane/tasks/` and begin executing (respecting dependency order)

### Validation

Before marking a task complete, the agent runs the project's configured checks:

- Test suite
- Linter
- Type checker
- Any custom validation commands defined in workspace settings

If checks fail, the agent attempts to fix the issues and re-run. The task only moves to `review` when all checks pass (or after a configurable retry limit).

### Review Flow

When an agent completes a task:
1. The task status moves to `review`
2. Pane shows an in-app diff view (file-by-file, hunk-by-hunk)
3. You can approve (triggers merge), request changes (agent resumes work), or close the task
4. On merge, the worktree is cleaned up and the task file is updated to `merged`

### Post-Merge Pipeline

After a task is merged, Pane can automate the remaining steps:

1. **Push** — push the merged branch to the remote
2. **PR creation** — create a GitHub PR with auto-generated title and description from the task
3. **Deploy trigger** — trigger CI/CD pipelines or deploy workflows

Each step is configurable per-workspace. The full pipeline runs automatically on merge, or you can run steps manually.

### Manual Work

The task system is primarily for agent-delegated work. When you're coding manually, you work directly in editor panes on your branch — no task overhead required. You can optionally create a task for your own manual work if you want it tracked on the board alongside agent tasks.

---

## Search & Navigation

Two complementary approaches:

### Built-in Search Pane

A search pane type for fast, direct lookups across all projects in the workspace:

- **File search** — fuzzy file name matching (Cmd+P equivalent)
- **Text search** — full-text search with regex support (Cmd+Shift+F equivalent)
- **Symbol search** — go-to-definition, find references, powered by tree-sitter

Results link directly to editor panes — click a result to open/focus the file at that line.

### Agent-Assisted Search

For complex queries that go beyond pattern matching:

- "Find all places where we handle authentication errors"
- "What functions call this API endpoint?"
- "Show me how the payment flow works end-to-end"

Ask any agent, and it uses its search/grep tools plus codebase understanding to answer. The built-in search handles the 80% case; agents handle the rest.

---

## Architecture (Rust + Swift)

### Rust Engine (`crates/`)

| Crate | Responsibility |
|-------|---------------|
| `pane-core` | Workspace model, project management, file tree, buffer management |
| `pane-agent` | Agent lifecycle, provider trait, session model, template storage |
| `pane-tools` | Tool execution for agents: file ops, search, shell, git |
| `pane-canvas` | Canvas state: pane positions, sizes, z-order, links between panes |
| `pane-tasks` | Task lifecycle, worktree management, dependency resolution, TOML storage |
| `pane-protocol` | UniFFI bridge — exposes engine to Swift |

### Swift App (`app/`)

| Layer | Responsibility |
|-------|---------------|
| **Metal Canvas** | Renders the infinite canvas: zoom, pan, pane compositing, connection lines |
| **Pane Views** | SwiftUI views for each pane type (editor, agent, terminal, preview) |
| **Input Handling** | Keyboard shortcuts, trackpad gestures, focus management |
| **State Sync** | Observes Rust engine state via UniFFI, drives SwiftUI updates |

### Data Flow

```
User input → Swift (gesture/keyboard) → Canvas state (Rust)
                                            ↓
                                      Pane layout updates
                                            ↓
                              Swift (Metal render + SwiftUI pane content)

Agent action → Rust (agent provider) → Tool execution → Buffer update
                                                             ↓
                                                   Swift (editor live sync)
```

---

## Workspace Persistence

Workspaces serialize to disk and fully restore:

```
~/.pane/workspaces/<id>/
    workspace.json      # projects, settings, canvas state
    agents/
        <agent-id>.json # conversation history, template ref, provider config
    layout.json         # pane positions, sizes, z-order, links
```

---

## Development Flow (End-to-End)

A complete cycle through Pane looks like this:

```
1. Open workspace          → canvas restores with all panes, agents, layout
2. Plan                    → open Planner pane, describe a goal, get a task graph
3. Edit the plan           → drag tasks, adjust deps, pick templates/providers
4. Approve                 → tasks created in .pane/tasks/, execution begins
5. Agents work             → each task on its own worktree, live on canvas
6. Validation              → agents run tests/lint/typecheck, fix failures
7. Review                  → in-app diff review per task, approve or request changes
8. Merge                   → branch merged locally, worktree cleaned up
9. Ship                    → push, PR creation, deploy trigger (configurable)
10. Meanwhile...           → you're editing code on your own branch the whole time
```

Multiple tasks run in parallel. Dependencies are respected. You can zoom out to see everything at a glance, or zoom in to watch a specific agent work.

---

## Open Questions

- **Conflict resolution**: When two agents edit the same file simultaneously within the same worktree, how are conflicts handled? (Lock per-file? Merge? Last-write-wins?) Cross-task conflicts are handled by git merge at merge time.
- **Canvas performance**: At what pane count does Metal compositing become a bottleneck? Need to prototype early.
- **Template sharing**: Should templates be exportable/importable between users? (Marketplace potential?)
- **Agent cost visibility**: Should the UI show token usage / cost per agent in real-time?
- **Task commit strategy**: Should agents commit continuously as they work, or produce one squashed commit at the end?
- **Task failure/retry**: If an agent gets stuck or errors out mid-task, what's the recovery flow? Auto-retry, manual intervention, or reassign to a different agent/provider?
- **Task board UI**: Should the Planner pane double as a task board (showing active/completed tasks), or is the task board a separate pane type?
- **GitHub issue sync**: When importing issues as tasks, should task status sync back to GitHub (close issue on merge, post progress comments)?
- **Worktree limits**: How many concurrent worktrees are practical? Should Pane enforce a limit or let the OS/disk be the constraint?
