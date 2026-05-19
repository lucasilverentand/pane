# Pane — Greenfield Implementation Plan

## Context

Building Pane from scratch as a native macOS IDE. The product spec (`SPEC.md`) defines the vision: an infinite canvas where AI agents and code editing are peers, with a git-native task system for orchestrating parallel agent work across worktrees.

**Starting point:** Empty repo. Everything is new.

**Stack:** Rust engine + SwiftUI pane content + Metal canvas rendering, connected via UniFFI. Tuist for the Xcode project. No SPM.

---

## Phase 1: Project Scaffold + Canvas Proof-of-Concept

**Goal:** App launches, shows an infinite canvas you can pan and zoom. One hardcoded rectangle renders on it. This proves the Metal + SwiftUI hybrid approach works before building anything else.

### Rust
- `Cargo.toml` workspace with one crate: `pane-core`
- `pane-core`: `CanvasState` (camera x/y/zoom), `Pane` (id, position, size, type), basic CRUD operations
- No UniFFI yet — canvas state is Swift-only in this phase

### Swift
- Tuist `Project.swift` — macOS app target, deployment target macOS 15
- `CanvasView` — `NSViewRepresentable` wrapping `MTKView`
- Metal renderer: camera transform matrix, draw pane rectangles as quads with rounded corners
- Input: trackpad pan (two-finger scroll), pinch zoom, click to select, drag to move
- One hardcoded pane rendered as a blue rounded rect

### Deliverable
App launches → infinite grey canvas → one blue rectangle → pan/zoom/drag works smoothly.

**Key risk retired:** Metal rendering + gesture handling for the canvas.

---

## Phase 2: SwiftUI-in-Metal Pane Rendering

**Goal:** Panes render actual SwiftUI content (not just colored rectangles). This is the hardest rendering problem — solve it early.

### Approach
- Render each pane's SwiftUI view into an offscreen `CALayer` / `NSBitmapImageRep`
- Upload the bitmap as a Metal texture
- Metal composites the texture at the pane's canvas position, scaled by camera zoom
- Re-render texture when pane content changes (dirty flag)
- At extreme zoom-out levels, reduce re-render frequency (LOD)

### Test panes
- A simple SwiftUI `Text("Hello")` pane
- A pane with a `TextEditor` — verify text input works when pane is focused
- Multiple panes at different positions — verify compositing

### Deliverable
Canvas renders N panes with live SwiftUI content. You can type in a focused text pane. Zoom in/out shows scaled content.

**Key risk retired:** SwiftUI content → Metal texture pipeline at interactive framerates.

---

## Phase 3: Rust Engine + UniFFI Bridge

**Goal:** Rust owns all state (workspace, canvas, buffers). Swift reads state via UniFFI and sends mutations back.

### New crates
```
crates/
  pane-core/         # workspace model, project, file tree, buffer manager
  pane-canvas/       # canvas state, pane CRUD, links
  pane-protocol/     # UniFFI bridge — PaneEngine object
```

### `pane-core`
- `Workspace` { id, name, projects, settings, created_at }
- `Project` { name, root: PathBuf }
- `FileNode` + `build_file_tree()` (recursive, with ignore rules)
- `BufferManager` — open/edit/save/close, dirty tracking

### `pane-canvas`
- `CanvasState` { camera: Camera, panes: Vec<Pane>, links: Vec<PaneLink> }
- `Camera` { x: f64, y: f64, zoom: f64 }
- `Pane` { id, pane_type, x, y, width, height, z_index, metadata }
- `PaneType` enum: Editor, Agent, Terminal, FileTree, Search, Planner, Preview
- `PaneLink` { from_id, to_id, link_type } — for agent parent→child
- Operations: add, remove, move, resize, reorder_z, add_link, remove_link

### `pane-protocol`
- `PaneEngine` UniFFI object
- Workspace ops: create, add_project, list_projects, save, load
- Canvas ops: add_pane, remove_pane, move_pane, resize_pane, get_state, set_camera
- Buffer ops: open_file, update_buffer, save_buffer, close_buffer
- File tree: file_tree(project_index)
- FFI record types for all the above

### Swift migration
- `AppModel` becomes a thin wrapper around `PaneEngine`
- Canvas reads layout from `PaneEngine.get_state()`
- All mutations go through `PaneEngine` calls

### Deliverable
Same canvas as Phase 2, but all state lives in Rust. Adding/removing/moving panes goes through the UniFFI bridge. `cargo test` passes for all Rust crates.

---

## Phase 4: Editor + File Tree Panes

**Goal:** You can open a project, browse files, and edit code. Pane becomes a usable (minimal) editor.

### File tree pane
- SwiftUI tree view reading from `PaneEngine.file_tree()`
- Click file → opens in an editor pane (creates one if needed, or uses the nearest existing one)
- File/directory icons, expand/collapse

### Editor pane
- Integrate CodeEditSourceEditor (SPM dep via Tuist)
- Tab bar: multiple files per editor pane, close/reorder tabs
- Content loaded via `PaneEngine.open_file()`
- Edits sync to Rust via `PaneEngine.update_buffer()`
- Cmd+S saves via `PaneEngine.save_buffer()`
- Dirty indicator on tab, unsaved changes warning

### Workspace persistence
- `WorkspaceStore` — save/load workspace + canvas layout to `~/.pane/workspaces/<id>/`
- Auto-save on quit, restore on launch
- "Open Workspace" / "New Workspace" in app menu

### Deliverable
Launch → pick a folder → file tree pane appears → click file → editor pane opens with syntax highlighting → edit → save. Close app → reopen → workspace restores with the same panes and files.

---

## Phase 5: Agent Engine + Agent Pane

**Goal:** Talk to an AI agent inside a canvas pane. Agent can read/write files in the workspace.

### New crates
```
crates/
  pane-agent/        # agent session, provider trait, templates
  pane-tools/        # tool execution: file ops, search, shell
```

### `pane-agent`
- `AgentSession` { id, name, provider, model, messages, status, system_prompt, parent_id }
- `AgentProvider` trait — `async fn send(session, message) → Stream<AgentEvent>`
- `AgentEvent` enum: Text, ToolUse, ToolResult, Done, Error
- `AnthropicProvider` — first real implementation
  - Streaming SSE via `reqwest`
  - Tool definitions mapped from `pane-tools`
  - API key from env var or workspace settings
- `AgentTemplate` { name, system_prompt, allowed_tools, default_provider, description }
- `TemplateStore` — load/save from `~/.pane/templates/` (global) and `.pane/templates/` (workspace)
- Agent session persistence to `~/.pane/workspaces/<id>/agents/`

### `pane-tools`
- `ToolCall` / `ToolResult` types
- `execute_tool()` dispatcher
- Tools: `read_file`, `write_file`, `list_directory`, `search_files` (ripgrep), `run_command`
- All tools are scoped to workspace project roots (security boundary)

### UniFFI additions (`pane-protocol`)
- `create_agent(name, provider, model) → agent_id`
- `send_message(agent_id, message)` — starts streaming
- `get_agent(agent_id) → FfiAgentSession`
- `list_agents() → Vec<FfiAgentSummary>`
- Callback interface for streaming events: `on_agent_event(agent_id, event)`

### Agent pane (Swift)
- Message list: user and assistant bubbles, streaming text
- Tool use indicators (file read/write/search shown inline)
- Text input + send button
- Provider/model display in pane header
- Loading state while agent is running

### Live editor sync
- When agent's `write_file` tool executes → updates Rust buffer
- Buffer change fires a UniFFI callback → Swift
- Editor pane re-reads buffer content → CodeEditSourceEditor updates
- Characters appear in real-time as agent writes

### Sub-agent spawning
- Agent can call `spawn_agent` tool → creates new `AgentSession` with `parent_id`
- New agent pane appears on canvas
- `PaneLink` drawn between parent and child panes

### Deliverable
Create agent pane → pick Claude → type "read the README and summarize it" → agent streams response → agent reads file via tool → you see the tool call inline → response completes. Agent edits a file → editor pane updates live.

---

## Phase 6: Task System + Worktrees

**Goal:** Create tasks, run agents on isolated git worktrees, review diffs, merge back.

### New crate
```
crates/
  pane-tasks/        # task lifecycle, worktree management, deps, storage
```

### `pane-tasks`
- `Task` struct matching TOML schema from spec
- `TaskStatus` enum: Created, Assigned, Running, Validating, Review, Merged, Closed
- `TaskStore` — CRUD for `.pane/tasks/<id>.toml` files
- `TaskDeps` — dependency resolution via topological sort, cycle detection
- `WorktreeManager`:
  - `create(repo_path, branch_name) → worktree_path` — wraps `git worktree add`
  - `remove(worktree_path)` — wraps `git worktree remove`
  - `merge(worktree_branch, target_branch)` — wraps `git merge`
- `ValidationRunner` — execute configured check commands (test/lint/typecheck) in worktree
- `Pipeline` — post-merge steps: push, PR creation (via `gh` CLI), deploy trigger

### UniFFI additions
- `create_task(title, description, base_branch) → task_id`
- `assign_task(task_id, template, provider)`
- `start_task(task_id)` → creates worktree, spawns agents scoped to it
- `list_tasks() → Vec<FfiTask>`
- `get_task_diff(task_id) → Vec<FfiFileDiff>`
- `approve_task(task_id)` → merge + cleanup
- `request_changes(task_id, feedback)` → agent resumes
- `close_task(task_id)`

### Task UI (Swift)
- "New Task" form: title, description, base branch, template/provider picker
- Task status badges on canvas (running/review/merged)
- Task pane group: visual boundary around a task's agent panes

### Review pane (Swift)
- File-by-file diff view (split or unified)
- Hunk-level approve/reject
- Approve / Request Changes / Close action buttons
- Shows validation results (test pass/fail)

### Task flow
1. User creates task → TOML written to `.pane/tasks/`
2. User assigns + starts → worktree created at `pane/task/<slug>` branch
3. Agent spawns in worktree context (all file tools scoped to worktree path)
4. Agent signals done → validation runs in worktree
5. Validation passes → task moves to Review → review pane opens
6. User approves → branch merged into base → worktree removed → TOML updated
7. Post-merge pipeline runs (push/PR/deploy per workspace config)

### Dependency execution
- Task scheduler watches for `merged` transitions
- When a dependency is satisfied, check if all deps for waiting tasks are met
- Auto-start tasks whose deps are all merged

### Deliverable
Create task "add input validation" → assign Claude → agent works on a worktree → runs tests → passes → review pane shows diff → approve → merged into main → worktree cleaned up.

---

## Phase 7: Planner Pane

**Goal:** Describe a goal, get a task graph, edit it visually, approve to create tasks.

### Planner agent
- Dedicated system prompt: analyze the codebase, produce a structured JSON plan
- Plan schema: `{ tasks: [{ title, description, deps: [index] }] }`
- Uses existing `AnthropicProvider` with a planning-specific template

### Graph editor (Swift)
- Task nodes as cards on a mini-canvas within the planner pane
- Drag to reposition nodes
- Click-drag between nodes to draw dependency edges
- Click edge to delete
- Inline edit: title, description, template, provider per node
- Add node / remove node buttons
- "Approve Plan" button → creates all tasks with deps, starts execution

### Deliverable
Open planner → "add OAuth with Google and GitHub" → agent produces 4-task plan → visual graph appears → you drag a node, add a dependency → approve → 4 tasks start executing.

---

## Phase 8: Terminal + Search Panes

**Goal:** Integrated terminal and search, completing the core pane types.

### Terminal pane
- Embed SwiftTerm (or build on pseudo-terminal directly)
- Shell session within the pane
- Working directory defaults to project root
- Persists across pane moves/resizes

### Search pane
- File search: fuzzy filename matching across all workspace projects
- Text search: regex search via ripgrep (backed by `pane-tools::search`)
- Results list with file path + line number + context
- Click result → opens in nearest editor pane at that line

### Deliverable
Terminal pane runs `cargo test`, search pane finds "TODO" across the workspace.

---

## Phase 9: Additional Providers + Polish

### More providers
- OpenAI API provider (GPT-4o, o3)
- Claude Code CLI wrapper (subprocess, parse streaming JSON output)
- Codex CLI wrapper

### Template management UI
- List/create/edit/delete templates
- Template picker in agent creation flow

### GitHub integration
- Import issues as tasks
- Sync task status back to issues (optional)

### Polish
- Canvas performance: texture caching, dirty-rect rendering, LOD at extreme zoom
- Keyboard shortcut system (configurable, Cmd+P command palette)
- App chrome: menu bar, window title, app icon
- Preferences window: API keys, default provider, theme

---

## Dependency Graph

```
P1 Canvas PoC
 └→ P2 SwiftUI-in-Metal
      └→ P3 Rust + UniFFI
           ├→ P4 Editor + File Tree
           │    └→ P5 Agent Engine
           │         └→ P6 Task System
           │              └→ P7 Planner
           └→ P8 Terminal + Search (independent of P5-P7)
                └→ P9 Polish (after P6+)
```

Phases 1-2 retire the biggest rendering risks. Phase 3 establishes the engine. Phases 4-7 build features in dependency order. Phase 8 is independent and can run in parallel with 5-7.

---

## Verification per Phase

| Phase | How to verify |
|-------|--------------|
| 1 | App launches, canvas renders one rect, pan/zoom/drag works at 60fps |
| 2 | Multiple panes render SwiftUI content, text input works in focused pane |
| 3 | `cargo test` passes, canvas reads state from Rust, adding a pane via UniFFI appears on canvas |
| 4 | Open folder → browse → edit → save → reopen app → state restored |
| 5 | Chat with Claude in a pane, agent edits file, editor updates live |
| 6 | Task runs on worktree, validation passes, review diff, approve, branch merged |
| 7 | Planner produces graph, edit visually, approve creates tasks |
| 8 | Terminal runs commands, search finds text across projects |
| 9 | Multiple providers work, templates manageable via UI |
