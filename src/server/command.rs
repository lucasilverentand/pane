#![allow(dead_code)]
use anyhow::{bail, Result};
use tokio::sync::broadcast;

use crate::layout::{PaneId, SplitDirection};
use crate::pane::{PaneGroupId, PaneKind};
use crate::server::id_map::IdMap;
use crate::server::protocol::{RenderState, ServerResponse};
use crate::server::state::ServerState;

/// A parsed, typed command that can be executed against the server state.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    // Session commands
    KillServer,
    ListSessions,
    RenameSession { new_name: String },

    // Window (PaneGroup) commands
    NewWindow,
    KillWindow { target: Option<TargetWindow> },
    SelectWindow { target: TargetWindow },
    RenameWindow { target: Option<TargetWindow>, new_name: String },
    ListWindows,

    // Pane commands
    SplitWindow { horizontal: bool, target: Option<TargetPane> },
    KillPane { target: Option<TargetPane> },
    SelectPane { target: TargetPane },
    ListPanes,
    SendKeys { target: Option<TargetPane>, keys: Vec<String> },

    // Layout commands
    SelectLayout { layout_name: String },
    ResizePane { target: Option<TargetPane>, direction: ResizeDirection, amount: u16 },

    // Misc commands
    DisplayMessage { message: String },
}

/// Target specifier for a window (group).
#[derive(Clone, Debug, PartialEq)]
pub enum TargetWindow {
    /// tmux-style `@N` window ID
    Id(u32),
    /// Window index within the current workspace
    Index(usize),
}

/// Target specifier for a pane.
#[derive(Clone, Debug, PartialEq)]
pub enum TargetPane {
    /// tmux-style `%N` pane ID
    Id(u32),
    /// Directional: left, right, up, down
    Direction(PaneDirection),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResizeDirection {
    Left,
    Right,
    Up,
    Down,
}

/// The result of executing a command.
pub enum CommandResult {
    /// Command executed successfully; send this response back.
    Ok(String),
    /// Command caused a layout change; broadcast the new render state.
    LayoutChanged,
    /// Command requires the server to shut down.
    SessionEnded,
}

/// Execute a command against the server state.
pub fn execute(
    cmd: &Command,
    state: &mut ServerState,
    id_map: &mut IdMap,
    broadcast_tx: &broadcast::Sender<ServerResponse>,
) -> Result<CommandResult> {
    match cmd {
        Command::KillServer => {
            let _ = broadcast_tx.send(ServerResponse::SessionEnded);
            Ok(CommandResult::SessionEnded)
        }

        Command::ListSessions => {
            let names = crate::server::daemon::list_sessions();
            let output = if names.is_empty() {
                "no sessions".to_string()
            } else {
                names.join("\n")
            };
            Ok(CommandResult::Ok(output))
        }

        Command::RenameSession { new_name } => {
            state.session_name = new_name.clone();
            Ok(CommandResult::Ok(String::new()))
        }

        Command::NewWindow => {
            let (w, h) = state.last_size;
            let bar_h = state.workspace_bar_height();
            let cols = w.saturating_sub(4);
            let rows = h.saturating_sub(2 + bar_h + 1);
            state.add_tab_to_active_group(PaneKind::Shell, None, cols, rows)?;
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::LayoutChanged)
        }

        Command::KillWindow { target } => {
            let group_id = resolve_window_target(target.as_ref(), state, id_map)?;
            let ws = state.active_workspace_mut();
            if ws.groups.len() <= 1 {
                bail!("cannot kill the last window");
            }
            if let Some(new_focus) = ws.layout.close_pane(group_id) {
                ws.groups.remove(&group_id);
                ws.active_group = new_focus;
                id_map.unregister_window(&group_id);
            }
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::LayoutChanged)
        }

        Command::SelectWindow { target } => {
            let group_id = resolve_window_target(Some(target), state, id_map)?;
            state.active_workspace_mut().active_group = group_id;
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::LayoutChanged)
        }

        Command::RenameWindow { target, new_name } => {
            let group_id = resolve_window_target(target.as_ref(), state, id_map)?;
            let ws = state.active_workspace_mut();
            if let Some(group) = ws.groups.get_mut(&group_id) {
                group.name = Some(new_name.clone());
            }
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::Ok(String::new()))
        }

        Command::ListWindows => {
            let ws = state.active_workspace();
            let mut lines = Vec::new();
            for (gid, group) in &ws.groups {
                let n = id_map.register_window(*gid);
                let name = group.name.as_deref().unwrap_or(
                    group.tabs.get(group.active_tab).map(|p| p.title.as_str()).unwrap_or(""),
                );
                let active = if *gid == ws.active_group { " (active)" } else { "" };
                lines.push(format!("@{}: {} [{} panes]{}", n, name, group.tab_count(), active));
            }
            Ok(CommandResult::Ok(lines.join("\n")))
        }

        Command::SplitWindow { horizontal, target } => {
            if let Some(target) = target {
                let group_id = resolve_pane_to_group(target, state, id_map)?;
                state.active_workspace_mut().active_group = group_id;
            }
            let direction = if *horizontal {
                SplitDirection::Horizontal
            } else {
                SplitDirection::Vertical
            };
            let (w, h) = state.last_size;
            let bar_h = state.workspace_bar_height();
            let cols = w.saturating_sub(4);
            let rows = h.saturating_sub(2 + bar_h + 1);
            state.split_active_group(direction, PaneKind::Shell, cols, rows)?;
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::LayoutChanged)
        }

        Command::KillPane { target } => {
            let (_group_id, pane_id) = resolve_pane_target(target.as_ref(), state, id_map)?;
            // Determine action before mutating
            let action = {
                let ws = state.active_workspace();
                let group = ws.groups.get(&_group_id);
                if let Some(group) = group {
                    if group.tab_count() > 1 {
                        Some("close_tab")
                    } else if ws.groups.len() > 1 || state.workspaces.len() > 1 {
                        Some("close_group")
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            match action {
                Some("close_tab") => {
                    let ws = state.active_workspace_mut();
                    if let Some(group) = ws.groups.get_mut(&_group_id) {
                        if let Some(idx) = group.tabs.iter().position(|p| p.id == pane_id) {
                            group.close_tab(idx);
                            id_map.unregister_pane(&pane_id);
                        }
                    }
                }
                Some("close_group") => {
                    let ws = state.active_workspace_mut();
                    if let Some(new_focus) = ws.layout.close_pane(_group_id) {
                        ws.groups.remove(&_group_id);
                        ws.active_group = new_focus;
                        id_map.unregister_window(&_group_id);
                        id_map.unregister_pane(&pane_id);
                    }
                }
                _ => {
                    bail!("cannot kill the last pane");
                }
            }
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::LayoutChanged)
        }

        Command::SelectPane { target } => {
            let group_id = resolve_pane_to_group(target, state, id_map)?;
            state.active_workspace_mut().active_group = group_id;
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::LayoutChanged)
        }

        Command::ListPanes => {
            let mut lines = Vec::new();
            for ws in &state.workspaces {
                for (gid, group) in &ws.groups {
                    let wn = id_map.register_window(*gid);
                    for pane in &group.tabs {
                        let pn = id_map.register_pane(pane.id);
                        let exited_flag = if pane.exited { " (dead)" } else { "" };
                        lines.push(format!(
                            "%{}: @{} {} [{}]{}", pn, wn, pane.title, pane.kind.label(), exited_flag
                        ));
                    }
                }
            }
            Ok(CommandResult::Ok(lines.join("\n")))
        }

        Command::SendKeys { target, keys } => {
            let (_group_id, pane_id) = resolve_pane_target(target.as_ref(), state, id_map)?;
            if let Some(pane) = state.find_pane_mut(pane_id) {
                for key_str in keys {
                    let bytes = parse_key_literal(key_str);
                    pane.write_input(&bytes);
                }
            }
            Ok(CommandResult::Ok(String::new()))
        }

        Command::SelectLayout { layout_name } => {
            // Delegate to layout presets if available
            let _ = layout_name;
            Ok(CommandResult::Ok("layout selection not yet implemented".to_string()))
        }

        Command::ResizePane { target, direction, amount } => {
            if let Some(target) = target {
                let group_id = resolve_pane_to_group(target, state, id_map)?;
                state.active_workspace_mut().active_group = group_id;
            }
            let active = state.active_workspace().active_group;
            let delta = match direction {
                ResizeDirection::Left | ResizeDirection::Up => -0.05,
                ResizeDirection::Right | ResizeDirection::Down => 0.05,
            };
            for _ in 0..*amount {
                state.active_workspace_mut().layout.resize(active, delta);
            }
            state.update_leaf_mins();
            let (w, h) = state.last_size;
            state.resize_all_panes(w, h);
            broadcast_layout(state, broadcast_tx);
            Ok(CommandResult::LayoutChanged)
        }

        Command::DisplayMessage { message } => {
            Ok(CommandResult::Ok(message.clone()))
        }
    }
}

/// Broadcast a layout update to all connected clients.
fn broadcast_layout(state: &ServerState, broadcast_tx: &broadcast::Sender<ServerResponse>) {
    let render_state = RenderState::from_server_state(state);
    let _ = broadcast_tx.send(ServerResponse::LayoutChanged { render_state });
}

/// Resolve a window target to a PaneGroupId.
fn resolve_window_target(
    target: Option<&TargetWindow>,
    state: &ServerState,
    id_map: &mut IdMap,
) -> Result<PaneGroupId> {
    match target {
        None => Ok(state.active_workspace().active_group),
        Some(TargetWindow::Id(n)) => {
            id_map.window_id(*n).ok_or_else(|| anyhow::anyhow!("no window with id @{}", n))
        }
        Some(TargetWindow::Index(idx)) => {
            let ws = state.active_workspace();
            let group_ids = ws.layout.pane_ids();
            group_ids.get(*idx).copied().ok_or_else(|| anyhow::anyhow!("window index {} out of range", idx))
        }
    }
}

/// Resolve a pane target to both the PaneGroupId and PaneId.
fn resolve_pane_target(
    target: Option<&TargetPane>,
    state: &ServerState,
    id_map: &mut IdMap,
) -> Result<(PaneGroupId, PaneId)> {
    match target {
        None => {
            let ws = state.active_workspace();
            let gid = ws.active_group;
            let pid = ws.groups.get(&gid)
                .map(|g| g.active_pane().id)
                .ok_or_else(|| anyhow::anyhow!("no active pane"))?;
            Ok((gid, pid))
        }
        Some(TargetPane::Id(n)) => {
            let pane_id = id_map.pane_id(*n)
                .ok_or_else(|| anyhow::anyhow!("no pane with id %{}", n))?;
            let (_, group_id) = state.find_pane_location(pane_id)
                .ok_or_else(|| anyhow::anyhow!("pane %{} not found in any workspace", n))?;
            Ok((group_id, pane_id))
        }
        Some(TargetPane::Direction(dir)) => {
            let ws = state.active_workspace();
            let active = ws.active_group;
            let (split_dir, side) = match dir {
                PaneDirection::Left => (SplitDirection::Horizontal, crate::layout::Side::First),
                PaneDirection::Right => (SplitDirection::Horizontal, crate::layout::Side::Second),
                PaneDirection::Up => (SplitDirection::Vertical, crate::layout::Side::First),
                PaneDirection::Down => (SplitDirection::Vertical, crate::layout::Side::Second),
            };
            let neighbor_id = ws.layout.find_neighbor(active, split_dir, side)
                .ok_or_else(|| anyhow::anyhow!("no pane in that direction"))?;
            let pid = ws.groups.get(&neighbor_id)
                .map(|g| g.active_pane().id)
                .ok_or_else(|| anyhow::anyhow!("neighbor group has no panes"))?;
            Ok((neighbor_id, pid))
        }
    }
}

/// Resolve a pane target to just the PaneGroupId.
fn resolve_pane_to_group(
    target: &TargetPane,
    state: &ServerState,
    id_map: &mut IdMap,
) -> Result<PaneGroupId> {
    let (gid, _) = resolve_pane_target(Some(target), state, id_map)?;
    Ok(gid)
}

/// Parse a key literal string into bytes to send to a pane.
/// Supports tmux-style key names: Enter, Escape, Tab, Space, BSpace, etc.
fn parse_key_literal(s: &str) -> Vec<u8> {
    match s {
        "Enter" | "enter" | "CR" | "C-m" => vec![b'\r'],
        "Escape" | "escape" | "Esc" | "esc" => vec![0x1b],
        "Tab" | "tab" => vec![b'\t'],
        "Space" | "space" => vec![b' '],
        "BSpace" | "bspace" | "Backspace" | "backspace" => vec![0x7f],
        "Up" | "up" => vec![0x1b, b'[', b'A'],
        "Down" | "down" => vec![0x1b, b'[', b'B'],
        "Right" | "right" => vec![0x1b, b'[', b'C'],
        "Left" | "left" => vec![0x1b, b'[', b'D'],
        "Home" | "home" => vec![0x1b, b'[', b'H'],
        "End" | "end" => vec![0x1b, b'[', b'F'],
        "PageUp" | "pageup" | "PgUp" => vec![0x1b, b'[', b'5', b'~'],
        "PageDown" | "pagedown" | "PgDn" => vec![0x1b, b'[', b'6', b'~'],
        "Delete" | "delete" | "DC" => vec![0x1b, b'[', b'3', b'~'],
        _ => {
            // C-x style control keys
            if s.len() == 3 && s.starts_with("C-") {
                let ch = s.as_bytes()[2];
                if ch.is_ascii_alphabetic() {
                    return vec![ch.to_ascii_lowercase() - b'a' + 1];
                }
            }
            // Plain text: send characters as UTF-8
            s.as_bytes().to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_literal_enter() {
        assert_eq!(parse_key_literal("Enter"), vec![b'\r']);
        assert_eq!(parse_key_literal("CR"), vec![b'\r']);
    }

    #[test]
    fn test_parse_key_literal_escape() {
        assert_eq!(parse_key_literal("Escape"), vec![0x1b]);
        assert_eq!(parse_key_literal("Esc"), vec![0x1b]);
    }

    #[test]
    fn test_parse_key_literal_control() {
        assert_eq!(parse_key_literal("C-c"), vec![3]); // Ctrl+C
        assert_eq!(parse_key_literal("C-a"), vec![1]); // Ctrl+A
        assert_eq!(parse_key_literal("C-z"), vec![26]); // Ctrl+Z
    }

    #[test]
    fn test_parse_key_literal_arrows() {
        assert_eq!(parse_key_literal("Up"), vec![0x1b, b'[', b'A']);
        assert_eq!(parse_key_literal("Down"), vec![0x1b, b'[', b'B']);
        assert_eq!(parse_key_literal("Right"), vec![0x1b, b'[', b'C']);
        assert_eq!(parse_key_literal("Left"), vec![0x1b, b'[', b'D']);
    }

    #[test]
    fn test_parse_key_literal_plain_text() {
        assert_eq!(parse_key_literal("ls"), b"ls".to_vec());
        assert_eq!(parse_key_literal("a"), b"a".to_vec());
    }

    #[test]
    fn test_parse_key_literal_special() {
        assert_eq!(parse_key_literal("Tab"), vec![b'\t']);
        assert_eq!(parse_key_literal("Space"), vec![b' ']);
        assert_eq!(parse_key_literal("BSpace"), vec![0x7f]);
    }

    #[test]
    fn test_command_equality() {
        assert_eq!(Command::KillServer, Command::KillServer);
        assert_eq!(
            Command::RenameSession { new_name: "x".to_string() },
            Command::RenameSession { new_name: "x".to_string() },
        );
        assert_ne!(Command::KillServer, Command::ListPanes);
    }

    #[test]
    fn test_target_window_variants() {
        let t1 = TargetWindow::Id(5);
        let t2 = TargetWindow::Index(2);
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_target_pane_variants() {
        let t1 = TargetPane::Id(3);
        let t2 = TargetPane::Direction(PaneDirection::Left);
        assert_ne!(t1, t2);
    }
}
