use std::io::Write;

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    // Try arboard first
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            clipboard.set_text(text)?;
            return Ok(());
        }
        Err(_) => {}
    }

    // Fallback: OSC 52 escape sequence (works over SSH/tmux)
    let encoded = base64_encode(text.as_bytes());
    let osc = format!("\x1b]52;c;{}\x07", encoded);
    let mut stdout = std::io::stdout();
    stdout.write_all(osc.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

pub fn paste_from_clipboard() -> anyhow::Result<String> {
    let mut clipboard = arboard::Clipboard::new()?;
    let text = clipboard.get_text()?;
    Ok(text)
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_base64_encode_hello() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn test_base64_encode_padding() {
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[test]
    fn test_base64_encode_longer() {
        assert_eq!(base64_encode(b"Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
    }
}
