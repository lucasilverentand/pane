use ratatui::{layout::Rect, Frame};

use crate::app::App;
use crate::ui::pane_view;

pub fn render_workspace(app: &App, frame: &mut Frame, area: Rect) {
    let ws = app.active_workspace();
    let resolved = ws.layout.resolve(area);
    for (group_id, rect) in resolved {
        if let Some(group) = ws.groups.get(&group_id) {
            let is_active = group_id == ws.active_group;
            pane_view::render_group(group, is_active, frame, rect);
        }
    }
}
