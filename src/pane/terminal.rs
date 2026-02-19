use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

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
                spans.push(Span::styled(std::mem::take(&mut current_text), current_style));
            }
            current_style = style;

            match cell {
                Some(cell) => {
                    let ch = cell.contents();
                    if ch.is_empty() {
                        current_text.push(' ');
                    } else {
                        current_text.push_str(&ch);
                    }
                }
                None => {
                    current_text.push(' ');
                }
            }
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
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
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

    fn make_screen(rows: u16, cols: u16, input: &[u8]) -> vt100::Parser {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(input);
        parser
    }

    #[test]
    fn test_render_empty_screen() {
        let parser = make_screen(5, 10, b"");
        let area = Rect::new(0, 0, 10, 5);
        let lines = render_screen(parser.screen(), area);

        assert_eq!(lines.len(), 5);
        for line in &lines {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert_eq!(text.len(), 10);
            assert!(text.chars().all(|c| c == ' '));
        }
    }

    #[test]
    fn test_render_plain_text() {
        let parser = make_screen(5, 20, b"Hello, world!");
        let area = Rect::new(0, 0, 20, 5);
        let lines = render_screen(parser.screen(), area);

        let first_line: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_line.starts_with("Hello, world!"));
    }

    #[test]
    fn test_render_multiline() {
        let parser = make_screen(5, 20, b"line one\r\nline two\r\nline three");
        let area = Rect::new(0, 0, 20, 5);
        let lines = render_screen(parser.screen(), area);

        let line0: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        let line1: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        let line2: String = lines[2].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(line0.starts_with("line one"));
        assert!(line1.starts_with("line two"));
        assert!(line2.starts_with("line three"));
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
        let line0: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(line0.len(), 10);
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

        let first_line: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        // Wide chars should not have extra spaces from continuation cells
        assert!(first_line.starts_with("中文"));
        // Each wide char = 2 display columns, so total rendered width should be cols
        // "中文" = 4 display cols + 16 spaces = 20
        assert_eq!(first_line.chars().count(), 2 + 16); // 2 CJK chars + 16 spaces
    }

    #[test]
    fn test_render_zero_area() {
        let parser = make_screen(5, 10, b"hello");
        let area = Rect::new(0, 0, 0, 0);
        let lines = render_screen(parser.screen(), area);
        assert!(lines.is_empty());
    }
}
