pub mod pty;
pub mod terminal;

use crate::event::AppEvent;
use crate::layout::PaneId;
use portable_pty::PtySize;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub type PaneGroupId = uuid::Uuid;

pub struct PaneGroup {
    #[allow(dead_code)]
    pub id: PaneGroupId,
    pub tabs: Vec<Pane>,
    pub active_tab: usize,
}

impl PaneGroup {
    pub fn new(id: PaneGroupId, pane: Pane) -> Self {
        Self {
            id,
            tabs: vec![pane],
            active_tab: 0,
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
    pty_writer: Option<Box<dyn Write + Send>>,
    pty_child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
}

impl Pane {
    pub fn spawn(
        id: PaneId,
        kind: PaneKind,
        cols: u16,
        rows: u16,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        command: Option<String>,
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

        let pty_handle = pty::spawn_pty(cmd, &args, size, event_tx, id, Some(&cwd))?;
        let vt = vt100::Parser::new(rows, cols, 1000);

        Ok(Self {
            id,
            kind,
            title,
            vt,
            exited: false,
            command,
            cwd,
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

    pub fn process_output(&mut self, bytes: &[u8]) {
        self.vt.process(bytes);
    }

    pub fn resize_pty(&mut self, cols: u16, rows: u16) {
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
