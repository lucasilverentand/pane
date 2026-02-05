#![allow(dead_code)]
use anyhow::{bail, Result};

use crate::server::command::*;

/// Parse a tmux-style command string into a `Command`.
///
/// Examples:
/// - `kill-server`
/// - `split-window -h`
/// - `send-keys -t %3 "ls -la" Enter`
/// - `select-pane -t %1`
/// - `rename-window -t @0 "my-window"`
/// - `resize-pane -L 5`
pub fn parse(input: &str) -> Result<Command> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        bail!("empty command");
    }

    let cmd_name = tokens[0].as_str();
    let args = &tokens[1..];

    match cmd_name {
        "kill-server" => Ok(Command::KillServer),
        "list-sessions" | "ls" => Ok(Command::ListSessions),
        "rename-session" => parse_rename_session(args),
        "new-window" => Ok(Command::NewWindow),
        "kill-window" | "killw" => parse_kill_window(args),
        "select-window" | "selectw" => parse_select_window(args),
        "rename-window" | "renamew" => parse_rename_window(args),
        "list-windows" | "lsw" => Ok(Command::ListWindows),
        "split-window" | "splitw" => parse_split_window(args),
        "kill-pane" | "killp" => parse_kill_pane(args),
        "select-pane" | "selectp" => parse_select_pane(args),
        "list-panes" | "lsp" => Ok(Command::ListPanes),
        "send-keys" | "send" => parse_send_keys(args),
        "select-layout" => parse_select_layout(args),
        "resize-pane" | "resizep" => parse_resize_pane(args),
        "display-message" | "display" => parse_display_message(args),
        _ => bail!("unknown command: {}", cmd_name),
    }
}

/// Tokenize a command string, respecting quoted strings.
fn tokenize(input: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quote = false;
    let mut quote_char = '"';

    while let Some(ch) = chars.next() {
        if in_quote {
            if ch == quote_char {
                in_quote = false;
            } else if ch == '\\' && quote_char == '"' {
                if let Some(&next) = chars.peek() {
                    chars.next();
                    match next {
                        'n' => current.push('\n'),
                        't' => current.push('\t'),
                        '\\' => current.push('\\'),
                        '"' => current.push('"'),
                        _ => {
                            current.push('\\');
                            current.push(next);
                        }
                    }
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = true;
            quote_char = ch;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }

    if in_quote {
        bail!("unterminated quote in command: {}", input);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

/// Parse `-t TARGET` from argument list. Returns (target, remaining_args).
fn extract_target(args: &[String]) -> (Option<String>, Vec<String>) {
    let mut target = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-t" && i + 1 < args.len() {
            target = Some(args[i + 1].clone());
            i += 2;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    (target, rest)
}

/// Parse a target string into a TargetWindow.
fn parse_target_window(s: &str) -> Result<TargetWindow> {
    if let Some(n_str) = s.strip_prefix('@') {
        let n: u32 = n_str.parse().map_err(|_| anyhow::anyhow!("invalid window id: {}", s))?;
        Ok(TargetWindow::Id(n))
    } else if let Ok(idx) = s.parse::<usize>() {
        Ok(TargetWindow::Index(idx))
    } else {
        bail!("invalid window target: {}", s)
    }
}

/// Parse a target string into a TargetPane.
fn parse_target_pane(s: &str) -> Result<TargetPane> {
    if let Some(n_str) = s.strip_prefix('%') {
        let n: u32 = n_str.parse().map_err(|_| anyhow::anyhow!("invalid pane id: {}", s))?;
        Ok(TargetPane::Id(n))
    } else {
        match s {
            "{left}" | "-L" => Ok(TargetPane::Direction(PaneDirection::Left)),
            "{right}" | "-R" => Ok(TargetPane::Direction(PaneDirection::Right)),
            "{up}" | "-U" => Ok(TargetPane::Direction(PaneDirection::Up)),
            "{down}" | "-D" => Ok(TargetPane::Direction(PaneDirection::Down)),
            _ => bail!("invalid pane target: {}", s),
        }
    }
}

fn parse_rename_session(args: &[String]) -> Result<Command> {
    if args.is_empty() {
        bail!("rename-session requires a name");
    }
    Ok(Command::RenameSession { new_name: args[0].clone() })
}

fn parse_kill_window(args: &[String]) -> Result<Command> {
    let (target_str, _rest) = extract_target(args);
    let target = target_str.map(|s| parse_target_window(&s)).transpose()?;
    Ok(Command::KillWindow { target })
}

fn parse_select_window(args: &[String]) -> Result<Command> {
    let (target_str, _rest) = extract_target(args);
    let target = target_str
        .map(|s| parse_target_window(&s))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("select-window requires -t TARGET"))?;
    Ok(Command::SelectWindow { target })
}

fn parse_rename_window(args: &[String]) -> Result<Command> {
    let (target_str, rest) = extract_target(args);
    let target = target_str.map(|s| parse_target_window(&s)).transpose()?;
    let new_name = rest.first()
        .ok_or_else(|| anyhow::anyhow!("rename-window requires a name"))?
        .clone();
    Ok(Command::RenameWindow { target, new_name })
}

fn parse_split_window(args: &[String]) -> Result<Command> {
    let (target_str, rest) = extract_target(args);
    let target = target_str.map(|s| parse_target_pane(&s)).transpose()?;
    let horizontal = rest.iter().any(|a| a == "-h");
    Ok(Command::SplitWindow { horizontal, target })
}

fn parse_kill_pane(args: &[String]) -> Result<Command> {
    let (target_str, _rest) = extract_target(args);
    let target = target_str.map(|s| parse_target_pane(&s)).transpose()?;
    Ok(Command::KillPane { target })
}

fn parse_select_pane(args: &[String]) -> Result<Command> {
    let (target_str, rest) = extract_target(args);

    // Check for directional flags: -L, -R, -U, -D
    for arg in &rest {
        match arg.as_str() {
            "-L" => return Ok(Command::SelectPane { target: TargetPane::Direction(PaneDirection::Left) }),
            "-R" => return Ok(Command::SelectPane { target: TargetPane::Direction(PaneDirection::Right) }),
            "-U" => return Ok(Command::SelectPane { target: TargetPane::Direction(PaneDirection::Up) }),
            "-D" => return Ok(Command::SelectPane { target: TargetPane::Direction(PaneDirection::Down) }),
            _ => {}
        }
    }

    let target = target_str
        .map(|s| parse_target_pane(&s))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("select-pane requires -t TARGET or direction flag"))?;
    Ok(Command::SelectPane { target })
}

fn parse_send_keys(args: &[String]) -> Result<Command> {
    let (target_str, rest) = extract_target(args);
    let target = target_str.map(|s| parse_target_pane(&s)).transpose()?;
    if rest.is_empty() {
        bail!("send-keys requires at least one key");
    }
    Ok(Command::SendKeys { target, keys: rest })
}

fn parse_select_layout(args: &[String]) -> Result<Command> {
    if args.is_empty() {
        bail!("select-layout requires a layout name");
    }
    Ok(Command::SelectLayout { layout_name: args[0].clone() })
}

fn parse_resize_pane(args: &[String]) -> Result<Command> {
    let (target_str, rest) = extract_target(args);
    let target = target_str.map(|s| parse_target_pane(&s)).transpose()?;

    let mut direction = ResizeDirection::Right;
    let mut amount = 1u16;

    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "-L" => direction = ResizeDirection::Left,
            "-R" => direction = ResizeDirection::Right,
            "-U" => direction = ResizeDirection::Up,
            "-D" => direction = ResizeDirection::Down,
            s if s.parse::<u16>().is_ok() => {
                amount = s.parse().unwrap();
            }
            _ => {}
        }
        i += 1;
    }

    Ok(Command::ResizePane { target, direction, amount })
}

fn parse_display_message(args: &[String]) -> Result<Command> {
    let message = if args.is_empty() {
        String::new()
    } else {
        args.join(" ")
    };
    Ok(Command::DisplayMessage { message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        let tokens = tokenize("kill-server").unwrap();
        assert_eq!(tokens, vec!["kill-server"]);
    }

    #[test]
    fn test_tokenize_with_args() {
        let tokens = tokenize("split-window -h").unwrap();
        assert_eq!(tokens, vec!["split-window", "-h"]);
    }

    #[test]
    fn test_tokenize_quoted_string() {
        let tokens = tokenize(r#"send-keys "ls -la" Enter"#).unwrap();
        assert_eq!(tokens, vec!["send-keys", "ls -la", "Enter"]);
    }

    #[test]
    fn test_tokenize_single_quoted() {
        let tokens = tokenize("rename-window 'my window'").unwrap();
        assert_eq!(tokens, vec!["rename-window", "my window"]);
    }

    #[test]
    fn test_tokenize_escape_in_quotes() {
        let tokens = tokenize(r#"send-keys "hello\nworld""#).unwrap();
        assert_eq!(tokens, vec!["send-keys", "hello\nworld"]);
    }

    #[test]
    fn test_tokenize_unterminated_quote() {
        let result = tokenize(r#"send-keys "hello"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_parse_kill_server() {
        let cmd = parse("kill-server").unwrap();
        assert_eq!(cmd, Command::KillServer);
    }

    #[test]
    fn test_parse_list_sessions() {
        let cmd = parse("list-sessions").unwrap();
        assert_eq!(cmd, Command::ListSessions);
    }

    #[test]
    fn test_parse_list_sessions_alias() {
        let cmd = parse("ls").unwrap();
        assert_eq!(cmd, Command::ListSessions);
    }

    #[test]
    fn test_parse_rename_session() {
        let cmd = parse("rename-session my-session").unwrap();
        assert_eq!(cmd, Command::RenameSession { new_name: "my-session".to_string() });
    }

    #[test]
    fn test_parse_new_window() {
        let cmd = parse("new-window").unwrap();
        assert_eq!(cmd, Command::NewWindow);
    }

    #[test]
    fn test_parse_split_window_horizontal() {
        let cmd = parse("split-window -h").unwrap();
        assert_eq!(cmd, Command::SplitWindow { horizontal: true, target: None });
    }

    #[test]
    fn test_parse_split_window_vertical() {
        let cmd = parse("split-window").unwrap();
        assert_eq!(cmd, Command::SplitWindow { horizontal: false, target: None });
    }

    #[test]
    fn test_parse_split_window_with_target() {
        let cmd = parse("split-window -h -t %3").unwrap();
        assert_eq!(cmd, Command::SplitWindow { horizontal: true, target: Some(TargetPane::Id(3)) });
    }

    #[test]
    fn test_parse_select_pane_by_id() {
        let cmd = parse("select-pane -t %5").unwrap();
        assert_eq!(cmd, Command::SelectPane { target: TargetPane::Id(5) });
    }

    #[test]
    fn test_parse_select_pane_directional() {
        let cmd = parse("select-pane -L").unwrap();
        assert_eq!(cmd, Command::SelectPane { target: TargetPane::Direction(PaneDirection::Left) });
    }

    #[test]
    fn test_parse_send_keys() {
        let cmd = parse(r#"send-keys "ls -la" Enter"#).unwrap();
        assert_eq!(cmd, Command::SendKeys {
            target: None,
            keys: vec!["ls -la".to_string(), "Enter".to_string()],
        });
    }

    #[test]
    fn test_parse_send_keys_with_target() {
        let cmd = parse("send-keys -t %3 hello Enter").unwrap();
        assert_eq!(cmd, Command::SendKeys {
            target: Some(TargetPane::Id(3)),
            keys: vec!["hello".to_string(), "Enter".to_string()],
        });
    }

    #[test]
    fn test_parse_kill_pane() {
        let cmd = parse("kill-pane").unwrap();
        assert_eq!(cmd, Command::KillPane { target: None });
    }

    #[test]
    fn test_parse_kill_pane_with_target() {
        let cmd = parse("kill-pane -t %2").unwrap();
        assert_eq!(cmd, Command::KillPane { target: Some(TargetPane::Id(2)) });
    }

    #[test]
    fn test_parse_resize_pane() {
        let cmd = parse("resize-pane -L 5").unwrap();
        assert_eq!(cmd, Command::ResizePane {
            target: None,
            direction: ResizeDirection::Left,
            amount: 5,
        });
    }

    #[test]
    fn test_parse_resize_pane_default() {
        let cmd = parse("resize-pane -D").unwrap();
        assert_eq!(cmd, Command::ResizePane {
            target: None,
            direction: ResizeDirection::Down,
            amount: 1,
        });
    }

    #[test]
    fn test_parse_list_panes() {
        let cmd = parse("list-panes").unwrap();
        assert_eq!(cmd, Command::ListPanes);
    }

    #[test]
    fn test_parse_list_panes_alias() {
        let cmd = parse("lsp").unwrap();
        assert_eq!(cmd, Command::ListPanes);
    }

    #[test]
    fn test_parse_select_window() {
        let cmd = parse("select-window -t @2").unwrap();
        assert_eq!(cmd, Command::SelectWindow { target: TargetWindow::Id(2) });
    }

    #[test]
    fn test_parse_select_window_by_index() {
        let cmd = parse("select-window -t 1").unwrap();
        assert_eq!(cmd, Command::SelectWindow { target: TargetWindow::Index(1) });
    }

    #[test]
    fn test_parse_display_message() {
        let cmd = parse("display-message hello world").unwrap();
        assert_eq!(cmd, Command::DisplayMessage { message: "hello world".to_string() });
    }

    #[test]
    fn test_parse_unknown_command() {
        let result = parse("nonexistent-command");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_command() {
        let result = parse("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_kill_window() {
        let cmd = parse("kill-window -t @1").unwrap();
        assert_eq!(cmd, Command::KillWindow { target: Some(TargetWindow::Id(1)) });
    }

    #[test]
    fn test_parse_rename_window() {
        let cmd = parse(r#"rename-window -t @0 "my window""#).unwrap();
        assert_eq!(cmd, Command::RenameWindow {
            target: Some(TargetWindow::Id(0)),
            new_name: "my window".to_string(),
        });
    }

    #[test]
    fn test_parse_list_windows() {
        let cmd = parse("list-windows").unwrap();
        assert_eq!(cmd, Command::ListWindows);
    }

    #[test]
    fn test_parse_list_windows_alias() {
        let cmd = parse("lsw").unwrap();
        assert_eq!(cmd, Command::ListWindows);
    }

    #[test]
    fn test_parse_select_layout() {
        let cmd = parse("select-layout even-horizontal").unwrap();
        assert_eq!(cmd, Command::SelectLayout { layout_name: "even-horizontal".to_string() });
    }

    #[test]
    fn test_target_pane_direction_strings() {
        let t = parse_target_pane("{left}").unwrap();
        assert_eq!(t, TargetPane::Direction(PaneDirection::Left));
        let t = parse_target_pane("{right}").unwrap();
        assert_eq!(t, TargetPane::Direction(PaneDirection::Right));
        let t = parse_target_pane("{up}").unwrap();
        assert_eq!(t, TargetPane::Direction(PaneDirection::Up));
        let t = parse_target_pane("{down}").unwrap();
        assert_eq!(t, TargetPane::Direction(PaneDirection::Down));
    }

    #[test]
    fn test_target_window_index() {
        let t = parse_target_window("3").unwrap();
        assert_eq!(t, TargetWindow::Index(3));
    }

    #[test]
    fn test_target_window_id() {
        let t = parse_target_window("@5").unwrap();
        assert_eq!(t, TargetWindow::Id(5));
    }

    #[test]
    fn test_invalid_target() {
        assert!(parse_target_window("abc").is_err());
        assert!(parse_target_pane("abc").is_err());
    }
}
