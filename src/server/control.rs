#![allow(dead_code)]
//! Control mode (-CC): line-based protocol for programmatic control.
//!
//! When started with `-CC`, the client uses stdin/stdout instead of a TUI.
//! The protocol uses tmux-compatible notification lines:
//!
//! - `%begin <timestamp> <num> <flags>` / `%end ...` — command output boundaries
//! - `%output %<pane_id> <data>` — pane output data
//! - `%layout-change ...` — window layout changed
//! - `%window-add @<id>` — new window created
//! - `%window-close @<id>` — window closed
//! - `%session-changed $<id> <name>` — session changed
//! - `%exit` — server is shutting down

use std::io::{self, BufRead, Write};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, Mutex};

use crate::server::command;
use crate::server::command_parser;
use crate::server::id_map::IdMap;
use crate::server::protocol::ServerResponse;
use crate::server::state::ServerState;

/// Run control mode: read commands from stdin, execute them, and write
/// notification events to stdout in tmux-compatible format.
pub async fn run_control_mode(
    state: Arc<Mutex<ServerState>>,
    id_map: Arc<Mutex<IdMap>>,
    broadcast_tx: broadcast::Sender<ServerResponse>,
    mut broadcast_rx: broadcast::Receiver<ServerResponse>,
) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Print initial greeting
    writeln!(out, "%begin 0 0 0")?;
    writeln!(out, "pane")?;
    writeln!(out, "%end 0 0 0")?;
    out.flush()?;
    drop(out);

    // Spawn a task to forward broadcast events to stdout
    let notify_task = tokio::spawn(async move {
        let stdout = io::stdout();
        while let Ok(response) = broadcast_rx.recv().await {
            let mut out = stdout.lock();
            match response {
                ServerResponse::PaneOutput { pane_id, data } => {
                    // Base64-encode binary data for control mode
                    let encoded = base64_encode(&data);
                    let _ = writeln!(out, "%output {} {}", pane_id, encoded);
                }
                ServerResponse::PaneExited { pane_id } => {
                    let _ = writeln!(out, "%pane-exited {}", pane_id);
                }
                ServerResponse::LayoutChanged { render_state } => {
                    let _ = writeln!(
                        out,
                        "%layout-change {} workspaces {} active",
                        render_state.workspaces.len(),
                        render_state.active_workspace,
                    );
                }
                ServerResponse::StatsUpdate(_) => {
                    // Skip stats in control mode
                }
                ServerResponse::SessionEnded => {
                    let _ = writeln!(out, "%exit");
                    let _ = out.flush();
                    break;
                }
                ServerResponse::Attached => {
                    let _ = writeln!(out, "%session-changed pane");
                }
                ServerResponse::Error(msg) => {
                    let _ = writeln!(out, "%error {}", msg);
                }
                ServerResponse::FullScreenDump { pane_id, data } => {
                    let encoded = base64_encode(&data);
                    let _ = writeln!(out, "%screen-dump {} {}", pane_id, encoded);
                }
                ServerResponse::ClientCountChanged(count) => {
                    let _ = writeln!(out, "%client-count {}", count);
                }
                ServerResponse::CommandOutput { .. } => {
                    // CommandSync responses are sent directly, not broadcast
                }
                ServerResponse::PluginSegments(_) => {
                    // Skip plugin segments in control mode
                }
            }
            let _ = out.flush();
        }
    });

    // Read commands from stdin
    let stdin = io::stdin();
    let reader = stdin.lock();
    let mut cmd_num: u64 = 1;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // EOF or read error
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let stdout = io::stdout();
        let mut out = stdout.lock();

        match command_parser::parse(trimmed) {
            Ok(cmd) => {
                let mut state = state.lock().await;
                let mut id_map = id_map.lock().await;

                writeln!(
                    out,
                    "%begin {} {} 0",
                    chrono::Utc::now().timestamp(),
                    cmd_num
                )?;

                match command::execute(&cmd, &mut state, &mut id_map, &broadcast_tx) {
                    Ok(command::CommandResult::Ok(output)) => {
                        if !output.is_empty() {
                            writeln!(out, "{}", output)?;
                        }
                        writeln!(out, "%end {} {} 0", chrono::Utc::now().timestamp(), cmd_num)?;
                    }
                    Ok(command::CommandResult::OkWithId { output, .. }) => {
                        if !output.is_empty() {
                            writeln!(out, "{}", output)?;
                        }
                        writeln!(out, "%end {} {} 0", chrono::Utc::now().timestamp(), cmd_num)?;
                    }
                    Ok(command::CommandResult::LayoutChanged) => {
                        writeln!(out, "%end {} {} 0", chrono::Utc::now().timestamp(), cmd_num)?;
                    }
                    Ok(command::CommandResult::SessionEnded) => {
                        writeln!(out, "%end {} {} 0", chrono::Utc::now().timestamp(), cmd_num)?;
                        out.flush()?;
                        break;
                    }
                    Ok(command::CommandResult::DetachRequested) => {
                        writeln!(out, "%end {} {} 0", chrono::Utc::now().timestamp(), cmd_num)?;
                    }
                    Err(e) => {
                        writeln!(out, "%error {}", e)?;
                        writeln!(out, "%end {} {} 1", chrono::Utc::now().timestamp(), cmd_num)?;
                    }
                }
                cmd_num += 1;
            }
            Err(e) => {
                writeln!(
                    out,
                    "%begin {} {} 0",
                    chrono::Utc::now().timestamp(),
                    cmd_num
                )?;
                writeln!(out, "%error {}", e)?;
                writeln!(out, "%end {} {} 1", chrono::Utc::now().timestamp(), cmd_num)?;
                cmd_num += 1;
            }
        }
        let _ = io::stdout().lock().flush();
    }

    notify_task.abort();
    Ok(())
}

/// Simple base64 encoding for control mode output.
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    let chunks = data.chunks(3);

    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_base64_encode_hello() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn test_base64_encode_single_byte() {
        assert_eq!(base64_encode(b"a"), "YQ==");
    }

    #[test]
    fn test_base64_encode_two_bytes() {
        assert_eq!(base64_encode(b"ab"), "YWI=");
    }

    #[test]
    fn test_base64_encode_three_bytes() {
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[test]
    fn test_base64_roundtrip_concept() {
        // Just verify our encoding matches expected values
        assert_eq!(base64_encode(b"Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
    }
}
