use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Convert a crossterm KeyEvent to bytes suitable for writing to a PTY.
pub fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    let mods = key.modifiers;

    match key.code {
        KeyCode::Char(c) => {
            if mods.contains(KeyModifiers::CONTROL) {
                if c.is_ascii_lowercase() {
                    return vec![c as u8 - b'a' + 1];
                }
                if c.is_ascii_uppercase() {
                    return vec![c.to_ascii_lowercase() as u8 - b'a' + 1];
                }
            }
            if mods.contains(KeyModifiers::ALT) {
                let mut bytes = vec![0x1b];
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                return bytes;
            }
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn plain(code: KeyCode) -> KeyEvent {
        key(code, KeyModifiers::NONE)
    }

    // --- Ctrl+letter ---

    #[test]
    fn ctrl_a() {
        assert_eq!(key_to_bytes(key(KeyCode::Char('a'), KeyModifiers::CONTROL)), vec![1]);
    }

    #[test]
    fn ctrl_c() {
        assert_eq!(key_to_bytes(key(KeyCode::Char('c'), KeyModifiers::CONTROL)), vec![3]);
    }

    #[test]
    fn ctrl_z() {
        assert_eq!(key_to_bytes(key(KeyCode::Char('z'), KeyModifiers::CONTROL)), vec![26]);
    }

    #[test]
    fn ctrl_uppercase_maps_same_as_lowercase() {
        assert_eq!(
            key_to_bytes(key(KeyCode::Char('A'), KeyModifiers::CONTROL)),
            key_to_bytes(key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        );
    }

    // --- Alt+char ---

    #[test]
    fn alt_char_produces_esc_prefix() {
        let bytes = key_to_bytes(key(KeyCode::Char('x'), KeyModifiers::ALT));
        assert_eq!(bytes, vec![0x1b, b'x']);
    }

    #[test]
    fn alt_uppercase_char() {
        let bytes = key_to_bytes(key(KeyCode::Char('Z'), KeyModifiers::ALT));
        assert_eq!(bytes, vec![0x1b, b'Z']);
    }

    // --- Arrow keys ---

    #[test]
    fn arrow_up() {
        assert_eq!(key_to_bytes(plain(KeyCode::Up)), b"\x1b[A".to_vec());
    }

    #[test]
    fn arrow_down() {
        assert_eq!(key_to_bytes(plain(KeyCode::Down)), b"\x1b[B".to_vec());
    }

    #[test]
    fn arrow_right() {
        assert_eq!(key_to_bytes(plain(KeyCode::Right)), b"\x1b[C".to_vec());
    }

    #[test]
    fn arrow_left() {
        assert_eq!(key_to_bytes(plain(KeyCode::Left)), b"\x1b[D".to_vec());
    }

    // --- Function keys F1-F12 ---

    #[test]
    fn function_keys() {
        let expected: Vec<(&[u8], u8)> = vec![
            (b"\x1bOP", 1),
            (b"\x1bOQ", 2),
            (b"\x1bOR", 3),
            (b"\x1bOS", 4),
            (b"\x1b[15~", 5),
            (b"\x1b[17~", 6),
            (b"\x1b[18~", 7),
            (b"\x1b[19~", 8),
            (b"\x1b[20~", 9),
            (b"\x1b[21~", 10),
            (b"\x1b[23~", 11),
            (b"\x1b[24~", 12),
        ];
        for (seq, n) in expected {
            assert_eq!(key_to_bytes(plain(KeyCode::F(n))), seq.to_vec(), "F{n}");
        }
    }

    #[test]
    fn function_key_out_of_range_returns_empty() {
        assert!(key_to_bytes(plain(KeyCode::F(0))).is_empty());
        assert!(key_to_bytes(plain(KeyCode::F(13))).is_empty());
        assert!(key_to_bytes(plain(KeyCode::F(255))).is_empty());
    }

    // --- Special keys ---

    #[test]
    fn enter() {
        assert_eq!(key_to_bytes(plain(KeyCode::Enter)), vec![b'\r']);
    }

    #[test]
    fn backspace() {
        assert_eq!(key_to_bytes(plain(KeyCode::Backspace)), vec![0x7f]);
    }

    #[test]
    fn delete() {
        assert_eq!(key_to_bytes(plain(KeyCode::Delete)), b"\x1b[3~".to_vec());
    }

    #[test]
    fn tab() {
        assert_eq!(key_to_bytes(plain(KeyCode::Tab)), vec![b'\t']);
    }

    #[test]
    fn esc() {
        assert_eq!(key_to_bytes(plain(KeyCode::Esc)), vec![0x1b]);
    }

    #[test]
    fn home() {
        assert_eq!(key_to_bytes(plain(KeyCode::Home)), b"\x1b[H".to_vec());
    }

    #[test]
    fn end() {
        assert_eq!(key_to_bytes(plain(KeyCode::End)), b"\x1b[F".to_vec());
    }

    #[test]
    fn page_up() {
        assert_eq!(key_to_bytes(plain(KeyCode::PageUp)), b"\x1b[5~".to_vec());
    }

    #[test]
    fn page_down() {
        assert_eq!(key_to_bytes(plain(KeyCode::PageDown)), b"\x1b[6~".to_vec());
    }

    #[test]
    fn insert() {
        assert_eq!(key_to_bytes(plain(KeyCode::Insert)), b"\x1b[2~".to_vec());
    }

    // --- Regular characters ---

    #[test]
    fn regular_ascii_char() {
        assert_eq!(key_to_bytes(plain(KeyCode::Char('a'))), vec![b'a']);
        assert_eq!(key_to_bytes(plain(KeyCode::Char('Z'))), vec![b'Z']);
        assert_eq!(key_to_bytes(plain(KeyCode::Char('5'))), vec![b'5']);
    }

    #[test]
    fn unicode_char() {
        let bytes = key_to_bytes(plain(KeyCode::Char('é')));
        assert_eq!(bytes, "é".as_bytes().to_vec());
    }

    // --- Unmapped keycodes ---

    #[test]
    fn unmapped_keycode_returns_empty() {
        assert!(key_to_bytes(plain(KeyCode::BackTab)).is_empty());
        assert!(key_to_bytes(plain(KeyCode::Null)).is_empty());
    }
}
