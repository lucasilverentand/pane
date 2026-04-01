pub mod pty;

// Re-export shared types from pane-protocol
pub use pane_protocol::window_types::{TabKind, WindowId};

use pane_protocol::event::AppEvent;
use pane_protocol::layout::TabId;
use portable_pty::PtySize;
use std::io::Write;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Get the name and full executable path of a process by PID in a single syscall.
#[cfg(target_os = "macos")]
fn process_info_by_pid(pid: u32) -> (Option<String>, Option<String>) {
    extern "C" {
        fn proc_pidpath(
            pid: std::ffi::c_int,
            buffer: *mut u8,
            buffersize: u32,
        ) -> std::ffi::c_int;
    }
    let mut buf = [0u8; 4096];
    let ret =
        unsafe { proc_pidpath(pid as std::ffi::c_int, buf.as_mut_ptr(), buf.len() as u32) };
    if ret > 0 {
        let path_str = String::from_utf8_lossy(&buf[..ret as usize]).to_string();
        let name = std::path::Path::new(&path_str)
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        (name, Some(path_str))
    } else {
        (None, None)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_info_by_pid(pid: u32) -> (Option<String>, Option<String>) {
    let path = std::fs::read_link(format!("/proc/{}/exe", pid))
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));
    let name = std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .ok()
        .map(|s| s.trim().to_string());
    (name, path)
}

#[cfg(not(unix))]
fn process_info_by_pid(_pid: u32) -> (Option<String>, Option<String>) {
    (None, None)
}

pub struct Window {
    #[allow(dead_code)] // groups are keyed by this id in the HashMap
    pub id: WindowId,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    /// Optional user-assigned window name.
    pub name: Option<String>,
}

impl Window {
    pub fn new(id: WindowId, tab: Tab) -> Self {
        Self {
            id,
            tabs: vec![tab],
            active_tab: 0,
            name: None,
        }
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    pub fn add_tab(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    pub fn close_tab(&mut self, idx: usize) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.tabs.remove(idx);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    pub fn remove_tab(&mut self, idx: usize) -> Option<Tab> {
        if self.tabs.len() <= 1 {
            return None;
        }
        let pane = self.tabs.remove(idx);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        Some(pane)
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = self
                .active_tab
                .checked_sub(1)
                .unwrap_or(self.tabs.len() - 1);
        }
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
}

pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
    pub title: String,
    pub vt: vt100::Parser,
    pub exited: bool,
    pub command: Option<String>,
    pub cwd: PathBuf,
    pub scroll_offset: usize,
    /// Cached name of the foreground process (e.g. "claude", "nvim").
    pub foreground_process: Option<String>,
    /// Full executable path of the foreground process for decoration matching.
    pub foreground_process_path: Option<String>,
    shell_pid: Option<u32>,
    pty_writer: Option<Box<dyn Write + Send>>,
    pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
}

impl Tab {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_with_env(
        id: TabId,
        kind: TabKind,
        cols: u16,
        rows: u16,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        command: Option<String>,
        shell: Option<String>,
        tmux_env: Option<pty::TmuxEnv>,
        cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<Self> {
        // vt100 panics on zero dimensions
        let cols = cols.max(1);
        let rows = rows.max(1);

        let (cmd, args): (&str, Vec<&str>) = match &kind {
            TabKind::Shell => {
                if let Some(ref cmd_str) = command {
                    if let Some(ref shell_path) = shell {
                        // Run command wrapped in the specified shell: shell -ic "command"
                        // -i (interactive) ensures aliases and shell config are loaded
                        let shell_leaked: &'static str =
                            Box::leak(shell_path.clone().into_boxed_str());
                        let cmd_leaked: &'static str =
                            Box::leak(cmd_str.clone().into_boxed_str());
                        (shell_leaked, vec!["-ic", cmd_leaked])
                    } else {
                        // Run command directly (split by whitespace)
                        let leaked: &'static str = Box::leak(cmd_str.clone().into_boxed_str());
                        let parts: Vec<&'static str> = leaked.split_whitespace().collect();
                        if parts.len() > 1 {
                            (parts[0], parts[1..].to_vec())
                        } else {
                            (leaked, vec![])
                        }
                    }
                } else if let Some(ref shell_path) = shell {
                    // Shell specified but no command — just start that shell
                    let shell_leaked: &'static str =
                        Box::leak(shell_path.clone().into_boxed_str());
                    (shell_leaked, vec![])
                } else {
                    let default_shell =
                        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                    let shell_leaked: &'static str = Box::leak(default_shell.into_boxed_str());
                    (shell_leaked, vec![])
                }
            }
            TabKind::Nvim => ("nvim", vec![]),
            TabKind::Agent => ("claude", vec![]),
            TabKind::DevServer => {
                let cmd_str = command.as_deref().unwrap_or("echo 'no command'");
                let default_shell =
                    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                let shell_leaked: &'static str = Box::leak(default_shell.into_boxed_str());
                let cmd_leaked: &'static str =
                    Box::leak(cmd_str.to_string().into_boxed_str());
                (shell_leaked, vec!["-ic", cmd_leaked])
            }
        };

        let title = match &command {
            Some(c) => clean_tab_title(c),
            None => kind.label().to_string(),
        };
        let cwd = cwd
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pty_handle = pty::spawn_pty(cmd, &args, size, event_tx, id, Some(&cwd), tmux_env)?;
        let shell_pid = pty_handle.shell_pid;
        let vt = vt100::Parser::new(rows, cols, 1000);

        Ok(Self {
            id,
            kind,
            title,
            vt,
            exited: false,
            command,
            cwd,
            scroll_offset: 0,
            foreground_process: None,
            foreground_process_path: None,
            shell_pid,
            pty_writer: Some(pty_handle.writer),
            pty_master: Some(pty_handle.master),
        })
    }

    /// Create a pane that shows an error message instead of a PTY.
    pub fn spawn_error(id: TabId, kind: TabKind, error_msg: &str) -> Self {
        let mut vt = vt100::Parser::new(24, 80, 0);
        vt.process(format!("error: {}\r\n", error_msg).as_bytes());
        Self {
            id,
            kind: kind.clone(),
            title: format!("{}: {}", kind.label(), error_msg),
            vt,
            exited: true,
            command: None,
            cwd: PathBuf::from("/"),
            scroll_offset: 0,
            foreground_process: None,
            foreground_process_path: None,
            shell_pid: None,
            pty_writer: None,
            pty_master: None,
        }
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        if let Some(writer) = &mut self.pty_writer {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    #[cfg(test)]
    pub fn is_scrolled(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Detect the foreground process running in this tab's PTY.
    /// Returns `true` if the foreground process changed since the last call.
    #[cfg(unix)]
    pub fn update_foreground_process(&mut self) -> bool {
        if self.exited {
            let changed = self.foreground_process.is_some() || self.foreground_process_path.is_some();
            self.foreground_process = None;
            self.foreground_process_path = None;
            return changed;
        }
        let fg_pid = self
            .pty_master
            .as_ref()
            .and_then(|m| m.process_group_leader())
            .map(|pgid| pgid as u32);
        let old_name = self.foreground_process.clone();
        let old_path = self.foreground_process_path.clone();
        match fg_pid {
            Some(pid) if self.shell_pid != Some(pid) => {
                // A child process has taken the foreground (e.g. user ran `claude` in a shell tab)
                let (name, path) = process_info_by_pid(pid);
                self.foreground_process = name;
                self.foreground_process_path = path;
            }
            Some(pid) if self.kind != TabKind::Shell => {
                // Non-shell tab (Agent, Nvim, DevServer): the spawned process IS the
                // interesting process, so detect it from the shell_pid itself.
                // Always re-check — the first lookup may happen before exec completes.
                let (name, path) = process_info_by_pid(pid);
                if name.is_none() {
                    eprintln!("pane: process lookup failed for pid={} tab={:?}", pid, self.id);
                }
                self.foreground_process = name;
                self.foreground_process_path = path;
            }
            Some(pid) if self.command.is_some() => {
                // Shell tab launched with a command (e.g. tab picker entry).
                // The shell may have exec'd into the target process, so
                // fg_pid == shell_pid but the process is no longer the shell.
                // Check the actual process name to detect this.
                let (name, path) = process_info_by_pid(pid);
                self.foreground_process = name;
                self.foreground_process_path = path;
            }
            Some(_) => {
                // fg == shell in a plain shell tab, no foreground process
                self.foreground_process = None;
                self.foreground_process_path = None;
            }
            None => {
                self.foreground_process = None;
                self.foreground_process_path = None;
            }
        }
        let changed = self.foreground_process != old_name || self.foreground_process_path != old_path;
        if changed {
            eprintln!(
                "pane: fg process changed: {:?} -> {:?} (pid={:?}, path={:?})",
                old_name, self.foreground_process, fg_pid, self.foreground_process_path
            );
        }
        changed
    }

    #[cfg(not(unix))]
    pub fn update_foreground_process(&mut self) -> bool {
        let changed = self.foreground_process.is_some() || self.foreground_process_path.is_some();
        self.foreground_process = None;
        self.foreground_process_path = None;
        changed
    }

    pub fn scroll_up(&mut self, n: usize) {
        let max_offset = self.vt.screen().size().0 as usize;
        self.scroll_offset = self.scroll_offset.saturating_add(n).min(max_offset);
        self.vt.screen_mut().set_scrollback(self.scroll_offset);
        self.scroll_offset = self.vt.screen().scrollback();
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        self.vt.screen_mut().set_scrollback(self.scroll_offset);
        self.scroll_offset = self.vt.screen().scrollback();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.vt.screen_mut().set_scrollback(0);
    }

    /// Process PTY output bytes.
    /// Returns `true` if the foreground process changed (caller should broadcast layout).
    pub fn process_output(&mut self, bytes: &[u8]) -> bool {
        self.vt.process(bytes);
        if self.scroll_offset > 0 {
            self.scroll_offset = self.vt.screen().scrollback();
        }
        let osc_title = self.vt.screen().title();
        if !osc_title.is_empty() {
            self.title = clean_tab_title(osc_title);
        }
        self.update_foreground_process()
    }

    pub fn resize_pty(&mut self, cols: u16, rows: u16) {
        self.resize_pty_with_pixels(cols, rows, 0, 0);
    }

    /// Resize the PTY with pixel dimensions. Native app clients (e.g. GhosttyKit)
    /// provide pixel_width/pixel_height so the kernel's TIOCSWINSZ includes them,
    /// enabling Sixel graphics, kitty image protocol, etc.
    pub fn resize_pty_with_pixels(
        &mut self,
        cols: u16,
        rows: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if self.scroll_offset > 0 {
            self.scroll_to_bottom();
        }
        self.vt.screen_mut().set_size(rows, cols);
        if let Some(master) = &self.pty_master {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width,
                pixel_height,
            });
        }
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.vt.screen()
    }
}

/// Clean up an OSC title for display in the tab bar.
/// Strips full paths like "/bin/zsh" to just "zsh".
fn clean_tab_title(title: &str) -> String {
    if title.starts_with('/') {
        if let Some(basename) = title.rsplit('/').next() {
            if !basename.is_empty() {
                return basename.to_string();
            }
        }
    }
    title.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_error_sets_title() {
        let pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "bad thing");
        assert_eq!(pane.title, "shell: bad thing");
    }

    #[test]
    fn test_spawn_error_sets_exited() {
        let pane = Tab::spawn_error(TabId::new_v4(), TabKind::Agent, "fail");
        assert!(pane.exited);
    }

    #[test]
    fn test_spawn_error_has_no_pty() {
        let pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "err");
        assert!(pane.pty_writer.is_none());
        assert!(pane.pty_master.is_none());
    }

    #[test]
    fn test_spawn_error_writes_message_to_screen() {
        let pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "something broke");
        let content = pane.screen().contents();
        assert!(content.contains("error: something broke"));
    }

    #[test]
    fn test_spawn_error_empty_message() {
        let pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        let content = pane.screen().contents();
        assert!(content.contains("error: "));
    }

    #[test]
    fn test_spawn_error_preserves_kind() {
        let pane = Tab::spawn_error(TabId::new_v4(), TabKind::DevServer, "fail");
        assert_eq!(pane.kind, TabKind::DevServer);
        assert_eq!(pane.title, "server: fail");
    }

    #[test]
    fn test_spawn_error_preserves_id() {
        let id = TabId::new_v4();
        let pane = Tab::spawn_error(id, TabKind::Shell, "err");
        assert_eq!(pane.id, id);
    }

    #[test]
    fn test_process_output_updates_screen() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(24, 80, 0);
        pane.process_output(b"hello world");
        let content = pane.screen().contents();
        assert!(content.contains("hello world"));
    }

    #[test]
    fn test_process_output_osc_title_update() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(24, 80, 0);
        pane.process_output(b"\x1b]0;my-custom-title\x07");
        assert_eq!(pane.title, "my-custom-title");
    }

    #[test]
    fn test_process_output_empty_osc_title_keeps_existing() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.title = "original".to_string();
        pane.vt = vt100::Parser::new(24, 80, 0);
        pane.process_output(b"some output");
        assert_eq!(pane.title, "original");
    }

    #[test]
    fn test_resize_pty_updates_vt_size() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.resize_pty(120, 40);
        let (rows, cols) = pane.screen().size();
        assert_eq!(rows, 40);
        assert_eq!(cols, 120);
    }

    #[test]
    fn test_write_input_on_error_pane_does_not_panic() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "err");
        pane.write_input(b"hello");
    }

    #[test]
    fn test_pane_group_close_middle_tab() {
        let gid = WindowId::new_v4();
        let p1_id = TabId::new_v4();
        let p2_id = TabId::new_v4();
        let p3_id = TabId::new_v4();
        let p1 = Tab::spawn_error(p1_id, TabKind::Shell, "t1");
        let p2 = Tab::spawn_error(p2_id, TabKind::Shell, "t2");
        let p3 = Tab::spawn_error(p3_id, TabKind::Shell, "t3");
        let mut group = Window::new(gid, p1);
        group.add_tab(p2);
        group.add_tab(p3);
        assert!(group.close_tab(1));
        assert_eq!(group.tab_count(), 2);
        assert_eq!(group.active_tab, 1);
        assert_eq!(group.active_tab().id, p3_id);
    }

    #[test]
    fn test_pane_group_close_active_first_tab() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let p2_id = TabId::new_v4();
        let p2 = Tab::spawn_error(p2_id, TabKind::Shell, "t2");
        let mut group = Window::new(gid, p1);
        group.add_tab(p2);
        group.active_tab = 0;
        assert!(group.close_tab(0));
        assert_eq!(group.tab_count(), 1);
        assert_eq!(group.active_tab, 0);
        assert_eq!(group.active_tab().id, p2_id);
    }

    #[test]
    fn test_pane_kind_label_devserver() {
        assert_eq!(TabKind::DevServer.label(), "server");
    }

    #[test]
    fn test_pane_kind_clone_eq() {
        let k1 = TabKind::Agent;
        let k2 = k1.clone();
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_scroll_initial_state() {
        let pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "test");
        assert_eq!(pane.scroll_offset, 0);
        assert!(!pane.is_scrolled());
    }

    #[test]
    fn test_scroll_up_no_scrollback() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        pane.scroll_up(5);
        assert_eq!(pane.scroll_offset, 0);
        assert!(!pane.is_scrolled());
    }

    #[test]
    fn test_scroll_up_with_scrollback() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        for i in 0..20 {
            pane.vt.process(format!("line {}\r\n", i).as_bytes());
        }
        pane.scroll_up(5);
        assert!(pane.is_scrolled());
        assert!(pane.scroll_offset > 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        for i in 0..20 {
            pane.vt.process(format!("line {}\r\n", i).as_bytes());
        }
        pane.scroll_up(5);
        assert!(pane.is_scrolled());
        pane.scroll_to_bottom();
        assert_eq!(pane.scroll_offset, 0);
        assert!(!pane.is_scrolled());
    }

    #[test]
    fn test_resize_resets_scroll() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        for i in 0..20 {
            pane.vt.process(format!("line {}\r\n", i).as_bytes());
        }
        pane.scroll_up(5);
        assert!(pane.is_scrolled());
        pane.resize_pty(80, 5);
        assert_eq!(pane.scroll_offset, 0);
        assert!(!pane.is_scrolled());
    }

    // ---- clean_tab_title tests ----

    #[test]
    fn test_clean_tab_title_full_path_zsh() {
        assert_eq!(clean_tab_title("/bin/zsh"), "zsh");
    }

    #[test]
    fn test_clean_tab_title_full_path_fish() {
        assert_eq!(clean_tab_title("/usr/bin/fish"), "fish");
    }

    #[test]
    fn test_clean_tab_title_full_path_node() {
        assert_eq!(clean_tab_title("/usr/local/bin/node"), "node");
    }

    #[test]
    fn test_clean_tab_title_bare_name() {
        assert_eq!(clean_tab_title("vim"), "vim");
    }

    #[test]
    fn test_clean_tab_title_bare_name_nvim() {
        assert_eq!(clean_tab_title("nvim"), "nvim");
    }

    #[test]
    fn test_clean_tab_title_empty_string() {
        assert_eq!(clean_tab_title(""), "");
    }

    #[test]
    fn test_clean_tab_title_trailing_slash() {
        // Path like "/" should return "/" (basename is empty, falls through)
        assert_eq!(clean_tab_title("/"), "/");
    }

    #[test]
    fn test_clean_tab_title_with_args() {
        // Not a path, should remain unchanged
        assert_eq!(clean_tab_title("vim file.rs"), "vim file.rs");
    }

    #[test]
    fn test_clean_tab_title_deep_path() {
        assert_eq!(clean_tab_title("/a/b/c/d/e/python3"), "python3");
    }

    // ---- tab cycling (next_tab, prev_tab, wrap around) ----

    #[test]
    fn test_next_tab_single_tab() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let mut group = Window::new(gid, p1);
        assert_eq!(group.active_tab, 0);
        group.next_tab();
        assert_eq!(group.active_tab, 0); // wraps around to 0
    }

    #[test]
    fn test_next_tab_two_tabs() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2");
        let mut group = Window::new(gid, p1);
        group.add_tab(p2);
        group.active_tab = 0;
        group.next_tab();
        assert_eq!(group.active_tab, 1);
        group.next_tab();
        assert_eq!(group.active_tab, 0); // wraps
    }

    #[test]
    fn test_prev_tab_single_tab() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let mut group = Window::new(gid, p1);
        group.prev_tab();
        assert_eq!(group.active_tab, 0); // wraps to last (which is 0)
    }

    #[test]
    fn test_prev_tab_two_tabs_wraps() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2");
        let mut group = Window::new(gid, p1);
        group.add_tab(p2);
        group.active_tab = 0;
        group.prev_tab();
        assert_eq!(group.active_tab, 1); // wraps to last
    }

    #[test]
    fn test_next_prev_cycle_three_tabs() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2");
        let p3 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t3");
        let mut group = Window::new(gid, p1);
        group.add_tab(p2);
        group.add_tab(p3);
        group.active_tab = 0;

        // Forward through all tabs
        group.next_tab();
        assert_eq!(group.active_tab, 1);
        group.next_tab();
        assert_eq!(group.active_tab, 2);
        group.next_tab();
        assert_eq!(group.active_tab, 0); // wrap

        // Backward through all tabs
        group.prev_tab();
        assert_eq!(group.active_tab, 2); // wrap backward
        group.prev_tab();
        assert_eq!(group.active_tab, 1);
        group.prev_tab();
        assert_eq!(group.active_tab, 0);
    }

    // ---- Window with multiple tabs ----

    #[test]
    fn test_window_add_tab_sets_active_to_last() {
        let gid = WindowId::new_v4();
        let p1_id = TabId::new_v4();
        let p1 = Tab::spawn_error(p1_id, TabKind::Shell, "t1");
        let mut group = Window::new(gid, p1);
        assert_eq!(group.active_tab, 0);

        let p2 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2");
        group.add_tab(p2);
        assert_eq!(group.active_tab, 1);

        let p3 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t3");
        group.add_tab(p3);
        assert_eq!(group.active_tab, 2);
    }

    #[test]
    fn test_window_close_last_tab_returns_false() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let mut group = Window::new(gid, p1);
        assert!(!group.close_tab(0));
        assert_eq!(group.tab_count(), 1);
    }

    #[test]
    fn test_window_close_tab_adjusts_active() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let p2_id = TabId::new_v4();
        let p2 = Tab::spawn_error(p2_id, TabKind::Shell, "t2");
        let p3 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t3");
        let mut group = Window::new(gid, p1);
        group.add_tab(p2);
        group.add_tab(p3);
        // active is 2 (last added)
        assert_eq!(group.active_tab, 2);
        // Close tab 2 (active)
        assert!(group.close_tab(2));
        assert_eq!(group.tab_count(), 2);
        // active should clamp to 1
        assert_eq!(group.active_tab, 1);
        assert_eq!(group.active_tab().id, p2_id);
    }

    #[test]
    fn test_window_remove_tab_returns_none_when_single() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let mut group = Window::new(gid, p1);
        assert!(group.remove_tab(0).is_none());
    }

    #[test]
    fn test_window_remove_tab_returns_removed() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let p2_id = TabId::new_v4();
        let p2 = Tab::spawn_error(p2_id, TabKind::Shell, "t2");
        let mut group = Window::new(gid, p1);
        group.add_tab(p2);
        let removed = group.remove_tab(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, p2_id);
        assert_eq!(group.tab_count(), 1);
    }

    #[test]
    fn test_window_tab_count() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let mut group = Window::new(gid, p1);
        assert_eq!(group.tab_count(), 1);
        group.add_tab(Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t2"));
        assert_eq!(group.tab_count(), 2);
        group.add_tab(Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t3"));
        assert_eq!(group.tab_count(), 3);
    }

    #[test]
    fn test_window_active_tab_and_active_tab_mut() {
        let gid = WindowId::new_v4();
        let p1_id = TabId::new_v4();
        let p1 = Tab::spawn_error(p1_id, TabKind::Shell, "t1");
        let mut group = Window::new(gid, p1);
        assert_eq!(group.active_tab().id, p1_id);

        // Mutate via active_tab_mut
        group.active_tab_mut().title = "modified".to_string();
        assert_eq!(group.active_tab().title, "modified");
    }

    #[test]
    fn test_window_name_default_none() {
        let gid = WindowId::new_v4();
        let p1 = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "t1");
        let group = Window::new(gid, p1);
        assert!(group.name.is_none());
    }

    #[test]
    fn test_scroll_down_below_zero() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        // Scroll down without any scroll offset should stay at 0
        pane.scroll_down(5);
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_up_then_down() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        for i in 0..20 {
            pane.vt.process(format!("line {}\r\n", i).as_bytes());
        }
        pane.scroll_up(10);
        let offset_after_up = pane.scroll_offset;
        assert!(offset_after_up > 0);

        pane.scroll_down(3);
        assert!(pane.scroll_offset < offset_after_up);
    }

    #[test]
    fn test_resize_pty_clamps_zero_dimensions() {
        let mut pane = Tab::spawn_error(TabId::new_v4(), TabKind::Shell, "");
        pane.resize_pty(0, 0);
        let (rows, cols) = pane.screen().size();
        // Should clamp to 1x1
        assert_eq!(rows, 1);
        assert_eq!(cols, 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_pty_foreground_process_detection() {
        use portable_pty::{native_pty_system, PtySize, CommandBuilder};
        use std::io::{Read, Write};

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
            .unwrap();

        let cmd = CommandBuilder::new("/bin/sh");
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        let shell_pid = child.process_id();
        drop(pair.slave);

        let mut writer = pair.master.take_writer().unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();

        // Drain output in background
        let drain = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(500));

        // Before running a command, fg should be the shell
        let fg1 = pair.master.process_group_leader();
        eprintln!("shell_pid={:?} fg_before={:?}", shell_pid, fg1);
        assert_eq!(fg1.map(|p| p as u32), shell_pid);

        // Run sleep in the shell
        writer.write_all(b"sleep 30\n").unwrap();
        writer.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Now fg should be sleep, not the shell
        let fg2 = pair.master.process_group_leader();
        eprintln!("fg_after_sleep={:?}", fg2);
        assert_ne!(fg2.map(|p| p as u32), shell_pid, "foreground should be sleep, not shell");

        // Check the process name
        if let Some(pid) = fg2 {
            let (name, _path) = process_info_by_pid(pid as u32);
            eprintln!("detected process: {:?}", name);
            assert_eq!(name.as_deref(), Some("sleep"));
        }

        // Cleanup
        writer.write_all(&[3]).unwrap(); // Ctrl-C
        writer.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        writer.write_all(b"exit\n").unwrap();
        writer.flush().unwrap();
        let _ = child.wait();
        let _ = drain.join();
    }

}
