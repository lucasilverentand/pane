//! tmux CLI compatibility shim.
//!
//! Translates raw `tmux` CLI arguments into Pane commands, sends them
//! via the Unix socket using the `CommandSync` protocol, and formats
//! the output in tmux-compatible format.

use anyhow::{bail, Result};
use tokio::net::UnixStream;

use crate::server::daemon;
use crate::server::framing;
use crate::server::protocol::{ClientRequest, ServerResponse};

/// Entry point: handle `pane tmux <args...>`.
pub fn handle_tmux_args(args: Vec<String>) -> Result<()> {
    // Handle -V (version) before anything else — no socket needed.
    if args.first().map(|a| a.as_str()) == Some("-V") {
        println!("pane {} (tmux-compatible)", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Determine the tmux subcommand (skip global flags like -S, -L, -f).
    let (session_override, subcmd, subcmd_args) = parse_global_flags(&args)?;

    match subcmd.as_str() {
        "-V" => {
            println!("pane {} (tmux-compatible)", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "has-session" | "has" => {
            let name = extract_session_name(&subcmd_args, &session_override)?;
            let sessions = daemon::list_sessions();
            if sessions.iter().any(|s| s == &name) {
                Ok(())
            } else {
                // tmux exits 1 for missing session
                std::process::exit(1);
            }
        }
        "list-sessions" | "ls" => {
            let sessions = daemon::list_sessions();
            if sessions.is_empty() {
                eprintln!("no server running on this host");
                std::process::exit(1);
            }
            for name in &sessions {
                println!("{}: 1 windows (created -) [80x24]", name);
            }
            Ok(())
        }
        "new-session" | "new" => handle_new_session(&subcmd_args, &session_override),
        "kill-session" => handle_kill_session(&subcmd_args, &session_override),
        _ => {
            // All other commands go through the socket via CommandSync.
            handle_socket_command(&subcmd, &subcmd_args, &session_override)
        }
    }
}

/// Parse global tmux flags: -S socket-path, -L socket-name, -f config-file.
/// Returns (session_override, subcommand, remaining_args).
fn parse_global_flags(args: &[String]) -> Result<(Option<String>, String, Vec<String>)> {
    let mut session = None;
    let mut i = 0;

    // Skip global flags before the subcommand
    while i < args.len() {
        match args[i].as_str() {
            "-S" if i + 1 < args.len() => {
                // -S socket-path: extract session name from path
                i += 2;
            }
            "-L" if i + 1 < args.len() => {
                session = Some(args[i + 1].clone());
                i += 2;
            }
            "-f" if i + 1 < args.len() => {
                i += 2; // skip config file
            }
            s if s.starts_with('-') && !is_subcommand(s) => {
                i += 1;
            }
            _ => break,
        }
    }

    if i >= args.len() {
        bail!("no tmux subcommand specified");
    }

    let subcmd = args[i].clone();
    let rest = args[i + 1..].to_vec();
    Ok((session, subcmd, rest))
}

fn is_subcommand(s: &str) -> bool {
    matches!(
        s,
        "has-session" | "has" | "list-sessions" | "ls" | "new-session" | "new"
            | "kill-session" | "new-window" | "neww" | "kill-window" | "killw"
            | "split-window" | "splitw" | "send-keys" | "send"
            | "select-pane" | "selectp" | "select-window" | "selectw"
            | "list-panes" | "lsp" | "list-windows" | "lsw"
            | "kill-pane" | "killp" | "kill-server"
            | "rename-session" | "rename-window" | "renamew"
            | "select-layout" | "resize-pane" | "resizep"
            | "display-message" | "display"
            | "set-option" | "set"
    )
}

/// Extract session name from -t flag or override.
fn extract_session_name(args: &[String], session_override: &Option<String>) -> Result<String> {
    // Check -t flag
    for (i, arg) in args.iter().enumerate() {
        if arg == "-t" {
            if let Some(name) = args.get(i + 1) {
                // Target may include `:window.pane`, take just the session part
                let session = name.split(':').next().unwrap_or(name);
                return Ok(session.to_string());
            }
        }
    }
    if let Some(s) = session_override {
        return Ok(s.clone());
    }
    // Try TMUX env var to find current session
    if let Ok(tmux_val) = std::env::var("TMUX") {
        if let Some(sock_path) = tmux_val.split(',').next() {
            let path = std::path::Path::new(sock_path);
            if let Some(stem) = path.file_stem() {
                return Ok(stem.to_string_lossy().into_owned());
            }
        }
    }
    Ok("default".to_string())
}

/// Determine which session to connect to for socket commands.
fn resolve_session(args: &[String], session_override: &Option<String>) -> Result<String> {
    // Check -t flag for session:window.pane format
    for (i, arg) in args.iter().enumerate() {
        if arg == "-t" {
            if let Some(target) = args.get(i + 1) {
                // If target contains ':', the part before ':' is the session name
                if let Some(session) = target.split(':').next() {
                    if !session.starts_with('%') && !session.starts_with('@') && !session.is_empty() {
                        // Check if this is actually a session name (not a pane/window target)
                        let sessions = daemon::list_sessions();
                        if sessions.iter().any(|s| s == session) {
                            return Ok(session.to_string());
                        }
                    }
                }
            }
        }
    }

    if let Some(s) = session_override {
        return Ok(s.clone());
    }

    // Use TMUX env var
    if let Ok(tmux_val) = std::env::var("TMUX") {
        if let Some(sock_path) = tmux_val.split(',').next() {
            let path = std::path::Path::new(sock_path);
            if let Some(stem) = path.file_stem() {
                return Ok(stem.to_string_lossy().into_owned());
            }
        }
    }

    // Fall back to first running session
    let sessions = daemon::list_sessions();
    if let Some(first) = sessions.first() {
        return Ok(first.clone());
    }

    bail!("no sessions running");
}

fn handle_new_session(args: &[String], session_override: &Option<String>) -> Result<()> {
    let mut name = session_override.clone();
    let mut window_name = None;
    let mut detached = false;
    let mut print_info = false;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-s" if i + 1 < args.len() => { name = Some(args[i + 1].clone()); i += 2; }
            "-n" if i + 1 < args.len() => { window_name = Some(args[i + 1].clone()); i += 2; }
            "-d" => { detached = true; i += 1; }
            "-P" => { print_info = true; i += 1; }
            "-F" if i + 1 < args.len() => { format = Some(args[i + 1].clone()); i += 2; }
            _ => { i += 1; }
        }
    }

    let session_name = name.unwrap_or_else(|| "default".to_string());

    // Check if session already exists
    let sessions = daemon::list_sessions();
    if sessions.iter().any(|s| s == &session_name) {
        // Session exists — if we need to create resources in it, send command
        if window_name.is_some() || print_info {
            let mut cmd_parts = vec!["new-window".to_string()];
            if let Some(ref wname) = window_name {
                cmd_parts.push("-n".to_string());
                cmd_parts.push(wname.clone());
            }
            let cmd_str = cmd_parts.join(" ");
            let rt = tokio::runtime::Runtime::new()?;
            let result = rt.block_on(send_command_sync(&session_name, &cmd_str))?;
            if print_info {
                print_formatted_output(&result, &format, &session_name);
            }
        }
        return Ok(());
    }

    if detached {
        // Start server in background
        let config = crate::config::Config::load();
        let session_name_clone = session_name.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(daemon::run_server(session_name_clone, config));
        });
        // Wait briefly for the server to start
        std::thread::sleep(std::time::Duration::from_millis(200));

        if let Some(wname) = &window_name {
            let rt = tokio::runtime::Runtime::new()?;
            let _ = rt.block_on(send_command_sync(&session_name, &format!("rename-window {}", wname)));
        }

        if print_info {
            let fmt = format.as_deref().unwrap_or("#{session_name}:");
            let output = fmt
                .replace("#{session_name}", &session_name)
                .replace("#{window_id}", "@0")
                .replace("#{window_index}", "0")
                .replace("#{pane_id}", "%0");
            println!("{}", output);
        }
    }

    Ok(())
}

fn handle_kill_session(args: &[String], session_override: &Option<String>) -> Result<()> {
    let name = extract_session_name(args, session_override)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(daemon::kill_session(&name))
}

/// Send a command through the socket and get a synchronous response.
fn handle_socket_command(subcmd: &str, args: &[String], session_override: &Option<String>) -> Result<()> {
    let session = resolve_session(args, session_override)?;

    // Check for -P (print info) and -F (format) flags
    let mut print_info = false;
    let mut format = None;
    for (i, arg) in args.iter().enumerate() {
        if arg == "-P" {
            print_info = true;
        }
        if arg == "-F" {
            format = args.get(i + 1).cloned();
        }
    }

    // Build the command string
    let cmd_str = build_command_string(subcmd, args);

    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(send_command_sync(&session, &cmd_str))?;

    match result {
        ServerResponse::CommandOutput { output, pane_id, window_id, success } => {
            if !success {
                eprintln!("{}", output);
                std::process::exit(1);
            }
            if print_info || subcmd == "display-message" {
                // Format output for -P -F
                if let Some(fmt) = &format {
                    let formatted = fmt
                        .replace("#{pane_id}", &pane_id.map(|n| format!("%{}", n)).unwrap_or_default())
                        .replace("#{window_id}", &window_id.map(|n| format!("@{}", n)).unwrap_or_default())
                        .replace("#{window_index}", &window_id.map(|n| format!("{}", n)).unwrap_or_default())
                        .replace("#{session_name}", &session);
                    println!("{}", formatted);
                } else if let Some(pane_n) = pane_id {
                    println!("%{}", pane_n);
                }
            } else if !output.is_empty() {
                // list-* commands print their output
                if subcmd.starts_with("list-") || subcmd == "lsp" || subcmd == "lsw" {
                    println!("{}", output);
                } else if subcmd == "display-message" || subcmd == "display" {
                    println!("{}", output);
                }
            }
        }
        ServerResponse::Error(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
        _ => {}
    }

    Ok(())
}

/// Build a tmux command string from subcommand and args.
fn build_command_string(subcmd: &str, args: &[String]) -> String {
    let mut parts = vec![subcmd.to_string()];
    for arg in args {
        // Quote args containing spaces
        if arg.contains(' ') || arg.contains('"') {
            parts.push(format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\"")));
        } else {
            parts.push(arg.clone());
        }
    }
    parts.join(" ")
}

/// Connect to the session socket, send a CommandSync, return the response.
async fn send_command_sync(session_name: &str, cmd: &str) -> Result<ServerResponse> {
    let path = daemon::socket_path(session_name);
    if !path.exists() {
        bail!("no server running for session '{}'", session_name);
    }
    let mut stream = UnixStream::connect(&path).await?;
    framing::send(&mut stream, &ClientRequest::CommandSync(cmd.to_string())).await?;
    let response: ServerResponse = framing::recv_required(&mut stream).await?;
    Ok(response)
}

fn print_formatted_output(response: &ServerResponse, format: &Option<String>, session_name: &str) {
    if let ServerResponse::CommandOutput { pane_id, window_id, .. } = response {
        if let Some(fmt) = format {
            let formatted = fmt
                .replace("#{pane_id}", &pane_id.map(|n| format!("%{}", n)).unwrap_or_default())
                .replace("#{window_id}", &window_id.map(|n| format!("@{}", n)).unwrap_or_default())
                .replace("#{window_index}", &window_id.map(|n| format!("{}", n)).unwrap_or_default())
                .replace("#{session_name}", session_name);
            println!("{}", formatted);
        } else if let Some(pane_n) = pane_id {
            println!("%{}", pane_n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_command_string_simple() {
        let cmd = build_command_string("split-window", &["-h".to_string()]);
        assert_eq!(cmd, "split-window -h");
    }

    #[test]
    fn test_build_command_string_with_target() {
        let cmd = build_command_string("send-keys", &["-t".to_string(), "%0".to_string(), "echo hello".to_string(), "Enter".to_string()]);
        assert_eq!(cmd, "send-keys -t %0 \"echo hello\" Enter");
    }

    #[test]
    fn test_parse_global_flags_simple() {
        let args: Vec<String> = vec!["split-window".to_string(), "-h".to_string()];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, None);
        assert_eq!(subcmd, "split-window");
        assert_eq!(rest, vec!["-h".to_string()]);
    }

    #[test]
    fn test_parse_global_flags_with_socket_name() {
        let args: Vec<String> = vec!["-L".to_string(), "mysession".to_string(), "list-windows".to_string()];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, Some("mysession".to_string()));
        assert_eq!(subcmd, "list-windows");
        assert!(rest.is_empty());
    }

    #[test]
    fn test_is_subcommand() {
        assert!(is_subcommand("split-window"));
        assert!(is_subcommand("send-keys"));
        assert!(is_subcommand("has-session"));
        assert!(!is_subcommand("-V"));
        assert!(!is_subcommand("-S"));
    }
}
