use crate::event::AppEvent;
use crate::layout::PaneId;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use tokio::sync::mpsc;

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
    pane_id: PaneId,
    cwd: Option<&std::path::Path>,
) -> anyhow::Result<PtyHandle> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(size)?;

    let mut cmd_builder = CommandBuilder::new(cmd);
    cmd_builder.args(args);
    if let Some(dir) = cwd {
        cmd_builder.cwd(dir);
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
        let mut buf = [0u8; 4096];
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

    Ok(PtyHandle { writer, child, master })
}
