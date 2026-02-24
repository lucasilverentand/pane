use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::config::Theme;
use crate::server::protocol::ClientListEntry;

/// State for the client picker overlay.
pub struct ClientPickerState {
    pub selected: usize,
    pub entries: Vec<ClientPickerEntry>,
}

pub struct ClientPickerEntry {
    pub id: u64,
    pub size: String,
    pub workspace: usize,
    pub is_self: bool,
}

impl ClientPickerState {
    pub fn new(clients: &[ClientListEntry], my_id: u64) -> Self {
        let mut entries: Vec<ClientPickerEntry> = clients
            .iter()
            .map(|c| ClientPickerEntry {
                id: c.id,
                size: format!("{}x{}", c.width, c.height),
                workspace: c.active_workspace,
                is_self: c.id == my_id,
            })
            .collect();
        entries.sort_by_key(|e| e.id);
        Self {
            selected: 0,
            entries,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn selected_client_id(&self) -> Option<u64> {
        self.entries.get(self.selected).map(|e| e.id)
    }
}

pub fn render(state: &ClientPickerState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup_w = 45u16.min(area.width.saturating_sub(4));
    let popup_h = (state.entries.len() as u16 + 5).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Clients ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 2 || inner.width < 10 {
        return;
    }

    for (i, entry) in state.entries.iter().enumerate() {
        if i as u16 >= inner.height.saturating_sub(1) {
            break;
        }
        let is_selected = i == state.selected;
        let style = if is_selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };

        let prefix = if is_selected { "  > " } else { "    " };
        let you_tag = if entry.is_self { " (you)" } else { "" };

        let mut spans = vec![
            Span::styled(prefix, style),
            Span::styled(format!("#{}", entry.id), style),
            Span::styled(
                format!("  {}  ws:{}", entry.size, entry.workspace + 1),
                Style::default().fg(Color::DarkGray),
            ),
        ];

        if entry.is_self {
            spans.push(Span::styled(
                you_tag,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::DIM),
            ));
        }

        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
    }

    // Hint footer
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint = Line::from(vec![Span::styled(
        "  j/k select  x kick  esc close",
        Style::default().fg(theme.dim),
    )]);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    frame.render_widget(Paragraph::new(hint), hint_area);
}
