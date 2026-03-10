use crate::layout::TabId;
use crate::system_stats::SystemStats;
use crossterm::event::KeyEvent;

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    MouseDown { x: u16, y: u16 },
    MouseRightDown { x: u16, y: u16 },
    MouseDrag { x: u16, y: u16 },
    MouseMove { x: u16, y: u16 },
    MouseUp { x: u16, y: u16 },
    MouseScroll { up: bool },
    Resize(u16, u16),
    Tick,
    PtyOutput { pane_id: TabId, bytes: Vec<u8> },
    PtyExited { pane_id: TabId },
    SystemStats(SystemStats),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn key_event_construction() {
        let event = AppEvent::Key(make_key(KeyCode::Char('a')));
        match event {
            AppEvent::Key(k) => assert_eq!(k.code, KeyCode::Char('a')),
            _ => panic!("Expected Key event"),
        }
    }

    #[test]
    fn mouse_down_fields() {
        let event = AppEvent::MouseDown { x: 42, y: 13 };
        match event {
            AppEvent::MouseDown { x, y } => {
                assert_eq!(x, 42);
                assert_eq!(y, 13);
            }
            _ => panic!("Expected MouseDown"),
        }
    }

    #[test]
    fn mouse_right_down_fields() {
        let event = AppEvent::MouseRightDown { x: 100, y: 200 };
        match event {
            AppEvent::MouseRightDown { x, y } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("Expected MouseRightDown"),
        }
    }

    #[test]
    fn mouse_drag_fields() {
        let event = AppEvent::MouseDrag { x: 5, y: 10 };
        match event {
            AppEvent::MouseDrag { x, y } => {
                assert_eq!(x, 5);
                assert_eq!(y, 10);
            }
            _ => panic!("Expected MouseDrag"),
        }
    }

    #[test]
    fn mouse_move_fields() {
        let event = AppEvent::MouseMove { x: 0, y: 0 };
        match event {
            AppEvent::MouseMove { x, y } => {
                assert_eq!(x, 0);
                assert_eq!(y, 0);
            }
            _ => panic!("Expected MouseMove"),
        }
    }

    #[test]
    fn mouse_up_fields() {
        let event = AppEvent::MouseUp { x: 99, y: 50 };
        match event {
            AppEvent::MouseUp { x, y } => {
                assert_eq!(x, 99);
                assert_eq!(y, 50);
            }
            _ => panic!("Expected MouseUp"),
        }
    }

    #[test]
    fn mouse_scroll_up() {
        let event = AppEvent::MouseScroll { up: true };
        match event {
            AppEvent::MouseScroll { up } => assert!(up),
            _ => panic!("Expected MouseScroll"),
        }
    }

    #[test]
    fn mouse_scroll_down() {
        let event = AppEvent::MouseScroll { up: false };
        match event {
            AppEvent::MouseScroll { up } => assert!(!up),
            _ => panic!("Expected MouseScroll"),
        }
    }

    #[test]
    fn resize_event() {
        let event = AppEvent::Resize(120, 40);
        match event {
            AppEvent::Resize(w, h) => {
                assert_eq!(w, 120);
                assert_eq!(h, 40);
            }
            _ => panic!("Expected Resize"),
        }
    }

    #[test]
    fn tick_event() {
        let event = AppEvent::Tick;
        assert!(matches!(event, AppEvent::Tick));
    }

    #[test]
    fn pty_output_event() {
        let id = TabId::new_v4();
        let data = vec![0x1b, b'[', b'H'];
        let event = AppEvent::PtyOutput {
            pane_id: id,
            bytes: data.clone(),
        };
        match event {
            AppEvent::PtyOutput { pane_id, bytes } => {
                assert_eq!(pane_id, id);
                assert_eq!(bytes, data);
            }
            _ => panic!("Expected PtyOutput"),
        }
    }

    #[test]
    fn pty_exited_event() {
        let id = TabId::new_v4();
        let event = AppEvent::PtyExited { pane_id: id };
        match event {
            AppEvent::PtyExited { pane_id } => assert_eq!(pane_id, id),
            _ => panic!("Expected PtyExited"),
        }
    }

    #[test]
    fn system_stats_event() {
        let stats = SystemStats {
            cpu_percent: 50.0,
            memory_percent: 75.0,
            load_avg_1: 2.5,
            disk_usage_percent: 30.0,
        };
        let event = AppEvent::SystemStats(stats);
        match event {
            AppEvent::SystemStats(s) => {
                assert!((s.cpu_percent - 50.0).abs() < f32::EPSILON);
                assert!((s.memory_percent - 75.0).abs() < f32::EPSILON);
            }
            _ => panic!("Expected SystemStats"),
        }
    }

    #[test]
    fn mouse_down_boundary_values() {
        // Max u16 values
        let event = AppEvent::MouseDown {
            x: u16::MAX,
            y: u16::MAX,
        };
        match event {
            AppEvent::MouseDown { x, y } => {
                assert_eq!(x, u16::MAX);
                assert_eq!(y, u16::MAX);
            }
            _ => panic!("Expected MouseDown"),
        }
    }

    #[test]
    fn debug_format_contains_variant_name() {
        let event = AppEvent::Tick;
        let debug = format!("{:?}", event);
        assert!(debug.contains("Tick"));

        let event = AppEvent::Resize(80, 24);
        let debug = format!("{:?}", event);
        assert!(debug.contains("Resize"));
    }

    #[test]
    fn pty_output_empty_bytes() {
        let id = TabId::new_v4();
        let event = AppEvent::PtyOutput {
            pane_id: id,
            bytes: vec![],
        };
        match event {
            AppEvent::PtyOutput { bytes, .. } => assert!(bytes.is_empty()),
            _ => panic!("Expected PtyOutput"),
        }
    }
}
