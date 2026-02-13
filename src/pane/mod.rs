pub mod pty;
pub mod terminal;

use crate::event::AppEvent;
use crate::layout::PaneId;
use portable_pty::PtySize;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Get the name of a process by PID. Returns None if lookup fails.
#[cfg(target_os = "macos")]
fn process_name_by_pid(pid: u32) -> Option<String> {
    extern "C" {
        fn proc_name(pid: std::ffi::c_int, buffer: *mut u8, buffersize: u32) -> std::ffi::c_int;
    }
    let mut buf = [0u8; 256];
    let ret = unsafe { proc_name(pid as std::ffi::c_int, buf.as_mut_ptr(), buf.len() as u32) };
    if ret > 0 {
        Some(String::from_utf8_lossy(&buf[..ret as usize]).to_string())
    } else {
        None
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_name_by_pid(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(not(unix))]
fn process_name_by_pid(_pid: u32) -> Option<String> {
    None
}

pub type PaneGroupId = uuid::Uuid;

pub struct PaneGroup {
    #[allow(dead_code)]
    pub id: PaneGroupId,
    pub tabs: Vec<Pane>,
    pub active_tab: usize,
    /// Optional user-assigned window name.
    pub name: Option<String>,
}

impl PaneGroup {
    pub fn new(id: PaneGroupId, pane: Pane) -> Self {
        Self {
            id,
            tabs: vec![pane],
            active_tab: 0,
            name: None,
        }
    }

    pub fn active_pane(&self) -> &Pane {
        &self.tabs[self.active_tab]
    }

    pub fn active_pane_mut(&mut self) -> &mut Pane {
        &mut self.tabs[self.active_tab]
    }

    pub fn add_tab(&mut self, pane: Pane) {
        self.tabs.push(pane);
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

    pub fn remove_tab(&mut self, idx: usize) -> Option<Pane> {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKind {
    Shell,
    Agent,
    Nvim,
    DevServer,
}

impl PaneKind {
    pub fn label(&self) -> &str {
        match self {
            PaneKind::Shell => "shell",
            PaneKind::Agent => "claude",
            PaneKind::Nvim => "nvim",
            PaneKind::DevServer => "server",
        }
    }
}

#[allow(dead_code)]
pub struct Pane {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
    pub vt: vt100::Parser,
    pub exited: bool,
    pub command: Option<String>,
    pub cwd: PathBuf,
    pub scroll_offset: usize,
    /// Cached: true when the foreground process is `claude`.
    pub running_claude: bool,
    shell_pid: Option<u32>,
    pty_writer: Option<Box<dyn Write + Send>>,
    pty_child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
}

impl Pane {
    #[allow(dead_code)]
    pub fn spawn(
        id: PaneId,
        kind: PaneKind,
        cols: u16,
        rows: u16,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        command: Option<String>,
    ) -> anyhow::Result<Self> {
        Self::spawn_with_env(id, kind, cols, rows, event_tx, command, None)
    }

    pub fn spawn_with_env(
        id: PaneId,
        kind: PaneKind,
        cols: u16,
        rows: u16,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        command: Option<String>,
        tmux_env: Option<pty::TmuxEnv>,
    ) -> anyhow::Result<Self> {
        let (cmd, args): (&str, Vec<&str>) = match &kind {
            PaneKind::Shell => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                // Leak is acceptable here; these are long-lived process strings
                let shell: &'static str = Box::leak(shell.into_boxed_str());
                (shell, vec![])
            }
            PaneKind::Nvim => ("nvim", vec![]),
            PaneKind::Agent => ("claude", vec![]),
            PaneKind::DevServer => {
                let cmd_str = command.as_deref().unwrap_or("echo 'no command'");
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                let shell: &'static str = Box::leak(shell.into_boxed_str());
                let cmd_str: &'static str = Box::leak(cmd_str.to_string().into_boxed_str());
                (shell, vec!["-c", cmd_str])
            }
        };

        let title = kind.label().to_string();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pty_handle = pty::spawn_pty(cmd, &args, size, event_tx, id, Some(&cwd), tmux_env)?;
        let shell_pid = pty_handle.child.process_id();
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
            running_claude: kind == PaneKind::Agent,
            shell_pid,
            pty_writer: Some(pty_handle.writer),
            pty_child: Some(pty_handle.child),
            pty_master: Some(pty_handle.master),
        })
    }

    /// Create a pane that shows an error message instead of a PTY.
    pub fn spawn_error(id: PaneId, kind: PaneKind, error_msg: &str) -> Self {
        let mut vt = vt100::Parser::new(24, 80, 0);
        vt.process(format!("[error: {}]\r\n", error_msg).as_bytes());
        Self {
            id,
            kind: kind.clone(),
            title: format!("{} (error)", kind.label()),
            vt,
            exited: true,
            command: None,
            cwd: PathBuf::from("/"),
            scroll_offset: 0,
            running_claude: false,
            shell_pid: None,
            pty_writer: None,
            pty_child: None,
            pty_master: None,
        }
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        if let Some(writer) = &mut self.pty_writer {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    pub fn is_scrolled(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Returns true if the pane is idle (exited, or shell at prompt with no foreground job).
    #[cfg(unix)]
    pub fn is_idle(&self) -> bool {
        if self.exited {
            return true;
        }
        let shell_pid = match self.shell_pid {
            Some(pid) => pid,
            None => return false,
        };
        let fg_pgid = match &self.pty_master {
            Some(master) => master.process_group_leader(),
            None => return false,
        };
        match fg_pgid {
            Some(pgid) => pgid as u32 == shell_pid,
            None => false,
        }
    }

    #[cfg(not(unix))]
    pub fn is_idle(&self) -> bool {
        self.exited
    }

    /// Check if the foreground process is claude and update the cached flag.
    #[cfg(unix)]
    pub fn update_running_claude(&mut self) {
        if self.exited {
            self.running_claude = false;
            return;
        }
        let fg_pid = self.pty_master.as_ref()
            .and_then(|m| m.process_group_leader())
            .map(|pgid| pgid as u32);
        self.running_claude = match fg_pid {
            Some(pid) => process_name_by_pid(pid)
                .map(|name| name == "claude")
                .unwrap_or(false),
            None => false,
        };
    }

    #[cfg(not(unix))]
    pub fn update_running_claude(&mut self) {
        self.running_claude = false;
    }

    pub fn scroll_up(&mut self, n: usize) {
        let max_offset = self.vt.screen().size().0 as usize;
        self.scroll_offset = self.scroll_offset.saturating_add(n).min(max_offset);
        self.vt.set_scrollback(self.scroll_offset);
        self.scroll_offset = self.vt.screen().scrollback();
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        self.vt.set_scrollback(self.scroll_offset);
        self.scroll_offset = self.vt.screen().scrollback();
    }

    pub fn scroll_to_top(&mut self) {
        let max_offset = self.vt.screen().size().0 as usize;
        self.scroll_offset = max_offset;
        self.vt.set_scrollback(self.scroll_offset);
        self.scroll_offset = self.vt.screen().scrollback();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.vt.set_scrollback(0);
    }

    pub fn process_output(&mut self, bytes: &[u8]) {
        self.vt.process(bytes);
        // Sync scrollback offset: vt100 auto-adjusts when content scrolls
        if self.scroll_offset > 0 {
            self.scroll_offset = self.vt.screen().scrollback();
        }
        // Sync title from OSC escape sequences (e.g. \e]0;title\a)
        let osc_title = self.vt.screen().title();
        if !osc_title.is_empty() {
            self.title = osc_title.to_string();
        }
    }

    pub fn resize_pty(&mut self, cols: u16, rows: u16) {
        if self.scroll_offset > 0 {
            self.scroll_to_bottom();
        }
        self.vt.set_size(rows, cols);
        if let Some(master) = &self.pty_master {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.vt.screen()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_error_sets_title() {
        let pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "bad thing");
        assert_eq!(pane.title, "shell (error)");
    }

    #[test]
    fn test_spawn_error_sets_exited() {
        let pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Agent, "fail");
        assert!(pane.exited);
    }

    #[test]
    fn test_spawn_error_has_no_pty() {
        let pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "err");
        assert!(pane.pty_writer.is_none());
        assert!(pane.pty_child.is_none());
        assert!(pane.pty_master.is_none());
    }

    #[test]
    fn test_spawn_error_writes_message_to_screen() {
        let pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "something broke");
        let content = pane.screen().contents();
        assert!(content.contains("[error: something broke]"));
    }

    #[test]
    fn test_spawn_error_empty_message() {
        let pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        let content = pane.screen().contents();
        assert!(content.contains("[error: ]"));
    }

    #[test]
    fn test_spawn_error_preserves_kind() {
        let pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::DevServer, "fail");
        assert_eq!(pane.kind, PaneKind::DevServer);
        assert_eq!(pane.title, "server (error)");
    }

    #[test]
    fn test_spawn_error_preserves_id() {
        let id = PaneId::new_v4();
        let pane = Pane::spawn_error(id, PaneKind::Shell, "err");
        assert_eq!(pane.id, id);
    }

    #[test]
    fn test_process_output_updates_screen() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        pane.vt = vt100::Parser::new(24, 80, 0);
        pane.process_output(b"hello world");
        let content = pane.screen().contents();
        assert!(content.contains("hello world"));
    }

    #[test]
    fn test_process_output_osc_title_update() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        pane.vt = vt100::Parser::new(24, 80, 0);
        // OSC 0 sets window title: ESC ] 0 ; title BEL
        pane.process_output(b"\x1b]0;my-custom-title\x07");
        assert_eq!(pane.title, "my-custom-title");
    }

    #[test]
    fn test_process_output_empty_osc_title_keeps_existing() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        pane.title = "original".to_string();
        pane.vt = vt100::Parser::new(24, 80, 0);
        // Regular output without OSC title — title should stay
        pane.process_output(b"some output");
        assert_eq!(pane.title, "original");
    }

    #[test]
    fn test_resize_pty_updates_vt_size() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        pane.resize_pty(120, 40);
        let (rows, cols) = pane.screen().size();
        assert_eq!(rows, 40);
        assert_eq!(cols, 120);
    }

    #[test]
    fn test_write_input_on_error_pane_does_not_panic() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "err");
        // Should be a no-op since there's no PTY writer
        pane.write_input(b"hello");
    }

    #[test]
    fn test_pane_group_close_middle_tab() {
        let gid = PaneGroupId::new_v4();
        let p1_id = PaneId::new_v4();
        let p2_id = PaneId::new_v4();
        let p3_id = PaneId::new_v4();
        let p1 = Pane::spawn_error(p1_id, PaneKind::Shell, "t1");
        let p2 = Pane::spawn_error(p2_id, PaneKind::Shell, "t2");
        let p3 = Pane::spawn_error(p3_id, PaneKind::Shell, "t3");
        let mut group = PaneGroup::new(gid, p1);
        group.add_tab(p2);
        group.add_tab(p3);
        // Active is 2 (last added). Close middle tab (index 1).
        assert!(group.close_tab(1));
        assert_eq!(group.tab_count(), 2);
        // active_tab was 2, now clamped to 1
        assert_eq!(group.active_tab, 1);
        assert_eq!(group.active_pane().id, p3_id);
    }

    #[test]
    fn test_pane_group_close_active_first_tab() {
        let gid = PaneGroupId::new_v4();
        let p1 = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "t1");
        let p2_id = PaneId::new_v4();
        let p2 = Pane::spawn_error(p2_id, PaneKind::Shell, "t2");
        let mut group = PaneGroup::new(gid, p1);
        group.add_tab(p2);
        // Switch to first tab
        group.active_tab = 0;
        assert!(group.close_tab(0));
        assert_eq!(group.tab_count(), 1);
        assert_eq!(group.active_tab, 0);
        assert_eq!(group.active_pane().id, p2_id);
    }

    #[test]
    fn test_pane_kind_label_devserver() {
        assert_eq!(PaneKind::DevServer.label(), "server");
    }

    #[test]
    fn test_pane_kind_clone_eq() {
        let k1 = PaneKind::Agent;
        let k2 = k1.clone();
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_scroll_initial_state() {
        let pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "test");
        assert_eq!(pane.scroll_offset, 0);
        assert!(!pane.is_scrolled());
    }

    #[test]
    fn test_scroll_up_no_scrollback() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        // No content → no scrollback available
        pane.scroll_up(5);
        // vt100 clamps to 0 since there's nothing to scroll
        assert_eq!(pane.scroll_offset, 0);
        assert!(!pane.is_scrolled());
    }

    #[test]
    fn test_scroll_up_with_scrollback() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        // Generate 20 lines to push content into scrollback
        for i in 0..20 {
            pane.vt.process(format!("line {}\r\n", i).as_bytes());
        }
        pane.scroll_up(5);
        assert!(pane.is_scrolled());
        assert!(pane.scroll_offset > 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
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
    fn test_scroll_to_top() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
        pane.vt = vt100::Parser::new(3, 80, 1000);
        for i in 0..20 {
            pane.vt.process(format!("line {}\r\n", i).as_bytes());
        }
        pane.scroll_to_top();
        assert!(pane.is_scrolled());
        // Should be at maximum available scrollback
        let offset = pane.scroll_offset;
        assert!(offset > 0);
        // Scrolling up more shouldn't increase it (already at top)
        pane.scroll_up(100);
        assert_eq!(pane.scroll_offset, offset);
    }

    #[test]
    fn test_resize_resets_scroll() {
        let mut pane = Pane::spawn_error(PaneId::new_v4(), PaneKind::Shell, "");
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
}
