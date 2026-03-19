use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use pane_protocol::config::{HubWidget, Theme};

use super::dialog;

/// How the widget picker was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetPickerMode {
    /// Replace the currently focused widget.
    Change,
    /// Split the current window right and add a new widget.
    SplitHorizontal,
    /// Split the current window down and add a new widget.
    SplitVertical,
}

/// State for the widget picker overlay.
pub struct WidgetPickerState {
    pub mode: WidgetPickerMode,
    pub selected: usize,
    pub items: Vec<HubWidget>,
}

impl WidgetPickerState {
    pub fn new(mode: WidgetPickerMode) -> Self {
        Self {
            mode,
            selected: 0,
            items: HubWidget::all(),
        }
    }

    pub fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.items.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    pub fn move_home(&mut self) {
        self.selected = 0;
    }

    pub fn move_end(&mut self) {
        if !self.items.is_empty() {
            self.selected = self.items.len() - 1;
        }
    }

    pub fn selected_widget(&self) -> Option<&HubWidget> {
        self.items.get(self.selected)
    }
}

/// Render the widget picker as a popup overlay.
pub fn render(state: &WidgetPickerState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let title = match state.mode {
        WidgetPickerMode::Change => " Change Widget ",
        WidgetPickerMode::SplitHorizontal => " Add Widget to the Right ",
        WidgetPickerMode::SplitVertical => " Add Widget Below ",
    };

    let item_count = state.items.len() as u16;
    let popup_w = 34u16.min(area.width);
    let popup_h = (item_count + 2).min(area.height);

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed {
            width: popup_w,
            height: popup_h,
        },
        dialog::PopupAnchor::Center,
        area,
    );

    let inner = dialog::render_popup(frame, popup_area, title, theme);

    let visible = inner.height as usize;
    let scroll = if state.selected >= visible {
        state.selected + 1 - visible
    } else {
        0
    };

    for (vi, idx) in (scroll..state.items.len()).enumerate() {
        if vi as u16 >= inner.height {
            break;
        }
        let widget = &state.items[idx];
        let is_selected = idx == state.selected;
        let row_style = if is_selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };
        let prefix = if is_selected { " > " } else { "   " };
        let line = Line::from(Span::styled(
            format!("{}{}", prefix, widget.label()),
            row_style,
        ));
        let row = Rect::new(inner.x, inner.y + vi as u16, inner.width, 1);
        frame.render_widget(Paragraph::new(line), row);
    }
}

/// Hit-test a click position against the widget picker list rows.
/// Returns the item index if the position falls on a valid row.
pub fn hit_test(state: &WidgetPickerState, inner: Rect, x: u16, y: u16) -> Option<usize> {
    if x < inner.x || x >= inner.x + inner.width || y < inner.y || y >= inner.y + inner.height {
        return None;
    }
    let visible = inner.height as usize;
    let scroll = if state.selected >= visible {
        state.selected + 1 - visible
    } else {
        0
    };
    let row = (y - inner.y) as usize;
    let idx = scroll + row;
    if idx < state.items.len() {
        Some(idx)
    } else {
        None
    }
}
