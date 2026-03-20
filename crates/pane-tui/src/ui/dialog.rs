//! Uniform dialog / form building blocks.
//!
//! Every overlay popup in the TUI shares the same visual language:
//! rounded border, accent-colored chrome, consistent text input and
//! list-selection widgets.  This module provides composable primitives
//! so each dialog can focus on *what* it shows rather than *how*.

use pane_protocol::config::Theme;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

// ---------------------------------------------------------------------------
// Popup positioning
// ---------------------------------------------------------------------------

/// How to position a dialog popup on screen.
pub enum PopupSize {
    /// Percentage of the available area (e.g. 60% × 50%).
    Percent { width: u16, height: u16 },
    /// Fixed character dimensions, clamped to available area.
    Fixed { width: u16, height: u16 },
    /// Fixed maximum, clamped to available area minus padding.
    FixedClamped { width: u16, height: u16, pad: u16 },
}

pub enum PopupAnchor {
    /// Center the popup in the area.
    Center,
    /// Anchor at a specific (x, y) position, clamped to fit.
    Position { x: u16, y: u16 },
    /// Center horizontally, anchor to bottom of area.
    #[allow(dead_code)]
    BottomCenter,
}

/// Calculate the popup rectangle given size, anchor and available area.
pub fn popup_rect(size: PopupSize, anchor: PopupAnchor, area: Rect) -> Rect {
    let (w, h) = match size {
        PopupSize::Percent { width, height } => {
            let r = centered_rect(width, height, area);
            return match anchor {
                PopupAnchor::Center => r,
                PopupAnchor::BottomCenter => {
                    let x = area.x + (area.width.saturating_sub(r.width)) / 2;
                    let y = area.y + area.height.saturating_sub(r.height + 2);
                    Rect::new(x, y, r.width, r.height)
                }
                PopupAnchor::Position { x, y } => {
                    let x = x.min(area.x + area.width.saturating_sub(r.width));
                    let y = y.min(area.y + area.height.saturating_sub(r.height));
                    Rect::new(x, y, r.width, r.height)
                }
            };
        }
        PopupSize::Fixed { width, height } => (width, height),
        PopupSize::FixedClamped { width, height, pad } => (
            width.min(area.width.saturating_sub(pad)),
            height.min(area.height.saturating_sub(pad)),
        ),
    };

    match anchor {
        PopupAnchor::Center => {
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            Rect::new(x, y, w, h)
        }
        PopupAnchor::Position { x, y } => {
            let x = x.min(area.x + area.width.saturating_sub(w));
            let y = if y + h <= area.y + area.height {
                y
            } else {
                y.saturating_sub(h)
            };
            Rect::new(x, y, w, h)
        }
        PopupAnchor::BottomCenter => {
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + area.height.saturating_sub(h + 2);
            Rect::new(x, y, w, h)
        }
    }
}

/// Compute the inner area of a popup without rendering (for hit-testing).
pub fn inner_rect(popup_area: Rect) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    block.inner(popup_area)
}

/// Dim the entire area by darkening every cell's foreground and background.
pub fn dim_background(frame: &mut Frame, area: Rect) {
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];
            cell.set_style(Style::default().add_modifier(Modifier::DIM));
        }
    }
}

/// Render the popup chrome (Clear + rounded border block) and return the inner area.
///
/// `title` can be empty for no title.  When non-empty, it's rendered in
/// accent-bold style.
pub fn render_popup(
    frame: &mut Frame,
    popup_area: Rect,
    title: &str,
    theme: &Theme,
) -> Rect {
    frame.render_widget(Clear, popup_area);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    if !title.is_empty() {
        block = block.title(Line::styled(
            format!(" {} ", title),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    inner
}

// ---------------------------------------------------------------------------
// Pill buttons
// ---------------------------------------------------------------------------

/// Build spans for a pill button with rounded end caps.
///
/// When `nerd_fonts` is true, uses Powerline rounded glyphs (`` / ``).
/// Otherwise falls back to parentheses (`(` / `)`).
///
/// The caps and text share the same foreground-only style — no background
/// color is used.
pub fn pill_spans<'a>(text: &str, style: Style, nerd_fonts: bool) -> Vec<Span<'a>> {
    let (left, right) = if nerd_fonts {
        ("\u{E0B6}", "\u{E0B4}") //  /
    } else {
        ("(", ")")
    };

    vec![
        Span::styled(left, style),
        Span::styled(text.to_string(), style),
        Span::styled(right, style),
    ]
}

// ---------------------------------------------------------------------------
// Text input widget
// ---------------------------------------------------------------------------

/// Render a labelled text input field on a single line.
///
/// - When `focused`, the cursor character `_` is appended and text is white.
/// - When unfocused, the value (or `placeholder`) is rendered in `dim`.
///
/// Returns the number of rows consumed (always 2: label + input).
#[allow(clippy::too_many_arguments)]
pub fn render_text_field(
    frame: &mut Frame,
    area: Rect,
    y: u16,
    label: &str,
    value: &str,
    placeholder: &str,
    focused: bool,
    theme: &Theme,
) -> u16 {
    if y + 1 >= area.y + area.height {
        return 0;
    }

    let label_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };

    // Label
    let label_line = Line::from(vec![Span::styled(format!("  {}", label), label_style)]);
    frame.render_widget(
        Paragraph::new(label_line),
        Rect::new(area.x, y, area.width, 1),
    );

    // Value
    let display = if focused {
        format!("{}_", value)
    } else if value.is_empty() {
        placeholder.to_string()
    } else {
        value.to_string()
    };
    let value_style = if focused {
        Style::default().fg(theme.fg)
    } else {
        Style::default().fg(theme.dim)
    };
    let value_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(display, value_style),
    ]);
    frame.render_widget(
        Paragraph::new(value_line),
        Rect::new(area.x, y + 1, area.width, 1),
    );

    2
}

/// Render a filter/search input in `> query_` style.
pub fn render_filter_input_placeholder(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    placeholder: Option<&str>,
    theme: &Theme,
) {
    let mut spans = vec![Span::styled("  ", Style::default())];
    if input.is_empty() {
        if let Some(ph) = placeholder {
            spans.push(Span::styled(ph, Style::default().fg(theme.dim)));
        }
    } else {
        spans.push(Span::raw(input));
    }
    spans.push(Span::styled("_", Style::default().fg(theme.dim)));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render a horizontal separator line (thin rule).
pub fn render_separator(frame: &mut Frame, area: Rect, theme: &Theme) {
    let sep = Line::from("\u{2500}".repeat(area.width as usize));
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(theme.dim)),
        area,
    );
}

// ---------------------------------------------------------------------------
// Selectable list widget
// ---------------------------------------------------------------------------

/// A single item in a selectable list.
pub struct ListItem<'a> {
    pub label: &'a str,
    pub description: &'a str,
    /// Optional section header (only shown when `show_sections` is true).
    pub section: Option<&'a str>,
    /// Optional right-aligned hint text (e.g. keybind).
    pub hint: Option<&'a str>,
}

/// Render a selectable list of items with scrolling.
///
/// Returns the number of visible rows consumed.
pub fn render_select_list(
    frame: &mut Frame,
    area: Rect,
    items: &[ListItem],
    selected: usize,
    show_sections: bool,
    hover: Option<(u16, u16)>,
    theme: &Theme,
) -> u16 {
    if area.height == 0 || items.is_empty() {
        return 0;
    }

    let visible_count = area.height as usize;

    // Calculate scroll offset to keep selected visible
    let scroll_offset = if selected >= visible_count {
        selected - visible_count + 1
    } else {
        0
    };

    let mut row_y = 0u16;
    let mut last_section: Option<&str> = None;
    let mut item_idx = 0usize; // tracks which visual item we're on for scroll

    for (i, item) in items.iter().enumerate() {
        // Section header
        if show_sections {
            if let Some(section) = item.section {
                let show_header = match last_section {
                    None => true,
                    Some(prev) => prev != section,
                };
                if show_header {
                    last_section = Some(section);
                    if item_idx >= scroll_offset {
                        if row_y >= area.height {
                            break;
                        }
                        let header_line = Line::from(Span::styled(
                            format!(" {} ", section),
                            Style::default()
                                .fg(theme.dim)
                                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                        ));
                        let row = Rect::new(area.x, area.y + row_y, area.width, 1);
                        frame.render_widget(Paragraph::new(header_line), row);
                        row_y += 1;
                    }
                    item_idx += 1;
                }
            }
        }

        if item_idx < scroll_offset {
            item_idx += 1;
            continue;
        }
        if row_y >= area.height {
            break;
        }

        let is_selected = i == selected;
        let actual_y = area.y + row_y;
        let is_hovered = hover.is_some_and(|(hx, hy)| {
            hy == actual_y && hx >= area.x && hx < area.x + area.width
        });
        let style = if is_selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if is_hovered {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.fg)
        };

        let mut spans = vec![
            Span::styled("  ", style),
            Span::styled(item.label, style),
        ];

        if !item.description.is_empty() {
            spans.push(Span::styled(
                format!("  {}", item.description),
                Style::default().fg(theme.dim),
            ));
        }

        if let Some(hint) = item.hint {
            if !hint.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", hint),
                    Style::default().fg(theme.dim),
                ));
            }
        }

        let line = Line::from(spans);
        let row = Rect::new(area.x, area.y + row_y, area.width, 1);
        frame.render_widget(Paragraph::new(line), row);
        row_y += 1;
        item_idx += 1;
    }

    row_y
}

// ---------------------------------------------------------------------------
// Confirm dialog
// ---------------------------------------------------------------------------

/// Which button was clicked / hovered in a confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmButton {
    Cancel,
    Confirm,
}

// Internal layout constants for the confirm dialog.
const CONFIRM_PAD: u16 = 4; // outer padding when clamping
const CONFIRM_MARGIN: u16 = 3; // left indent for text & buttons
const CANCEL_TEXT: &str = " Cancel ";
const CONFIRM_TEXT: &str = " Confirm ";
const BUTTON_GAP: u16 = 2; // space between buttons
const PILL_CAPS: u16 = 2; // ▐ left + ▌ right per pill

/// Compute the popup area for a confirm dialog with the given message.
fn confirm_popup_area(message: &str, area: Rect) -> Rect {
    // Width: message + margins, or button row, whichever is wider, + border
    let msg_width = message.len() as u16 + CONFIRM_MARGIN + 1;
    let btn_width = CONFIRM_MARGIN + CANCEL_TEXT.len() as u16 + PILL_CAPS + BUTTON_GAP + CONFIRM_TEXT.len() as u16 + PILL_CAPS + 1;
    let content_width = msg_width.max(btn_width);
    let width = content_width + 2; // +2 for left/right border
    let height = 5; // border + message + blank + buttons + border
    popup_rect(
        PopupSize::FixedClamped { width, height, pad: CONFIRM_PAD },
        PopupAnchor::Center,
        area,
    )
}

/// Render a confirm dialog and return the inner area.
///
/// `hovered` highlights the corresponding button on mouse-over.
pub fn render_confirm(
    frame: &mut Frame,
    area: Rect,
    message: &str,
    hovered: Option<ConfirmButton>,
    theme: &Theme,
    nerd_fonts: bool,
) {
    let popup_area = confirm_popup_area(message, area);
    let inner = render_popup(frame, popup_area, "Confirm", theme);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    // Row 0: message
    let msg_line = Line::from(vec![
        Span::raw(" ".repeat(CONFIRM_MARGIN as usize)),
        Span::styled(message, Style::default().fg(theme.fg)),
    ]);
    frame.render_widget(
        Paragraph::new(msg_line),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    // Row 2: buttons (skip row 1 as blank spacer)
    let button_y = inner.y + 2;

    let cancel_style = if hovered == Some(ConfirmButton::Cancel) {
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim).add_modifier(Modifier::BOLD)
    };
    let confirm_style = if hovered == Some(ConfirmButton::Confirm) {
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
    };

    let mut btn_spans: Vec<Span<'_>> = vec![Span::raw(" ".repeat(CONFIRM_MARGIN as usize))];
    btn_spans.extend(pill_spans(CANCEL_TEXT, cancel_style, nerd_fonts));
    btn_spans.push(Span::raw(" ".repeat(BUTTON_GAP as usize)));
    btn_spans.extend(pill_spans(CONFIRM_TEXT, confirm_style, nerd_fonts));
    let btn_line = Line::from(btn_spans);
    frame.render_widget(
        Paragraph::new(btn_line),
        Rect::new(inner.x, button_y, inner.width, 1),
    );
}

/// Hit-test a confirm dialog. Returns which button was clicked, if any.
pub fn confirm_hit_test(
    area: Rect,
    message: &str,
    x: u16,
    y: u16,
) -> Option<ConfirmButton> {
    let popup_area = confirm_popup_area(message, area);
    let inner = inner_rect(popup_area);

    let button_y = inner.y + 2;
    if y != button_y {
        return None;
    }

    let cancel_start = inner.x + CONFIRM_MARGIN;
    let cancel_end = cancel_start + PILL_CAPS + CANCEL_TEXT.len() as u16;
    let confirm_start = cancel_end + BUTTON_GAP;
    let confirm_end = confirm_start + PILL_CAPS + CONFIRM_TEXT.len() as u16;

    if x >= cancel_start && x < cancel_end {
        Some(ConfirmButton::Cancel)
    } else if x >= confirm_start && x < confirm_end {
        Some(ConfirmButton::Confirm)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Key handling helpers
// ---------------------------------------------------------------------------

/// Handle standard text-input keys (Char, Backspace).
/// Returns `true` if the key was consumed.
pub fn handle_text_input(key: crossterm::event::KeyCode, input: &mut String) -> bool {
    match key {
        crossterm::event::KeyCode::Char(c) => {
            input.push(c);
            true
        }
        crossterm::event::KeyCode::Backspace => {
            input.pop();
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// centered_rect (canonical implementation)
// ---------------------------------------------------------------------------

/// Compute a centered rectangle as a percentage of the given area.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_is_within_area() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_rect(50, 50, area);
        assert!(result.x >= area.x);
        assert!(result.y >= area.y);
        assert!(result.x + result.width <= area.x + area.width);
        assert!(result.y + result.height <= area.y + area.height);
    }

    #[test]
    fn centered_rect_is_roughly_centered() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered_rect(50, 50, area);
        let cx = result.x + result.width / 2;
        let cy = result.y + result.height / 2;
        assert!((cx as i16 - 50).unsigned_abs() <= 2);
        assert!((cy as i16 - 50).unsigned_abs() <= 2);
    }

    #[test]
    fn popup_rect_center_fixed() {
        let area = Rect::new(0, 0, 80, 40);
        let r = popup_rect(
            PopupSize::Fixed { width: 30, height: 10 },
            PopupAnchor::Center,
            area,
        );
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 10);
        assert_eq!(r.x, 25); // (80-30)/2
        assert_eq!(r.y, 15); // (40-10)/2
    }

    #[test]
    fn popup_rect_anchored_clamps() {
        let area = Rect::new(0, 0, 80, 40);
        let r = popup_rect(
            PopupSize::Fixed { width: 20, height: 10 },
            PopupAnchor::Position { x: 70, y: 35 },
            area,
        );
        assert!(r.x + r.width <= 80);
        // Should flip above since no room below
        assert!(r.y + r.height <= 40);
    }

    #[test]
    fn popup_rect_fixed_clamped() {
        let area = Rect::new(0, 0, 30, 15);
        let r = popup_rect(
            PopupSize::FixedClamped { width: 50, height: 20, pad: 4 },
            PopupAnchor::Center,
            area,
        );
        assert_eq!(r.width, 26); // 30 - 4
        assert_eq!(r.height, 11); // 15 - 4
    }

    // --- BottomCenter anchor for all size types ---

    #[test]
    fn popup_rect_bottom_center_fixed() {
        let area = Rect::new(0, 0, 80, 40);
        let r = popup_rect(
            PopupSize::Fixed { width: 30, height: 10 },
            PopupAnchor::BottomCenter,
            area,
        );
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 10);
        // Centered horizontally: (80 - 30) / 2 = 25
        assert_eq!(r.x, 25);
        // Bottom-anchored: 0 + 40 - (10 + 2) = 28
        assert_eq!(r.y, 28);
    }

    #[test]
    fn popup_rect_bottom_center_percent() {
        let area = Rect::new(0, 0, 100, 50);
        let r = popup_rect(
            PopupSize::Percent { width: 50, height: 50 },
            PopupAnchor::BottomCenter,
            area,
        );
        // The width/height come from centered_rect, centered horizontally
        let cx = r.x + r.width / 2;
        assert!((cx as i16 - 50).unsigned_abs() <= 2, "should be roughly centered horizontally");
        // Should be anchored toward the bottom
        assert!(r.y + r.height <= area.height, "should fit within area");
    }

    #[test]
    fn popup_rect_bottom_center_fixed_clamped() {
        let area = Rect::new(0, 0, 40, 20);
        let r = popup_rect(
            PopupSize::FixedClamped { width: 30, height: 10, pad: 4 },
            PopupAnchor::BottomCenter,
            area,
        );
        // Clamped: min(30, 40-4) = 30, min(10, 20-4) = 10
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 10);
        // Centered: (40 - 30) / 2 = 5
        assert_eq!(r.x, 5);
        // Bottom: 0 + 20 - (10 + 2) = 8
        assert_eq!(r.y, 8);
    }

    #[test]
    fn popup_rect_bottom_center_fixed_clamped_too_large() {
        let area = Rect::new(0, 0, 20, 10);
        let r = popup_rect(
            PopupSize::FixedClamped { width: 100, height: 50, pad: 2 },
            PopupAnchor::BottomCenter,
            area,
        );
        // Clamped: min(100, 18) = 18, min(50, 8) = 8
        assert_eq!(r.width, 18);
        assert_eq!(r.height, 8);
        // Centered: (20 - 18) / 2 = 1
        assert_eq!(r.x, 1);
    }

    // --- handle_text_input ---

    #[test]
    fn handle_text_input_char() {
        let mut buf = String::new();
        let consumed = handle_text_input(crossterm::event::KeyCode::Char('a'), &mut buf);
        assert!(consumed);
        assert_eq!(buf, "a");
    }

    #[test]
    fn handle_text_input_multiple_chars() {
        let mut buf = String::new();
        handle_text_input(crossterm::event::KeyCode::Char('h'), &mut buf);
        handle_text_input(crossterm::event::KeyCode::Char('i'), &mut buf);
        assert_eq!(buf, "hi");
    }

    #[test]
    fn handle_text_input_backspace() {
        let mut buf = "hello".to_string();
        let consumed = handle_text_input(crossterm::event::KeyCode::Backspace, &mut buf);
        assert!(consumed);
        assert_eq!(buf, "hell");
    }

    #[test]
    fn handle_text_input_backspace_on_empty() {
        let mut buf = String::new();
        let consumed = handle_text_input(crossterm::event::KeyCode::Backspace, &mut buf);
        assert!(consumed);
        assert_eq!(buf, "");
    }

    #[test]
    fn handle_text_input_enter_not_consumed() {
        let mut buf = "test".to_string();
        let consumed = handle_text_input(crossterm::event::KeyCode::Enter, &mut buf);
        assert!(!consumed);
        assert_eq!(buf, "test");
    }

    #[test]
    fn handle_text_input_esc_not_consumed() {
        let mut buf = "test".to_string();
        let consumed = handle_text_input(crossterm::event::KeyCode::Esc, &mut buf);
        assert!(!consumed);
        assert_eq!(buf, "test");
    }

    #[test]
    fn handle_text_input_tab_not_consumed() {
        let mut buf = "test".to_string();
        let consumed = handle_text_input(crossterm::event::KeyCode::Tab, &mut buf);
        assert!(!consumed);
        assert_eq!(buf, "test");
    }

    #[test]
    fn handle_text_input_special_chars() {
        let mut buf = String::new();
        handle_text_input(crossterm::event::KeyCode::Char('!'), &mut buf);
        handle_text_input(crossterm::event::KeyCode::Char('@'), &mut buf);
        handle_text_input(crossterm::event::KeyCode::Char(' '), &mut buf);
        assert_eq!(buf, "!@ ");
    }

    // --- centered_rect edge cases ---

    #[test]
    fn centered_rect_100_percent() {
        let area = Rect::new(0, 0, 100, 50);
        let r = centered_rect(100, 100, area);
        assert_eq!(r.width, area.width);
        assert_eq!(r.height, area.height);
    }

    #[test]
    fn centered_rect_small_area() {
        let area = Rect::new(5, 10, 4, 4);
        let r = centered_rect(50, 50, area);
        assert!(r.x >= area.x);
        assert!(r.y >= area.y);
        assert!(r.x + r.width <= area.x + area.width);
        assert!(r.y + r.height <= area.y + area.height);
    }

    // --- popup_rect Position anchor ---

    #[test]
    fn popup_rect_position_fits() {
        let area = Rect::new(0, 0, 80, 40);
        let r = popup_rect(
            PopupSize::Fixed { width: 10, height: 5 },
            PopupAnchor::Position { x: 20, y: 10 },
            area,
        );
        assert_eq!(r.x, 20);
        assert_eq!(r.y, 10);
        assert_eq!(r.width, 10);
        assert_eq!(r.height, 5);
    }

    #[test]
    fn popup_rect_position_clamps_x() {
        let area = Rect::new(0, 0, 80, 40);
        let r = popup_rect(
            PopupSize::Fixed { width: 20, height: 5 },
            PopupAnchor::Position { x: 75, y: 5 },
            area,
        );
        // x should be clamped so popup fits: 80 - 20 = 60
        assert_eq!(r.x, 60);
    }

    #[test]
    fn popup_rect_position_flips_y_when_no_room_below() {
        let area = Rect::new(0, 0, 80, 40);
        let r = popup_rect(
            PopupSize::Fixed { width: 10, height: 10 },
            PopupAnchor::Position { x: 5, y: 35 },
            area,
        );
        // 35 + 10 = 45 > 40, so should flip: 35 - 10 = 25
        assert_eq!(r.y, 25);
    }

    // --- render_separator width ---

    #[test]
    fn separator_string_has_correct_width() {
        // Directly test the separator line construction logic
        let width = 42u16;
        let sep = "\u{2500}".repeat(width as usize);
        assert_eq!(sep.chars().count(), 42);
        // Each ─ char is 1 display column wide
        assert_eq!(unicode_width::UnicodeWidthStr::width(sep.as_str()), 42);
    }

    #[test]
    fn separator_string_zero_width() {
        let sep = "\u{2500}".repeat(0);
        assert!(sep.is_empty());
    }
}
