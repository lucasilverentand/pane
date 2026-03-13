use std::io::Write;

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    // Try arboard first
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        clipboard.set_text(text)?;
        return Ok(());
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

    #[test]
    fn test_base64_encode_single_byte() {
        // 1 byte → 2 chars + "==" padding
        assert_eq!(base64_encode(b"\x00"), "AA==");
        assert_eq!(base64_encode(b"\xff"), "/w==");
    }

    #[test]
    fn test_base64_encode_exactly_3_bytes() {
        // 3 bytes = exactly one chunk, no padding
        assert_eq!(base64_encode(b"abc"), "YWJj");
        assert_eq!(base64_encode(b"\x00\x00\x00"), "AAAA");
    }

    #[test]
    fn test_base64_encode_non_ascii_bytes() {
        // 0x80, 0xFF, 0xFE
        assert_eq!(base64_encode(&[0x80, 0xFF, 0xFE]), "gP/+");
        // High bytes with padding
        assert_eq!(base64_encode(&[0xDE, 0xAD]), "3q0=");
        assert_eq!(base64_encode(&[0xCA, 0xFE, 0xBA, 0xBE]), "yv66vg==");
    }

    #[test]
    fn test_base64_encode_6_bytes_no_padding() {
        // 6 bytes = exactly 2 chunks, no padding
        assert_eq!(base64_encode(b"abcdef"), "YWJjZGVm");
    }

    #[test]
    fn test_base64_encode_4_bytes_has_padding() {
        // 4 bytes = 1 full chunk + 1 byte remainder → "==" padding on last group
        let result = base64_encode(b"abcd");
        assert_eq!(result, "YWJjZA==");
        assert!(result.ends_with("=="));
    }

    #[test]
    fn test_base64_encode_5_bytes_has_single_pad() {
        // 5 bytes = 1 full chunk + 2 byte remainder → "=" padding on last group
        let result = base64_encode(b"abcde");
        assert_eq!(result, "YWJjZGU=");
        assert!(result.ends_with('='));
        assert!(!result.ends_with("=="));
    }

    // --- Binary data: all byte values 0-255 ---

    #[test]
    fn test_base64_encode_all_bytes() {
        let data: Vec<u8> = (0..=255).collect();
        let result = base64_encode(&data);

        // 256 bytes → ceil(256/3) = 86 groups, but 256 % 3 = 1,
        // so 85 full groups + 1 partial = 86 groups → 86 * 4 = 344 chars
        assert_eq!(result.len(), 344);

        // Verify it's valid base64 characters
        for c in result.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=',
                "invalid base64 char: {:?}",
                c
            );
        }

        // Verify padding: 256 % 3 = 1 → 2 pad chars ("==")
        assert!(result.ends_with("=="));
    }

    #[test]
    fn test_base64_encode_all_bytes_first_128() {
        let data: Vec<u8> = (0..128).collect();
        let result = base64_encode(&data);

        // 128 bytes → 128 % 3 = 2, so single "=" pad
        assert!(result.ends_with('='));
        assert!(!result.ends_with("=="));

        for c in result.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=',
                "invalid base64 char: {:?}",
                c
            );
        }
    }

    #[test]
    fn test_base64_encode_binary_roundtrip_length() {
        // For N input bytes, output length = ceil(N/3) * 4
        for n in 0..=50 {
            let data: Vec<u8> = (0..n).map(|i| i as u8).collect();
            let result = base64_encode(&data);
            let expected_len = if n == 0 {
                0
            } else {
                ((n as usize + 2) / 3) * 4
            };
            assert_eq!(
                result.len(),
                expected_len,
                "wrong length for {} input bytes",
                n
            );
        }
    }

    // --- Very long strings ---

    #[test]
    fn test_base64_encode_long_string() {
        let input = "A".repeat(10_000);
        let result = base64_encode(input.as_bytes());

        // 10000 bytes → ceil(10000/3) * 4 = 3334 * 4 = 13336
        // 10000 % 3 = 1 → "==" padding
        assert_eq!(result.len(), 13336);
        assert!(result.ends_with("=="));

        // Verify the content is consistent (all 'A' bytes = 0x41)
        // "AAA" → base64 "QUFB" (repeating pattern)
        // First 4 chars should be "QUFB"
        assert!(result.starts_with("QUFB"));
    }

    #[test]
    fn test_base64_encode_long_mixed() {
        let input: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let result = base64_encode(&input);

        // 5000 % 3 = 2 → single "=" pad
        assert!(result.ends_with('='));
        assert!(!result.ends_with("=="));

        let expected_len = ((5000 + 2) / 3) * 4;
        assert_eq!(result.len(), expected_len);
    }

    #[test]
    fn test_base64_encode_exactly_divisible_by_3() {
        // 999 bytes / 3 = 333 groups → no padding
        let data: Vec<u8> = (0..999).map(|i| (i % 256) as u8).collect();
        let result = base64_encode(&data);
        assert!(!result.ends_with('='), "no padding for length divisible by 3");
        assert_eq!(result.len(), 333 * 4);
    }

    // --- Edge cases ---

    #[test]
    fn test_base64_encode_newlines() {
        let result = base64_encode(b"\n\n\n");
        assert_eq!(result, "CgoK");
    }

    #[test]
    fn test_base64_encode_null_bytes() {
        let result = base64_encode(b"\x00\x00\x00\x00\x00\x00");
        assert_eq!(result, "AAAAAAAA");
    }

    #[test]
    fn test_base64_encode_max_bytes() {
        let result = base64_encode(b"\xff\xff\xff");
        assert_eq!(result, "////");
    }

    #[test]
    fn test_base64_encode_unicode_string() {
        // UTF-8 encoding of emoji
        let result = base64_encode("🦀".as_bytes());
        // 🦀 = 4 bytes (F0 9F A6 80), 4 % 3 = 1 → "==" padding
        assert_eq!(result.len(), 8);
        assert!(result.ends_with("=="));
    }
}
