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
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
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

    for (i, widget) in state.items.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let is_selected = i == state.selected;
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
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        frame.render_widget(Paragraph::new(line), row);
    }
}
