use ratatui::{layout::Rect, Frame};

use crate::app::{App, Mode};
use crate::layout::{LayoutParams, ResolvedPane};
use crate::ui::pane_view;

pub fn render_workspace(app: &App, frame: &mut Frame, area: Rect) {
    let ws = app.active_workspace();
    let params = LayoutParams::from(&app.state.config.behavior);
    let theme = &app.state.config.theme;
    let copy_mode_state = if app.mode == Mode::Copy {
        app.copy_mode_state.as_ref()
    } else {
        None
    };
    let resolved = ws.layout.resolve_with_fold(area, params, &ws.leaf_min_sizes);

    // First pass: render visible panes
    for rp in &resolved {
        if let ResolvedPane::Visible { id: group_id, rect } = rp {
            if let Some(group) = ws.groups.get(group_id) {
                let is_active = *group_id == ws.active_group;
                pane_view::render_group(
                    group,
                    is_active,
                    &app.mode,
                    copy_mode_state,
                    theme,
                    frame,
                    *rect,
                );
            }
        }
    }

    // Second pass: render fold bars on top of pane borders
    for rp in &resolved {
        if let ResolvedPane::Folded { id: group_id, rect, direction } = rp {
            if rect.width == 0 || rect.height == 0 {
                continue;
            }
            let is_active = *group_id == ws.active_group;
            pane_view::render_folded(is_active, *direction, theme, frame, *rect);
        }
    }
}
