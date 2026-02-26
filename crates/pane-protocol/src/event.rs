use crate::layout::TabId;
use crate::system_stats::SystemStats;
use crossterm::event::KeyEvent;

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
