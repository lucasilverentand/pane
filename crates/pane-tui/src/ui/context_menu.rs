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
pub fn tab_bar_menu(x: u16, y: u16) -> ContextMenuState {
    ContextMenuState {
        items: vec![
            ContextMenuItem {
                label: "Close Tab".into(),
                action: Action::CloseTab,
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
pub fn render(
    state: &ContextMenuState,
    theme: &Theme,
    hover: Option<(u16, u16)>,
    frame: &mut Frame,
    area: Rect,
) {
    let (menu_w, menu_h) = menu_dimensions(state, area);

    let popup_area = dialog::popup_rect(
        dialog::PopupSize::Fixed {
            width: menu_w,
            height: menu_h,
        },
        dialog::PopupAnchor::Position {
            x: state.anchor_x,
            y: state.anchor_y,
        },
        area,
    );

    // No title for context menus
    let inner = dialog::render_popup(frame, popup_area, "", theme);

    let hovered_item = hover.and_then(|(hx, hy)| hit_test(state, area, hx, hy));

    for (i, item) in state.items.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let is_selected = i == state.selected;
        let is_hovered = hovered_item == Some(i);
        let style = if is_selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if is_hovered {
            Style::default().fg(theme.accent)
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
        dialog::PopupSize::Fixed {
            width: menu_w,
            height: menu_h,
        },
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
        assert_eq!(menu.items.len(), 1);
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
        // Anchor at far right — should clamp
        let menu = pane_body_menu(18, 8);
        // Just verify render doesn't panic
        let result = hit_test(&menu, area, 0, 0);
        assert_eq!(result, None);
    }

    // --- Menu with zero items ---

    #[test]
    fn empty_menu_selected_action_is_none() {
        let menu = ContextMenuState {
            items: vec![],
            selected: 0,
            context: ContextMenuContext::PaneBody,
            anchor_x: 0,
            anchor_y: 0,
        };
        assert_eq!(menu.selected_action(), None);
    }

    #[test]
    fn empty_menu_move_up_stays_at_zero() {
        let mut menu = ContextMenuState {
            items: vec![],
            selected: 0,
            context: ContextMenuContext::PaneBody,
            anchor_x: 0,
            anchor_y: 0,
        };
        menu.move_up();
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn empty_menu_move_down_stays_at_zero() {
        let mut menu = ContextMenuState {
            items: vec![],
            selected: 0,
            context: ContextMenuContext::PaneBody,
            anchor_x: 0,
            anchor_y: 0,
        };
        menu.move_down();
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn empty_menu_hit_test_returns_none() {
        let area = Rect::new(0, 0, 80, 40);
        let menu = ContextMenuState {
            items: vec![],
            selected: 0,
            context: ContextMenuContext::PaneBody,
            anchor_x: 10,
            anchor_y: 10,
        };
        // The menu_dimensions gives max_label=10 (default), item_count=0 → height=2
        // With height=2, inner_h=0 → no items to click
        for x in 0..80 {
            for y in 0..40 {
                assert_eq!(hit_test(&menu, area, x, y), None);
            }
        }
    }

    // --- Navigation wrapping (no wrapping in current impl) ---

    #[test]
    fn navigation_does_not_wrap_top() {
        let mut menu = tab_bar_menu(0, 0);
        assert_eq!(menu.selected, 0);
        // Repeatedly press up — should stay at 0
        for _ in 0..10 {
            menu.move_up();
        }
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn navigation_does_not_wrap_bottom() {
        let mut menu = tab_bar_menu(0, 0);
        // tab_bar_menu has 1 item (index 0)
        for _ in 0..10 {
            menu.move_down();
        }
        assert_eq!(menu.selected, 0); // clamped at last item
    }

    #[test]
    fn navigation_selected_action_tracks() {
        let mut menu = pane_body_menu(0, 0);
        assert_eq!(menu.selected_action(), Some(&Action::SplitHorizontal));
        menu.move_down();
        assert_eq!(menu.selected_action(), Some(&Action::SplitVertical));
        menu.move_down();
        assert_eq!(menu.selected_action(), Some(&Action::CloseTab));
        menu.move_down();
        assert_eq!(menu.selected_action(), Some(&Action::CopyMode));
        menu.move_down();
        assert_eq!(menu.selected_action(), Some(&Action::PasteClipboard));
    }

    // --- Hit test at exact boundaries ---

    #[test]
    fn hit_test_at_inner_top_left() {
        let area = Rect::new(0, 0, 80, 40);
        let menu = pane_body_menu(10, 10);
        let (menu_w, menu_h) = menu_dimensions(&menu, area);
        let popup = dialog::popup_rect(
            dialog::PopupSize::Fixed {
                width: menu_w,
                height: menu_h,
            },
            dialog::PopupAnchor::Position { x: 10, y: 10 },
            area,
        );
        // Inner area starts at (popup.x+1, popup.y+1)
        let ix = popup.x + 1;
        let iy = popup.y + 1;
        assert_eq!(hit_test(&menu, area, ix, iy), Some(0));
    }

    #[test]
    fn hit_test_at_inner_bottom_right() {
        let area = Rect::new(0, 0, 80, 40);
        let menu = pane_body_menu(10, 10);
        let (menu_w, menu_h) = menu_dimensions(&menu, area);
        let popup = dialog::popup_rect(
            dialog::PopupSize::Fixed {
                width: menu_w,
                height: menu_h,
            },
            dialog::PopupAnchor::Position { x: 10, y: 10 },
            area,
        );
        // Inner area: x in [popup.x+1, popup.x+menu_w-1), y in [popup.y+1, popup.y+menu_h-1)
        let ix_last = popup.x + popup.width - 2; // last inner x
        let iy_last = popup.y + popup.height - 2; // last inner y
        let expected_idx = (iy_last - (popup.y + 1)) as usize;
        if expected_idx < menu.items.len() {
            assert_eq!(hit_test(&menu, area, ix_last, iy_last), Some(expected_idx));
        }
    }

    #[test]
    fn hit_test_on_border_returns_none() {
        let area = Rect::new(0, 0, 80, 40);
        let menu = pane_body_menu(10, 10);
        // The popup starts at (10, 10). The border is the outer edge.
        // Top border row
        assert_eq!(hit_test(&menu, area, 10, 10), None);
        // Left border column
        assert_eq!(hit_test(&menu, area, 10, 11), None);
    }

    #[test]
    fn hit_test_just_outside_inner_returns_none() {
        let area = Rect::new(0, 0, 80, 40);
        let menu = pane_body_menu(10, 10);
        let (menu_w, menu_h) = menu_dimensions(&menu, area);
        let popup = dialog::popup_rect(
            dialog::PopupSize::Fixed {
                width: menu_w,
                height: menu_h,
            },
            dialog::PopupAnchor::Position { x: 10, y: 10 },
            area,
        );
        // One pixel past the inner bottom
        let past_bottom = popup.y + popup.height - 1;
        assert_eq!(hit_test(&menu, area, popup.x + 1, past_bottom), None);
        // One pixel past the inner right
        let past_right = popup.x + popup.width - 1;
        assert_eq!(hit_test(&menu, area, past_right, popup.y + 1), None);
    }

    #[test]
    fn hit_test_beyond_item_count_returns_none() {
        let area = Rect::new(0, 0, 80, 40);
        // workspace_bar_menu has 2 items
        let menu = workspace_bar_menu(10, 10);
        let (menu_w, menu_h) = menu_dimensions(&menu, area);
        let popup = dialog::popup_rect(
            dialog::PopupSize::Fixed {
                width: menu_w,
                height: menu_h,
            },
            dialog::PopupAnchor::Position { x: 10, y: 10 },
            area,
        );
        // Try clicking row after the last item (if inner area has room)
        let inner_y = popup.y + 1;
        let beyond_items = inner_y + menu.items.len() as u16;
        if beyond_items < popup.y + popup.height - 1 {
            assert_eq!(
                hit_test(&menu, area, popup.x + 1, beyond_items),
                None,
                "clicking beyond items should return None"
            );
        }
    }

    // --- Single-item menu ---

    #[test]
    fn single_item_menu() {
        let mut menu = ContextMenuState {
            items: vec![ContextMenuItem {
                label: "Only Option".into(),
                action: Action::CloseTab,
            }],
            selected: 0,
            context: ContextMenuContext::PaneBody,
            anchor_x: 5,
            anchor_y: 5,
        };
        assert_eq!(menu.selected_action(), Some(&Action::CloseTab));
        menu.move_down();
        assert_eq!(menu.selected, 0); // can't go beyond single item
        menu.move_up();
        assert_eq!(menu.selected, 0);
    }
}
