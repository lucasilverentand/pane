# Pane Codebase Improvement Audit

Run a comprehensive audit of the pane codebase to find bugs, gaps, edge cases, and optimization opportunities. Deploy a team of specialized agents in parallel, then synthesize findings into an actionable report.

## Instructions

Launch ALL of the following agents in parallel using the Agent tool (subagent_type: Explore). Each agent should read the relevant files thoroughly and report issues with file path, line number, description, and severity (critical/high/medium/low).

### Agent 1: UI Rendering Audit
Audit all UI rendering code for bugs and gaps:
- `crates/pane-tui/src/ui/mod.rs` — main render dispatch
- `crates/pane-tui/src/ui/project_hub.rs` — home page / project hub
- `crates/pane-tui/src/ui/window_view.rs` — window chrome, tab bars, fold bars
- `crates/pane-tui/src/ui/workspace_bar.rs` — workspace bar
- `crates/pane-tui/src/ui/widget_picker.rs` — widget picker popup

Look for: UTF-8/multibyte panics (byte vs char indexing), overflow/truncation bugs, off-by-one errors, missing edge cases (empty state, zero-width areas, very small terminals), layout calculation errors, hit-test mismatches with rendering, dead code, missing visual indicators, accessibility gaps (color-only distinctions).

### Agent 2: Focus & Modal State Audit
Audit the entire focus system and modal state management:
- Search for `FocusLocation`, `focus_stack`, `push_focus`, `pop_focus`, `Mode::` enum variants
- `crates/pane-tui/src/client.rs` — all focus transitions and mode changes

Look for: push/pop asymmetry (push without matching pop, or `focus_stack.clear()` instead of `pop_focus()`), focus getting stuck or lost, focus not restored after modal dismiss, mode transitions that skip focus management, focus visual indicators missing or wrong, edge cases where focus stack becomes empty/invalid.

### Agent 3: Input Handling Audit
Audit keyboard and mouse input handling:
- `crates/pane-tui/src/client.rs` — all `handle_*_key()` methods and mouse event routing
- Key mapping and normalization logic

Look for: keys not handled in certain focus/mode states, key conflicts, mouse events not routed correctly, input swallowed incorrectly or forwarded when it shouldn't be, missing escape/cancel handling, modal state not cleared on dismiss, race conditions between mouse and keyboard events, drag state not cleaned up, missing keyboard shortcuts (Home/End/Tab/Space in pickers).

### Agent 4: Protocol & Daemon Integration Audit
Audit the client-daemon communication and state synchronization:
- `crates/pane-protocol/src/protocol.rs` — protocol types and messages
- `crates/pane-daemon/src/server/daemon.rs` — daemon-side handling
- `crates/pane-tui/src/client.rs` — client-side request/response handling

Look for: state sync issues (client state diverging from daemon), missing error handling on requests, race conditions in async message handling, stale cache/state not invalidated, missing protocol messages for features, snapshot data inconsistencies.

### Agent 5: Architecture & Code Quality Audit
Audit overall code quality and architecture:
- Scan all crates for duplicated logic that should be extracted into helpers
- Check for `#[allow(dead_code)]` and actually-dead code
- Look for inconsistent patterns (some places do X, others do Y for same purpose)
- Check test coverage gaps for critical paths
- Look for performance issues (unnecessary allocations, repeated calculations per frame)
- Check for unsafe arithmetic (underflows, overflows without saturating ops)

## Output Format

After all agents complete, synthesize findings into a single report with these sections:

### Summary Table
A markdown table of ALL findings sorted by severity, with columns: #, Severity, Area, Issue (one-line), Location (file:line).

### Critical & High Priority
Detailed description of each critical/high issue with reproduction context.

### Recommended Fix Order
Numbered list of fixes in priority order, grouping related issues that should be fixed together.

### Quick Wins
Issues that are easy to fix (< 5 min each) listed separately so they can be knocked out fast.

Keep the report concise and actionable — no fluff, just findings and fix guidance.
