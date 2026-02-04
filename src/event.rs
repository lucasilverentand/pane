use crate::layout::PaneId;
use crossterm::event::{Event, EventStream, KeyEvent, MouseEventKind};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    MouseDown { x: u16, y: u16 },
    Resize(u16, u16),
    Tick,
    PtyOutput { pane_id: PaneId, bytes: Vec<u8> },
    PtyExited { pane_id: PaneId },
}

pub fn start_event_loop(event_tx: mpsc::UnboundedSender<AppEvent>) {
    // Crossterm event reader
    let tx = event_tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        loop {
            match reader.next().await {
                Some(Ok(event)) => {
                    let app_event = match event {
                        Event::Key(key) => AppEvent::Key(key),
                        Event::Mouse(m) => match m.kind {
                            MouseEventKind::Down(_) => {
                                AppEvent::MouseDown { x: m.column, y: m.row }
                            }
                            _ => continue,
                        },
                        Event::Resize(w, h) => AppEvent::Resize(w, h),
                        _ => continue,
                    };
                    if tx.send(app_event).is_err() {
                        break;
                    }
                }
                Some(Err(_)) => break,
                None => break,
            }
        }
    });

    // Tick timer at ~30fps
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(33));
        loop {
            interval.tick().await;
            if event_tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
}
