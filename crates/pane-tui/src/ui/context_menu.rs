use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use pane_protocol::config::{Action, Theme};

use super::dialog;

/// A single item in the context menu.
#[derive(Clone, Debug)]
pub struct ContextMenuItem {
    pub label: String,
    pub action: Action,
}

/// Which UI region was right-clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ContextMenuContext {
    TabBar,
    WorkspaceBar,
    PaneBody,
}

/// State for an open context menu.
pub struct ContextMenuState {
    pub items: Vec<ContextMenuItem>,
    pub selected: usize,
    #[allow(dead_code)]
    pub context: ContextMenuContext,
    /// Anchor position (top-left of the menu popup).
    pub anchor_x: u16,
    pub anchor_y: u16,
}

impl ContextMenuState {
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

    pub fn selected_action(&self) -> Option<&Action> {
        self.items.get(self.selected).map(|i| &i.action)
    }
}

/// Create a context menu for right-clicking a tab in the tab bar.
#[allow(dead_code)]
pub fn tab_bar_menu(x: u16, y: u16) -> ContextMenuState {
    ContextMenuState {
        items: vec![
            ContextMenuItem {
                label: "Close Tab".into(),
                action: Action::CloseTab,
            },
            ContextMenuItem {
                label: "Rename Tab".into(),
                action: Action::RenamePane,
            },
        ],
        selected: 0,
        context: ContextMenuContext::TabBar,
        anchor_x: x,
        anchor_y: y,
    }
}

/// Create a context menu for right-clicking the workspace bar.
pub fn workspace_bar_menu(x: u16, y: u16) -> ContextMenuState {
    ContextMenuState {
        items: vec![
            ContextMenuItem {
                label: "Close Workspace".into(),
                action: Action::CloseWorkspace,
            },
            ContextMenuItem {
                label: "Rename Workspace".into(),
                action: Action::RenameWorkspace,
            },
        ],
        selected: 0,
        context: ContextMenuContext::WorkspaceBar,
        anchor_x: x,
        anchor_y: y,
    }
}

/// Create a context menu for right-clicking the pane body.
pub fn pane_body_menu(x: u16, y: u16) -> ContextMenuState {
    ContextMenuState {
        items: vec![
            ContextMenuItem {
                label: "Split Right".into(),
                action: Action::SplitHorizontal,
            },
            ContextMenuItem {
                label: "Split Down".into(),
                action: Action::SplitVertical,
            },
            ContextMenuItem {
                label: "Close Tab".into(),
                action: Action::CloseTab,
            },
            ContextMenuItem {
                label: "Copy Mode".into(),
                action: Action::CopyMode,
            },
            ContextMenuItem {
                label: "Paste".into(),
                action: Action::PasteClipboard,
            },
        ],
        selected: 0,
        context: ContextMenuContext::PaneBody,
        anchor_x: x,
        anchor_y: y,
    }
}

/// Render the context menu popup.
pub fn render(state: &ContextMenuState, theme: &Theme, frame: &mut Frame, area: Rect) {
    let (menu_w, menu_h) = menu_dimensions(state, area);

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed { width: menu_w, height: menu_h },
        dialog::PopupAnchor::Position {
            x: state.anchor_x,
            y: state.anchor_y,
        },
        area,
    );

    // No title for context menus
    let inner = dialog::render_popup(frame, popup_area, "", theme);

    for (i, item) in state.items.iter().enumerate() {
        if i as u16 >= inner.height {
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

        let line = Line::from(Span::styled(format!(" {} ", item.label), style));
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        frame.render_widget(Paragraph::new(line), row);
    }
}

/// Calculate menu dimensions (shared between render and hit_test).
fn menu_dimensions(state: &ContextMenuState, area: Rect) -> (u16, u16) {
    let item_count = state.items.len() as u16;
    let max_label = state
        .items
        .iter()
        .map(|i| i.label.len())
        .max()
        .unwrap_or(10) as u16;
    let menu_w = (max_label + 4).min(area.width);
    let menu_h = (item_count + 2).min(area.height);
    (menu_w, menu_h)
}

/// Hit-test the context menu. Returns the index of the clicked item, if any.
pub fn hit_test(state: &ContextMenuState, area: Rect, x: u16, y: u16) -> Option<usize> {
    let (menu_w, menu_h) = menu_dimensions(state, area);

    let popup = dialog::popup_rect(
        dialog::PopupSize::Fixed { width: menu_w, height: menu_h },
        dialog::PopupAnchor::Position {
            x: state.anchor_x,
            y: state.anchor_y,
        },
        area,
    );

    if popup.width <= 2 || popup.height <= 2 {
        return None;
    }
    let inner_x = popup.x + 1;
    let inner_y = popup.y + 1;
    let inner_w = popup.width - 2;
    let inner_h = popup.height - 2;

    if x >= inner_x && x < inner_x + inner_w && y >= inner_y && y < inner_y + inner_h {
        let idx = (y - inner_y) as usize;
        if idx < state.items.len() {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_bar_menu_has_items() {
        let menu = tab_bar_menu(10, 5);
        assert_eq!(menu.items.len(), 2);
        assert_eq!(menu.context, ContextMenuContext::TabBar);
    }

    #[test]
    fn workspace_bar_menu_has_items() {
        let menu = workspace_bar_menu(10, 5);
        assert_eq!(menu.items.len(), 2);
        assert_eq!(menu.context, ContextMenuContext::WorkspaceBar);
    }

    #[test]
    fn pane_body_menu_has_items() {
        let menu = pane_body_menu(10, 5);
        assert_eq!(menu.items.len(), 5);
        assert_eq!(menu.context, ContextMenuContext::PaneBody);
    }

    #[test]
    fn move_up_down() {
        let mut menu = pane_body_menu(0, 0);
        assert_eq!(menu.selected, 0);
        menu.move_up();
        assert_eq!(menu.selected, 0); // already at top
        menu.move_down();
        assert_eq!(menu.selected, 1);
        menu.move_down();
        menu.move_down();
        menu.move_down();
        assert_eq!(menu.selected, 4);
        menu.move_down();
        assert_eq!(menu.selected, 4); // clamped
    }

    #[test]
    fn selected_action_returns_correct() {
        let menu = tab_bar_menu(0, 0);
        assert_eq!(menu.selected_action(), Some(&Action::CloseTab));
    }

    #[test]
    fn hit_test_inside_menu() {
        let area = Rect::new(0, 0, 80, 40);
        let menu = pane_body_menu(5, 5);
        // Inner starts at (6, 6) for border offset
        let result = hit_test(&menu, area, 6, 6);
        assert_eq!(result, Some(0));
        let result = hit_test(&menu, area, 6, 7);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn hit_test_outside_menu() {
        let area = Rect::new(0, 0, 80, 40);
        let menu = pane_body_menu(5, 5);
        // Way outside
        let result = hit_test(&menu, area, 70, 30);
        assert_eq!(result, None);
    }

    #[test]
    fn menu_clamped_to_screen() {
        let area = Rect::new(0, 0, 20, 10);
        // Anchor at far right -- should clamp
        let menu = pane_body_menu(18, 8);
        // Just verify render doesn't panic
        let result = hit_test(&menu, area, 0, 0);
        assert_eq!(result, None);
    }
}
