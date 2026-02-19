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
        "has-session" | "has" => parse_has_session(args),
        "new-session" | "new" => parse_new_session(args),
        "new-window" | "neww" => parse_new_window(args),
        "kill-window" | "killw" => parse_kill_window(args),
        "select-window" | "selectw" => parse_select_window(args),
        "rename-window" | "renamew" => parse_rename_window(args),
        "list-windows" | "lsw" => parse_list_windows(args),
        "split-window" | "splitw" => parse_split_window(args),
        "kill-pane" | "killp" => parse_kill_pane(args),
        "select-pane" | "selectp" => parse_select_pane(args),
        "list-panes" | "lsp" => parse_list_panes(args),
        "send-keys" | "send" => parse_send_keys(args),
        "select-layout" => parse_select_layout(args),
        "resize-pane" | "resizep" => parse_resize_pane(args),
        "display-message" | "display" => parse_display_message(args),
        "close-workspace" => Ok(Command::CloseWorkspace),
        "select-workspace" => parse_select_workspace(args),
        "next-window" | "next" => Ok(Command::NextWindow),
        "previous-window" | "prev" => Ok(Command::PreviousWindow),
        "restart-pane" => Ok(Command::RestartPane),
        "move-tab" => parse_move_tab(args),
        "equalize-layout" | "equalize" => Ok(Command::EqualizeLayout),
        "toggle-sync" => Ok(Command::ToggleSync),
        "paste-buffer" | "pasteb" => parse_paste_buffer(args),
        "detach-client" | "detach" => Ok(Command::DetachClient),
        "toggle-float" | "float" => Ok(Command::ToggleFloat),
        "new-float" => Ok(Command::NewFloat),
        "maximize-focused" | "maximize" => Ok(Command::MaximizeFocused),
        "toggle-zoom" | "zoom" => Ok(Command::ToggleZoom),
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

fn parse_has_session(args: &[String]) -> Result<Command> {
    let (target_str, _rest) = extract_target(args);
    let name = target_str.ok_or_else(|| anyhow::anyhow!("has-session requires -t NAME"))?;
    Ok(Command::HasSession { name })
}

fn parse_new_session(args: &[String]) -> Result<Command> {
    let mut name = None;
    let mut window_name = None;
    let mut detached = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-s" if i + 1 < args.len() => { name = Some(args[i + 1].clone()); i += 2; }
            "-n" if i + 1 < args.len() => { window_name = Some(args[i + 1].clone()); i += 2; }
            "-d" => { detached = true; i += 1; }
            "-P" | "-F" => { i += 1; if args.get(i).map(|a| !a.starts_with('-')).unwrap_or(false) { i += 1; } }
            _ => { i += 1; }
        }
    }
    let name = name.unwrap_or_else(|| "default".to_string());
    Ok(Command::NewSession { name, window_name, detached })
}

fn parse_new_window(args: &[String]) -> Result<Command> {
    let mut target_session = None;
    let mut window_name = None;
    let mut command = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-t" if i + 1 < args.len() => { target_session = Some(args[i + 1].clone()); i += 2; }
            "-n" if i + 1 < args.len() => { window_name = Some(args[i + 1].clone()); i += 2; }
            "-c" if i + 1 < args.len() => { command = Some(args[i + 1].clone()); i += 2; }
            "-P" | "-F" => { i += 1; if args.get(i).map(|a| !a.starts_with('-')).unwrap_or(false) { i += 1; } }
            _ => { i += 1; }
        }
    }
    Ok(Command::NewWindow { target_session, window_name, command })
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
    let mut target = None;
    let mut horizontal = false;
    let mut size = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" => { horizontal = true; i += 1; }
            "-v" => { horizontal = false; i += 1; }
            "-t" if i + 1 < args.len() => {
                target = Some(parse_target_pane(&args[i + 1])?);
                i += 2;
            }
            "-l" if i + 1 < args.len() => {
                let val = &args[i + 1];
                if let Some(pct) = val.strip_suffix('%') {
                    size = Some(SplitSize::Percent(pct.parse().unwrap_or(50)));
                } else if let Ok(cells) = val.parse::<u16>() {
                    size = Some(SplitSize::Cells(cells));
                }
                i += 2;
            }
            "-P" | "-F" => {
                // Skip -P and -F (format) flags - handled by shim
                i += 1;
                if i < args.len() && !args[i].starts_with('-') { i += 1; }
            }
            _ => { i += 1; }
        }
    }
    Ok(Command::SplitWindow { horizontal, target, size })
}

fn parse_kill_pane(args: &[String]) -> Result<Command> {
    let (target_str, _rest) = extract_target(args);
    let target = target_str.map(|s| parse_target_pane(&s)).transpose()?;
    Ok(Command::KillPane { target })
}

fn parse_select_pane(args: &[String]) -> Result<Command> {
    let mut target_str = None;
    let mut title = None;
    let mut direction = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-t" if i + 1 < args.len() => { target_str = Some(args[i + 1].clone()); i += 2; }
            "-T" if i + 1 < args.len() => { title = Some(args[i + 1].clone()); i += 2; }
            "-P" => { i += 1; if i < args.len() && !args[i].starts_with('-') { i += 1; } } // skip -P "fg=..."
            "-L" => { direction = Some(PaneDirection::Left); i += 1; }
            "-R" => { direction = Some(PaneDirection::Right); i += 1; }
            "-U" => { direction = Some(PaneDirection::Up); i += 1; }
            "-D" => { direction = Some(PaneDirection::Down); i += 1; }
            _ => { i += 1; }
        }
    }

    if let Some(dir) = direction {
        return Ok(Command::SelectPane { target: TargetPane::Direction(dir), title });
    }

    let target = target_str
        .map(|s| parse_target_pane(&s))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("select-pane requires -t TARGET or direction flag"))?;
    Ok(Command::SelectPane { target, title })
}

fn parse_list_windows(args: &[String]) -> Result<Command> {
    let mut format = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-F" if i + 1 < args.len() => { format = Some(args[i + 1].clone()); i += 2; }
            "-t" if i + 1 < args.len() => { i += 2; } // skip -t TARGET for now
            _ => { i += 1; }
        }
    }
    Ok(Command::ListWindows { format })
}

fn parse_list_panes(args: &[String]) -> Result<Command> {
    let mut format = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-F" if i + 1 < args.len() => { format = Some(args[i + 1].clone()); i += 2; }
            "-t" if i + 1 < args.len() => { i += 2; } // skip -t TARGET for now
            _ => { i += 1; }
        }
    }
    Ok(Command::ListPanes { format })
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
    let mut to_stdout = false;
    let mut msg_parts = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-p" => { to_stdout = true; i += 1; }
            "-t" if i + 1 < args.len() => { i += 2; } // skip target
            _ => { msg_parts.push(args[i].clone()); i += 1; }
        }
    }
    let message = msg_parts.join(" ");
    Ok(Command::DisplayMessage { message, to_stdout })
}

fn parse_select_workspace(args: &[String]) -> Result<Command> {
    let (target_str, _rest) = extract_target(args);
    let idx_str = target_str.ok_or_else(|| anyhow::anyhow!("select-workspace requires -t INDEX"))?;
    let index: usize = idx_str.parse().map_err(|_| anyhow::anyhow!("invalid workspace index: {}", idx_str))?;
    Ok(Command::SelectWorkspaceByIndex { index })
}

fn parse_move_tab(args: &[String]) -> Result<Command> {
    let mut direction = None;
    for arg in args {
        match arg.as_str() {
            "-L" => direction = Some(PaneDirection::Left),
            "-R" => direction = Some(PaneDirection::Right),
            "-U" => direction = Some(PaneDirection::Up),
            "-D" => direction = Some(PaneDirection::Down),
            _ => {}
        }
    }
    let direction = direction.ok_or_else(|| anyhow::anyhow!("move-tab requires direction flag (-L/-R/-U/-D)"))?;
    Ok(Command::MoveTab { direction })
}

fn parse_paste_buffer(args: &[String]) -> Result<Command> {
    let text = args.join(" ");
    Ok(Command::PasteBuffer { text })
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
        assert_eq!(cmd, Command::NewWindow { target_session: None, window_name: None, command: None });
    }

    #[test]
    fn test_parse_split_window_horizontal() {
        let cmd = parse("split-window -h").unwrap();
        assert_eq!(cmd, Command::SplitWindow { horizontal: true, target: None, size: None });
    }

    #[test]
    fn test_parse_split_window_vertical() {
        let cmd = parse("split-window").unwrap();
        assert_eq!(cmd, Command::SplitWindow { horizontal: false, target: None, size: None });
    }

    #[test]
    fn test_parse_split_window_with_target() {
        let cmd = parse("split-window -h -t %3").unwrap();
        assert_eq!(cmd, Command::SplitWindow { horizontal: true, target: Some(TargetPane::Id(3)), size: None });
    }

    #[test]
    fn test_parse_split_window_with_size() {
        let cmd = parse("split-window -h -l 70%").unwrap();
        assert_eq!(cmd, Command::SplitWindow { horizontal: true, target: None, size: Some(SplitSize::Percent(70)) });
    }

    #[test]
    fn test_parse_select_pane_by_id() {
        let cmd = parse("select-pane -t %5").unwrap();
        assert_eq!(cmd, Command::SelectPane { target: TargetPane::Id(5), title: None });
    }

    #[test]
    fn test_parse_select_pane_directional() {
        let cmd = parse("select-pane -L").unwrap();
        assert_eq!(cmd, Command::SelectPane { target: TargetPane::Direction(PaneDirection::Left), title: None });
    }

    #[test]
    fn test_parse_select_pane_with_title() {
        let cmd = parse(r#"select-pane -t %0 -T "my title""#).unwrap();
        assert_eq!(cmd, Command::SelectPane { target: TargetPane::Id(0), title: Some("my title".to_string()) });
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
        assert_eq!(cmd, Command::ListPanes { format: None });
    }

    #[test]
    fn test_parse_list_panes_alias() {
        let cmd = parse("lsp").unwrap();
        assert_eq!(cmd, Command::ListPanes { format: None });
    }

    #[test]
    fn test_parse_list_panes_with_format() {
        let input = "list-panes -F \"#{pane_id}\"";
        let cmd = parse(input).unwrap();
        assert_eq!(cmd, Command::ListPanes { format: Some("#{pane_id}".to_string()) });
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
        assert_eq!(cmd, Command::DisplayMessage { message: "hello world".to_string(), to_stdout: false });
    }

    #[test]
    fn test_parse_display_message_stdout() {
        let cmd = parse("display-message -p hello").unwrap();
        assert_eq!(cmd, Command::DisplayMessage { message: "hello".to_string(), to_stdout: true });
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
        assert_eq!(cmd, Command::ListWindows { format: None });
    }

    #[test]
    fn test_parse_list_windows_alias() {
        let cmd = parse("lsw").unwrap();
        assert_eq!(cmd, Command::ListWindows { format: None });
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

    // --- Tokenizer edge cases ---

    #[test]
    fn test_tokenize_multiple_consecutive_spaces() {
        let tokens = tokenize("split-window    -h").unwrap();
        assert_eq!(tokens, vec!["split-window", "-h"]);
    }

    #[test]
    fn test_tokenize_leading_spaces() {
        let tokens = tokenize("   kill-server").unwrap();
        assert_eq!(tokens, vec!["kill-server"]);
    }

    #[test]
    fn test_tokenize_trailing_spaces() {
        let tokens = tokenize("kill-server   ").unwrap();
        assert_eq!(tokens, vec!["kill-server"]);
    }

    #[test]
    fn test_tokenize_only_spaces() {
        let tokens = tokenize("     ").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_mixed_quotes() {
        // Double quotes inside single quotes are literal
        let tokens = tokenize(r#"send-keys 'say "hello"' Enter"#).unwrap();
        assert_eq!(tokens, vec!["send-keys", r#"say "hello""#, "Enter"]);
    }

    #[test]
    fn test_tokenize_single_quotes_no_escape() {
        // Backslash inside single quotes is literal (no escape processing)
        let tokens = tokenize(r"send-keys 'hello\nworld'").unwrap();
        assert_eq!(tokens, vec!["send-keys", r"hello\nworld"]);
    }

    #[test]
    fn test_tokenize_escaped_backslash_in_double_quotes() {
        let tokens = tokenize(r#"send-keys "back\\slash""#).unwrap();
        assert_eq!(tokens, vec!["send-keys", r"back\slash"]);
    }

    #[test]
    fn test_tokenize_escaped_tab_in_double_quotes() {
        let tokens = tokenize(r#"send-keys "col1\tcol2""#).unwrap();
        assert_eq!(tokens, vec!["send-keys", "col1\tcol2"]);
    }

    #[test]
    fn test_tokenize_unknown_escape_in_double_quotes() {
        // Unknown escape like \x keeps both characters
        let tokens = tokenize(r#"send-keys "hello\xworld""#).unwrap();
        assert_eq!(tokens, vec!["send-keys", r"hello\xworld"]);
    }

    #[test]
    fn test_tokenize_unterminated_single_quote() {
        let result = tokenize("send-keys 'hello");
        assert!(result.is_err());
    }

    #[test]
    fn test_tokenize_adjacent_quoted_and_unquoted() {
        // Quoted string adjacent to unquoted text gets concatenated
        let tokens = tokenize(r#"send-keys prefix"quoted""#).unwrap();
        assert_eq!(tokens, vec!["send-keys", "prefixquoted"]);
    }

    #[test]
    fn test_tokenize_empty_quoted_string() {
        let tokens = tokenize(r#"send-keys "" Enter"#).unwrap();
        // Empty quotes produce an empty token is NOT pushed (current is empty when quote closes)
        // Actually: the empty quote sets in_quote=true then immediately closes, current stays empty
        // The empty token only gets pushed if followed by whitespace — let's verify
        assert_eq!(tokens, vec!["send-keys", "Enter"]);
    }

    #[test]
    fn test_tokenize_tab_as_whitespace() {
        let tokens = tokenize("split-window\t-h").unwrap();
        assert_eq!(tokens, vec!["split-window", "-h"]);
    }

    // --- Malformed input / parse edge cases ---

    #[test]
    fn test_parse_whitespace_only_command() {
        let result = parse("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_command_with_args() {
        let result = parse("totally-bogus -x -y");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("unknown command"));
    }

    // --- Invalid numeric values ---

    #[test]
    fn test_parse_split_window_invalid_percent() {
        // -l abc% should fail to parse percent, falls through to unwrap_or(50)
        let cmd = parse("split-window -h -l abc%").unwrap();
        assert_eq!(cmd, Command::SplitWindow {
            horizontal: true,
            target: None,
            size: Some(SplitSize::Percent(50)),
        });
    }

    #[test]
    fn test_parse_split_window_invalid_cells() {
        // -l xyz (no % suffix, not a valid u16) → size stays None
        let cmd = parse("split-window -h -l xyz").unwrap();
        assert_eq!(cmd, Command::SplitWindow {
            horizontal: true,
            target: None,
            size: None,
        });
    }

    #[test]
    fn test_parse_split_window_zero_percent() {
        let cmd = parse("split-window -l 0%").unwrap();
        assert_eq!(cmd, Command::SplitWindow {
            horizontal: false,
            target: None,
            size: Some(SplitSize::Percent(0)),
        });
    }

    #[test]
    fn test_parse_split_window_large_percent() {
        // 255 fits in u8
        let cmd = parse("split-window -l 255%").unwrap();
        assert_eq!(cmd, Command::SplitWindow {
            horizontal: false,
            target: None,
            size: Some(SplitSize::Percent(255)),
        });
    }

    #[test]
    fn test_parse_split_window_cells() {
        let cmd = parse("split-window -l 20").unwrap();
        assert_eq!(cmd, Command::SplitWindow {
            horizontal: false,
            target: None,
            size: Some(SplitSize::Cells(20)),
        });
    }

    #[test]
    fn test_parse_resize_pane_non_numeric_amount_ignored() {
        // Non-numeric value after direction flag: amount stays at default 1
        let cmd = parse("resize-pane -L abc").unwrap();
        assert_eq!(cmd, Command::ResizePane {
            target: None,
            direction: ResizeDirection::Left,
            amount: 1,
        });
    }

    // --- Multi-flag combinations ---

    #[test]
    fn test_parse_split_window_both_h_and_v_last_wins() {
        let cmd = parse("split-window -h -v").unwrap();
        // -v is parsed last, overrides -h
        assert_eq!(cmd, Command::SplitWindow {
            horizontal: false,
            target: None,
            size: None,
        });
    }

    #[test]
    fn test_parse_split_window_v_then_h() {
        let cmd = parse("split-window -v -h").unwrap();
        assert_eq!(cmd, Command::SplitWindow {
            horizontal: true,
            target: None,
            size: None,
        });
    }

    #[test]
    fn test_parse_select_pane_all_directions() {
        for (flag, dir) in [
            ("-L", PaneDirection::Left),
            ("-R", PaneDirection::Right),
            ("-U", PaneDirection::Up),
            ("-D", PaneDirection::Down),
        ] {
            let cmd = parse(&format!("select-pane {}", flag)).unwrap();
            assert_eq!(cmd, Command::SelectPane {
                target: TargetPane::Direction(dir),
                title: None,
            });
        }
    }

    #[test]
    fn test_parse_resize_pane_all_directions() {
        for (flag, dir) in [
            ("-L", ResizeDirection::Left),
            ("-R", ResizeDirection::Right),
            ("-U", ResizeDirection::Up),
            ("-D", ResizeDirection::Down),
        ] {
            let cmd = parse(&format!("resize-pane {} 3", flag)).unwrap();
            assert_eq!(cmd, Command::ResizePane {
                target: None,
                direction: dir,
                amount: 3,
            });
        }
    }

    #[test]
    fn test_parse_resize_pane_with_target_and_direction() {
        let cmd = parse("resize-pane -t %5 -U 10").unwrap();
        assert_eq!(cmd, Command::ResizePane {
            target: Some(TargetPane::Id(5)),
            direction: ResizeDirection::Up,
            amount: 10,
        });
    }

    #[test]
    fn test_parse_display_message_with_stdout_flag() {
        let cmd = parse("display-message -p #{session_name}").unwrap();
        assert_eq!(cmd, Command::DisplayMessage {
            message: "#{session_name}".to_string(),
            to_stdout: true,
        });
    }

    #[test]
    fn test_parse_display_message_with_target_skipped() {
        let cmd = parse("display-message -t %0 hello").unwrap();
        assert_eq!(cmd, Command::DisplayMessage {
            message: "hello".to_string(),
            to_stdout: false,
        });
    }

    // --- Commands with missing required arguments ---

    #[test]
    fn test_parse_rename_session_missing_name() {
        let result = parse("rename-session");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_send_keys_missing_keys() {
        let result = parse("send-keys");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_send_keys_with_target_but_no_keys() {
        let result = parse("send-keys -t %0");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_select_layout_missing_name() {
        let result = parse("select-layout");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_select_window_missing_target() {
        let result = parse("select-window");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_has_session_missing_target() {
        let result = parse("has-session");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rename_window_missing_name() {
        let result = parse("rename-window -t @0");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_move_tab_missing_direction() {
        let result = parse("move-tab");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_select_workspace_missing_target() {
        let result = parse("select-workspace");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_select_workspace_non_numeric_index() {
        let result = parse("select-workspace -t abc");
        assert!(result.is_err());
    }

    // --- Target parsing edge cases ---

    #[test]
    fn test_parse_target_pane_invalid_id() {
        let result = parse_target_pane("%abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_target_pane_empty() {
        let result = parse_target_pane("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_target_pane_percent_only() {
        let result = parse_target_pane("%");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_target_window_at_only() {
        let result = parse_target_window("@");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_target_window_at_invalid() {
        let result = parse_target_window("@xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_target_window_negative_rejected() {
        // "-1" starts with "-", not "@", and isn't a valid usize
        let result = parse_target_window("-1");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_target_pane_direction_aliases() {
        assert_eq!(parse_target_pane("-L").unwrap(), TargetPane::Direction(PaneDirection::Left));
        assert_eq!(parse_target_pane("-R").unwrap(), TargetPane::Direction(PaneDirection::Right));
        assert_eq!(parse_target_pane("-U").unwrap(), TargetPane::Direction(PaneDirection::Up));
        assert_eq!(parse_target_pane("-D").unwrap(), TargetPane::Direction(PaneDirection::Down));
    }

    // --- Command alias coverage ---

    #[test]
    fn test_parse_all_aliases() {
        assert_eq!(parse("has -t test").unwrap(), Command::HasSession { name: "test".to_string() });
        assert_eq!(parse("neww").unwrap(), Command::NewWindow { target_session: None, window_name: None, command: None });
        assert_eq!(parse("killw").unwrap(), Command::KillWindow { target: None });
        assert_eq!(parse("selectw -t @0").unwrap(), Command::SelectWindow { target: TargetWindow::Id(0) });
        assert_eq!(parse("selectw -t 0").unwrap(), Command::SelectWindow { target: TargetWindow::Index(0) });
        assert_eq!(parse("splitw").unwrap(), Command::SplitWindow { horizontal: false, target: None, size: None });
        assert_eq!(parse("killp").unwrap(), Command::KillPane { target: None });
        assert_eq!(parse("selectp -L").unwrap(), Command::SelectPane { target: TargetPane::Direction(PaneDirection::Left), title: None });
        assert_eq!(parse("send hello Enter").unwrap(), Command::SendKeys { target: None, keys: vec!["hello".to_string(), "Enter".to_string()] });
        assert_eq!(parse("resizep -R 2").unwrap(), Command::ResizePane { target: None, direction: ResizeDirection::Right, amount: 2 });
        assert_eq!(parse("display hello").unwrap(), Command::DisplayMessage { message: "hello".to_string(), to_stdout: false });
        assert_eq!(parse("renamew new-name").unwrap(), Command::RenameWindow { target: None, new_name: "new-name".to_string() });
        assert_eq!(parse("next").unwrap(), Command::NextWindow);
        assert_eq!(parse("prev").unwrap(), Command::PreviousWindow);
        assert_eq!(parse("equalize").unwrap(), Command::EqualizeLayout);
        assert_eq!(parse("pasteb hello world").unwrap(), Command::PasteBuffer { text: "hello world".to_string() });
    }

    // --- extract_target edge cases ---

    #[test]
    fn test_extract_target_no_flag() {
        let (target, rest) = extract_target(&["hello".to_string(), "world".to_string()]);
        assert_eq!(target, None);
        assert_eq!(rest, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn test_extract_target_t_at_end() {
        // -t with no following arg: -t stays as a regular arg
        let (target, rest) = extract_target(&["-t".to_string()]);
        assert_eq!(target, None);
        assert_eq!(rest, vec!["-t".to_string()]);
    }

    #[test]
    fn test_extract_target_multiple_t_flags() {
        // Second -t overrides first
        let (target, rest) = extract_target(&[
            "-t".to_string(), "first".to_string(),
            "-t".to_string(), "second".to_string(),
        ]);
        assert_eq!(target, Some("second".to_string()));
        assert!(rest.is_empty());
    }

    // --- New session parsing ---

    #[test]
    fn test_parse_new_session_with_name() {
        let cmd = parse("new-session -s mysession").unwrap();
        assert_eq!(cmd, Command::NewSession {
            name: "mysession".to_string(),
            window_name: None,
            detached: false,
        });
    }

    #[test]
    fn test_parse_new_session_detached_with_window_name() {
        let cmd = parse("new-session -d -s test -n mywindow").unwrap();
        assert_eq!(cmd, Command::NewSession {
            name: "test".to_string(),
            window_name: Some("mywindow".to_string()),
            detached: true,
        });
    }

    #[test]
    fn test_parse_new_session_default_name() {
        let cmd = parse("new-session").unwrap();
        assert_eq!(cmd, Command::NewSession {
            name: "default".to_string(),
            window_name: None,
            detached: false,
        });
    }

    #[test]
    fn test_parse_new_window_with_name() {
        let cmd = parse("new-window -n mywin").unwrap();
        assert_eq!(cmd, Command::NewWindow {
            target_session: None,
            window_name: Some("mywin".to_string()),
            command: None,
        });
    }

    #[test]
    fn test_parse_new_window_with_target() {
        let cmd = parse("new-window -t mysession -n mywin").unwrap();
        assert_eq!(cmd, Command::NewWindow {
            target_session: Some("mysession".to_string()),
            window_name: Some("mywin".to_string()),
            command: None,
        });
    }

    // --- Misc zero-arg commands ---

    #[test]
    fn test_parse_zero_arg_commands() {
        assert_eq!(parse("close-workspace").unwrap(), Command::CloseWorkspace);
        assert_eq!(parse("next-window").unwrap(), Command::NextWindow);
        assert_eq!(parse("previous-window").unwrap(), Command::PreviousWindow);
        assert_eq!(parse("restart-pane").unwrap(), Command::RestartPane);
        assert_eq!(parse("equalize-layout").unwrap(), Command::EqualizeLayout);
        assert_eq!(parse("toggle-sync").unwrap(), Command::ToggleSync);
    }

    #[test]
    fn test_parse_paste_buffer_empty() {
        let cmd = parse("paste-buffer").unwrap();
        assert_eq!(cmd, Command::PasteBuffer { text: "".to_string() });
    }

    #[test]
    fn test_parse_list_windows_with_format() {
        let fmt = format!("list-windows -F \"#{{window_id}}: #{{window_name}}\"");
        let cmd = parse(&fmt).unwrap();
        assert_eq!(cmd, Command::ListWindows {
            format: Some("#{window_id}: #{window_name}".to_string()),
        });
    }

    #[test]
    fn test_parse_list_panes_with_format_and_target() {
        let fmt = format!("list-panes -t @0 -F \"#{{pane_id}}\"");
        let cmd = parse(&fmt).unwrap();
        assert_eq!(cmd, Command::ListPanes {
            format: Some("#{pane_id}".to_string()),
        });
    }
}
