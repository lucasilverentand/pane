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
    let (_session_override, subcmd, subcmd_args) = parse_global_flags(&args)?;

    match subcmd.as_str() {
        "-V" => {
            println!("pane {} (tmux-compatible)", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "has-session" | "has" => {
            if daemon::is_running() {
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
        "list-sessions" | "ls" => {
            if !daemon::is_running() {
                eprintln!("no server running on this host");
                std::process::exit(1);
            }
            println!("pane: 1 windows (created -) [80x24]");
            Ok(())
        }
        "new-session" | "new" => handle_new_session(&subcmd_args),
        "kill-session" => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(daemon::kill_daemon())
        }
        _ => {
            // All other commands go through the socket via CommandSync.
            handle_socket_command(&subcmd, &subcmd_args)
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
                // -S socket-path: ignored (single daemon)
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
        "has-session"
            | "has"
            | "list-sessions"
            | "ls"
            | "new-session"
            | "new"
            | "kill-session"
            | "new-window"
            | "neww"
            | "kill-window"
            | "killw"
            | "split-window"
            | "splitw"
            | "send-keys"
            | "send"
            | "select-pane"
            | "selectp"
            | "select-window"
            | "selectw"
            | "list-panes"
            | "lsp"
            | "list-windows"
            | "lsw"
            | "kill-pane"
            | "killp"
            | "kill-server"
            | "rename-session"
            | "rename-window"
            | "renamew"
            | "select-layout"
            | "resize-pane"
            | "resizep"
            | "display-message"
            | "display"
            | "set-option"
            | "set"
            | "toggle-fold"
            | "fold"
    )
}

fn handle_new_session(args: &[String]) -> Result<()> {
    let mut window_name = None;
    let mut detached = false;
    let mut print_info = false;
    let mut format = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-s" if i + 1 < args.len() => {
                // Session name ignored (single daemon), skip
                i += 2;
            }
            "-n" if i + 1 < args.len() => {
                window_name = Some(args[i + 1].clone());
                i += 2;
            }
            "-d" => {
                detached = true;
                i += 1;
            }
            "-P" => {
                print_info = true;
                i += 1;
            }
            "-F" if i + 1 < args.len() => {
                format = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    if daemon::is_running() {
        // Daemon exists — create a new workspace
        if window_name.is_some() || print_info {
            let mut cmd_parts = vec!["new-session -d -s workspace".to_string()];
            if let Some(ref wname) = window_name {
                cmd_parts.push(format!("-n {}", wname));
            }
            let cmd_str = cmd_parts.join(" ");
            let rt = tokio::runtime::Runtime::new()?;
            let result = rt.block_on(send_command_sync(&cmd_str))?;
            if print_info {
                print_formatted_output(&result, &format);
            }
        }
        return Ok(());
    }

    if detached {
        // Start daemon in background
        daemon::start_daemon()?;

        if let Some(wname) = &window_name {
            let rt = tokio::runtime::Runtime::new()?;
            let _ = rt.block_on(send_command_sync(&format!("rename-window {}", wname)));
        }

        if print_info {
            let fmt = format.as_deref().unwrap_or("pane:");
            let output = fmt
                .replace("#{session_name}", "pane")
                .replace("#{window_id}", "@0")
                .replace("#{window_index}", "0")
                .replace("#{pane_id}", "%0");
            println!("{}", output);
        }
    }

    Ok(())
}

/// Send a command through the socket and get a synchronous response.
fn handle_socket_command(subcmd: &str, args: &[String]) -> Result<()> {
    if !daemon::is_running() {
        bail!("no server running");
    }

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
    let result = rt.block_on(send_command_sync(&cmd_str))?;

    match result {
        ServerResponse::CommandOutput {
            output,
            pane_id,
            window_id,
            success,
        } => {
            if !success {
                eprintln!("{}", output);
                std::process::exit(1);
            }
            if print_info || subcmd == "display-message" {
                if let Some(fmt) = &format {
                    let formatted = fmt
                        .replace(
                            "#{pane_id}",
                            &pane_id.map(|n| format!("%{}", n)).unwrap_or_default(),
                        )
                        .replace(
                            "#{window_id}",
                            &window_id.map(|n| format!("@{}", n)).unwrap_or_default(),
                        )
                        .replace(
                            "#{window_index}",
                            &window_id.map(|n| format!("{}", n)).unwrap_or_default(),
                        )
                        .replace("#{session_name}", "pane");
                    println!("{}", formatted);
                } else if let Some(pane_n) = pane_id {
                    println!("%{}", pane_n);
                }
            } else if !output.is_empty() {
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
            parts.push(format!(
                "\"{}\"",
                arg.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        } else {
            parts.push(arg.clone());
        }
    }
    parts.join(" ")
}

/// Connect to the daemon socket, send a CommandSync, return the response.
async fn send_command_sync(cmd: &str) -> Result<ServerResponse> {
    let path = daemon::socket_path();
    if !path.exists() {
        bail!("no server running");
    }
    let mut stream = UnixStream::connect(&path).await?;
    framing::send(&mut stream, &ClientRequest::CommandSync(cmd.to_string())).await?;
    let response: ServerResponse = framing::recv_required(&mut stream).await?;
    Ok(response)
}

fn print_formatted_output(response: &ServerResponse, format: &Option<String>) {
    if let ServerResponse::CommandOutput {
        pane_id, window_id, ..
    } = response
    {
        if let Some(fmt) = format {
            let formatted = fmt
                .replace(
                    "#{pane_id}",
                    &pane_id.map(|n| format!("%{}", n)).unwrap_or_default(),
                )
                .replace(
                    "#{window_id}",
                    &window_id.map(|n| format!("@{}", n)).unwrap_or_default(),
                )
                .replace(
                    "#{window_index}",
                    &window_id.map(|n| format!("{}", n)).unwrap_or_default(),
                )
                .replace("#{session_name}", "pane");
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
        let cmd = build_command_string(
            "send-keys",
            &[
                "-t".to_string(),
                "%0".to_string(),
                "echo hello".to_string(),
                "Enter".to_string(),
            ],
        );
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
        let args: Vec<String> = vec![
            "-L".to_string(),
            "mysession".to_string(),
            "list-windows".to_string(),
        ];
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

    #[test]
    fn test_build_command_string_no_args() {
        let cmd = build_command_string("kill-server", &[]);
        assert_eq!(cmd, "kill-server");
    }

    #[test]
    fn test_build_command_string_empty_arg() {
        let cmd = build_command_string("send-keys", &["".to_string()]);
        assert_eq!(cmd, "send-keys ");
    }

    #[test]
    fn test_build_command_string_arg_with_spaces() {
        let cmd = build_command_string("send-keys", &["echo hello world".to_string()]);
        assert_eq!(cmd, r#"send-keys "echo hello world""#);
    }

    #[test]
    fn test_build_command_string_arg_with_quotes() {
        let cmd = build_command_string("send-keys", &[r#"echo "hi""#.to_string()]);
        assert_eq!(cmd, r#"send-keys "echo \"hi\"""#);
    }

    #[test]
    fn test_build_command_string_arg_with_backslash() {
        let cmd = build_command_string("send-keys", &[r"path\to\file".to_string()]);
        assert_eq!(cmd, r"send-keys path\to\file");
    }

    #[test]
    fn test_build_command_string_arg_with_backslash_and_spaces() {
        let cmd = build_command_string("send-keys", &[r"path\to some\file".to_string()]);
        assert_eq!(cmd, r#"send-keys "path\\to some\\file""#);
    }

    #[test]
    fn test_build_command_string_arg_with_quotes_and_backslash() {
        let cmd = build_command_string("send-keys", &[r#"say "hello\" world"#.to_string()]);
        assert_eq!(cmd, r#"send-keys "say \"hello\\\" world""#);
    }

    #[test]
    fn test_build_command_string_multiple_special_args() {
        let cmd = build_command_string(
            "send-keys",
            &[
                "-t".to_string(),
                "%0".to_string(),
                "ls -la".to_string(),
                "Enter".to_string(),
            ],
        );
        assert_eq!(cmd, r#"send-keys -t %0 "ls -la" Enter"#);
    }

    #[test]
    fn test_parse_global_flags_no_flags() {
        let args: Vec<String> = vec!["kill-server".to_string()];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, None);
        assert_eq!(subcmd, "kill-server");
        assert!(rest.is_empty());
    }

    #[test]
    fn test_parse_global_flags_empty_args() {
        let args: Vec<String> = vec![];
        let result = parse_global_flags(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_global_flags_only_flags_no_subcommand() {
        let args: Vec<String> = vec!["-L".to_string(), "test".to_string()];
        let result = parse_global_flags(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_global_flags_socket_path_s() {
        let args: Vec<String> = vec![
            "-S".to_string(),
            "/tmp/my.sock".to_string(),
            "list-sessions".to_string(),
        ];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, None);
        assert_eq!(subcmd, "list-sessions");
        assert!(rest.is_empty());
    }

    #[test]
    fn test_parse_global_flags_config_file() {
        let args: Vec<String> = vec![
            "-f".to_string(),
            "/etc/tmux.conf".to_string(),
            "new-session".to_string(),
            "-s".to_string(),
            "test".to_string(),
        ];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, None);
        assert_eq!(subcmd, "new-session");
        assert_eq!(rest, vec!["-s".to_string(), "test".to_string()]);
    }

    #[test]
    fn test_parse_global_flags_multiple_flags() {
        let args: Vec<String> = vec![
            "-L".to_string(),
            "mysock".to_string(),
            "-f".to_string(),
            "/my/config".to_string(),
            "split-window".to_string(),
            "-h".to_string(),
        ];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, Some("mysock".to_string()));
        assert_eq!(subcmd, "split-window");
        assert_eq!(rest, vec!["-h".to_string()]);
    }

    #[test]
    fn test_parse_global_flags_unknown_flag_skipped() {
        let args: Vec<String> = vec!["-u".to_string(), "list-sessions".to_string()];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, None);
        assert_eq!(subcmd, "list-sessions");
        assert!(rest.is_empty());
    }

    #[test]
    fn test_parse_global_flags_subcommand_at_start() {
        let args: Vec<String> = vec![
            "send-keys".to_string(),
            "-t".to_string(),
            "%0".to_string(),
            "hello".to_string(),
        ];
        let (session, subcmd, rest) = parse_global_flags(&args).unwrap();
        assert_eq!(session, None);
        assert_eq!(subcmd, "send-keys");
        assert_eq!(
            rest,
            vec!["-t".to_string(), "%0".to_string(), "hello".to_string()]
        );
    }

    #[test]
    fn test_is_subcommand_all_valid() {
        let valid = vec![
            "has-session",
            "has",
            "list-sessions",
            "ls",
            "new-session",
            "new",
            "kill-session",
            "new-window",
            "neww",
            "kill-window",
            "killw",
            "split-window",
            "splitw",
            "send-keys",
            "send",
            "select-pane",
            "selectp",
            "select-window",
            "selectw",
            "list-panes",
            "lsp",
            "list-windows",
            "lsw",
            "kill-pane",
            "killp",
            "kill-server",
            "rename-session",
            "rename-window",
            "renamew",
            "select-layout",
            "resize-pane",
            "resizep",
            "display-message",
            "display",
            "set-option",
            "set",
        ];
        for cmd in valid {
            assert!(
                is_subcommand(cmd),
                "expected '{}' to be a valid subcommand",
                cmd
            );
        }
    }

    #[test]
    fn test_is_subcommand_invalid() {
        let invalid = vec![
            "-V",
            "-S",
            "-L",
            "-f",
            "-u",
            "--help",
            "nonexistent",
            "kill",
            "list",
            "split",
            "",
            "KILL-SERVER",
            "Kill-Server",
        ];
        for cmd in invalid {
            assert!(
                !is_subcommand(cmd),
                "expected '{}' to NOT be a valid subcommand",
                cmd
            );
        }
    }
}
