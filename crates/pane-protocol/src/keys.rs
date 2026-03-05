use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Convert a crossterm KeyEvent to bytes suitable for writing to a PTY.
///
/// When `application_cursor` is true, unmodified arrow keys use SS3 (`\x1bO`)
/// sequences instead of CSI (`\x1b[`), matching DEC application cursor mode.
pub fn key_to_bytes(key: KeyEvent, application_cursor: bool) -> Vec<u8> {
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
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => arrow_key(b'A', mods, application_cursor),
        KeyCode::Down => arrow_key(b'B', mods, application_cursor),
        KeyCode::Right => arrow_key(b'C', mods, application_cursor),
        KeyCode::Left => arrow_key(b'D', mods, application_cursor),
        KeyCode::Home => special_key(b"[H", None, mods),
        KeyCode::End => special_key(b"[F", None, mods),
        KeyCode::PageUp => special_key(b"[5~", Some(b'5'), mods),
        KeyCode::PageDown => special_key(b"[6~", Some(b'6'), mods),
        KeyCode::Delete => special_key(b"[3~", Some(b'3'), mods),
        KeyCode::Insert => special_key(b"[2~", Some(b'2'), mods),
        KeyCode::F(n) => f_key(n, mods),
        _ => vec![],
    }
}

/// Encode an arrow key, respecting application cursor mode and modifiers.
fn arrow_key(letter: u8, mods: KeyModifiers, application_cursor: bool) -> Vec<u8> {
    let mod_param = modifier_param(mods);
    if mod_param > 0 {
        // Modified arrows always use CSI: \x1b[1;{mod}{letter}
        format!("\x1b[1;{}{}", mod_param, letter as char)
            .into_bytes()
    } else if application_cursor {
        // Unmodified in application mode: \x1bO{letter}
        vec![0x1b, b'O', letter]
    } else {
        // Unmodified normal mode: \x1b[{letter}
        vec![0x1b, b'[', letter]
    }
}

/// Encode a special key (Home, End, PageUp, etc.) with optional modifiers.
///
/// For tilde-style keys (PageUp `5~`, Delete `3~`), `tilde_num` holds the number
/// before the tilde. Modified form: `\x1b[{num};{mod}~`.
/// For letter-style keys (Home `H`, End `F`), modified form: `\x1b[1;{mod}{letter}`.
fn special_key(base: &[u8], tilde_num: Option<u8>, mods: KeyModifiers) -> Vec<u8> {
    let mod_param = modifier_param(mods);
    if mod_param == 0 {
        let mut v = vec![0x1b];
        v.extend_from_slice(base);
        return v;
    }
    if let Some(num) = tilde_num {
        // Tilde-style: \x1b[{num};{mod}~
        format!("\x1b[{};{}~", num as char, mod_param).into_bytes()
    } else {
        // Letter-style (Home/End): \x1b[1;{mod}{last_char}
        let last = *base.last().unwrap();
        format!("\x1b[1;{}{}", mod_param, last as char).into_bytes()
    }
}

/// Encode function keys F1-F12 with optional modifiers.
fn f_key(n: u8, mods: KeyModifiers) -> Vec<u8> {
    let mod_param = modifier_param(mods);
    if mod_param == 0 {
        // Unmodified function keys
        return match n {
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
        };
    }
    // Modified function keys: F1-4 use \x1b[1;{mod}P/Q/R/S, F5+ use \x1b[{num};{mod}~
    match n {
        1 => format!("\x1b[1;{}P", mod_param).into_bytes(),
        2 => format!("\x1b[1;{}Q", mod_param).into_bytes(),
        3 => format!("\x1b[1;{}R", mod_param).into_bytes(),
        4 => format!("\x1b[1;{}S", mod_param).into_bytes(),
        5 => format!("\x1b[15;{}~", mod_param).into_bytes(),
        6 => format!("\x1b[17;{}~", mod_param).into_bytes(),
        7 => format!("\x1b[18;{}~", mod_param).into_bytes(),
        8 => format!("\x1b[19;{}~", mod_param).into_bytes(),
        9 => format!("\x1b[20;{}~", mod_param).into_bytes(),
        10 => format!("\x1b[21;{}~", mod_param).into_bytes(),
        11 => format!("\x1b[23;{}~", mod_param).into_bytes(),
        12 => format!("\x1b[24;{}~", mod_param).into_bytes(),
        _ => vec![],
    }
}

/// Compute the xterm-style modifier parameter (0 = none).
///
/// The parameter value is `1 + bitmask` where Shift=1, Alt=2, Ctrl=4.
fn modifier_param(mods: KeyModifiers) -> u8 {
    let mut bits: u8 = 0;
    if mods.contains(KeyModifiers::SHIFT) {
        bits |= 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        bits |= 2;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        bits |= 4;
    }
    if bits == 0 { 0 } else { 1 + bits }
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

    /// Shorthand: key_to_bytes with application_cursor = false.
    fn kb(ev: KeyEvent) -> Vec<u8> {
        key_to_bytes(ev, false)
    }

    /// Shorthand: key_to_bytes with application_cursor = true.
    fn kb_app(ev: KeyEvent) -> Vec<u8> {
        key_to_bytes(ev, true)
    }

    // --- Ctrl+letter ---

    #[test]
    fn ctrl_a() {
        assert_eq!(
            kb(key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            vec![1]
        );
    }

    #[test]
    fn ctrl_c() {
        assert_eq!(
            kb(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            vec![3]
        );
    }

    #[test]
    fn ctrl_z() {
        assert_eq!(
            kb(key(KeyCode::Char('z'), KeyModifiers::CONTROL)),
            vec![26]
        );
    }

    #[test]
    fn ctrl_uppercase_maps_same_as_lowercase() {
        assert_eq!(
            kb(key(KeyCode::Char('A'), KeyModifiers::CONTROL)),
            kb(key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        );
    }

    // --- Alt+char ---

    #[test]
    fn alt_char_produces_esc_prefix() {
        let bytes = kb(key(KeyCode::Char('x'), KeyModifiers::ALT));
        assert_eq!(bytes, vec![0x1b, b'x']);
    }

    #[test]
    fn alt_uppercase_char() {
        let bytes = kb(key(KeyCode::Char('Z'), KeyModifiers::ALT));
        assert_eq!(bytes, vec![0x1b, b'Z']);
    }

    // --- Arrow keys (normal mode) ---

    #[test]
    fn arrow_up() {
        assert_eq!(kb(plain(KeyCode::Up)), b"\x1b[A".to_vec());
    }

    #[test]
    fn arrow_down() {
        assert_eq!(kb(plain(KeyCode::Down)), b"\x1b[B".to_vec());
    }

    #[test]
    fn arrow_right() {
        assert_eq!(kb(plain(KeyCode::Right)), b"\x1b[C".to_vec());
    }

    #[test]
    fn arrow_left() {
        assert_eq!(kb(plain(KeyCode::Left)), b"\x1b[D".to_vec());
    }

    // --- Arrow keys (application cursor mode) ---

    #[test]
    fn arrow_up_application_cursor() {
        assert_eq!(kb_app(plain(KeyCode::Up)), b"\x1bOA".to_vec());
    }

    #[test]
    fn arrow_down_application_cursor() {
        assert_eq!(kb_app(plain(KeyCode::Down)), b"\x1bOB".to_vec());
    }

    #[test]
    fn arrow_right_application_cursor() {
        assert_eq!(kb_app(plain(KeyCode::Right)), b"\x1bOC".to_vec());
    }

    #[test]
    fn arrow_left_application_cursor() {
        assert_eq!(kb_app(plain(KeyCode::Left)), b"\x1bOD".to_vec());
    }

    // --- Modified arrow keys ---

    #[test]
    fn shift_up() {
        assert_eq!(
            kb(key(KeyCode::Up, KeyModifiers::SHIFT)),
            b"\x1b[1;2A".to_vec()
        );
    }

    #[test]
    fn ctrl_right() {
        assert_eq!(
            kb(key(KeyCode::Right, KeyModifiers::CONTROL)),
            b"\x1b[1;5C".to_vec()
        );
    }

    #[test]
    fn alt_left() {
        assert_eq!(
            kb(key(KeyCode::Left, KeyModifiers::ALT)),
            b"\x1b[1;3D".to_vec()
        );
    }

    #[test]
    fn ctrl_shift_down() {
        assert_eq!(
            kb(key(KeyCode::Down, KeyModifiers::CONTROL | KeyModifiers::SHIFT)),
            b"\x1b[1;6B".to_vec()
        );
    }

    #[test]
    fn modified_arrows_ignore_application_cursor() {
        // Modified arrows always use CSI even in application cursor mode
        assert_eq!(
            kb_app(key(KeyCode::Up, KeyModifiers::SHIFT)),
            b"\x1b[1;2A".to_vec()
        );
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
            assert_eq!(kb(plain(KeyCode::F(n))), seq.to_vec(), "F{n}");
        }
    }

    #[test]
    fn function_key_out_of_range_returns_empty() {
        assert!(kb(plain(KeyCode::F(0))).is_empty());
        assert!(kb(plain(KeyCode::F(13))).is_empty());
        assert!(kb(plain(KeyCode::F(255))).is_empty());
    }

    #[test]
    fn shift_f1() {
        assert_eq!(
            kb(key(KeyCode::F(1), KeyModifiers::SHIFT)),
            b"\x1b[1;2P".to_vec()
        );
    }

    #[test]
    fn ctrl_f5() {
        assert_eq!(
            kb(key(KeyCode::F(5), KeyModifiers::CONTROL)),
            b"\x1b[15;5~".to_vec()
        );
    }

    // --- Special keys ---

    #[test]
    fn enter() {
        assert_eq!(kb(plain(KeyCode::Enter)), vec![b'\r']);
    }

    #[test]
    fn backspace() {
        assert_eq!(kb(plain(KeyCode::Backspace)), vec![0x7f]);
    }

    #[test]
    fn delete() {
        assert_eq!(kb(plain(KeyCode::Delete)), b"\x1b[3~".to_vec());
    }

    #[test]
    fn tab() {
        assert_eq!(kb(plain(KeyCode::Tab)), vec![b'\t']);
    }

    #[test]
    fn backtab() {
        assert_eq!(kb(plain(KeyCode::BackTab)), b"\x1b[Z".to_vec());
    }

    #[test]
    fn esc() {
        assert_eq!(kb(plain(KeyCode::Esc)), vec![0x1b]);
    }

    #[test]
    fn home() {
        assert_eq!(kb(plain(KeyCode::Home)), b"\x1b[H".to_vec());
    }

    #[test]
    fn end() {
        assert_eq!(kb(plain(KeyCode::End)), b"\x1b[F".to_vec());
    }

    #[test]
    fn page_up() {
        assert_eq!(kb(plain(KeyCode::PageUp)), b"\x1b[5~".to_vec());
    }

    #[test]
    fn page_down() {
        assert_eq!(kb(plain(KeyCode::PageDown)), b"\x1b[6~".to_vec());
    }

    #[test]
    fn insert() {
        assert_eq!(kb(plain(KeyCode::Insert)), b"\x1b[2~".to_vec());
    }

    // --- Modified special keys ---

    #[test]
    fn ctrl_home() {
        assert_eq!(
            kb(key(KeyCode::Home, KeyModifiers::CONTROL)),
            b"\x1b[1;5H".to_vec()
        );
    }

    #[test]
    fn shift_delete() {
        assert_eq!(
            kb(key(KeyCode::Delete, KeyModifiers::SHIFT)),
            b"\x1b[3;2~".to_vec()
        );
    }

    // --- Regular characters ---

    #[test]
    fn regular_ascii_char() {
        assert_eq!(kb(plain(KeyCode::Char('a'))), vec![b'a']);
        assert_eq!(kb(plain(KeyCode::Char('Z'))), vec![b'Z']);
        assert_eq!(kb(plain(KeyCode::Char('5'))), vec![b'5']);
    }

    #[test]
    fn unicode_char() {
        let bytes = kb(plain(KeyCode::Char('é')));
        assert_eq!(bytes, "é".as_bytes().to_vec());
    }

    // --- Unmapped keycodes ---

    #[test]
    fn unmapped_keycode_returns_empty() {
        assert!(kb(plain(KeyCode::Null)).is_empty());
    }
}
