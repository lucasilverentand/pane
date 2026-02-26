// Re-export session types from pane-protocol
pub use pane_protocol::session::*;

use crate::server::state::ServerState;
use chrono::Utc;
use std::collections::HashMap;

pub fn session_from_state(state: &ServerState) -> Session {
    let mut workspaces = Vec::new();

    for ws in &state.workspaces {
        let mut groups = Vec::new();

        for (gid, group) in &ws.groups {
            let tabs: Vec<TabConfig> = group
                .tabs
                .iter()
                .map(|pane| {
                    let screen = pane.screen();
                    let mut scrollback = Vec::new();
                    let rows = screen.size().0;
                    for row in 0..rows {
                        let line = screen.contents_between(row, 0, row + 1, screen.size().1);
                        scrollback.push(line);
                    }
                    while scrollback
                        .last()
                        .map(|l| l.trim().is_empty())
                        .unwrap_or(false)
                    {
                        scrollback.pop();
                    }

                    TabConfig {
                        id: pane.id,
                        kind: pane.kind.clone(),
                        title: pane.title.clone(),
                        command: pane.command.clone(),
                        cwd: pane.cwd.clone(),
                        env: HashMap::new(),
                        scrollback,
                    }
                })
                .collect();

            groups.push(WindowConfig {
                id: *gid,
                tabs,
                active_tab: group.active_tab,
                name: group.name.clone(),
            });
        }

        workspaces.push(WorkspaceConfig {
            name: ws.name.clone(),
            layout: ws.layout.clone(),
            groups,
            active_group: ws.active_group,
            sync_panes: ws.sync_panes,
        });
    }

    Session {
        id: state.session_id,
        name: state.session_name.clone(),
        created_at: state.session_created_at,
        updated_at: Utc::now(),
        version: 2,
        workspaces,
        active_workspace: state.active_workspace,
    }
}
