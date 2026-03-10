use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// Convert a vt100 screen to ratatui Lines for rendering.
pub fn render_screen(screen: &vt100::Screen, area: Rect) -> Vec<Line<'static>> {
    render_screen_inner(screen, area, None)
}

/// Render screen with copy mode highlights overlaid.
#[allow(dead_code)]
pub fn render_screen_copy_mode(
    screen: &vt100::Screen,
    area: Rect,
    cms: &crate::copy_mode::CopyModeState,
) -> Vec<Line<'static>> {
    render_screen_inner(screen, area, Some(cms))
}

fn render_screen_inner(
    screen: &vt100::Screen,
    area: Rect,
    cms: Option<&crate::copy_mode::CopyModeState>,
) -> Vec<Line<'static>> {
    let rows = area.height as usize;
    let cols = area.width as usize;
    let mut lines = Vec::with_capacity(rows);

    for row in 0..rows {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut current_text = String::new();
        let mut current_style = Style::default();
        let mut rendered_cols = 0usize;

        for col in 0..cols {
            let cell = screen.cell(row as u16, col as u16);

            // Skip wide char continuation cells — the wide char already occupies 2 columns
            if cell.as_ref().map_or(false, |c| c.is_wide_continuation()) {
                continue;
            }

            let mut style = match cell {
                Some(ref cell) => cell_style(cell),
                None => Style::default(),
            };

            // Apply copy mode overlays
            if let Some(cms) = cms {
                if row == cms.cursor_row && col == cms.cursor_col {
                    // Copy mode cursor: reversed
                    style = style.add_modifier(Modifier::REVERSED);
                } else if cms.is_selected(row, col) {
                    // Selection: reversed fg/bg
                    style = style.add_modifier(Modifier::REVERSED);
                } else if cms.is_search_match(row, col) {
                    // Search match: yellow background
                    style = style.bg(Color::Yellow).fg(Color::Black);
                }
            }

            if style != current_style && !current_text.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut current_text),
                    current_style,
                ));
            }
            current_style = style;

            let mut text = match cell {
                Some(cell) => {
                    let ch = cell.contents();
                    if ch.is_empty() {
                        " ".to_string()
                    } else {
                        ch.to_string()
                    }
                }
                None => " ".to_string(),
            };

            let mut width = UnicodeWidthStr::width(text.as_str());
            if width == 0 {
                text = " ".to_string();
                width = 1;
            }

            let remaining = cols.saturating_sub(rendered_cols);
            if remaining == 0 {
                break;
            }
            if width > remaining {
                text = " ".repeat(remaining);
                width = remaining;
            }

            current_text.push_str(&text);
            rendered_cols += width;
        }

        if rendered_cols < cols {
            current_text.push_str(&" ".repeat(cols - rendered_cols));
        }

        if !current_text.is_empty() {
            spans.push(Span::styled(current_text, current_style));
        }

        lines.push(Line::from(spans));
    }

    lines
}

fn cell_style(cell: &vt100::Cell) -> Style {
    let mut style = Style::default();

    style = style.fg(convert_color(cell.fgcolor()));
    style = style.bg(convert_color(cell.bgcolor()));

    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.dim() {
        style = style.add_modifier(Modifier::DIM);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }
    if cell.strikethrough() {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if cell.blink() {
        style = style.add_modifier(Modifier::SLOW_BLINK);
    }

    style
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(n) => Color::Indexed(n),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn make_screen(rows: u16, cols: u16, input: &[u8]) -> vt100::Parser {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(input);
        parser
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn line_display_width(line: &Line<'_>) -> usize {
        line.spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum()
    }

    #[test]
    fn test_render_empty_screen() {
        let parser = make_screen(5, 10, b"");
        let area = Rect::new(0, 0, 10, 5);
        let lines = render_screen(parser.screen(), area);

        assert_eq!(lines.len(), 5);
        for line in &lines {
            let text = line_text(line);
            assert_eq!(line_display_width(line), 10);
            assert!(text.chars().all(|c| c == ' '));
        }
    }

    #[test]
    fn test_render_plain_text() {
        let parser = make_screen(5, 20, b"Hello, world!");
        let area = Rect::new(0, 0, 20, 5);
        let lines = render_screen(parser.screen(), area);

        let first_line = line_text(&lines[0]);
        assert!(first_line.starts_with("Hello, world!"));
        assert_eq!(line_display_width(&lines[0]), 20);
    }

    #[test]
    fn test_render_multiline() {
        let parser = make_screen(5, 20, b"line one\r\nline two\r\nline three");
        let area = Rect::new(0, 0, 20, 5);
        let lines = render_screen(parser.screen(), area);

        let line0 = line_text(&lines[0]);
        let line1 = line_text(&lines[1]);
        let line2 = line_text(&lines[2]);
        assert!(line0.starts_with("line one"));
        assert!(line1.starts_with("line two"));
        assert!(line2.starts_with("line three"));
        assert_eq!(line_display_width(&lines[0]), 20);
        assert_eq!(line_display_width(&lines[1]), 20);
        assert_eq!(line_display_width(&lines[2]), 20);
    }

    #[test]
    fn test_render_bold_text() {
        // ESC[1m = bold on, ESC[0m = reset
        let parser = make_screen(3, 20, b"\x1b[1mbold\x1b[0m normal");
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        // The bold span should have BOLD modifier
        let bold_span = &lines[0].spans[0];
        assert!(bold_span.content.starts_with("bold"));
        assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_render_colored_text() {
        // ESC[31m = red foreground
        let parser = make_screen(3, 20, b"\x1b[31mred text\x1b[0m");
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let red_span = &lines[0].spans[0];
        assert!(red_span.content.starts_with("red text"));
        assert_eq!(red_span.style.fg, Some(Color::Indexed(1)));
    }

    #[test]
    fn test_render_rgb_colored_text() {
        // ESC[38;2;255;128;0m = RGB foreground (255, 128, 0)
        let parser = make_screen(3, 30, b"\x1b[38;2;255;128;0morange\x1b[0m");
        let area = Rect::new(0, 0, 30, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("orange"));
        assert_eq!(span.style.fg, Some(Color::Rgb(255, 128, 0)));
    }

    #[test]
    fn test_render_smaller_area_than_screen() {
        let parser = make_screen(10, 40, b"visible\r\nline2\r\nline3");
        // Only render 2 rows x 10 cols
        let area = Rect::new(0, 0, 10, 2);
        let lines = render_screen(parser.screen(), area);

        assert_eq!(lines.len(), 2);
        assert_eq!(line_display_width(&lines[0]), 10);
    }

    #[test]
    fn test_render_underline_and_italic() {
        // ESC[3m = italic, ESC[4m = underline
        let parser = make_screen(3, 30, b"\x1b[3mitalic\x1b[0m \x1b[4munderline\x1b[0m");
        let area = Rect::new(0, 0, 30, 3);
        let lines = render_screen(parser.screen(), area);

        let italic_span = &lines[0].spans[0];
        assert!(italic_span.content.starts_with("italic"));
        assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_render_wide_chars() {
        // CJK character "中" is a wide char occupying 2 columns
        let parser = make_screen(3, 20, "中文".as_bytes());
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let first_line = line_text(&lines[0]);
        // Wide chars should not have extra spaces from continuation cells
        assert!(first_line.starts_with("中文"));
        assert_eq!(line_display_width(&lines[0]), 20);
    }

    #[test]
    fn test_render_combining_and_wide_chars_have_exact_display_width() {
        let parser = make_screen(2, 8, "中e\u{301}".as_bytes());
        let area = Rect::new(0, 0, 8, 2);
        let lines = render_screen(parser.screen(), area);

        let first_line = line_text(&lines[0]);
        assert!(first_line.starts_with("中e\u{301}"));
        assert_eq!(line_display_width(&lines[0]), 8);
    }

    #[test]
    fn test_render_zero_area() {
        let parser = make_screen(5, 10, b"hello");
        let area = Rect::new(0, 0, 0, 0);
        let lines = render_screen(parser.screen(), area);
        assert!(lines.is_empty());
    }

    // ---- inverse/strikethrough/blink styles ----

    #[test]
    fn test_render_inverse_text() {
        // ESC[7m = inverse
        let parser = make_screen(3, 20, b"\x1b[7minverted\x1b[0m");
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("inverted"));
        assert!(span.style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_render_strikethrough_text() {
        // ESC[9m = strikethrough
        let parser = make_screen(3, 20, b"\x1b[9mstruck\x1b[0m");
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("struck"));
        assert!(span.style.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn test_render_blink_text() {
        // ESC[5m = blink
        let parser = make_screen(3, 20, b"\x1b[5mblinky\x1b[0m");
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("blinky"));
        assert!(span.style.add_modifier.contains(Modifier::SLOW_BLINK));
    }

    #[test]
    fn test_render_combined_modifiers() {
        // ESC[1;3;4m = bold + italic + underline
        let parser = make_screen(3, 30, b"\x1b[1;3;4mcombined\x1b[0m");
        let area = Rect::new(0, 0, 30, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("combined"));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert!(span.style.add_modifier.contains(Modifier::ITALIC));
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    // ---- screen larger than area ----

    #[test]
    fn test_render_area_smaller_rows_than_screen() {
        let parser = make_screen(20, 40, b"line1\r\nline2\r\nline3\r\nline4\r\nline5");
        // Only render 3 rows
        let area = Rect::new(0, 0, 40, 3);
        let lines = render_screen(parser.screen(), area);

        assert_eq!(lines.len(), 3);
        assert!(line_text(&lines[0]).starts_with("line1"));
        assert!(line_text(&lines[1]).starts_with("line2"));
        assert!(line_text(&lines[2]).starts_with("line3"));
    }

    #[test]
    fn test_render_area_smaller_cols_than_screen() {
        let parser = make_screen(5, 40, b"abcdefghijklmnopqrstuvwxyz");
        // Only render 10 cols
        let area = Rect::new(0, 0, 10, 5);
        let lines = render_screen(parser.screen(), area);

        assert_eq!(lines.len(), 5);
        let text = line_text(&lines[0]);
        assert_eq!(line_display_width(&lines[0]), 10);
        assert!(text.starts_with("abcdefghij"));
    }

    #[test]
    fn test_render_area_1x1() {
        let parser = make_screen(5, 10, b"X");
        let area = Rect::new(0, 0, 1, 1);
        let lines = render_screen(parser.screen(), area);

        assert_eq!(lines.len(), 1);
        assert_eq!(line_display_width(&lines[0]), 1);
        assert_eq!(line_text(&lines[0]), "X");
    }

    // ---- mixed wide and narrow chars on same line ----

    #[test]
    fn test_render_mixed_wide_narrow_chars() {
        // "A中B文C" - narrow, wide, narrow, wide, narrow
        let parser = make_screen(3, 20, "A中B文C".as_bytes());
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let text = line_text(&lines[0]);
        assert!(text.starts_with("A中B文C"));
        assert_eq!(line_display_width(&lines[0]), 20);
    }

    #[test]
    fn test_render_wide_char_at_boundary() {
        // Fill a narrow area so a wide char might not fit at the edge
        // Area is 5 cols wide, fill with 4 narrow chars + 1 wide (needs 2 cols = overflow)
        let parser = make_screen(2, 10, "abcd中".as_bytes());
        let area = Rect::new(0, 0, 5, 2);
        let lines = render_screen(parser.screen(), area);

        assert_eq!(lines.len(), 2);
        assert_eq!(line_display_width(&lines[0]), 5);
    }

    #[test]
    fn test_render_all_wide_chars() {
        // All CJK characters
        let parser = make_screen(3, 20, "中文測試".as_bytes());
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let text = line_text(&lines[0]);
        assert!(text.starts_with("中文測試"));
        assert_eq!(line_display_width(&lines[0]), 20);
    }

    #[test]
    fn test_render_dim_text() {
        // ESC[2m = dim
        let parser = make_screen(3, 20, b"\x1b[2mdimmed\x1b[0m");
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("dimmed"));
        assert!(span.style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn test_render_background_color() {
        // ESC[42m = green background
        let parser = make_screen(3, 20, b"\x1b[42mgreen bg\x1b[0m");
        let area = Rect::new(0, 0, 20, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("green bg"));
        assert_eq!(span.style.bg, Some(Color::Indexed(2)));
    }

    #[test]
    fn test_render_rgb_background() {
        // ESC[48;2;100;150;200m = RGB background
        let parser = make_screen(3, 30, b"\x1b[48;2;100;150;200mrgb bg\x1b[0m");
        let area = Rect::new(0, 0, 30, 3);
        let lines = render_screen(parser.screen(), area);

        let span = &lines[0].spans[0];
        assert!(span.content.starts_with("rgb bg"));
        assert_eq!(span.style.bg, Some(Color::Rgb(100, 150, 200)));
    }

    #[test]
    fn test_render_style_changes_mid_line() {
        // "normal" then bold "bold" then reset "normal"
        let parser = make_screen(3, 40, b"normal\x1b[1mbold\x1b[0mnormal");
        let area = Rect::new(0, 0, 40, 3);
        let lines = render_screen(parser.screen(), area);

        // Should produce multiple spans with different styles
        assert!(lines[0].spans.len() >= 2, "should have multiple spans for style changes");
        // First span should not be bold
        assert!(!lines[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
    }
}
