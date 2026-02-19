use crate::layout::TabId;
use crate::system_stats::SystemStats;
use crossterm::event::{Event, EventStream, KeyEvent, MouseButton, MouseEventKind};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    MouseDown { x: u16, y: u16 },
    MouseRightDown,
    MouseDrag { x: u16, y: u16 },
    MouseMove { x: u16, y: u16 },
    MouseUp,
    MouseScroll { up: bool },
    Resize(u16, u16),
    Tick,
    PtyOutput { pane_id: TabId, bytes: Vec<u8> },
    PtyExited { pane_id: TabId },
    SystemStats(SystemStats),
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
                            MouseEventKind::Down(MouseButton::Left) => AppEvent::MouseDown {
                                x: m.column,
                                y: m.row,
                            },
                            MouseEventKind::Down(MouseButton::Right) => AppEvent::MouseRightDown,
                            MouseEventKind::Drag(MouseButton::Left) => AppEvent::MouseDrag {
                                x: m.column,
                                y: m.row,
                            },
                            MouseEventKind::Moved => AppEvent::MouseMove {
                                x: m.column,
                                y: m.row,
                            },
                            MouseEventKind::Up(_) => AppEvent::MouseUp,
                            MouseEventKind::ScrollUp => AppEvent::MouseScroll { up: true },
                            MouseEventKind::ScrollDown => AppEvent::MouseScroll { up: false },
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
