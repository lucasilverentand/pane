use pane_protocol::event::AppEvent;
use pane_protocol::layout::TabId;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use tokio::sync::mpsc;

/// Environment variables to make child processes think they're inside tmux.
pub struct TmuxEnv {
    /// Value for $TMUX: "<socket_path>,<pid>,0"
    pub tmux_value: String,
    /// Value for $TMUX_PANE: "%N"
    pub tmux_pane: String,
}

pub struct PtyHandle {
    pub writer: Box<dyn Write + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
}

pub fn spawn_pty(
    cmd: &str,
    args: &[&str],
    size: PtySize,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    pane_id: TabId,
    cwd: Option<&std::path::Path>,
    tmux_env: Option<TmuxEnv>,
) -> anyhow::Result<PtyHandle> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(size)?;

    let mut cmd_builder = CommandBuilder::new(cmd);
    cmd_builder.args(args);
    if let Some(dir) = cwd {
        cmd_builder.cwd(dir);
    }
    cmd_builder.env("PANE", "1");
    cmd_builder.env("PANE_PANE", pane_id.to_string());
    cmd_builder.env("TERM", "xterm-256color");
    cmd_builder.env("COLORTERM", "truecolor");

    if let Some(ref env) = tmux_env {
        cmd_builder.env("TMUX", &env.tmux_value);
        cmd_builder.env("TMUX_PANE", &env.tmux_pane);
    }

    let child = pair.slave.spawn_command(cmd_builder)?;
    drop(pair.slave);

    let writer = pair.master.take_writer()?;
    let mut reader = pair.master.try_clone_reader()?;
    let master = pair.master;

    // Spawn blocking reader task
    let tx = event_tx.clone();
    let pid = pane_id;
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 65536];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(AppEvent::PtyExited { pane_id: pid });
                    break;
                }
                Ok(n) => {
                    let bytes = buf[..n].to_vec();
                    if tx
                        .send(AppEvent::PtyOutput {
                            pane_id: pid,
                            bytes,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(AppEvent::PtyExited { pane_id: pid });
                    break;
                }
            }
        }
    });

    Ok(PtyHandle {
        writer,
        child,
        master,
    })
}
